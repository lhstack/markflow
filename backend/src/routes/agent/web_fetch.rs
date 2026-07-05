//! web_fetch 工具：后端本地执行 HTTP 请求。
//!
//! 对齐 awake 的 web_fetch 能力：GET/POST/PUT/DELETE、headers、proxy、text/json/form/multipart/file body、
//! timeout 与 max_response_bytes。文件路径限制在后端当前工作目录内，避免路径穿越。

use std::{
    collections::BTreeMap,
    convert::Infallible,
    fs,
    path::{Component, Path, PathBuf},
    time::Duration,
};

use axum::response::sse::Event;
use rig::{
    completion::ToolDefinition as RigToolDefinition,
    tool::{ToolDyn, ToolError},
    wasm_compat::WasmBoxedFuture,
};
use serde::Deserialize;
use serde_json::{json, Value};
use tokio::sync::mpsc;

use super::send_json_event;

const DEFAULT_TIMEOUT_SECS: u64 = 30;
const DEFAULT_MAX_RESPONSE_BYTES: usize = 1_000_000;
const MAX_TIMEOUT_SECS: u64 = 300;
const MAX_RESPONSE_BYTES: usize = 5_000_000;

#[derive(Debug, Clone)]
pub struct WebFetchRuntimeConfig {
    pub default_proxy: Option<String>,
    pub default_timeout_secs: u64,
    pub default_max_response_bytes: usize,
}

impl Default for WebFetchRuntimeConfig {
    fn default() -> Self {
        Self {
            default_proxy: None,
            default_timeout_secs: DEFAULT_TIMEOUT_SECS,
            default_max_response_bytes: DEFAULT_MAX_RESPONSE_BYTES,
        }
    }
}

#[derive(Debug, Deserialize)]
struct WebFetchArgs {
    method: Option<String>,
    url: String,
    headers: Option<BTreeMap<String, String>>,
    proxy: Option<String>,
    body_type: Option<String>,
    body: Option<String>,
    json_body: Option<String>,
    form: Option<BTreeMap<String, String>>,
    form_data: Option<Vec<WebFetchFormPart>>,
    file: Option<WebFetchFileBody>,
    timeout_secs: Option<u64>,
    max_response_bytes: Option<usize>,
}

#[derive(Debug, Deserialize)]
struct WebFetchFormPart {
    name: String,
    value: Option<String>,
    file_path: Option<String>,
    file_name: Option<String>,
    mime: Option<String>,
}

#[derive(Debug, Deserialize)]
struct WebFetchFileBody {
    path: String,
    mime: Option<String>,
}

pub struct WebFetchTool {
    config: WebFetchRuntimeConfig,
    workspace: PathBuf,
    tx: mpsc::Sender<Result<Event, Infallible>>,
}

impl WebFetchTool {
    pub fn new(config: WebFetchRuntimeConfig, tx: mpsc::Sender<Result<Event, Infallible>>) -> Self {
        let workspace = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        Self { config, workspace, tx }
    }
}

impl ToolDyn for WebFetchTool {
    fn name(&self) -> String {
        "web_fetch".to_string()
    }

    fn definition(&self, _prompt: String) -> WasmBoxedFuture<'_, RigToolDefinition> {
        Box::pin(async move {
            RigToolDefinition {
                name: "web_fetch".to_string(),
                description: "Send an HTTP request. Supports GET, POST, PUT, DELETE, headers, proxy, and various body types.".to_string(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "method": {
                            "type": "string",
                            "enum": ["GET", "POST", "PUT", "DELETE"],
                            "description": "Optional HTTP method. Defaults to GET when omitted or empty."
                        },
                        "url": {
                            "type": "string",
                            "description": "Absolute http or https URL."
                        },
                        "headers": {
                            "type": "object",
                            "additionalProperties": { "type": "string" },
                            "description": "Optional request headers."
                        },
                        "proxy": {
                            "type": "string",
                            "description": "Optional proxy URL, for example http://127.0.0.1:7890. Overrides configured proxy."
                        },
                        "body_type": {
                            "type": "string",
                            "enum": ["none", "text", "json", "form", "multipart", "file"],
                            "description": "Request body type."
                        },
                        "body": {
                            "type": "string",
                            "description": "Plain text request body."
                        },
                        "json_body": {
                            "type": "string",
                            "description": "JSON body as a string."
                        },
                        "form": {
                            "type": "object",
                            "additionalProperties": { "type": "string" },
                            "description": "application/x-www-form-urlencoded fields."
                        },
                        "form_data": {
                            "type": "array",
                            "description": "Multipart form-data parts.",
                            "items": {
                                "type": "object",
                                "properties": {
                                    "name": { "type": "string" },
                                    "value": { "type": "string" },
                                    "file_path": { "type": "string", "description": "Workspace-relative file path for file part." },
                                    "file_name": { "type": "string" },
                                    "mime": { "type": "string" }
                                },
                                "required": ["name"]
                            }
                        },
                        "file": {
                            "type": "object",
                            "description": "Raw file upload body.",
                            "properties": {
                                "path": { "type": "string", "description": "Workspace-relative file path." },
                                "mime": { "type": "string" }
                            },
                            "required": ["path"]
                        },
                        "timeout_secs": {
                            "type": "integer",
                            "description": "Optional request timeout seconds."
                        },
                        "max_response_bytes": {
                            "type": "integer",
                            "description": "Optional max response bytes."
                        }
                    },
                    "required": ["url"],
                    "additionalProperties": false
                }),
            }
        })
    }

    fn call<'a>(&'a self, args: String) -> WasmBoxedFuture<'a, Result<String, ToolError>> {
        Box::pin(async move {
            let raw_arguments: Value = serde_json::from_str(&args).unwrap_or_else(|_| json!({}));
            let output = match serde_json::from_value::<WebFetchArgs>(raw_arguments.clone()) {
                Ok(arguments) => match web_fetch_impl(arguments, &self.config, &self.workspace).await {
                    Ok(value) => json!({ "ok": true, "tool": "web_fetch", "result": value }),
                    Err(error) => json!({ "ok": false, "tool": "web_fetch", "error": error }),
                },
                Err(error) => json!({ "ok": false, "tool": "web_fetch", "error": format!("参数解析失败: {error}") }),
            };
            emit_tool_event(&self.tx, &raw_arguments, &output).await;
            Ok(output.to_string())
        })
    }
}

