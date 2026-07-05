//! Skills 工具：从配置的 skills 目录按需加载技能手册。
//!
//! 设计对齐 awake，但做了裁剪以适配 markflow：
//! - 两个工具 `skills_list` / `skills_view` 都在后端本地执行（读文件系统），
//!   与 `McpChatTool` 同属"后端本地工具"，不走前端 broker 往返。
//! - 只解析 SKILL.md frontmatter 里的 `description`；不做平台/脚本可用性检测
//!   （markflow 单机场景下所有技能恒可用）。
//! - 路径全部约束在 skills 根目录内，拒绝 `..` / 绝对路径穿越。

use std::{
    convert::Infallible,
    fs,
    path::{Component, Path, PathBuf},
};

use axum::{
    extract::Extension,
    http::StatusCode,
    response::{sse::Event, IntoResponse, Response},
    Json,
};
use rig::{
    completion::ToolDefinition as RigToolDefinition,
    tool::{ToolDyn, ToolError},
    wasm_compat::WasmBoxedFuture,
};
use serde::Deserialize;
use serde_json::{json, Value};
use tokio::sync::mpsc;
use std::sync::Arc;

use super::send_json_event;

const DEFAULT_LIST_LIMIT: usize = 50;
const MAX_LIST_LIMIT: usize = 200;
const DEFAULT_VIEW_BYTES: usize = 200_000;
const MAX_VIEW_BYTES: usize = 1_000_000;


#[derive(Debug, Deserialize)]
pub struct SkillRequest {
    pub skill: String,
}

#[derive(Debug, Deserialize)]
pub struct SkillFileReadRequest {
    pub skill: String,
    pub path: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct SkillFileSaveRequest {
    pub skill: String,
    pub path: String,
    pub content: String,
}

fn json_ok(value: Value) -> Response {
    Json(value).into_response()
}

fn json_error(status: StatusCode, message: String) -> Response {
    (status, Json(json!({ "error": message }))).into_response()
}

fn runtime_skills_root(runtime_config: &crate::BackendRuntimeConfig) -> Option<PathBuf> {
    resolve_skills_root(&runtime_config.skills_root_dir)
}

pub async fn list_skills_handler(
    Extension(runtime_config): Extension<Arc<crate::BackendRuntimeConfig>>,
) -> Response {
    match skills_ui_list_impl(&runtime_skills_root(&runtime_config)) {
        Ok(skills) => json_ok(json!({ "skills": skills })),
        Err(error) => json_error(StatusCode::BAD_REQUEST, error),
    }
}

pub async fn list_skill_files_handler(
    Extension(runtime_config): Extension<Arc<crate::BackendRuntimeConfig>>,
    Json(body): Json<SkillRequest>,
) -> Response {
    match skill_files_impl(&runtime_skills_root(&runtime_config), &body.skill) {
        Ok(tree) => json_ok(json!({ "tree": tree })),
        Err(error) => json_error(StatusCode::BAD_REQUEST, error),
    }
}

pub async fn read_skill_file_handler(
    Extension(runtime_config): Extension<Arc<crate::BackendRuntimeConfig>>,
    Json(body): Json<SkillFileReadRequest>,
) -> Response {
    match read_skill_file_impl(
        &runtime_skills_root(&runtime_config),
        &body.skill,
        body.path.as_deref(),
        MAX_VIEW_BYTES,
    ) {
        Ok(file) => json_ok(json!({ "file": file })),
        Err(error) => json_error(StatusCode::BAD_REQUEST, error),
    }
}

pub async fn save_skill_file_handler(
    Extension(runtime_config): Extension<Arc<crate::BackendRuntimeConfig>>,
    Json(body): Json<SkillFileSaveRequest>,
) -> Response {
    match write_skill_file_impl(
        &runtime_skills_root(&runtime_config),
        &body.skill,
        &body.path,
        &body.content,
    ) {
        Ok(()) => json_ok(json!({ "ok": true })),
        Err(error) => json_error(StatusCode::BAD_REQUEST, error),
    }
}

pub async fn delete_skill_handler(
    Extension(runtime_config): Extension<Arc<crate::BackendRuntimeConfig>>,
    Json(body): Json<SkillRequest>,
) -> Response {
    match delete_skill_impl(&runtime_skills_root(&runtime_config), &body.skill) {
        Ok(()) => json_ok(json!({ "ok": true })),
        Err(error) => json_error(StatusCode::BAD_REQUEST, error),
    }
}

/// skills_list：列出 skills 根目录下的全部技能及其可读文件。
pub struct SkillsListTool {
    root_dir: Option<PathBuf>,
    tx: mpsc::Sender<Result<Event, Infallible>>,
}

/// skills_view：读取某个技能下的资源文件（默认 SKILL.md）。
pub struct SkillsViewTool {
    root_dir: Option<PathBuf>,
    tx: mpsc::Sender<Result<Event, Infallible>>,
}

impl SkillsListTool {
    pub fn new(root_dir: Option<PathBuf>, tx: mpsc::Sender<Result<Event, Infallible>>) -> Self {
        Self { root_dir, tx }
    }
}

impl SkillsViewTool {
    pub fn new(root_dir: Option<PathBuf>, tx: mpsc::Sender<Result<Event, Infallible>>) -> Self {
        Self { root_dir, tx }
    }
}

impl ToolDyn for SkillsListTool {
    fn name(&self) -> String {
        "skills_list".to_string()
    }

