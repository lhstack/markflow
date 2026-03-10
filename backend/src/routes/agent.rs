use std::{
    convert::Infallible,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
    time::Duration,
};

use aes_gcm::{
    aead::{Aead, KeyInit},
    Aes256Gcm, Nonce,
};
use async_openai::{
    config::OpenAIConfig,
    types::{
        chat::{
            ChatCompletionMessageToolCall, ChatCompletionMessageToolCalls,
            ChatCompletionRequestAssistantMessage, ChatCompletionRequestMessage,
            ChatCompletionRequestSystemMessage, ChatCompletionRequestToolMessage,
            ChatCompletionRequestToolMessageContent, ChatCompletionRequestUserMessage,
            ChatCompletionTool, ChatCompletionTools, CreateChatCompletionRequestArgs,
            FinishReason, FunctionCall as ChatFunctionCall, FunctionObject,
        },
        responses::{
            CreateResponseArgs, EasyInputContent, EasyInputMessage, FunctionCallOutput,
            FunctionCallOutputItemParam, FunctionTool, FunctionToolCall, InputItem, InputParam,
            Item, MessageType, OutputItem, ReasoningArgs, ReasoningEffort, ReasoningSummary,
            ResponseStreamEvent, Role, Tool,
        },
    },
    Client,
};
use axum::{
    extract::{Extension, Path},
    http::{HeaderMap, StatusCode},
    response::{
        sse::{Event, KeepAlive, Sse},
        IntoResponse, Response,
    },
    Json,
};
use base64::{engine::general_purpose, Engine as _};
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use serde_json::json;
use sha2::{Digest, Sha256};
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;

use crate::{auth, db::Database, models::AgentProvider};

enum AgentStreamError {
    Retryable(String),
    Fatal(String),
}

static NEXT_AGENT_STREAM_LOG_ID: AtomicU64 = AtomicU64::new(1);

#[derive(Debug, Deserialize, Clone)]
pub struct AgentProviderPayload {
    pub provider_id: i64,
    pub model: String,
}

#[derive(Debug, Deserialize, Clone)]
pub struct AgentModelsRequest {
    pub provider_id: i64,
}

#[derive(Debug, Deserialize)]
pub struct AgentProviderUpsertRequest {
    pub id: Option<i64>,
    pub name: String,
    pub base_url: Option<String>,
    pub api_key: Option<String>,
    pub remote_models: Option<Vec<String>>,
    pub enabled_models: Option<Vec<String>>,
    pub custom_models: Option<Vec<String>>,
}

#[derive(Debug, Serialize)]
pub struct AgentProviderSummary {
    pub id: i64,
    pub name: String,
    pub base_url: String,
    pub remote_models: Vec<String>,
    pub enabled_models: Vec<String>,
    pub custom_models: Vec<String>,
    pub is_active: bool,
    pub has_api_key: bool,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Serialize)]
pub struct AgentProvidersResponse {
    pub providers: Vec<AgentProviderSummary>,
    pub active_provider_id: Option<i64>,
}

#[derive(Debug, Serialize)]
pub struct AgentProviderDetailResponse {
    pub id: i64,
    pub name: String,
    pub base_url: String,
    pub api_key: String,
    pub remote_models: Vec<String>,
    pub enabled_models: Vec<String>,
    pub custom_models: Vec<String>,
    pub is_active: bool,
}

#[derive(Debug, Serialize)]
pub struct AgentModelSummary {
    pub id: String,
    pub owned_by: String,
    pub created: u32,
}

#[derive(Debug, Deserialize, Clone)]
pub struct AgentMessagePayload {
    pub role: String,
    pub content: String,
}

#[derive(Debug, Deserialize, Default, Clone)]
pub struct AgentContextPayload {
    pub page_scope: Option<String>,
    pub project_name: Option<String>,
    pub doc_id: Option<i64>,
    pub doc_name: Option<String>,
    pub doc_content: Option<String>,
    pub project_catalog: Option<String>,
    pub current_node_catalog: Option<String>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct AgentChatStreamRequest {
    pub provider: AgentProviderPayload,
    pub messages: Vec<AgentMessagePayload>,
    pub mode: Option<String>,
    pub write_mode: Option<String>,
    pub context: Option<AgentContextPayload>,
    pub previous_response_id: Option<String>,
    pub tool_outputs: Option<Vec<AgentToolOutputPayload>>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct AgentToolOutputPayload {
    pub call_id: String,
    pub name: Option<String>,
    pub arguments: Option<String>,
    pub output: serde_json::Value,
}

#[derive(Debug, Serialize, Clone)]
pub struct AgentToolCallRequest {
    pub call_id: String,
    pub name: String,
    pub arguments: String,
}

enum AgentStreamOutcome {
    Message {
        text: String,
        response_id: Option<String>,
    },
    ToolCalls {
        response_id: String,
        calls: Vec<AgentToolCallRequest>,
    },
}

fn truncate_chars(raw: &str, max_chars: usize) -> String {
    let truncated: String = raw.chars().take(max_chars).collect();
    if raw.chars().count() > max_chars {
        format!("{}\n\n[内容已截断]", truncated)
    } else {
        truncated
    }
}

fn preview_for_log(raw: &str, max_chars: usize) -> String {
    let compact = raw.replace('\r', "\\r").replace('\n', "\\n");
    truncate_chars(&compact, max_chars)
}

fn summarize_context_for_log(ctx: Option<&AgentContextPayload>) -> serde_json::Value {
    match ctx {
        Some(ctx) => json!({
            "page_scope": ctx.page_scope,
            "project_name": ctx.project_name,
            "doc_id": ctx.doc_id,
            "doc_name": ctx.doc_name,
            "doc_content": ctx.doc_content.as_deref().map(|value| preview_for_log(value, 600)),
            "project_catalog": ctx.project_catalog.as_deref().map(|value| preview_for_log(value, 400)),
            "current_node_catalog": ctx.current_node_catalog.as_deref().map(|value| preview_for_log(value, 400)),
        }),
        None => serde_json::Value::Null,
    }
}

fn summarize_messages_for_log(messages: &[AgentMessagePayload]) -> Vec<serde_json::Value> {
    messages
        .iter()
        .map(|message| {
            json!({
                "role": message.role,
                "content": preview_for_log(&message.content, 600),
            })
        })
        .collect()
}

fn summarize_tool_outputs_for_log(
    outputs: Option<&[AgentToolOutputPayload]>,
) -> Vec<serde_json::Value> {
    outputs
        .unwrap_or(&[])
        .iter()
        .map(|output| {
            json!({
                "call_id": output.call_id,
                "name": output.name,
                "arguments": output.arguments.as_deref().map(|value| preview_for_log(value, 600)),
                "output": preview_for_log(&output.output.to_string(), 600),
            })
        })
        .collect()
}

fn normalize_base_url(value: Option<&str>) -> String {
    match value.map(str::trim).filter(|v| !v.is_empty()) {
        Some(url) => url.trim_end_matches('/').to_string(),
        None => "https://api.openai.com/v1".to_string(),
    }
}

fn provider_secret() -> String {
    std::env::var("SHARE_PASSWORD_SECRET")
        .or_else(|_| std::env::var("JWT_SECRET"))
        .unwrap_or_else(|_| "markflow_dev_secret_change_in_production".to_string())
}

fn provider_cipher() -> Aes256Gcm {
    let digest = Sha256::digest(provider_secret().as_bytes());
    Aes256Gcm::new_from_slice(&digest).expect("agent provider key length should be valid")
}

fn encrypt_provider_api_key(api_key: &str) -> anyhow::Result<String> {
    let cipher = provider_cipher();
    let nonce_bytes: [u8; 12] = rand::random();
    let nonce = Nonce::from_slice(&nonce_bytes);
    let ciphertext = cipher
        .encrypt(nonce, api_key.as_bytes())
        .map_err(|_| anyhow::anyhow!("agent provider api key encryption failed"))?;

    let mut payload = Vec::with_capacity(nonce_bytes.len() + ciphertext.len());
    payload.extend_from_slice(&nonce_bytes);
    payload.extend_from_slice(&ciphertext);
    Ok(general_purpose::STANDARD.encode(payload))
}

fn decrypt_provider_api_key(ciphertext: &str) -> anyhow::Result<String> {
    let decoded = general_purpose::STANDARD.decode(ciphertext)?;
    if decoded.len() < 13 {
        anyhow::bail!("invalid agent provider api key ciphertext");
    }
    let (nonce_bytes, body) = decoded.split_at(12);
    let cipher = provider_cipher();
    let plaintext = cipher
        .decrypt(Nonce::from_slice(nonce_bytes), body)
        .map_err(|_| anyhow::anyhow!("agent provider api key decryption failed"))?;
    Ok(String::from_utf8(plaintext)?)
}

fn parse_json_string_array(raw: &str) -> Vec<String> {
    serde_json::from_str::<Vec<String>>(raw)
        .unwrap_or_default()
        .into_iter()
        .map(|item| item.trim().to_string())
        .filter(|item| !item.is_empty())
        .collect()
}

fn serialize_json_string_array(values: &[String]) -> String {
    serde_json::to_string(values).unwrap_or_else(|_| "[]".to_string())
}

fn unique_strings(values: Vec<String>) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    let mut items = Vec::new();

    for value in values {
        let normalized = value.trim();
        if normalized.is_empty() {
            continue;
        }
        if seen.insert(normalized.to_string()) {
            items.push(normalized.to_string());
        }
    }

    items
}

fn provider_to_summary(provider: AgentProvider) -> AgentProviderSummary {
    AgentProviderSummary {
        id: provider.id,
        name: provider.name,
        base_url: provider.base_url,
        remote_models: parse_json_string_array(&provider.remote_models),
        enabled_models: parse_json_string_array(&provider.enabled_models),
        custom_models: parse_json_string_array(&provider.custom_models),
        is_active: provider.is_active == 1,
        has_api_key: !provider.api_key_ciphertext.trim().is_empty(),
        created_at: provider.created_at,
        updated_at: provider.updated_at,
    }
}

async fn list_user_providers(db: &Database, user_id: i64) -> Result<Vec<AgentProvider>, Response> {
    sqlx::query_as::<_, AgentProvider>(
        "SELECT * FROM agent_providers WHERE user_id = ? ORDER BY is_active DESC, updated_at DESC, id DESC",
    )
    .bind(user_id)
    .fetch_all(&db.pool)
    .await
    .map_err(|err| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": format!("获取供应商列表失败: {}", err)})),
        )
            .into_response()
    })
}