async fn emit_tool_event(
    tx: &mpsc::Sender<Result<Event, Infallible>>,
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
            "tool": "web_fetch",
            "status": status,
            "source": "web_fetch",
            "arguments": arguments.clone(),
            "output": output.clone(),
        }),
    )
    .await;
}

async fn web_fetch_impl(
    args: WebFetchArgs,
    config: &WebFetchRuntimeConfig,
    workspace: &Path,
) -> Result<Value, String> {
    let method = parse_http_method(args.method.as_deref())?;
    let url = args.url.trim();
    if !(url.starts_with("http://") || url.starts_with("https://")) {
        return Err("url 必须是 http 或 https 绝对地址".to_string());
    }

    let timeout_secs = args
        .timeout_secs
        .unwrap_or(config.default_timeout_secs)
        .clamp(1, MAX_TIMEOUT_SECS);
    let max_response_bytes = args
        .max_response_bytes
        .unwrap_or(config.default_max_response_bytes)
        .clamp(1, MAX_RESPONSE_BYTES);

    let mut client_builder = reqwest::Client::builder().timeout(Duration::from_secs(timeout_secs));
    let proxy = args.proxy.as_ref().or(config.default_proxy.as_ref());
    if let Some(proxy) = proxy.filter(|value| !value.trim().is_empty()) {
        client_builder = client_builder
            .proxy(reqwest::Proxy::all(proxy.trim()).map_err(|err| format!("代理配置无效: {err}"))?);
    }
    let client = client_builder.build().map_err(|err| format!("创建 HTTP 客户端失败: {err}"))?;
    let mut request = client.request(method, url);

    if let Some(headers) = &args.headers {
        for (name, value) in headers {
            let header_name = reqwest::header::HeaderName::from_bytes(name.as_bytes())
                .map_err(|_| format!("请求头名称无效: {name}"))?;
            let header_value = reqwest::header::HeaderValue::from_str(value)
                .map_err(|_| format!("请求头 `{name}` 的值无效"))?;
            request = request.header(header_name, header_value);
        }
    }

    request = apply_body(request, &args, workspace)?;
    let response = request.send().await.map_err(|err| format!("HTTP 请求失败: {err}"))?;
    let status = response.status();
    let final_url = response.url().to_string();
    let headers = response_headers_to_map(response.headers());
    let bytes = response.bytes().await.map_err(|err| format!("读取响应失败: {err}"))?;
    let truncated = bytes.len() > max_response_bytes;
    let body_bytes = if truncated { &bytes[..max_response_bytes] } else { &bytes };

    Ok(json!({
        "status": status.as_u16(),
        "success": status.is_success(),
        "final_url": final_url,
        "headers": headers,
        "body": String::from_utf8_lossy(body_bytes).to_string(),
        "bytes_read": body_bytes.len(),
        "truncated": truncated,
    }))
}

fn parse_http_method(method: Option<&str>) -> Result<reqwest::Method, String> {
    let method = method.map(str::trim).filter(|value| !value.is_empty()).unwrap_or("GET");
    match method.to_ascii_uppercase().as_str() {
        "GET" => Ok(reqwest::Method::GET),
        "POST" => Ok(reqwest::Method::POST),
        "PUT" => Ok(reqwest::Method::PUT),
        "DELETE" => Ok(reqwest::Method::DELETE),
        _ => Err(format!("HTTP method `{method}` 无效")),
    }
}