    fn definition(&self, _prompt: String) -> WasmBoxedFuture<'_, RigToolDefinition> {
        Box::pin(async move {
            RigToolDefinition {
                name: "skills_list".to_string(),
                description: "列出配置的 skills 目录下的可用技能。每个技能包含 `files` 列出该技能下所有可读文件。用 skills_view 配合 `path` 读取任意列出的文件。".to_string(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "max_results": {
                            "type": "integer",
                            "description": "可选，最多列出多少个技能。默认 50，上限 200。"
                        }
                    },
                    "additionalProperties": false
                }),
            }
        })
    }

    fn call<'a>(&'a self, args: String) -> WasmBoxedFuture<'a, Result<String, ToolError>> {
        Box::pin(async move {
            let arguments: Value = serde_json::from_str(&args).unwrap_or_else(|_| json!({}));
            let max_results = arguments
                .get("max_results")
                .and_then(Value::as_u64)
                .map(|value| (value as usize).clamp(1, MAX_LIST_LIMIT))
                .unwrap_or(DEFAULT_LIST_LIMIT);

            let output = match skills_list_impl(&self.root_dir, max_results) {
                Ok(value) => json!({ "ok": true, "tool": "skills_list", "result": value }),
                Err(error) => json!({ "ok": false, "tool": "skills_list", "error": error }),
            };
            emit_tool_event(&self.tx, "skills_list", &arguments, &output).await;
            Ok(output.to_string())
        })
    }
}

impl ToolDyn for SkillsViewTool {
    fn name(&self) -> String {
        "skills_view".to_string()
    }

    fn definition(&self, _prompt: String) -> WasmBoxedFuture<'_, RigToolDefinition> {
        Box::pin(async move {
            RigToolDefinition {
                name: "skills_view".to_string(),
                description: "读取技能资源文件。这是加载技能目录下任何内容的唯一工具。先调用 skills_list 发现可用技能及其 `files`，再用 `name` 和可选 `path` 读取具体文件；省略 path 时读取该技能的 SKILL.md。".to_string(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "name": {
                            "type": "string",
                            "description": "来自 skills_list 的技能名。"
                        },
                        "path": {
                            "type": "string",
                            "description": "来自技能 `files` 的文件路径。省略则读取 SKILL.md。"
                        },
                        "max_bytes": {
                            "type": "integer",
                            "description": "可选，最多读取多少字节。默认 200000，上限 1000000。"
                        }
                    },
                    "required": ["name"],
                    "additionalProperties": false
                }),
            }
        })
    }

    fn call<'a>(&'a self, args: String) -> WasmBoxedFuture<'a, Result<String, ToolError>> {
        Box::pin(async move {
            let arguments: Value = serde_json::from_str(&args).unwrap_or_else(|_| json!({}));
            let output = match skills_view_impl(&self.root_dir, &arguments) {
                Ok(value) => json!({ "ok": true, "tool": "skills_view", "result": value }),
                Err(error) => json!({ "ok": false, "tool": "skills_view", "error": error }),
            };
            emit_tool_event(&self.tx, "skills_view", &arguments, &output).await;
            Ok(output.to_string())
        })
    }
}