async fn find_user_provider(
    db: &Database,
    user_id: i64,
    provider_id: i64,
) -> Result<AgentProvider, Response> {
    sqlx::query_as::<_, AgentProvider>("SELECT * FROM agent_providers WHERE id = ? AND user_id = ?")
        .bind(provider_id)
        .bind(user_id)
        .fetch_optional(&db.pool)
        .await
        .map_err(|err| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": format!("查询供应商失败: {}", err)})),
            )
                .into_response()
        })?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(json!({"error": "供应商不存在"})),
            )
                .into_response()
        })
}

async fn set_active_provider(
    db: &Database,
    user_id: i64,
    provider_id: i64,
) -> Result<(), Response> {
    sqlx::query("UPDATE agent_providers SET is_active = CASE WHEN id = ? THEN 1 ELSE 0 END, updated_at = datetime('now') WHERE user_id = ?")
        .bind(provider_id)
        .bind(user_id)
        .execute(&db.pool)
        .await
        .map_err(|err| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": format!("激活供应商失败: {}", err)})),
            )
                .into_response()
        })?;

    Ok(())
}

fn normalize_agent_mode(value: Option<&str>) -> String {
    match value.map(str::trim).filter(|v| !v.is_empty()) {
        Some("chat") => "chat".to_string(),
        Some("write") => "write".to_string(),
        _ => "auto".to_string(),
    }
}

fn normalize_write_mode(value: Option<&str>) -> Option<String> {
    match value.map(str::trim).filter(|v| !v.is_empty()) {
        Some("append") => Some("append".to_string()),
        Some("replace") => Some("replace".to_string()),
        _ => None,
    }
}