fn apply_body(
    request: reqwest::RequestBuilder,
    args: &WebFetchArgs,
    workspace: &Path,
) -> Result<reqwest::RequestBuilder, String> {
    let Some(body_type) = args.body_type.as_deref().map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(request);
    };

    match body_type {
        "none" => Ok(request),
        "text" => args
            .body
            .clone()
            .map(|body| request.body(body))
            .ok_or_else(|| format!("body_type `{body_type}` 缺少 body")),
        "json" => {
            let json_body = args
                .json_body
                .as_deref()
                .filter(|body| !body.trim().is_empty())
                .ok_or_else(|| format!("body_type `{body_type}` 缺少 json_body"))?;
            let value: Value = serde_json::from_str(json_body)
                .map_err(|err| format!("json_body 无效: {err}"))?;
            Ok(request.json(&value))
        }
        "x-www-form-urlencoded" | "urlencoded" | "form" => {
            let form = args
                .form
                .as_ref()
                .ok_or_else(|| format!("body_type `{body_type}` 缺少 form"))?;
            let body = url::form_urlencoded::Serializer::new(String::new())
                .extend_pairs(form.iter().map(|(key, value)| (key.as_str(), value.as_str())))
                .finish();
            Ok(request
                .header(
                    reqwest::header::CONTENT_TYPE,
                    "application/x-www-form-urlencoded",
                )
                .body(body))
        }
        "form_data" | "multipart" => {
            let parts = args
                .form_data
                .as_ref()
                .ok_or_else(|| format!("body_type `{body_type}` 缺少 form_data"))?;
            Ok(request.multipart(build_multipart_form(parts, workspace)?))
        }
        "file" => {
            let file = args
                .file
                .as_ref()
                .ok_or_else(|| format!("body_type `{body_type}` 缺少 file"))?;
            let path = resolve_workspace_file(workspace, &file.path)?;
            let bytes = fs::read(&path).map_err(|err| format!("读取文件失败: {err}"))?;
            let mut request = request.body(bytes);
            if let Some(mime) = &file.mime {
                request = request.header(reqwest::header::CONTENT_TYPE, mime);
            }
            Ok(request)
        }
        other => Err(format!("body_type `{other}` 无效")),
    }
}

fn build_multipart_form(parts: &[WebFetchFormPart], workspace: &Path) -> Result<reqwest::multipart::Form, String> {
    let mut form = reqwest::multipart::Form::new();
    for part in parts {
        if part.name.trim().is_empty() {
            return Err("form_data part 缺少 name".to_string());
        }
        if let Some(file_path) = &part.file_path {
            let path = resolve_workspace_file(workspace, file_path)?;
            let bytes = fs::read(&path).map_err(|err| format!("读取文件失败: {err}"))?;
            let file_name = part
                .file_name
                .clone()
                .or_else(|| path.file_name().and_then(|name| name.to_str()).map(ToOwned::to_owned))
                .unwrap_or_else(|| "file".to_string());
            let mut reqwest_part = reqwest::multipart::Part::bytes(bytes).file_name(file_name);
            if let Some(mime) = &part.mime {
                reqwest_part = reqwest_part
                    .mime_str(mime)
                    .map_err(|err| format!("multipart mime 无效: {err}"))?;
            }
            form = form.part(part.name.clone(), reqwest_part);
        } else if let Some(value) = &part.value {
            form = form.text(part.name.clone(), value.clone());
        } else {
            return Err(format!("form_data part `{}` 缺少 value 或 file_path", part.name));
        }
    }
    Ok(form)
}

fn resolve_workspace_file(workspace: &Path, raw: &str) -> Result<PathBuf, String> {
    let raw = raw.trim();
    if raw.is_empty() {
        return Err("文件路径不能为空".to_string());
    }
    let rel = Path::new(raw);
    if rel.is_absolute() {
        return Err("文件路径必须是工作目录相对路径".to_string());
    }
    if rel.components().any(|component| matches!(component, Component::ParentDir)) {
        return Err("文件路径不能包含 ..".to_string());
    }
    let root = fs::canonicalize(workspace).map_err(|err| format!("工作目录不可访问: {err}"))?;
    let path = fs::canonicalize(root.join(rel)).map_err(|err| format!("文件不存在或不可访问: {err}"))?;
    if !path.starts_with(&root) {
        return Err("文件路径越界".to_string());
    }
    if !path.is_file() {
        return Err("文件路径不是普通文件".to_string());
    }
    Ok(path)
}

fn response_headers_to_map(headers: &reqwest::header::HeaderMap) -> BTreeMap<String, String> {
    headers
        .iter()
        .map(|(name, value)| (name.to_string(), value.to_str().unwrap_or("<non-utf8>").to_string()))
        .collect()
}

pub fn build_web_fetch_tools(
    config: WebFetchRuntimeConfig,
    tx: &mpsc::Sender<Result<Event, Infallible>>,
) -> Vec<Box<dyn ToolDyn>> {
    vec![Box::new(WebFetchTool::new(config, tx.clone())) as Box<dyn ToolDyn>]
}