async fn emit_tool_event(
    tx: &mpsc::Sender<Result<Event, Infallible>>,
    tool: &str,
    arguments: &Value,
    output: &Value,
) {
    let status = if output.get("ok").and_then(Value::as_bool).unwrap_or(false) {
        "completed"
    } else {
        "failed"
    };
    let _ = send_json_event(
        tx,
        "tool_event",
        json!({
            "tool": tool,
            "status": status,
            "source": "skills",
            "arguments": arguments.clone(),
            "output": output.clone(),
        }),
    )
    .await;
}

fn skills_list_impl(root_dir: &Option<PathBuf>, max_results: usize) -> Result<Value, String> {
    let root = configured_skills_root(root_dir)?;
    let mut skills = Vec::new();
    collect_skill_summaries(&root, &root, max_results, &mut skills)?;
    skills.sort_by(|left, right| left.name.cmp(&right.name));
    let truncated = skills.len() >= max_results;

    let items = skills
        .into_iter()
        .map(|item| {
            json!({
                "name": item.name,
                "description": item.description,
                "files": item.files,
            })
        })
        .collect::<Vec<_>>();

    Ok(json!({
        "root_dir": display_path(&root),
        "skills": items,
        "truncated": truncated,
    }))
}

fn skills_view_impl(root_dir: &Option<PathBuf>, arguments: &Value) -> Result<Value, String> {
    let name = arguments
        .get("name")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "缺少技能名 name".to_string())?;
    let max_bytes = arguments
        .get("max_bytes")
        .and_then(Value::as_u64)
        .map(|value| (value as usize).clamp(1, MAX_VIEW_BYTES))
        .unwrap_or(DEFAULT_VIEW_BYTES);

    let root = configured_skills_root(root_dir)?;
    let skill_rel = normalize_relative_path(name)?;
    let skill_dir = root.join(&skill_rel);
    let skill_dir = fs::canonicalize(&skill_dir).map_err(|_| format!("技能 `{name}` 不存在"))?;
    ensure_path_inside_root(&skill_dir, &root)?;
    if !skill_dir.is_dir() || !skill_dir.join("SKILL.md").is_file() {
        return Err(format!("技能 `{name}` 不存在"));
    }

    let resource = arguments
        .get("path")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("SKILL.md");
    let resource_path = join_resource_path(&skill_dir, resource)?;
    if !resource_path.exists() {
        return Err(format!("技能资源 `{resource}` 不存在"));
    }
    if !resource_path.is_file() {
        return Err(format!("技能资源 `{resource}` 不是文件"));
    }

    let bytes = fs::read(&resource_path).map_err(|err| format!("读取技能资源失败: {err}"))?;
    let truncated = bytes.len() > max_bytes;
    let content_bytes = if truncated { &bytes[..max_bytes] } else { &bytes[..] };

    Ok(json!({
        "name": name,
        "path": display_path(&resource_path),
        "bytes_read": content_bytes.len(),
        "truncated": truncated,
        "content": String::from_utf8_lossy(content_bytes).to_string(),
    }))
}

struct SkillSummary {
    name: String,
    description: Option<String>,
    files: Vec<String>,
}