fn build_system_prompt(
    mode: &str,
    write_mode: Option<&str>,
    username: &str,
    ctx: &AgentContextPayload,
    use_tools: bool,
) -> String {
    let mut parts = vec![
        "你是 MarkFlow 内置的智能文档助手。请使用中文，回答要直接、可执行、少废话。".to_string(),
        "如果用户要求输出 Markdown 文档内容，优先输出结构清晰的 Markdown。".to_string(),
        format!("当前登录用户：{}", username),
    ];

    if let Some(scope) = ctx.page_scope.as_deref() {
        parts.push(format!("当前页面作用域：{}", scope));
    }
    if let Some(project_name) = ctx.project_name.as_deref() {
        parts.push(format!("当前项目：{}", project_name));
    }
    if let Some(doc_name) = ctx.doc_name.as_deref() {
        parts.push(format!("当前文档：{}", doc_name));
    }
    if let Some(doc_id) = ctx.doc_id {
        parts.push(format!("当前文档 ID：{}", doc_id));
    }
    if let Some(project_catalog) = ctx
        .project_catalog
        .as_deref()
        .filter(|value| !value.trim().is_empty())
    {
        parts.push(format!("当前可见项目列表：{}", project_catalog));
    }
    if let Some(current_node_catalog) = ctx
        .current_node_catalog
        .as_deref()
        .filter(|value| !value.trim().is_empty())
    {
        parts.push(format!("当前项目可见目录/文档：{}", current_node_catalog));
    }

    if use_tools {
        parts.push(
            r#"硬性规则：当目标是生成或修改文档正文时，工具只允许用于定位、打开、创建空文档、读取文档、读取编辑器快照、保存文档、更新文档信息。这条规则优先级高于下面的通用“先用工具”指导。最终正文必须通过 assistant 文本流输出，且正文的第一个字节必须是 [[ACTION:append]]、[[ACTION:replace]]、[[ACTION:rewrite_section]]、[[ACTION:replace_block]] 之一。禁止在 create_tree_node 参数中携带完整 Markdown 正文。
只有在所有必要工具调用完成后，才允许输出 [[ACTION:...]] 标记；并且该标记必须位于最终正文最开头，不能放在结尾。
写文档正文时，必须严格使用 [[ACTION:append]]...[[/ACTION]]、[[ACTION:replace]]...[[/ACTION]]、[[ACTION:rewrite_section]]...[[/ACTION]] 或 [[ACTION:replace_block]]...[[/ACTION]]；只有两个标记之间的内容才视为文档正文。
局部重写协议（rewrite_section）块内必须包含：
[[TARGET]]要重写的小节标题（不带 #）[[/TARGET]]
[[CONTENT]]该小节重写后的完整 Markdown（建议包含标题行）[[/CONTENT]]
rewrite_section 的语义是“整节替换”：TARGET 对应旧小节必须被完整替换并清理，不得保留旧段落后再追加新段落。
局部替换协议（replace_block）块内必须包含：
[[FIND]]原文中要替换的片段[[/FIND]]
[[REPLACE]]替换后的新片段[[/REPLACE]]
replace_block 的语义是“定位替换”：FIND 片段在结果中应被 REPLACE 完整替代，不得与旧片段并存。
当用户要求“把某段内容移动到指定位置”时，结果必须满足“源位置删除 + 目标位置插入”，最终文档中该内容只能保留一份，禁止旧位置和新位置同时保留。
执行局部重写/局部替换后必须自检：1）旧内容已清理；2）无重复段落/重复小节；3）目录与编号顺序与新结构一致。
协议标记必须完整且精确：不能缺少括号、不能改变大小写、不能在标记内加入空格、不能输出半截标记。
混合输出硬性规则：[[ACTION:...]]...[[/ACTION]] 内的内容进入 Markdown 编辑器；标记外内容进入聊天面板。输出 [[/ACTION]] 后必须继续输出聊天文本，不能在 [[/ACTION]] 处立即结束回复。
在 [[/ACTION]] 之后，必须按固定结构输出“保存状态与优化建议”模块（放在标记外，不得放进正文标记内）。
保存状态行必须二选一且原样输出：`保存状态：已保存` 或 `保存状态：未保存`。
若为未保存，下一行必须补充：`原因：...`，说明未保存原因（例如仅生成草稿、待用户确认等）。
然后必须输出 `优化建议：` 小节，并给出与当前文档直接相关的 3-5 条编号建议（1. 2. 3. ...），每条建议聚焦一个可执行点，不得少于 3 条，不得超过 5 条。
在 `优化建议` 之后，必须追加 `本次修改总结：` 小节，用 a-z 条简短编号项总结“本次具体改了什么、影响了哪些位置、预期收益是什么”，禁止空泛表述。
本轮优先通过函数调用完成页面导航、项目管理、文档树操作、文档读取、保存与元信息更新，以及浏览器端自动化。
不要输出旧版 [[ROUTE:...]] 标记；需要切换页面时优先使用工具调用。
执行任何写操作前，先尽量读取当前页面状态、项目列表、项目树或文档内容，确保目标准确。
当用户询问当前页面内容、当前处于什么页面、有哪些页面/路由、有哪些项目、项目树结构、当前文档内容、当前有哪些函数/工具可用时，不要凭上下文猜测，必须优先调用对应工具获取信息。
读取当前文档正文时，先判断是否未保存：先调用 get_current_page_state 获取当前状态；若当前文档存在未保存修改，必须调用 read_editor_snapshot 读取实时快照；若当前文档已保存或没有可用编辑器快照，再调用 read_document 读取已保存正文。
禁止在“当前文档未保存”时仅依赖 read_document 进行续写、改写或替换判断，避免覆盖用户尚未保存的编辑内容。
如果用户想知道你有哪些页面操作能力或函数调用能力，先结合工具列表本身回答；如果还需要当前页面实时信息，再调用 get_current_page_state、list_page_routes、list_projects、get_project_tree 等工具补充。
当用户意图仅为“保存当前文档/确认保存”且未要求改写正文时，禁止输出任何 [[ACTION:...]] 文档协议，必须调用 save_current_document 工具完成真实保存，再在聊天面板反馈保存结果。
当用户意图仅为“修改项目信息”（项目名/描述/背景图）且未要求改写正文时，禁止输出任何 [[ACTION:...]] 文档协议，必须调用 update_project 工具完成修改，再在聊天面板反馈结果。
当用户意图仅为“修改节点信息”（例如重命名文档或目录）且未要求改写正文时，禁止输出任何 [[ACTION:...]] 文档协议，必须调用 update_tree_node_meta 工具完成修改，再在聊天面板反馈结果。
如果要改写、补写、完善文档，除非用户明确说了“保存”“直接保存”“写完保存”“应用到文档”“提交修改”这类意思，否则默认只生成未保存草稿。
当你在未得到明确保存授权的情况下写入了文档，保存状态必须输出为“保存状态：未保存”，并补充原因；不要擅自声称已经保存。
如果需要填写表单、点击按钮、操作弹窗或调用编辑器对象，优先使用专用工具；只有专用工具不足时再使用 execute_browser_javascript。
当工具已经完成用户需求时，用简短中文总结结果；不要重复输出工具返回的整段 JSON。
编辑意图判断流程：必须先结合当前文档内容、历史对话上下文和用户最新输入，先识别编辑意图，再评估影响范围，最后决定协议动作。
意图识别优先级：判断本轮属于 追加 / 插入 / 替换 / 润色 / 重写 中的哪一类；“补充、继续、完善、展开、加上、重写、整理”等词只能作为参考信号，不能机械映射为固定动作。
范围评估要求：判断本轮影响是 局部片段 / 单个小节 / 多个相关小节 / 文档大部分内容 / 整篇结构。
协议动作决策：若本轮主要是局部新增，优先使用 [[ACTION:append]]；若是整篇改写或大范围重排，使用 [[ACTION:replace]]；若是按标题重写某个小节，使用 [[ACTION:rewrite_section]]；若是替换一段原文片段，使用 [[ACTION:replace_block]]。
关键约束：append / replace / rewrite_section / replace_block 是协议动作，不等同于编辑意图；编辑意图与协议动作不是一一对应关系。若局部编辑可以满足需求，不得选择整体重写。并且必须在标记外明确给出：`操作类型：追加/插入/替换/润色/重写` 和 `修改位置：...`。"#
                .to_string(),
        );
    } else {
        match mode {
            "chat" => {
                parts.push("本轮只进行对话回答，不要输出任何 [[ACTION:...]] 动作标记。".to_string());
            }
            "write" => {
                let action = match write_mode {
                    Some("replace") => "replace",
                    _ => "append",
                };
                parts.push(format!(
                    "本轮必须以 [[ACTION:{}]] 开头输出正文，不要输出其他动作标记，也不要解释动作选择。",
                    action
                ));
                parts.push(
                    r#"正文必须以 [[ACTION:...]] 开始，并以 [[/ACTION]] 结束。开始和结束标记之间的内容才是要写入文档的 Markdown。
在 [[/ACTION]] 结束后，必须继续输出聊天文本，并严格使用以下结构：`保存状态：已保存` 或 `保存状态：未保存`；若未保存，下一行补 `原因：...`；然后输出 `优化建议：`，并给出 3-5 条编号建议；最后输出 `本次修改总结：`，并给出 2-4 条编号总结。"#
                        .to_string(),
                );
            }
            _ => {
                parts.push(
                    r#"你必须在最终正文开头只选择一个动作标记：[[ACTION:chat]]、[[ACTION:append]]、[[ACTION:replace]]、[[ACTION:rewrite_section]]、[[ACTION:replace_block]]。
如果只是回答问题或解释，使用 [[ACTION:chat]]。如果用户要求继续完善现有文档，使用 [[ACTION:append]]。如果用户要求整体改写、重写、整理当前文档，使用 [[ACTION:replace]]。如果用户要求按标题重写某个小节，使用 [[ACTION:rewrite_section]]。如果用户要求替换指定片段，使用 [[ACTION:replace_block]]。
动作标记后面紧接正文，不要解释你选择了什么动作。
如果选择 [[ACTION:append]]、[[ACTION:replace]]、[[ACTION:rewrite_section]] 或 [[ACTION:replace_block]]，必须再输出一个结束标记 [[/ACTION]]，并且只有这两个标记之间的内容属于文档正文。
当动作为 [[ACTION:rewrite_section]] 时，块内必须包含 [[TARGET]]...[[/TARGET]] 和 [[CONTENT]]...[[/CONTENT]]。
当动作为 [[ACTION:replace_block]] 时，块内必须包含 [[FIND]]...[[/FIND]] 和 [[REPLACE]]...[[/REPLACE]]。
若使用 rewrite_section，必须完整替换目标小节并清理旧内容；若使用 replace_block，必须让 FIND 在结果中被 REPLACE 完整替代，禁止旧内容残留。
若用户意图是移动内容，必须同时满足“源位置删除 + 目标位置插入”，最终只保留一份内容，并保证目录/编号顺序正确。
如果选择上述任一文档动作，在 [[/ACTION]] 之后必须继续输出聊天文本，并严格使用以下结构：`保存状态：已保存` 或 `保存状态：未保存`；若未保存，下一行补 `原因：...`；然后输出 `优化建议：`，并给出 3-5 条编号建议；最后输出 `本次修改总结：`，并给出 2-4 条编号总结。
如果用户明确要求切换页面、进入项目概览、进入某个项目或打开当前项目中的某个文档，你可以在最前面额外输出一个路由标记，然后紧跟动作标记。
可用路由标记格式只有三种：[[ROUTE:overview]]、[[ROUTE:project:项目名]]、[[ROUTE:doc:文档名]]。
只有在你能从上下文中确认目标名称时才输出路由标记；不要编造项目名或文档名。"#
                        .to_string(),
                );
            }
        }
    }

    if matches!(mode, "auto" | "write") {
        if let Some(content) = ctx
            .doc_content
            .as_deref()
            .filter(|value| !value.trim().is_empty())
        {
            parts.push("以下是当前文档正文摘要，可用于参考：".to_string());
            parts.push(truncate_chars(content, 6000));
        }
    }

    parts.join("\n\n")
}

fn build_response_input(payload: &AgentChatStreamRequest) -> Vec<InputItem> {
    if let Some(outputs) = payload.tool_outputs.as_ref().filter(|items| !items.is_empty()) {
        let mut items: Vec<InputItem> = payload
            .messages
            .iter()
            .filter_map(|message| {
                let content = message.content.trim();
                if content.is_empty() {
                    return None;
                }

                let role = match message.role.as_str() {
                    "assistant" => Role::Assistant,
                    "system" => Role::System,
                    "developer" => Role::Developer,
                    _ => Role::User,
                };

                Some(InputItem::from(EasyInputMessage {
                    r#type: MessageType::Message,
                    role,
                    content: EasyInputContent::Text(content.to_string()),
                }))
            })
            .collect();

        for output in outputs {
            if let (Some(name), Some(arguments)) = (
                output.name.as_deref().map(str::trim).filter(|value| !value.is_empty()),
                output
                    .arguments
                    .as_deref()
                    .map(str::trim)
                    .filter(|value| !value.is_empty()),
            ) {
                items.push(InputItem::from(Item::FunctionCall(FunctionToolCall {
                    arguments: arguments.to_string(),
                    call_id: output.call_id.clone(),
                    name: name.to_string(),
                    id: None,
                    status: None,
                })));
            }

            items.push(InputItem::from(Item::FunctionCallOutput(
                FunctionCallOutputItemParam {
                    call_id: output.call_id.clone(),
                    output: FunctionCallOutput::Text(output.output.to_string()),
                    id: None,
                    status: None,
                },
            )));
        }

        return items;
    }

    payload
        .messages
        .iter()
        .filter_map(|message| {
            let content = message.content.trim();
            if content.is_empty() {
                return None;
            }

            let role = match message.role.as_str() {
                "assistant" => Role::Assistant,
                "system" => Role::System,
                "developer" => Role::Developer,
                _ => Role::User,
            };

            Some(InputItem::from(EasyInputMessage {
                r#type: MessageType::Message,
                role,
                content: EasyInputContent::Text(content.to_string()),
            }))
        })
        .collect()
}

fn function_tool(name: &str, description: &str, parameters: serde_json::Value) -> Tool {
    FunctionTool {
        name: name.to_string(),
        description: Some(description.to_string()),
        parameters: Some(parameters),
        strict: Some(false),
    }
    .into()
}

