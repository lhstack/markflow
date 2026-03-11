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
            r#"文档正文规则：
1. 生成或修改正文时，工具只用于定位/打开/创建空文档/读取正文或快照/保存/改元信息；正文必须由 assistant 文本流输出，禁止在 create_tree_node 中携带完整正文。
2. 所有必要工具调用完成后，才能输出正文协议；正文最开头必须是 [[ACTION:append]]、[[ACTION:replace]]、[[ACTION:rewrite_section]]、[[ACTION:replace_block]] 之一，并以 [[/ACTION]] 结束；标记内写入编辑器，标记外显示在聊天面板。
3. rewrite_section 块必须含 [[TARGET]]标题[[/TARGET]] 和 [[CONTENT]]完整小节[[/CONTENT]]；replace_block 块必须含 [[FIND]]原片段[[/FIND]] 和 [[REPLACE]]新片段[[/REPLACE]]。
4. 局部改写后必须清理旧内容，禁止重复；若是移动内容，必须满足“源位置删除 + 目标位置插入”，最终只保留一份。
5. 协议标记必须完整、精确、无半截；[[/ACTION]] 后继续聊天，不得立即结束。
6. [[/ACTION]] 后按固定结构输出：`操作类型：...`、`修改位置：...`、`保存状态：已保存/未保存`；然后输出 `优化建议：`（3-5 条）和 `本次修改总结：\r\n` (总结内容)。
工具与读取规则：
1. 优先用工具完成页面导航、项目管理、文档树、正文读取、保存、重命名、浏览器自动化；不要输出旧版 [[ROUTE:...]]。
2. 工具调用要“按需、一次到位”：同一轮内若已拿到足够信息，不要重复调用同一工具（尤其是 get_current_page_state、read_document、read_editor_snapshot）。
3. get_current_page_state 只在无法确认当前项目/文档/未保存状态时调用；同一轮最多调用一次。若上下文已明确目标，不要先调状态工具再重复确认。
4. 读取当前文档时：若已知存在未保存修改或用户刚改过内容，优先 read_editor_snapshot；否则 read_document。当前文档未保存时，禁止只依赖 read_document 做续写/替换判断。
5. 仅保存当前文档时必须调用 save_current_document；仅修改项目元信息时调用 update_project；仅重命名文档/目录时调用 update_tree_node_meta。
6. 除非用户明确授权保存，否则默认生成未保存草稿，并在保存状态中写“未保存”；不要擅自声称已保存。
7. 填表、点按钮、弹窗、细粒度编辑器操作优先用专用工具；专用工具不足时才用 execute_browser_javascript。工具完成后用简短中文总结，不要复述整段 JSON。
8. 防循环约束：连续两次工具调用若没有新增关键信息，立即停止继续探测，直接给出当前最佳结果；如确实缺信息，只提出一个最小澄清问题。
动作选择规则：
1. 先判断编辑意图（追加/插入/替换/润色/重写），再判断影响范围（片段/小节/多节/大部分/整篇），最后决定协议动作。
2. append / replace / rewrite_section / replace_block 是协议动作，不等同于编辑意图；能局部解决时不要整体重写。
3. 一般规则：局部新增优先 append；整篇改写或大范围重排用 replace；按标题整节改写用 rewrite_section；替换指定片段用 replace_block。"#
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
                    r#"正文必须包在 [[ACTION:...]]...[[/ACTION]] 内；标记内才是写入文档的 Markdown。[[/ACTION]] 后继续输出聊天文本，并按以下结构给出：`操作类型：...`、`修改位置：...`、`保存状态：已保存/未保存`；若未保存，再输出 `原因：...`；然后输出 `优化建议：`（3-5 条）和 `本次修改总结：\r\n`（总结内容）。"#
                        .to_string(),
                );
            }
            _ => {
                parts.push(
                    r#"最终开头只能选择一个动作：[[ACTION:chat]]、[[ACTION:append]]、[[ACTION:replace]]、[[ACTION:rewrite_section]]、[[ACTION:replace_block]]。
聊天答复用 [[ACTION:chat]]；继续补写通常用 [[ACTION:append]]；整篇改写用 [[ACTION:replace]]；按标题整节重写用 [[ACTION:rewrite_section]]；替换指定片段用 [[ACTION:replace_block]]。
若是文档动作，必须输出 [[/ACTION]]，且只有标记内内容属于文档正文。rewrite_section 必须带 TARGET/CONTENT；replace_block 必须带 FIND/REPLACE；局部改写后要清理旧内容，移动内容时最终只保留一份。
[[/ACTION]] 后继续输出聊天文本，并按以下结构给出：`操作类型：...`、`修改位置：...`、`保存状态：已保存/未保存`；若未保存，再输出 `原因：...`；然后输出 `优化建议：`（3-5 条）和 `本次修改总结：\r\n`（总结内容）。
若用户明确要求切换页面，可在最前面额外输出一个路由标记：[[ROUTE:overview]]、[[ROUTE:project:项目名]]、[[ROUTE:doc:文档名]]；只有目标名称明确时才可输出，禁止编造名称。"#
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
            "读取当前前端页面的实时状态与可操作上下文。适用于回答“我现在在哪个页面”“当前打开了哪个项目/文档”“当前文档是否存在未保存修改”“当前页面还能执行哪些操作”这类问题，也适用于在调用导航、项目管理、文档树、文档读取、保存、重命名等工具前先确认 UI 所处位置。返回结果会包含当前路由、页面作用域、页面状态、当前项目、当前节点、编辑器快照来源、未保存状态、当前可见项目/节点摘要以及能力开关。",
            json!({
                "type": "object",
                "properties": {},
                "additionalProperties": false,
            }),
        ),
        function_tool(
            "list_page_routes",
            "列出当前前端应用支持的所有页面路由、每个路由的用途说明、典型使用场景和参数要求。适用于模型需要回答“你知道哪些页面”“如何跳转到某个页面”时使用。返回结果会包含 login、register、login.2fa、share 这些真实路由，以及 home.overview、home.project、home.doc、home.dir 这些 Home 页内部状态路由。",
            json!({
                "type": "object",
                "properties": {},
                "additionalProperties": false,
            }),
        ),
        function_tool(
            "navigate_to_page",
            "按“route + 定位参数”的方式跳转页面。适用于明确切换到项目概览、登录、注册、2FA、分享页，或按照页面语义进入某个项目、某个文档、某个目录。它是通用跳转工具，适合已经明确 route 的场景；如果目标只是打开项目，优先用 open_project；如果目标只是打开文档或目录，优先用 open_tree_node。参数定位优先级为 ID > 路径 > 名称。",
            json!({
                "type": "object",
                "properties": {
                    "route": {
                        "type": "string",
                        "description": "目标页面路由。home.overview=项目概览；home.project=项目工作区；home.doc=文档编辑页；home.dir=目录详情页；login/register/login.2fa/share 为真实页面。只有 route 明确时才使用此工具。",
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
                    "share_token": { "type": "string", "description": "当 route=share 时必填，表示分享链接 token；可从分享 URL 中提取。" },
                    "project_id": { "type": "integer", "description": "目标项目 ID。route=home.project/home.doc/home.dir 时优先使用，用于稳定定位项目。" },
                    "project_name": { "type": "string", "description": "目标项目名称。只有拿不到 project_id 时再使用，适合用户只提供项目名时兜底定位。" },
                    "node_id": { "type": "integer", "description": "目标文档或目录节点 ID。route=home.doc/home.dir 时优先使用，用于稳定定位节点。" },
                    "node_path": { "type": "string", "description": "目标节点路径，例如 产品文档/接口/API说明。只有拿不到 node_id 时再使用，适合已知层级路径时定位。" },
                    "node_name": { "type": "string", "description": "目标节点名称。只有拿不到 node_id 和 node_path 时再使用，可能存在重名风险。" }
                },
                "required": ["route"],
                "additionalProperties": false,
            }),
        ),
        function_tool(
            "list_projects",
            "获取当前用户当前可访问的项目列表。适用于回答“我有哪些项目”、在打开项目、创建项目、更新项目、删除项目之前先确认项目全集，或在项目列表可能已变化时刷新数据。返回结果包含项目列表、总数和当前激活项目 ID。",
            json!({
                "type": "object",
                "properties": {
                    "refresh": { "type": "boolean", "description": "是否强制从后端刷新项目列表。true=忽略当前页面缓存，适合刚创建/删除项目后重新确认结果。" }
                },
                "additionalProperties": false,
            }),
        ),
        function_tool(
            "open_project",
            "打开指定项目，并进入该项目的工作区和文档树视图。适用于“打开某个项目”“进入某个项目的文档编辑列表”“切换当前工作项目”等场景。可以按 project_id 或 project_name 指定目标，优先使用 project_id；打开后通常应继续通过 get_project_tree 或 open_tree_node 处理具体文档。",
            json!({
                "type": "object",
                "properties": {
                    "project_id": { "type": "integer", "description": "要打开的项目 ID。优先使用这个字段，可避免项目重名带来的歧义。" },
                    "project_name": { "type": "string", "description": "要打开的项目名称。只有拿不到 project_id 时再使用。" },
                    "fetch_tree": { "type": "boolean", "description": "打开后是否确保加载该项目的文档树。默认 true；若后续马上要定位文档，建议保持 true。" }
                },
                "additionalProperties": false,
            }),
        ),
        function_tool(
            "create_project",
            "创建新项目。适用于帮用户新建项目、填写项目名称、项目描述、背景图等场景。默认创建成功后立即进入该项目；如果用户只想创建而不切换页面，可传 open_after_create=false。",
            json!({
                "type": "object",
                "properties": {
                    "name": { "type": "string", "description": "项目名称。必填，应直接使用用户确认后的最终名称。" },
                    "description": { "type": "string", "description": "项目描述，可为空；适合补充项目用途、背景、说明。" },
                    "background_image": { "type": "string", "description": "项目背景图 URL，可为空；用于卡片展示。" },
                    "open_after_create": { "type": "boolean", "description": "创建完成后是否立即打开该项目。默认 true；若只需创建不打断当前页面，可传 false。" }
                },
                "required": ["name"],
                "additionalProperties": false,
            }),
        ),
        function_tool(
            "update_project",
            "更新项目信息。适用于重命名项目、修改项目描述、更新项目背景图等非正文类操作。可按 project_id 或 project_name 定位目标项目，支持单字段修改，也支持一次同时修改多个字段。",
            json!({
                "type": "object",
                "properties": {
                    "project_id": { "type": "integer", "description": "目标项目 ID。优先使用，用于稳定定位项目。" },
                    "project_name": { "type": "string", "description": "目标项目名称。只有拿不到 project_id 时再使用。" },
                    "name": { "type": "string", "description": "项目新名称。传入时表示重命名项目。" },
                    "description": { "type": "string", "description": "项目新描述；可传空字符串以清空原描述。" },
                    "background_image": { "type": "string", "description": "项目新背景图 URL；可传空字符串以清空背景图。" }
                },
                "additionalProperties": false,
            }),
        ),
        function_tool(
            "delete_projects",
            "删除一个或多个项目。适用于明确删除项目的场景，删除前应先通过 list_projects 确认目标，避免误删。支持按 project_ids 或 project_names 批量删除；优先使用 ID。",
            json!({
                "type": "object",
                "properties": {
                    "project_ids": {
                        "type": "array",
                        "description": "要删除的项目 ID 列表。批量删除时优先使用，最稳定。",
                        "items": { "type": "integer" }
                    },
                    "project_names": {
                        "type": "array",
                        "description": "要删除的项目名称列表。只有拿不到 project_ids 时再使用，存在项目重名风险。",
                        "items": { "type": "string" }
                    }
                },
                "additionalProperties": false,
            }),
        ),
        function_tool(
            "execute_browser_javascript",
            "在浏览器端执行一段 JavaScript。适用于点击按钮、填写表单、操作弹窗、读取 DOM、触发事件、直接调用编辑器桥接对象或 markflow 页面助手等细粒度动作。代码运行在 async 环境，可直接使用 window、document、location、history、navigator、localStorage、sessionStorage、console、editor、markflow。只有在现有专用工具不足以表达需求时才使用它；代码最后必须 return 一个可序列化结果，避免返回 DOM 循环引用。",
            json!({
                "type": "object",
                "properties": {
                    "code": { "type": "string", "description": "要执行的 JavaScript 代码，运行在 async 函数体内。建议显式 return 结果，便于模型读取执行反馈。" }
                },
                "required": ["code"],
                "additionalProperties": false,
            }),
        ),
        function_tool(
            "get_project_tree",
            "读取指定项目下的完整文档树。适用于在创建文档、创建目录、移动节点、删除节点、打开节点之前先确认层级结构，也适用于回答“某个项目下面有哪些目录/文档”。可以通过 project_id 或 project_name 指定目标；返回结果会包含树结构、扁平节点列表和统计信息。",
            json!({
                "type": "object",
                "properties": {
                    "project_id": { "type": "integer", "description": "目标项目 ID。优先使用，用于稳定定位项目。" },
                    "project_name": { "type": "string", "description": "目标项目名称。只有拿不到 project_id 时再使用。" },
                    "refresh": { "type": "boolean", "description": "是否强制从后端刷新该项目文档树。默认 false；在节点刚发生创建/删除/移动后建议传 true。" }
                },
                "additionalProperties": false,
            }),
        ),
        function_tool(
            "create_tree_node",
            "在指定项目下创建文档或目录。适用于“在根目录创建文档”“在某个路径下创建目录”“先建空文档再写正文”等场景。支持通过 parent_id、parent_path 或 parent_name 指定父目录；node_type 只能是 doc 或 dir。创建文档时只创建空文档壳，不要在这个工具里携带完整正文；正文必须在文档打开后通过 assistant 文本流协议输出。",
            json!({
                "type": "object",
                "properties": {
                    "project_id": { "type": "integer", "description": "目标项目 ID。优先使用。" },
                    "project_name": { "type": "string", "description": "目标项目名称。只有拿不到 project_id 时再使用。" },
                    "parent_id": { "type": "integer", "description": "父目录节点 ID。优先使用；不传表示在项目根目录创建。" },
                    "parent_path": { "type": "string", "description": "父目录路径，例如 产品文档/接口。只有拿不到 parent_id 时再使用，适合已知层级路径时定位。" },
                    "parent_name": { "type": "string", "description": "父目录名称。只有拿不到 parent_id 和 parent_path 时再使用，可能存在重名风险。" },
                    "name": { "type": "string", "description": "新建节点名称。必填。" },
                    "node_type": { "type": "string", "description": "新建节点类型。doc=文档，dir=目录。" , "enum": ["doc", "dir"] },
                    "open_after_create": { "type": "boolean", "description": "创建后是否立即打开该节点。默认 true；若只需创建结构不切换视图，可传 false。" }
                },
                "required": ["name", "node_type"],
                "additionalProperties": false,
            }),
        ),
        function_tool(
            "move_tree_node",
            "移动一个文档或目录到同一项目内的另一个目录下，或移动到项目根目录。适用于“把某篇文档移到某个目录”“把某个目录提升到根目录”“调整父目录归属”这类结构性操作。优先传源节点 ID 和目标父目录 ID；路径和名称只作为兜底。当前不支持跨项目移动。",
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
            "点击并打开某个文档或目录。适用于“打开某篇文档”“进入某个目录”“点击文档树中的某个节点”。可以通过 node_id、node_path 或 node_name 指定目标；如果同时传了项目信息，会先切到对应项目再打开节点。若目标是文档，工具会等待编辑器初始化完成后再返回。",
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
            "读取指定 Markdown 文档的已保存正文和元信息。适用于在完善、修复、续写、重写文档前读取后端已保存版本，也适用于比较“已保存内容”与“编辑器实时快照”的差异。默认读取当前文档；也可以通过项目和文档定位参数读取其他文档。注意：当当前文档存在未保存修改时，应优先结合 read_editor_snapshot 一起使用。",
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
            "读取当前编辑器中的实时内容快照，优先返回未保存的实时草稿。适用于用户刚改过文档、模型需要基于最新未保存内容继续写作、润色或判断是否应追加/替换时使用。可选传入项目和文档定位参数，工具会先打开目标文档，再返回编辑器当前内容；若当前没有激活编辑器但存在本地草稿缓存，也会返回草稿缓存；只有两者都不存在时才回退到已保存正文。支持 max_chars 控制返回长度。",
            json!({
                "type": "object",
                "properties": {
                    "project_id": { "type": "integer", "description": "目标项目 ID。优先使用。" },
                    "project_name": { "type": "string", "description": "目标项目名称。只有拿不到 project_id 时再使用。" },
                    "doc_id": { "type": "integer", "description": "目标文档 ID。优先使用，用于稳定定位文档。" },
                    "doc_path": { "type": "string", "description": "目标文档路径。只有拿不到 doc_id 时再使用，适合已知目录层级时定位。" },
                    "doc_name": { "type": "string", "description": "目标文档名称。只有拿不到 doc_id 和 doc_path 时再使用，可能存在重名风险。" },
                    "max_chars": { "type": "integer", "description": "可选，限制返回内容的最大字符数。适合只读取开头摘要、避免超长正文。默认不截断。" }
                },
                "additionalProperties": false,
            }),
        ),
        function_tool(
            "save_current_document",
            "保存当前正在编辑的 Markdown 文档。适用于用户明确要求“保存”“提交修改”“应用更改”“确认保存”时调用。可选传入项目和文档定位参数，工具会先打开目标文档再执行保存；如果编辑器尚未初始化完成会报错，不应在仅需生成草稿时调用。",
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
            "修改文档树节点元信息（当前主要支持重命名文档或目录）。适用于“重命名文档”“重命名目录”“修改节点名称”这类非正文操作。可选传入项目和节点定位参数，工具会先打开目标节点再执行修改；文档和目录都支持。",
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
            "删除一个或多个文档或目录。适用于明确删除文档/目录的场景，支持批量删除。删除前应先通过 get_project_tree 确认目标路径和节点，避免误删；优先使用 node_ids，路径仅作兜底。",
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
            "获取当前 Markdown 编辑器运行时说明。适用于模型需要了解当前编辑器是否可用、当前文档是谁、有哪些桥接方法可调用、每个方法适合做什么，以及执行 JavaScript 时可直接访问哪些对象。常用于在调用 execute_browser_javascript 前先做能力探测。",
            json!({
                "type": "object",
                "properties": {},
                "additionalProperties": false,
            }),
        ),
        function_tool(
            "get_browser_runtime",
            "获取浏览器运行时摘要和可用对象说明。适用于模型需要判断当前 URL、页面标题、视口大小、浏览器环境、本地存储 key 摘要，以及 execute_browser_javascript 中可直接使用哪些对象。通常用于执行 JavaScript 之前先做环境确认。",
            json!({
                "type": "object",
                "properties": {
                    "include_storage": { "type": "boolean", "description": "是否把 localStorage 和 sessionStorage 的 key 摘要一并返回。默认 false；只有在需要排查本地状态或读取存储线索时才建议开启。" }
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