fn configured_skills_root(root_dir: &Option<PathBuf>) -> Result<PathBuf, String> {
    let root = root_dir
        .as_ref()
        .ok_or_else(|| "未配置 skills 目录".to_string())?;
    if !root.is_dir() {
        return Err(format!("skills 目录 `{}` 不是目录", display_path(root)));
    }
    fs::canonicalize(root).map_err(|err| format!("skills 目录不可访问: {err}"))
}

fn collect_skill_summaries(
    root: &Path,
    current: &Path,
    max_results: usize,
    skills: &mut Vec<SkillSummary>,
) -> Result<(), String> {
    if skills.len() >= max_results {
        return Ok(());
    }
    let entries = fs::read_dir(current).map_err(|err| format!("读取 skills 目录失败: {err}"))?;
    for entry in entries {
        let entry = entry.map_err(|err| format!("读取 skills 目录失败: {err}"))?;
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        if path.join("SKILL.md").is_file() {
            let name = path
                .strip_prefix(root)
                .map_err(|_| "技能路径解析失败".to_string())?
                .components()
                .filter_map(|component| match component {
                    Component::Normal(part) => Some(part.to_string_lossy().to_string()),
                    _ => None,
                })
                .collect::<Vec<_>>()
                .join("/");
            let description = parse_skill_description(&path.join("SKILL.md"));
            let files = list_skill_files(&path, &path);
            skills.push(SkillSummary {
                name,
                description,
                files,
            });
        }
        if skills.len() >= max_results {
            break;
        }
        collect_skill_summaries(root, &path, max_results, skills)?;
    }
    Ok(())
}

/// 列出技能目录下的全部文件，返回相对该技能目录的路径。
fn list_skill_files(skill_root: &Path, current: &Path) -> Vec<String> {
    let mut files = Vec::new();
    let Ok(entries) = fs::read_dir(current) else {
        return files;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            files.extend(list_skill_files(skill_root, &path));
        } else if path.is_file() {
            if let Ok(rel) = path.strip_prefix(skill_root) {
                let rel = rel
                    .components()
                    .filter_map(|component| match component {
                        Component::Normal(part) => Some(part.to_string_lossy().to_string()),
                        _ => None,
                    })
                    .collect::<Vec<_>>()
                    .join("/");
                if !rel.is_empty() {
                    files.push(rel);
                }
            }
        }
    }
    files.sort();
    files
}

/// 解析 SKILL.md frontmatter 里的 description（只支持首块 `---` 包裹的 YAML 的单行 description）。
fn parse_skill_description(skill_md: &Path) -> Option<String> {
    let content = fs::read_to_string(skill_md).ok()?;
    let rest = content.strip_prefix("---")?;
    let rest = rest.strip_prefix('\n').or_else(|| rest.strip_prefix("\r\n"))?;
    let end = rest.find("\n---").or_else(|| rest.find("\r\n---"))?;
    let frontmatter = &rest[..end];
    for line in frontmatter.lines() {
        let line = line.trim();
        if let Some(value) = line.strip_prefix("description:") {
            let value = value.trim().trim_matches('"').trim_matches('\'').trim();
            if !value.is_empty() {
                return Some(value.to_string());
            }
        }
    }
    None
}

/// 规范化相对路径，拒绝绝对路径、根、盘符与 `..` 穿越。
fn normalize_relative_path(path: &str) -> Result<PathBuf, String> {
    if path.trim().is_empty() {
        return Err(format!("非法路径 `{path}`"));
    }
    let mut normalized = PathBuf::new();
    for component in Path::new(path).components() {
        match component {
            Component::Normal(part) => normalized.push(part),
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                return Err(format!("非法路径 `{path}`"));
            }
        }
    }
    if normalized.as_os_str().is_empty() {
        Err(format!("非法路径 `{path}`"))
    } else {
        Ok(normalized)
    }
}