fn agent_function_tools() -> Vec<Tool> {
    vec![
        function_tool(
            "get_current_page_state",
            "读取当前前端页面的实时状态。会返回当前路由、页面作用域、页面状态、当前项目、当前文档或目录、编辑器状态、当前可见项目列表摘要、当前可见文档树摘要和当前可执行能力。执行任何导航、项目操作、文档树操作、文档读写之前，优先先调用它确认当前 UI 所处位置。",
            json!({
                "type": "object",
                "properties": {},
                "additionalProperties": false,
            }),
        ),
        function_tool(
            "list_page_routes",
            "列出当前前端应用支持的页面路由、说明和参数要求。返回结果会包含 login、register、login.2fa、share 这些真实路由，以及 home.overview、home.project、home.doc、home.dir 这些 Home 页内部状态路由。无参数。",
            json!({
                "type": "object",
                "properties": {},
                "additionalProperties": false,
            }),
        ),
        function_tool(
            "navigate_to_page",
            "按“route + 定位参数”的方式跳转页面。适合明确切换到概览页、登录页、注册页、2FA 页、分享页，或按路由语义进入某个项目、文档、目录。优先传 ID；名称和路径只作为兜底。如果目标只是打开项目，优先用 open_project；如果目标只是打开文档或目录，优先用 open_tree_node。",
            json!({
                "type": "object",
                "properties": {
                    "route": {
                        "type": "string",
                        "description": "目标页面路由。home.overview=项目概览；home.project=项目工作区；home.doc=文档编辑页；home.dir=目录详情页；login/register/login.2fa/share 为真实页面。",
                        "enum": [
                            "home.overview",
                            "home.project",
                            "home.doc",
                            "home.dir",
                            "login",
                            "register",
                            "login.2fa",
                            "share"
                        ]
                    },
                    "share_token": { "type": "string", "description": "当 route=share 时必填，表示分享链接 token。" },
                    "project_id": { "type": "integer", "description": "目标项目 ID。route=home.project/home.doc/home.dir 时优先使用。" },
                    "project_name": { "type": "string", "description": "目标项目名称。只有拿不到 project_id 时再使用。" },
                    "node_id": { "type": "integer", "description": "目标文档或目录节点 ID。route=home.doc/home.dir 时优先使用。" },
                    "node_path": { "type": "string", "description": "目标节点路径，例如 产品文档/接口/API说明 。只有拿不到 node_id 时再使用。" },
                    "node_name": { "type": "string", "description": "目标节点名称。只有拿不到 node_id 和 node_path 时再使用。" }
                },
                "required": ["route"],
                "additionalProperties": false,
            }),
        ),
        function_tool(
            "list_projects",
            "获取当前用户当前可访问的项目列表。适合在打开项目、创建项目、删除项目前先确认项目全集。可选 refresh=true 强制刷新，返回项目列表、总数和当前项目信息。",
            json!({
                "type": "object",
                "properties": {
                    "refresh": { "type": "boolean", "description": "是否强制从后端刷新项目列表。true=忽略当前页面缓存。" }
                },
                "additionalProperties": false,
            }),
        ),
        function_tool(
            "open_project",
            "打开指定项目，并进入该项目的工作区和文档树视图。适合“打开某个项目”“进入某个项目的文档编辑列表”这类需求。可以按 project_id 或 project_name 指定目标，优先使用 project_id。",
            json!({
                "type": "object",
                "properties": {
                    "project_id": { "type": "integer", "description": "要打开的项目 ID。优先使用这个字段。" },
                    "project_name": { "type": "string", "description": "要打开的项目名称。只有拿不到 project_id 时再使用。" },
                    "fetch_tree": { "type": "boolean", "description": "打开后是否确保加载该项目的文档树。默认 true。" }
                },
                "additionalProperties": false,
            }),
        ),
        function_tool(
            "create_project",
            "创建新项目。适用于帮用户新建项目、填写项目名称、描述、背景图等场景。默认创建后进入该项目；如不想跳转，可传 open_after_create=false。",
            json!({
                "type": "object",
                "properties": {
                    "name": { "type": "string", "description": "项目名称" },
                    "description": { "type": "string", "description": "项目描述，可为空" },
                    "background_image": { "type": "string", "description": "项目背景图 URL，可为空" },
                    "open_after_create": { "type": "boolean", "description": "创建完成后是否立即打开该项目。默认 true。" }
                },
                "required": ["name"],
                "additionalProperties": false,
            }),
        ),
        function_tool(
            "update_project",
            "更新项目信息。可按 project_id 或 project_name 定位目标项目，支持修改 name、description、background_image。适用于重命名项目、更新项目描述、更新项目背景图。",
            json!({
                "type": "object",
                "properties": {
                    "project_id": { "type": "integer", "description": "目标项目 ID。优先使用。" },
                    "project_name": { "type": "string", "description": "目标项目名称。只有拿不到 project_id 时再使用。" },
                    "name": { "type": "string", "description": "项目新名称。" },
                    "description": { "type": "string", "description": "项目新描述，可为空字符串。" },
                    "background_image": { "type": "string", "description": "项目新背景图 URL，可为空字符串。" }
                },
                "additionalProperties": false,
            }),
        ),
        function_tool(
            "delete_projects",
            "删除一个或多个项目。删除前应先通过 list_projects 确认目标准确，避免误删。可以传 project_ids，也可以传 project_names；支持批量删除。",
            json!({
                "type": "object",
                "properties": {
                    "project_ids": {
                        "type": "array",
                        "description": "要删除的项目 ID 列表。批量删除时优先使用。",
                        "items": { "type": "integer" }
                    },
                    "project_names": {
                        "type": "array",
                        "description": "要删除的项目名称列表。只有拿不到 project_ids 时再使用。",
                        "items": { "type": "string" }
                    }
                },
                "additionalProperties": false,
            }),
        ),
        function_tool(
            "execute_browser_javascript",
            "在浏览器端执行一段 JavaScript。适合处理点击按钮、填写表单、操作弹窗、读取 DOM、触发事件、直接调用编辑器桥接对象这类细粒度动作。代码运行在 async 环境，可直接使用 window、document、location、history、navigator、localStorage、sessionStorage、console、editor、markflow；其中 editor 是 Markdown 编辑器桥接对象，markflow 是前端工具助手对象。只有在专用工具不足以完成任务时才使用它。必须 return 可序列化结果。",
            json!({
                "type": "object",
                "properties": {
                    "code": { "type": "string", "description": "要执行的 JavaScript 代码，运行在 async 函数体内" }
                },
                "required": ["code"],
                "additionalProperties": false,
            }),
        ),
        function_tool(
            "get_project_tree",
            "读取指定项目下的完整文档树。适合在创建文档、创建目录、定位父目录、打开节点、删除节点前先确认结构。可以通过 project_id 或 project_name 指定目标；返回结果会包含树结构、扁平节点列表和统计信息。",
            json!({
                "type": "object",
                "properties": {
                    "project_id": { "type": "integer", "description": "目标项目 ID。优先使用。" },
                    "project_name": { "type": "string", "description": "目标项目名称。只有拿不到 project_id 时再使用。" },
                    "refresh": { "type": "boolean", "description": "是否强制从后端刷新该项目文档树。默认 false。" }
                },
                "additionalProperties": false,
            }),
        ),
        function_tool(
            "create_tree_node",
            "在指定项目下创建文档或目录。支持在根目录创建，也支持通过 parent_id、parent_path 或 parent_name 指定父目录；node_type 只能是 doc 或 dir。创建文档时只创建空文档壳，不要在这个工具里写入完整正文；正文必须在文档打开后通过 assistant 文本流输出。",
            json!({
                "type": "object",
                "properties": {
                    "project_id": { "type": "integer", "description": "目标项目 ID。优先使用。" },
                    "project_name": { "type": "string", "description": "目标项目名称。只有拿不到 project_id 时再使用。" },
                    "parent_id": { "type": "integer", "description": "父目录节点 ID。优先使用；不传表示在项目根目录创建。" },
                    "parent_path": { "type": "string", "description": "父目录路径，例如 产品文档/接口 。只有拿不到 parent_id 时再使用。" },
                    "parent_name": { "type": "string", "description": "父目录名称。只有拿不到 parent_id 和 parent_path 时再使用。" },
                    "name": { "type": "string", "description": "新建节点名称。" },
                    "node_type": { "type": "string", "description": "新建节点类型。doc=文档，dir=目录。", "enum": ["doc", "dir"] },
                    "open_after_create": { "type": "boolean", "description": "创建后是否立即打开该节点。默认 true。" }
                },
                "required": ["name", "node_type"],
                "additionalProperties": false,
            }),
        ),
        function_tool(
            "move_tree_node",
            "移动一个文档或目录到同一项目内的另一个目录下，或移动到项目根目录。适合“把某篇文档移到某个目录”“把某个目录移到根目录”。优先传源节点 ID 和目标父目录 ID；路径和名称只作为兜底。当前不支持跨项目移动。",
            json!({
                "type": "object",
                "properties": {
                    "project_id": { "type": "integer", "description": "源节点所在项目 ID。优先使用。" },
                    "project_name": { "type": "string", "description": "源节点所在项目名称。只有拿不到 project_id 时再使用。" },
                    "node_id": { "type": "integer", "description": "要移动的源节点 ID。优先使用。" },
                    "node_path": { "type": "string", "description": "要移动的源节点路径。只有拿不到 node_id 时再使用。" },
                    "node_name": { "type": "string", "description": "要移动的源节点名称。只有拿不到 node_id 和 node_path 时再使用。" },
                    "target_parent_id": { "type": "integer", "description": "目标父目录节点 ID。优先使用；不传且 to_root=true 表示移到项目根目录。" },
                    "target_parent_path": { "type": "string", "description": "目标父目录路径。只有拿不到 target_parent_id 时再使用。" },
                    "target_parent_name": { "type": "string", "description": "目标父目录名称。只有拿不到 target_parent_id 和 target_parent_path 时再使用。" },
                    "to_root": { "type": "boolean", "description": "是否直接移动到项目根目录。true 时忽略 target_parent_*。" },
                    "sort_order": { "type": "integer", "description": "移动后在目标父目录下的排序位置。默认追加到末尾。" }
                },
                "additionalProperties": false,
            }),
        ),
        function_tool(
            "open_tree_node",
            "点击并打开某个文档或目录。适合“打开某篇文档”“进入某个目录”“点击某个节点”。可以通过 node_id、node_path 或 node_name 指定目标；如果同时传了项目信息，会先切到对应项目再打开节点。",
            json!({
                "type": "object",
                "properties": {
                    "project_id": { "type": "integer", "description": "目标项目 ID。优先使用。" },
                    "project_name": { "type": "string", "description": "目标项目名称。只有拿不到 project_id 时再使用。" },
                    "node_id": { "type": "integer", "description": "要打开的节点 ID。优先使用。" },
                    "node_path": { "type": "string", "description": "要打开的节点路径，例如 产品文档/接口/API说明 。只有拿不到 node_id 时再使用。" },
                    "node_name": { "type": "string", "description": "要打开的节点名称。只有拿不到 node_id 和 node_path 时再使用。" }
                },
                "additionalProperties": false,
            }),
        ),
        function_tool(
            "read_document",
            "读取指定 Markdown 文档的完整正文和元信息。适合在完善、修复、续写、重写文档前先读取原文。默认读取当前文档；也可以通过项目和文档定位参数读取其他文档。",
            json!({
                "type": "object",
                "properties": {
                    "project_id": { "type": "integer", "description": "目标项目 ID。优先使用。" },
                    "project_name": { "type": "string", "description": "目标项目名称。只有拿不到 project_id 时再使用。" },
                    "doc_id": { "type": "integer", "description": "目标文档 ID。优先使用。" },
                    "doc_path": { "type": "string", "description": "目标文档路径。只有拿不到 doc_id 时再使用。" },
                    "doc_name": { "type": "string", "description": "目标文档名称。只有拿不到 doc_id 和 doc_path 时再使用。" }
                },
                "additionalProperties": false,
            }),
        ),
        function_tool(
            "read_editor_snapshot",
            "读取当前编辑器中的实时内容快照（包含未保存修改）。可选传入项目和文档定位参数，工具会先打开目标文档，再返回编辑器当前内容；支持 max_chars 控制返回长度。",
            json!({
                "type": "object",
                "properties": {
                    "project_id": { "type": "integer", "description": "目标项目 ID。优先使用。" },
                    "project_name": { "type": "string", "description": "目标项目名称。只有拿不到 project_id 时再使用。" },
                    "doc_id": { "type": "integer", "description": "目标文档 ID。优先使用。" },
                    "doc_path": { "type": "string", "description": "目标文档路径。只有拿不到 doc_id 时再使用。" },
                    "doc_name": { "type": "string", "description": "目标文档名称。只有拿不到 doc_id 和 doc_path 时再使用。" },
                    "max_chars": { "type": "integer", "description": "可选，限制返回内容的最大字符数。默认不截断。" }
                },
                "additionalProperties": false,
            }),
        ),
        function_tool(
            "save_current_document",
            "保存当前正在编辑的 Markdown 文档。可选传入项目和文档定位参数，工具会先打开目标文档再执行保存。适合用户明确要求“保存”“提交修改”“应用更改”时调用。",
            json!({
                "type": "object",
                "properties": {
                    "project_id": { "type": "integer", "description": "目标项目 ID。优先使用。" },
                    "project_name": { "type": "string", "description": "目标项目名称。只有拿不到 project_id 时再使用。" },
                    "doc_id": { "type": "integer", "description": "目标文档 ID。优先使用。" },
                    "doc_path": { "type": "string", "description": "目标文档路径。只有拿不到 doc_id 时再使用。" },
                    "doc_name": { "type": "string", "description": "目标文档名称。只有拿不到 doc_id 和 doc_path 时再使用。" }
                },
                "additionalProperties": false,
            }),
        ),
        function_tool(
            "update_tree_node_meta",
            "修改文档树节点元信息（当前支持重命名文档或目录）。可选传入项目和节点定位参数，工具会先打开目标节点再执行修改。适合用户明确要求“重命名文档”“重命名目录”“修改节点名称”时调用。",
            json!({
                "type": "object",
                "properties": {
                    "project_id": { "type": "integer", "description": "目标项目 ID。优先使用。" },
                    "project_name": { "type": "string", "description": "目标项目名称。只有拿不到 project_id 时再使用。" },
                    "node_id": { "type": "integer", "description": "目标节点 ID（文档或目录）。优先使用。" },
                    "node_path": { "type": "string", "description": "目标节点路径。只有拿不到 node_id 时再使用。" },
                    "node_name": { "type": "string", "description": "目标节点当前名称。只有拿不到 node_id 和 node_path 时再使用。" },
                    "doc_id": { "type": "integer", "description": "兼容字段：目标文档 ID。" },
                    "doc_path": { "type": "string", "description": "兼容字段：目标文档路径。" },
                    "doc_name": { "type": "string", "description": "兼容字段：目标文档名称。" },
                    "new_name": { "type": "string", "description": "节点新名称（必填）。" }
                },
                "required": ["new_name"],
                "additionalProperties": false,
            }),
        ),
        function_tool(
            "delete_tree_nodes",
            "删除一个或多个文档或目录。支持 node_ids 或 node_paths 批量指定目标；删除前应先通过 get_project_tree 确认目标路径和节点，避免误删。",
            json!({
                "type": "object",
                "properties": {
                    "project_id": { "type": "integer", "description": "目标项目 ID。优先使用。" },
                    "project_name": { "type": "string", "description": "目标项目名称。只有拿不到 project_id 时再使用。" },
                    "node_ids": {
                        "type": "array",
                        "description": "要删除的节点 ID 列表。批量删除时优先使用。",
                        "items": { "type": "integer" }
                    },
                    "node_paths": {
                        "type": "array",
                        "description": "要删除的节点路径列表。只有拿不到 node_ids 时再使用。",
                        "items": { "type": "string" }
                    }
                },
                "additionalProperties": false,
            }),
        ),
        function_tool(
            "get_markdown_editor_runtime",
            "获取当前 Markdown 编辑器运行时说明。返回结果会包含编辑器是否可用、当前文档、可调用方法、每个方法的用途说明、可用全局对象和使用建议，便于后续通过 execute_browser_javascript 细粒度操作编辑器。",
            json!({
                "type": "object",
                "properties": {},
                "additionalProperties": false,
            }),
        ),
        function_tool(
            "get_browser_runtime",
            "获取浏览器运行时摘要和可用对象说明。返回结果会包含 location、document、history、navigator、viewport、storage 摘要，以及 window、document、editor、markflow 等可用于 execute_browser_javascript 的对象说明，用于判断后续是否需要执行 JavaScript。",
            json!({
                "type": "object",
                "properties": {
                    "include_storage": { "type": "boolean", "description": "是否把 localStorage 和 sessionStorage 的 key 摘要一并返回。默认 false。" }
                },
                "additionalProperties": false,
            }),
        ),
    ]
}

fn agent_chat_completion_tools() -> Vec<ChatCompletionTools> {
    agent_function_tools()
        .into_iter()
        .filter_map(|tool| match tool {
            Tool::Function(function) => Some(ChatCompletionTools::Function(ChatCompletionTool {
                function: FunctionObject {
                    name: function.name,
                    description: function.description,
                    parameters: function.parameters,
                    strict: function.strict,
                },
            })),
            _ => None,
        })
        .collect()
}

fn build_chat_completion_messages(
    payload: &AgentChatStreamRequest,
    system_prompt: &str,
) -> Vec<ChatCompletionRequestMessage> {
    let mut messages =
        vec![ChatCompletionRequestSystemMessage::from(system_prompt.to_string()).into()];

    for message in &payload.messages {
        let content = message.content.trim();
        if content.is_empty() {
            continue;
        }

        let next_message: ChatCompletionRequestMessage = match message.role.as_str() {
            "assistant" => ChatCompletionRequestAssistantMessage::from(content.to_string()).into(),
            "system" | "developer" => {
                ChatCompletionRequestSystemMessage::from(content.to_string()).into()
            }
            _ => ChatCompletionRequestUserMessage::from(content.to_string()).into(),
        };

        messages.push(next_message);
    }

    if let Some(outputs) = payload.tool_outputs.as_ref().filter(|items| !items.is_empty()) {
        let tool_calls: Vec<ChatCompletionMessageToolCalls> = outputs
            .iter()
            .filter_map(|output| {
                let name = output
                    .name
                    .as_deref()
                    .map(str::trim)
                    .filter(|value| !value.is_empty())?;
                let arguments = output
                    .arguments
                    .as_deref()
                    .map(str::trim)
                    .filter(|value| !value.is_empty())?;

                Some(ChatCompletionMessageToolCalls::Function(
                    ChatCompletionMessageToolCall {
                        id: output.call_id.clone(),
                        function: ChatFunctionCall {
                            name: name.to_string(),
                            arguments: arguments.to_string(),
                        },
                    },
                ))
            })
            .collect();

        if !tool_calls.is_empty() {
            messages.push(
                ChatCompletionRequestAssistantMessage {
                    content: None,
                    refusal: None,
                    name: None,
                    audio: None,
                    tool_calls: Some(tool_calls),
                    ..Default::default()
                }
                .into(),
            );
        }

        for output in outputs {
            messages.push(
                ChatCompletionRequestToolMessage {
                    content: ChatCompletionRequestToolMessageContent::Text(
                        output.output.to_string(),
                    ),
                    tool_call_id: output.call_id.clone(),
                }
                .into(),
            );
        }
    }

    messages
}