fn join_resource_path(skill_dir: &Path, path: &str) -> Result<PathBuf, String> {
    let normalized = normalize_relative_path(path)?;
    let joined = skill_dir.join(normalized);
    let canonical = if joined.exists() {
        fs::canonicalize(&joined).map_err(|err| format!("路径解析失败: {err}"))?
    } else {
        joined
    };
    if canonical.exists() {
        ensure_path_inside_root(&canonical, skill_dir)?;
    }
    Ok(canonical)
}

fn ensure_path_inside_root(path: &Path, root: &Path) -> Result<(), String> {
    let canonical_root = fs::canonicalize(root).map_err(|err| format!("根路径解析失败: {err}"))?;
    if path.starts_with(&canonical_root) {
        Ok(())
    } else {
        Err(format!("路径 `{}` 超出 skills 根目录", display_path(path)))
    }
}

fn display_path(path: &Path) -> String {
    path.to_string_lossy().to_string()
}


fn skills_ui_list_impl(root_dir: &Option<PathBuf>) -> Result<Vec<Value>, String> {
    let root = match configured_skills_root(root_dir) {
        Ok(root) => root,
        Err(_) => return Ok(Vec::new()),
    };
    let mut skills = Vec::new();
    collect_skill_summaries(&root, &root, MAX_LIST_LIMIT, &mut skills)?;
    skills.sort_by(|left, right| left.name.cmp(&right.name));
    Ok(skills
        .into_iter()
        .map(|skill| {
            let skill_dir = root.join(&skill.name);
            json!({
                "name": skill.name,
                "display_name": skill.name.rsplit('/').next().unwrap_or(skill.name.as_str()),
                "description": skill.description,
                "path": display_path(&skill_dir),
                "relative_path": skill.name,
                "file_count": skill.files.len(),
                "available": true,
            })
        })
        .collect())
}

fn resolve_skill_dir(root_dir: &Option<PathBuf>, skill: &str) -> Result<(PathBuf, PathBuf), String> {
    let root = configured_skills_root(root_dir)?;
    let relative = normalize_relative_path(skill)?;
    let skill_dir = fs::canonicalize(root.join(relative)).map_err(|_| format!("Skill `{skill}` 不存在"))?;
    ensure_path_inside_root(&skill_dir, &root)?;
    if !skill_dir.is_dir() || !skill_dir.join("SKILL.md").is_file() {
        return Err(format!("Skill `{skill}` 不存在或缺少 SKILL.md"));
    }
    Ok((root, skill_dir))
}

fn skill_files_impl(root_dir: &Option<PathBuf>, skill: &str) -> Result<Value, String> {
    let (_root, skill_dir) = resolve_skill_dir(root_dir, skill)?;
    skill_file_tree(&skill_dir, &skill_dir)
}

fn skill_file_tree(root: &Path, current: &Path) -> Result<Value, String> {
    let metadata = fs::metadata(current).map_err(|err| format!("读取 Skill 文件失败: {err}"))?;
    let rel = current
        .strip_prefix(root)
        .ok()
        .map(path_to_slash_string)
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| current.file_name().map(|name| name.to_string_lossy().to_string()).unwrap_or_default());
    let name = current
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_else(|| rel.clone());
    if metadata.is_file() {
        return Ok(json!({ "name": name, "path": rel, "kind": "file", "children": [] }));
    }

    let mut children = Vec::new();
    let entries = fs::read_dir(current).map_err(|err| format!("读取 Skill 目录失败: {err}"))?;
    for entry in entries {
        let entry = entry.map_err(|err| format!("读取 Skill 目录失败: {err}"))?;
        let path = entry.path();
        if path.is_dir() || path.is_file() {
            children.push(skill_file_tree(root, &path)?);
        }
    }
    children.sort_by(|left, right| {
        left.get("kind").and_then(Value::as_str).unwrap_or("")
            .cmp(right.get("kind").and_then(Value::as_str).unwrap_or(""))
            .then_with(|| left.get("name").and_then(Value::as_str).unwrap_or("").cmp(right.get("name").and_then(Value::as_str).unwrap_or("")))
    });
    Ok(json!({ "name": name, "path": rel, "kind": "dir", "children": children }))
}