async fn send_json_event(
    tx: &mpsc::Sender<Result<Event, Infallible>>,
    name: &str,
    payload: serde_json::Value,
) -> bool {
    tx.send(Ok(Event::default().event(name).data(payload.to_string())))
        .await
        .is_ok()
}

async fn stream_via_responses(
    request_id: u64,
    client: &Client<OpenAIConfig>,
    payload: &AgentChatStreamRequest,
    system_prompt: &str,
    model: &str,
    tx: &mpsc::Sender<Result<Event, Infallible>>,
) -> Result<AgentStreamOutcome, AgentStreamError> {
    let reasoning = ReasoningArgs::default()
        .effort(ReasoningEffort::Medium)
        .summary(ReasoningSummary::Detailed)
        .build()
        .map_err(|err| AgentStreamError::Fatal(format!("Reasoning 参数构造失败: {}", err)))?;

    let mut request = CreateResponseArgs::default();
    request
        .model(model.to_string())
        .input(InputParam::Items(build_response_input(payload)))
        .instructions(system_prompt.to_string())
        .reasoning(reasoning)
        .tools(agent_function_tools())
        .parallel_tool_calls(false)
        .stream(true);
    let can_reuse_previous_response = payload
        .tool_outputs
        .as_ref()
        .map(|items| items.is_empty())
        .unwrap_or(true);

    if can_reuse_previous_response {
        if let Some(previous_response_id) = payload
            .previous_response_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
        request.previous_response_id(previous_response_id.to_string());
        }
    }
    let request = request
        .build()
        .map_err(|err| AgentStreamError::Fatal(format!("Responses 请求构造失败: {}", err)))?;

    let mut stream = client
        .responses()
        .create_stream(request)
        .await
        .map_err(|err| AgentStreamError::Retryable(format!("Responses 初始化失败: {}", err)))?;

    let mut final_text = String::new();
    let mut response_id = payload.previous_response_id.clone().unwrap_or_default();
    let mut tool_calls: Vec<AgentToolCallRequest> = Vec::new();

    while let Some(event) = stream.next().await {
        match event {
            Ok(ResponseStreamEvent::ResponseCreated(ev)) => {
                response_id = ev.response.id;
                tracing::info!(
                    "agent stream #{} responses.created response_id={}",
                    request_id,
                    response_id
                );
            }
            Ok(ResponseStreamEvent::ResponseOutputTextDelta(ev)) => {
                if !ev.delta.is_empty() {
                    tracing::info!(
                        "agent stream #{} responses.delta {}",
                        request_id,
                        preview_for_log(&ev.delta, 300)
                    );
                    final_text.push_str(&ev.delta);
                    if !send_json_event(tx, "message.delta", json!({ "content": ev.delta })).await {
                        return Err(AgentStreamError::Fatal("客户端连接已关闭".to_string()));
                    }
                }
            }
            Ok(ResponseStreamEvent::ResponseReasoningTextDelta(ev)) => {
                if !ev.delta.is_empty() {
                    tracing::info!(
                        "agent stream #{} responses.reasoning.delta {}",
                        request_id,
                        preview_for_log(&ev.delta, 300)
                    );
                    if !send_json_event(
                        tx,
                        "reasoning.delta",
                        json!({ "item_id": ev.item_id, "delta": ev.delta }),
                    )
                    .await
                    {
                        return Err(AgentStreamError::Fatal("客户端连接已关闭".to_string()));
                    }
                }
            }
            Ok(ResponseStreamEvent::ResponseReasoningSummaryTextDelta(ev)) => {
                if !ev.delta.is_empty() {
                    tracing::info!(
                        "agent stream #{} responses.reasoning.summary {}",
                        request_id,
                        preview_for_log(&ev.delta, 300)
                    );
                    if !send_json_event(
                        tx,
                        "reasoning.delta",
                        json!({ "item_id": ev.item_id, "delta": ev.delta }),
                    )
                    .await
                    {
                        return Err(AgentStreamError::Fatal("客户端连接已关闭".to_string()));
                    }
                }
            }
            Ok(ResponseStreamEvent::ResponseOutputItemDone(ev)) => {
                if let OutputItem::FunctionCall(call) = ev.item {
                    tracing::info!(
                        "agent stream #{} responses.tool_call name={} call_id={} arguments={}",
                        request_id,
                        call.name,
                        call.call_id,
                        preview_for_log(&call.arguments, 600)
                    );
                    tool_calls.push(AgentToolCallRequest {
                        call_id: call.call_id,
                        name: call.name,
                        arguments: call.arguments,
                    });
                }
            }
            Ok(ResponseStreamEvent::ResponseCompleted(ev)) => {
                if response_id.is_empty() {
                    response_id = ev.response.id;
                }
                tracing::info!(
                    "agent stream #{} responses.completed response_id={} text={}",
                    request_id,
                    response_id,
                    preview_for_log(&final_text, 1000)
                );
            }
            Ok(ResponseStreamEvent::ResponseFailed(ev)) => {
                let message = ev
                    .response
                    .error
                    .as_ref()
                    .map(|error| error.message.clone())
                    .unwrap_or_else(|| "Responses 调用失败".to_string());
                tracing::warn!(
                    "agent stream #{} responses.failed response_id={} message={}",
                    request_id,
                    response_id,
                    message
                );
                return if final_text.is_empty() {
                    Err(AgentStreamError::Retryable(message))
                } else {
                    Err(AgentStreamError::Fatal(message))
                };
            }
            Ok(ResponseStreamEvent::ResponseError(ev)) => {
                tracing::warn!(
                    "agent stream #{} responses.error response_id={} message={}",
                    request_id,
                    response_id,
                    ev.message
                );
                return if final_text.is_empty() {
                    Err(AgentStreamError::Retryable(ev.message))
                } else {
                    Err(AgentStreamError::Fatal(ev.message))
                };
            }
            Ok(_) => {}
            Err(err) => {
                let message = format!("Responses 流式调用失败: {}", err);
                tracing::warn!(
                    "agent stream #{} responses.stream_error response_id={} message={}",
                    request_id,
                    response_id,
                    message
                );
                return if final_text.is_empty() {
                    Err(AgentStreamError::Retryable(message))
                } else {
                    Err(AgentStreamError::Fatal(message))
                };
            }
        }
    }

    if !tool_calls.is_empty() {
        if response_id.is_empty() {
            return Err(AgentStreamError::Fatal(
                "模型请求了工具调用，但缺少 response_id".to_string(),
            ));
        }
        return Ok(AgentStreamOutcome::ToolCalls {
            response_id,
            calls: tool_calls,
        });
    }

    Ok(AgentStreamOutcome::Message {
        text: final_text,
        response_id: if response_id.trim().is_empty() {
            None
        } else {
            Some(response_id)
        },
    })
}

async fn stream_via_chat_completions(
    request_id: u64,
    client: &Client<OpenAIConfig>,
    payload: &AgentChatStreamRequest,
    system_prompt: &str,
    model: &str,
    tx: &mpsc::Sender<Result<Event, Infallible>>,
) -> Result<AgentStreamOutcome, AgentStreamError> {
    let request = CreateChatCompletionRequestArgs::default()
        .model(model.to_string())
        .reasoning_effort(ReasoningEffort::Medium)
        .messages(build_chat_completion_messages(payload, system_prompt))
        .tools(agent_chat_completion_tools())
        .parallel_tool_calls(false)
        .stream(true)
        .build()
        .map_err(|err| {
            AgentStreamError::Fatal(format!("Chat Completions 请求构造失败: {}", err))
        })?;
    let mut stream =
        client.chat().create_stream(request).await.map_err(|err| {
            AgentStreamError::Fatal(format!("Chat Completions 初始化失败: {}", err))
        })?;
    let mut final_text = String::new();
    let mut response_id = String::new();
    let mut tool_calls: std::collections::BTreeMap<u32, AgentToolCallRequest> =
        std::collections::BTreeMap::new();

    while let Some(event) = stream.next().await {
        match event {
            Ok(chunk) => {
                if response_id.is_empty() {
                    response_id = chunk.id.clone();
                    tracing::info!(
                        "agent stream #{} chat.created response_id={}",
                        request_id,
                        response_id
                    );
                }
                for choice in chunk.choices {
                    if let Some(content) = choice.delta.content {
                        if !content.is_empty() {
                            tracing::info!(
                                "agent stream #{} chat.delta {}",
                                request_id,
                                preview_for_log(&content, 300)
                            );
                            final_text.push_str(&content);
                            if !send_json_event(tx, "message.delta", json!({ "content": content }))
                                .await
                            {
                                return Err(AgentStreamError::Fatal("客户端连接已关闭".to_string()));
                            }
                        }
                    }

                    if let Some(refusal) = choice.delta.refusal {
                        if !refusal.is_empty() {
                            tracing::info!(
                                "agent stream #{} chat.refusal {}",
                                request_id,
                                preview_for_log(&refusal, 300)
                            );
                            final_text.push_str(&refusal);
                            if !send_json_event(tx, "message.delta", json!({ "content": refusal }))
                                .await
                            {
                                return Err(AgentStreamError::Fatal("客户端连接已关闭".to_string()));
                            }
                        }
                    }

                    if let Some(delta_tool_calls) = choice.delta.tool_calls {
                        for tool_call in delta_tool_calls {
                            let entry = tool_calls.entry(tool_call.index).or_insert_with(|| {
                                AgentToolCallRequest {
                                    call_id: String::new(),
                                    name: String::new(),
                                    arguments: String::new(),
                                }
                            });

                            if let Some(id) = tool_call.id {
                                entry.call_id = id;
                            }

                            if let Some(function) = tool_call.function {
                                if let Some(name) = function.name {
                                    entry.name.push_str(&name);
                                }
                                if let Some(arguments) = function.arguments {
                                    entry.arguments.push_str(&arguments);
                                }
                            }

                            tracing::info!(
                                "agent stream #{} chat.tool_call.partial index={} call_id={} name={} arguments={}",
                                request_id,
                                tool_call.index,
                                entry.call_id,
                                entry.name,
                                preview_for_log(&entry.arguments, 600)
                            );
                        }
                    }

                    if matches!(
                        choice.finish_reason,
                        Some(FinishReason::ToolCalls | FinishReason::FunctionCall)
                    ) {
                        let calls: Vec<AgentToolCallRequest> = tool_calls
                            .values()
                            .filter(|call| {
                                !call.call_id.trim().is_empty() && !call.name.trim().is_empty()
                            })
                            .cloned()
                            .collect();

                        if !calls.is_empty() {
                            tracing::info!(
                                "agent stream #{} chat.tool_calls.required response_id={} calls={}",
                                request_id,
                                if response_id.trim().is_empty() { model } else { &response_id },
                                json!(calls.iter().map(|call| {
                                    json!({
                                        "call_id": call.call_id,
                                        "name": call.name,
                                        "arguments": preview_for_log(&call.arguments, 600),
                                    })
                                }).collect::<Vec<_>>()).to_string()
                            );
                            return Ok(AgentStreamOutcome::ToolCalls {
                                response_id: if response_id.trim().is_empty() {
                                    model.to_string()
                                } else {
                                    response_id
                                },
                                calls,
                            });
                        }
                    }
                }
            }
            Err(err) => {
                tracing::warn!(
                    "agent stream #{} chat.stream_error response_id={} message={}",
                    request_id,
                    response_id,
                    err
                );
                return Err(AgentStreamError::Fatal(format!(
                    "Chat Completions 流式调用失败: {}",
                    err
                )));
            }
        }
    }

    Ok(AgentStreamOutcome::Message {
        text: {
            tracing::info!(
                "agent stream #{} chat.completed response_id={} text={}",
                request_id,
                response_id,
                preview_for_log(&final_text, 1000)
            );
            final_text
        },
        response_id: None,
    })
}

pub async fn list_providers(
    Extension(db): Extension<Arc<Database>>,
    headers: HeaderMap,
) -> Result<Json<AgentProvidersResponse>, Response> {
    let user = auth::require_user(&db, &headers).await?;
    let providers = list_user_providers(&db, user.id).await?;
    let active_provider_id = providers
        .iter()
        .find(|provider| provider.is_active == 1)
        .map(|provider| provider.id);

    Ok(Json(AgentProvidersResponse {
        providers: providers.into_iter().map(provider_to_summary).collect(),
        active_provider_id,
    }))
}

pub async fn save_provider(
    Extension(db): Extension<Arc<Database>>,
    headers: HeaderMap,
    Json(payload): Json<AgentProviderUpsertRequest>,
) -> Result<Json<AgentProvidersResponse>, Response> {
    let user = auth::require_user(&db, &headers).await?;
    let name = payload.name.trim();
    if name.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "请填写供应商名称"})),
        )
            .into_response());
    }

    let base_url = normalize_base_url(payload.base_url.as_deref());
    let remote_models = unique_strings(payload.remote_models.unwrap_or_default());
    let custom_models = unique_strings(payload.custom_models.unwrap_or_default());
    let enabled_models = unique_strings(
        payload
            .enabled_models
            .unwrap_or_default()
            .into_iter()
            .filter(|model| remote_models.contains(model) || custom_models.contains(model))
            .collect(),
    );

    let existing_active = list_user_providers(&db, user.id)
        .await?
        .into_iter()
        .find(|provider| provider.is_active == 1)
        .map(|provider| provider.id);

    let saved_id = if let Some(provider_id) = payload.id {
        let existing = find_user_provider(&db, user.id, provider_id).await?;
        let next_api_key = payload.api_key.unwrap_or_default().trim().to_string();
        let api_key_ciphertext = if next_api_key.is_empty() {
            existing.api_key_ciphertext
        } else {
            encrypt_provider_api_key(&next_api_key).map_err(|err| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(json!({"error": format!("供应商 API Key 加密失败: {}", err)})),
                )
                    .into_response()
            })?
        };

        sqlx::query(
            "UPDATE agent_providers
             SET name = ?, base_url = ?, api_key_ciphertext = ?, remote_models = ?, enabled_models = ?, custom_models = ?, updated_at = datetime('now')
             WHERE id = ? AND user_id = ?",
        )
        .bind(name)
        .bind(&base_url)
        .bind(api_key_ciphertext)
        .bind(serialize_json_string_array(&remote_models))
        .bind(serialize_json_string_array(&enabled_models))
        .bind(serialize_json_string_array(&custom_models))
        .bind(provider_id)
        .bind(user.id)
        .execute(&db.pool)
        .await
        .map_err(|err| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": format!("更新供应商失败: {}", err)})),
            )
                .into_response()
        })?;

        provider_id
    } else {
        let api_key = payload.api_key.unwrap_or_default();
        if api_key.trim().is_empty() {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(json!({"error": "请填写 API Key"})),
            )
                .into_response());
        }

        let api_key_ciphertext = encrypt_provider_api_key(api_key.trim()).map_err(|err| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": format!("供应商 API Key 加密失败: {}", err)})),
            )
                .into_response()
        })?;

        sqlx::query(
            "INSERT INTO agent_providers (user_id, name, base_url, api_key_ciphertext, remote_models, enabled_models, custom_models, is_active)
             VALUES (?, ?, ?, ?, ?, ?, ?, 0)",
        )
        .bind(user.id)
        .bind(name)
        .bind(&base_url)
        .bind(api_key_ciphertext)
        .bind(serialize_json_string_array(&remote_models))
        .bind(serialize_json_string_array(&enabled_models))
        .bind(serialize_json_string_array(&custom_models))
        .execute(&db.pool)
        .await
        .map_err(|err| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": format!("创建供应商失败: {}", err)})),
            )
                .into_response()
        })?
        .last_insert_rowid()
    };

    if existing_active.is_none() {
        set_active_provider(&db, user.id, saved_id).await?;
    }

    let providers = list_user_providers(&db, user.id).await?;
    let active_provider_id = providers
        .iter()
        .find(|provider| provider.is_active == 1)
        .map(|provider| provider.id);

    Ok(Json(AgentProvidersResponse {
        providers: providers.into_iter().map(provider_to_summary).collect(),
        active_provider_id,
    }))
}

pub async fn get_provider(
    Extension(db): Extension<Arc<Database>>,
    headers: HeaderMap,
    Path(provider_id): Path<i64>,
) -> Result<Json<AgentProviderDetailResponse>, Response> {
    let user = auth::require_user(&db, &headers).await?;
    let provider = find_user_provider(&db, user.id, provider_id).await?;
    let api_key = decrypt_provider_api_key(&provider.api_key_ciphertext).map_err(|err| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": format!("读取供应商密钥失败: {}", err)})),
        )
            .into_response()
    })?;

    Ok(Json(AgentProviderDetailResponse {
        id: provider.id,
        name: provider.name,
        base_url: provider.base_url,
        api_key,
        remote_models: parse_json_string_array(&provider.remote_models),
        enabled_models: parse_json_string_array(&provider.enabled_models),
        custom_models: parse_json_string_array(&provider.custom_models),
        is_active: provider.is_active == 1,
    }))
}