fn path_to_slash_string(path: &Path) -> String {
    path.components()
        .filter_map(|component| match component {
            Component::Normal(part) => Some(part.to_string_lossy().to_string()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("/")
}

fn read_skill_file_impl(
    root_dir: &Option<PathBuf>,
    skill: &str,
    path: Option<&str>,
    max_bytes: usize,
) -> Result<Value, String> {
    let (_root, skill_dir) = resolve_skill_dir(root_dir, skill)?;
    let resource = path.unwrap_or("SKILL.md");
    let file = join_resource_path(&skill_dir, resource)?;
    if !file.is_file() {
        return Err(format!("Skill 文件 `{resource}` 不存在"));
    }
    let bytes = fs::read(&file).map_err(|err| format!("读取 Skill 文件失败: {err}"))?;
    let max_bytes = max_bytes.clamp(1, MAX_VIEW_BYTES);
    let truncated = bytes.len() > max_bytes;
    let content_bytes = if truncated { &bytes[..max_bytes] } else { &bytes[..] };
    Ok(json!({
        "skill": skill,
        "path": normalize_relative_path(resource)?.to_string_lossy().replace('\\', "/"),
        "content": String::from_utf8_lossy(content_bytes).to_string(),
        "bytes_read": content_bytes.len(),
        "truncated": truncated,
    }))
}

fn write_skill_file_impl(
    root_dir: &Option<PathBuf>,
    skill: &str,
    path: &str,
    content: &str,
) -> Result<(), String> {
    let (_root, skill_dir) = resolve_skill_dir(root_dir, skill)?;
    let file = join_resource_path(&skill_dir, path)?;
    if !file.is_file() {
        return Err(format!("Skill 文件 `{path}` 不存在"));
    }
    fs::write(&file, content).map_err(|err| format!("写入 Skill 文件失败: {err}"))
}

fn delete_skill_impl(root_dir: &Option<PathBuf>, skill: &str) -> Result<(), String> {
    let (root, skill_dir) = resolve_skill_dir(root_dir, skill)?;
    if skill_dir == root {
        return Err("不能删除 Skills 根目录".to_string());
    }
    fs::remove_dir_all(&skill_dir).map_err(|err| format!("删除 Skill 目录失败: {err}"))
}

/// 供 build_agent_tools 组装：返回 skills 两个后端本地工具。
pub fn build_skills_tools(
    root_dir: Option<PathBuf>,
    tx: &mpsc::Sender<Result<Event, Infallible>>,
) -> Vec<Box<dyn ToolDyn>> {
    vec![
        Box::new(SkillsListTool::new(root_dir.clone(), tx.clone())) as Box<dyn ToolDyn>,
        Box::new(SkillsViewTool::new(root_dir, tx.clone())) as Box<dyn ToolDyn>,
    ]
}

/// 构建系统提示词用的技能索引：每个技能一行“名字：描述”。
/// 目录未配置、不存在或无技能时返回 None，调用方据此决定是否注入这一段。
pub fn build_skills_index(root_dir: &Option<PathBuf>) -> Option<String> {
    let root = configured_skills_root(root_dir).ok()?;
    let mut skills = Vec::new();
    collect_skill_summaries(&root, &root, MAX_LIST_LIMIT, &mut skills).ok()?;
    if skills.is_empty() {
        return None;
    }
    skills.sort_by(|left, right| left.name.cmp(&right.name));
    let lines = skills
        .into_iter()
        .map(|skill| match skill.description {
            Some(description) if !description.is_empty() => {
                format!("- {}：{}", skill.name, description)
            }
            _ => format!("- {}", skill.name),
        })
        .collect::<Vec<_>>()
        .join("\n");
    Some(lines)
}

/// 供上层从字符串路径构造 skills 根：空字符串视为未配置。
pub fn resolve_skills_root(raw: &str) -> Option<PathBuf> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(PathBuf::from(trimmed))
    }
}