pub async fn activate_provider(
    Extension(db): Extension<Arc<Database>>,
    headers: HeaderMap,
    Path(provider_id): Path<i64>,
) -> Result<Json<AgentProvidersResponse>, Response> {
    let user = auth::require_user(&db, &headers).await?;
    let _provider = find_user_provider(&db, user.id, provider_id).await?;
    set_active_provider(&db, user.id, provider_id).await?;
    let providers = list_user_providers(&db, user.id).await?;

    Ok(Json(AgentProvidersResponse {
        active_provider_id: Some(provider_id),
        providers: providers.into_iter().map(provider_to_summary).collect(),
    }))
}

pub async fn delete_provider(
    Extension(db): Extension<Arc<Database>>,
    headers: HeaderMap,
    Path(provider_id): Path<i64>,
) -> Result<Json<AgentProvidersResponse>, Response> {
    let user = auth::require_user(&db, &headers).await?;
    let provider = find_user_provider(&db, user.id, provider_id).await?;

    sqlx::query("DELETE FROM agent_providers WHERE id = ? AND user_id = ?")
        .bind(provider.id)
        .bind(user.id)
        .execute(&db.pool)
        .await
        .map_err(|err| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": format!("删除供应商失败: {}", err)})),
            )
                .into_response()
        })?;

    let providers = list_user_providers(&db, user.id).await?;
    let next_active_provider_id = if provider.is_active == 1 {
        providers.first().map(|item| item.id)
    } else {
        providers
            .iter()
            .find(|item| item.is_active == 1)
            .map(|item| item.id)
    };

    if let Some(active_id) = next_active_provider_id {
        set_active_provider(&db, user.id, active_id).await?;
    }

    let refreshed = list_user_providers(&db, user.id).await?;
    Ok(Json(AgentProvidersResponse {
        active_provider_id: refreshed
            .iter()
            .find(|item| item.is_active == 1)
            .map(|item| item.id),
        providers: refreshed.into_iter().map(provider_to_summary).collect(),
    }))
}

pub async fn list_models(
    Extension(db): Extension<Arc<Database>>,
    headers: HeaderMap,
    Json(payload): Json<AgentModelsRequest>,
) -> Result<Json<serde_json::Value>, Response> {
    let user = auth::require_user(&db, &headers).await?;
    let provider = find_user_provider(&db, user.id, payload.provider_id).await?;
    let api_key = decrypt_provider_api_key(&provider.api_key_ciphertext).map_err(|err| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": format!("读取供应商密钥失败: {}", err)})),
        )
            .into_response()
    })?;

    let config = OpenAIConfig::new()
        .with_api_key(api_key)
        .with_api_base(normalize_base_url(Some(&provider.base_url)));
    let client = Client::with_config(config);

    let response = client.models().list().await.map_err(|err| {
        (
            StatusCode::BAD_GATEWAY,
            Json(json!({"error": format!("获取模型列表失败: {}", err)})),
        )
            .into_response()
    })?;

    let mut models: Vec<AgentModelSummary> = response
        .data
        .into_iter()
        .map(|model| AgentModelSummary {
            id: model.id,
            owned_by: model.owned_by,
            created: model.created,
        })
        .collect();
    models.sort_by(|a, b| a.id.cmp(&b.id));

    Ok(Json(json!({ "models": models })))
}

pub async fn chat_stream(
    Extension(db): Extension<Arc<Database>>,
    headers: HeaderMap,
    Json(payload): Json<AgentChatStreamRequest>,
) -> Result<Sse<ReceiverStream<Result<Event, Infallible>>>, Response> {
    let user = auth::require_user(&db, &headers).await?;
    if payload.provider.model.trim().is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "缺少模型名称"})),
        )
            .into_response());
    }
    let has_messages = payload.messages.iter().any(|message| !message.content.trim().is_empty());
    let has_tool_outputs = payload
        .tool_outputs
        .as_ref()
        .map(|items| !items.is_empty())
        .unwrap_or(false);
    if !has_messages && !has_tool_outputs {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "消息或工具输出不能为空"})),
        )
            .into_response());
    }

    let ctx = payload.context.clone().unwrap_or_default();
    let provider = find_user_provider(&db, user.id, payload.provider.provider_id).await?;
    let api_key = decrypt_provider_api_key(&provider.api_key_ciphertext).map_err(|err| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": format!("读取供应商密钥失败: {}", err)})),
        )
            .into_response()
    })?;
    let config = OpenAIConfig::new()
        .with_api_key(api_key)
        .with_api_base(normalize_base_url(Some(&provider.base_url)));
    let client = Client::with_config(config);

    let (tx, rx) = mpsc::channel::<Result<Event, Infallible>>(256);
    let model = payload.provider.model.clone();
    let mode = normalize_agent_mode(payload.mode.as_deref());
    let write_mode = normalize_write_mode(payload.write_mode.as_deref());
    let username = user.username.clone();
    let request_id = NEXT_AGENT_STREAM_LOG_ID.fetch_add(1, Ordering::Relaxed);
    let responses_prompt = build_system_prompt(&mode, write_mode.as_deref(), &username, &ctx, true);
    let fallback_prompt = build_system_prompt(&mode, write_mode.as_deref(), &username, &ctx, true);

    tracing::info!(
        "agent stream request #{}: {}",
        request_id,
        json!({
            "user": username,
            "provider_id": provider.id,
            "base_url": provider.base_url,
            "model": model,
            "mode": mode,
            "write_mode": write_mode,
            "previous_response_id": payload.previous_response_id,
            "messages": summarize_messages_for_log(&payload.messages),
            "tool_outputs": summarize_tool_outputs_for_log(payload.tool_outputs.as_deref()),
            "context": summarize_context_for_log(payload.context.as_ref()),
        })
        .to_string()
    );

    tokio::spawn(async move {
        let _ = send_json_event(
            &tx,
            "message.started",
            json!({
                "model": model,
                "mode": mode,
                "write_mode": write_mode,
                "user": username,
            }),
        )
        .await;
        let outcome = match stream_via_responses(request_id, &client, &payload, &responses_prompt, &model, &tx).await {
            Ok(outcome) => {
                let _ = send_json_event(
                    &tx,
                    "agent.transport",
                    json!({
                        "mode": "responses",
                        "tools_available": true,
                    }),
                )
                .await;
                Ok(outcome)
            }
            Err(AgentStreamError::Retryable(responses_error)) => {
                tracing::warn!(
                    "agent stream #{} responses transport unavailable for model {} via {}: {}",
                    request_id,
                    model,
                    provider.base_url,
                    responses_error
                );
                let _ = send_json_event(
                    &tx,
                    "agent.transport",
                    json!({
                        "mode": "chat_fallback",
                        "tools_available": true,
                        "reason": responses_error,
                    }),
                )
                .await;
                stream_via_chat_completions(
                    request_id,
                    &client,
                    &payload,
                    &fallback_prompt,
                    &model,
                    &tx,
                )
                    .await
                    .map_err(|chat_error| match chat_error {
                        AgentStreamError::Retryable(chat_message) | AgentStreamError::Fatal(chat_message) => {
                            format!(
                                "Responses 接口不可用，且 Chat Completions 回退失败。responses: {}; chat: {}",
                                responses_error, chat_message
                            )
                        }
                    })
            }
            Err(AgentStreamError::Fatal(message)) => Err(message),
        };

        match outcome {
            Ok(AgentStreamOutcome::Message { text, response_id }) => {
                tracing::info!(
                    "agent stream #{} message.completed response_id={} text={}",
                    request_id,
                    response_id.as_deref().unwrap_or(""),
                    preview_for_log(&text, 1000)
                );
                let _ = send_json_event(
                    &tx,
                    "message.completed",
                    json!({
                        "content": text,
                        "response_id": response_id,
                    }),
                )
                .await;
                let _ = send_json_event(&tx, "done", json!({})).await;
            }
            Ok(AgentStreamOutcome::ToolCalls { response_id, calls }) => {
                tracing::info!(
                    "agent stream #{} tool.calls.required response_id={} calls={}",
                    request_id,
                    response_id,
                    json!(calls.iter().map(|call| {
                        json!({
                            "call_id": call.call_id,
                            "name": call.name,
                            "arguments": preview_for_log(&call.arguments, 600),
                        })
                    }).collect::<Vec<_>>()).to_string()
                );
                let _ = send_json_event(
                    &tx,
                    "tool.calls.required",
                    json!({
                        "response_id": response_id,
                        "calls": calls,
                    }),
                )
                .await;
                let _ = send_json_event(&tx, "done", json!({})).await;
            }
            Err(message) => {
                tracing::warn!(
                    "agent stream #{} failed message={}",
                    request_id,
                    message
                );
                let _ = send_json_event(&tx, "error", json!({ "error": message })).await;
            }
        }
    });

    Ok(Sse::new(ReceiverStream::new(rx)).keep_alive(
        KeepAlive::new()
            .interval(Duration::from_secs(15))
            .text("keep-alive"),
    ))
}
