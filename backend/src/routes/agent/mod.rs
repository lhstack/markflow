//! Agent 路由模块（完全重构）。
//!
//! 子模块划分对齐 awake：
//! - `crypto`      供应商 API Key 加解密
//! - `provider`    供应商 + 模型两表数据层与 CRUD
//! - `model_http`  远程模型拉取（openai / anthropic）
//! - `mcp`         MCP 管理（设置 / 服务 CRUD / 测试 / 刷新）
//! - `broker`      前端工具往返 broker
//! - `tools`       前端工具 + MCP 工具的 rig `ToolDyn` 适配层
//! - `params`      模型运行参数 -> provider additional_params 组装
//! - `runtime`     基于 rig 高层 Agent 的多轮流式执行
//!
//! 本文件保留：请求/响应 payload、附件转换、会话历史组装、system prompt、
//! `chat_stream` 入口编排与 `submit_tool_callback` 回调。

use std::{convert::Infallible, path::PathBuf, sync::Arc, time::Duration};

use axum::{
    extract::Extension,
    http::{HeaderMap, StatusCode},
    response::{
        sse::{Event, KeepAlive, Sse},
        IntoResponse, Response,
    },
    Json,
};
use base64::{engine::general_purpose, Engine as _};
use rig::{
    client::CompletionClient as RigCompletionClient,
    completion::Message as RigMessage,
    message::{
        AssistantContent as RigAssistantContent, Document as RigDocument,
        DocumentMediaType as RigDocumentMediaType, DocumentSourceKind as RigDocumentSourceKind,
        ImageDetail as RigImageDetail, ImageMediaType as RigImageMediaType, MimeType as _,
        ToolResultContent as RigToolResultContent,
        UserContent as RigUserContent,
    },
    providers::{anthropic, openai},
    OneOrMany as RigOneOrMany,
};
use serde::Deserialize;
use serde_json::{json, Value};
use tokio::fs;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;

use crate::{
    auth,
    db::Database,
    models::UploadAsset,
};
use crate::mcp_chat;
use crate::models::AgentProvider;

mod broker;
mod crypto;
mod mcp;
mod model_http;
mod params;
mod provider;
mod runtime;
mod skills;
mod tools;
mod web_fetch;

// 供 main.rs 路由绑定使用的对外 handler。
pub use mcp::{
    delete_mcp_server, get_mcp_server, get_mcp_settings, get_runtime_capabilities,
    list_mcp_servers_handler, refresh_mcp_server, refresh_mcp_server_draft, save_mcp_server,
    test_mcp_server, test_mcp_server_draft, update_mcp_settings,
};
// 供 src/mcp.rs 复用的 MCP DTO 类型。
pub use mcp::{
    McpHttpAuthDetailResponse, McpHttpAuthEditRequest, McpHttpAuthPayload, McpSecretEntryResponse,
    McpSecretState, McpSecretValueResponse, McpServerUpsertRequest,
};
pub use model_http::list_models;
pub use skills::{delete_skill_handler, list_skill_files_handler, list_skills_handler, read_skill_file_handler, save_skill_file_handler};
pub use provider::{
    activate_provider, delete_model, delete_provider, get_provider, list_providers, save_model,
    save_provider,
};

use broker::frontend_tool_broker;


#[derive(Debug, Deserialize, Clone)]
pub struct AgentProviderPayload {
    pub provider_id: i64,
    pub model: String,
}

#[derive(Debug, Deserialize, Clone)]
pub struct AgentMessagePayload {
    pub role: String,
    pub content: String,
    #[serde(default)]
    pub attachments: Vec<AgentMessageAttachmentPayload>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct AgentMessageAttachmentPayload {
    pub upload_id: i64,
    pub kind: Option<String>,
    pub content_type: Option<String>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct AgentChatStreamRequest {
    pub provider: AgentProviderPayload,
    pub messages: Vec<AgentMessagePayload>,
    pub previous_response_id: Option<String>,
    #[serde(default, deserialize_with = "deserialize_null_default")]
    pub tool_outputs: Vec<AgentToolOutputPayload>,
}

fn deserialize_null_default<'de, D, T>(deserializer: D) -> Result<T, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de> + Default,
{
    Option::<T>::deserialize(deserializer).map(|value| value.unwrap_or_default())
}


#[derive(Debug, Deserialize, Clone)]
pub struct AgentToolOutputPayload {
    pub call_id: String,
    pub name: Option<String>,
    pub arguments: Option<String>,
    pub output: Value,
}

#[derive(Debug, Deserialize)]
pub struct AgentToolCallbackRequest {
    pub run_id: String,
    pub call_id: String,
    pub output: Value,
}

fn upload_root() -> PathBuf {
    std::env::var("UPLOAD_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("uploads"))
}

fn image_media_type_from_name_or_mime(
    content_type: Option<&str>,
    original_name: &str,
) -> Option<RigImageMediaType> {
    content_type
        .and_then(RigImageMediaType::from_mime_type)
        .or_else(|| match original_name.rsplit('.').next().unwrap_or_default().to_ascii_lowercase().as_str() {
            "jpg" | "jpeg" => Some(RigImageMediaType::JPEG),
            "png" => Some(RigImageMediaType::PNG),
            "gif" => Some(RigImageMediaType::GIF),
            "webp" => Some(RigImageMediaType::WEBP),
            "heic" => Some(RigImageMediaType::HEIC),
            "heif" => Some(RigImageMediaType::HEIF),
            "svg" => Some(RigImageMediaType::SVG),
            _ => None,
        })
}

fn document_media_type_from_name_or_mime(
    content_type: Option<&str>,
    original_name: &str,
) -> Option<RigDocumentMediaType> {
    content_type
        .and_then(RigDocumentMediaType::from_mime_type)
        .or_else(|| match original_name.rsplit('.').next().unwrap_or_default().to_ascii_lowercase().as_str() {
            "pdf" => Some(RigDocumentMediaType::PDF),
            "txt" => Some(RigDocumentMediaType::TXT),
            "rtf" => Some(RigDocumentMediaType::RTF),
            "html" | "htm" => Some(RigDocumentMediaType::HTML),
            "css" => Some(RigDocumentMediaType::CSS),
            "md" | "markdown" => Some(RigDocumentMediaType::MARKDOWN),
            "csv" => Some(RigDocumentMediaType::CSV),
            "xml" => Some(RigDocumentMediaType::XML),
            "js" | "mjs" | "cjs" => Some(RigDocumentMediaType::Javascript),
            "py" => Some(RigDocumentMediaType::Python),
            _ => None,
        })
}

async fn attachment_to_user_content(
    db: &Database,
    user_id: i64,
    attachment: &AgentMessageAttachmentPayload,
) -> Result<RigUserContent, String> {
    let asset = sqlx::query_as::<_, UploadAsset>(
        "SELECT * FROM uploads WHERE id = ? AND user_id = ?",
    )
    .bind(attachment.upload_id)
    .bind(user_id)
    .fetch_optional(&db.pool)
    .await
    .map_err(|err| format!("读取附件 {} 失败: {}", attachment.upload_id, err))?
    .ok_or_else(|| format!("未找到附件 {}", attachment.upload_id))?;

    let file_path = upload_root().join(asset.stored_path.replace('\\', "/"));
    let bytes = fs::read(&file_path)
        .await
        .map_err(|err| format!("读取附件文件 {} 失败: {}", asset.original_name, err))?;
    let content_type = asset.content_type.as_deref().or(attachment.content_type.as_deref());
    let declared_kind = attachment
        .kind
        .as_deref()
        .map(str::trim)
        .unwrap_or_default()
        .to_ascii_lowercase();

    if declared_kind == "image" || content_type.unwrap_or_default().starts_with("image/") {
        let media_type = image_media_type_from_name_or_mime(content_type, &asset.original_name)
            .ok_or_else(|| format!("附件 {} 不是受支持的图片类型", asset.original_name))?;
        return Ok(RigUserContent::image_base64(
            general_purpose::STANDARD.encode(bytes),
            Some(media_type),
            Some(RigImageDetail::default()),
        ));
    }

    let media_type = match document_media_type_from_name_or_mime(content_type, &asset.original_name) {
        Some(media_type) => media_type,
        None => {
            String::from_utf8(bytes.clone())
                .map(|_| RigDocumentMediaType::TXT)
                .map_err(|_| format!("附件 {} 不是受支持的文件类型", asset.original_name))?
        }
    };

    if matches!(media_type, RigDocumentMediaType::PDF) {
        return Ok(RigUserContent::Document(RigDocument {
            data: RigDocumentSourceKind::Base64(general_purpose::STANDARD.encode(bytes)),
            media_type: Some(media_type),
            additional_params: None,
        }));
    }

    let text = String::from_utf8(bytes)
        .map_err(|_| format!("附件 {} 需要为 UTF-8 文本编码", asset.original_name))?;
    Ok(RigUserContent::document(text, Some(media_type)))
}

async fn build_rig_conversation(
    db: &Database,
    user_id: i64,
    payload: &AgentChatStreamRequest,
) -> Result<Vec<RigMessage>, String> {
    let mut messages = Vec::new();

    for message in &payload.messages {
        let content = message.content.trim();

        match message.role.as_str() {
            "assistant" => {
                if !content.is_empty() {
                    messages.push(RigMessage::assistant(content));
                }
            }
            "system" | "developer" => {}
            _ => {
                let mut parts = Vec::new();
                if !content.is_empty() {
                    parts.push(RigUserContent::text(content));
                }
                for attachment in &message.attachments {
                    parts.push(attachment_to_user_content(db, user_id, attachment).await?);
                }
                if parts.is_empty() {
                    continue;
                }
                messages.push(RigMessage::User {
                    content: RigOneOrMany::many(parts).map_err(|_| "用户消息内容不能为空".to_string())?,
                });
            }
        }
    }

    append_pending_tool_outputs_history(&mut messages, &payload.tool_outputs)?;

    Ok(messages)
}

fn parse_tool_arguments(raw: Option<&str>) -> Value {
    raw.and_then(|value| serde_json::from_str::<Value>(value).ok())
        .unwrap_or_else(|| json!({}))
}

fn append_pending_tool_outputs_history(
    messages: &mut Vec<RigMessage>,
    tool_outputs: &[AgentToolOutputPayload],
) -> Result<(), String> {
    if tool_outputs.is_empty() {
        return Ok(());
    }

    let mut assistant_items = Vec::new();
    let mut normalized_outputs = Vec::new();

    for (index, output) in tool_outputs.iter().enumerate() {
        let call_id = output.call_id.trim();
        if call_id.is_empty() {
            continue;
        }

        let tool_name = output
            .name
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or("unknown_tool");
        let tool_id = format!("resume_tool_{}_{}", index, call_id);
        let arguments = parse_tool_arguments(output.arguments.as_deref());

        assistant_items.push(RigAssistantContent::tool_call_with_call_id(
            tool_id.clone(),
            call_id.to_string(),
            tool_name.to_string(),
            arguments,
        ));

        let output_text = serde_json::to_string(&output.output)
            .map_err(|err| format!("序列化延续工具结果失败: {err}"))?;
        normalized_outputs.push((tool_id, call_id.to_string(), output_text));
    }

    if assistant_items.is_empty() {
        return Ok(());
    }

    let assistant_message = RigOneOrMany::many(assistant_items)
        .map_err(|_| "恢复工具结果历史时缺少 assistant 内容".to_string())?;
    messages.push(RigMessage::Assistant {
        id: None,
        content: assistant_message,
    });

    for (tool_id, call_id, output_text) in normalized_outputs {
        messages.push(tool_result_user_message(
            &tool_id,
            Some(call_id.as_str()),
            &output_text,
        ));
    }

    Ok(())
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

fn tool_result_user_message(id: &str, call_id: Option<&str>, output: &str) -> RigMessage {
    let content = RigToolResultContent::from_tool_output(output.to_string());
    let user_content = match call_id.map(str::trim).filter(|value| !value.is_empty()) {
        Some(call_id) => RigUserContent::tool_result_with_call_id(
            id.to_string(),
            call_id.to_string(),
            content,
        ),
        None => RigUserContent::tool_result(id.to_string(), content),
    };

    RigMessage::User {
        content: RigOneOrMany::one(user_content),
    }
}

fn build_loop_system_prompt(
    skills_root: &Option<std::path::PathBuf>,
) -> String {
    let mut lines = vec![
        "你是 MarkFlow 内置的工作区智能助手。".to_string(),
        "默认使用用户最新一条消息的语言回复；只有在用户明确要求时才切换到其他语言。".to_string(),
        "回答要简洁、基于事实，并以推进任务为目标。".to_string(),
        "## 事实纪律（必须严格遵守）".to_string(),
        "你对工作区的一切认知，必须来自工具的真实返回结果。未经工具确认，禁止断言任何项目、目录、文档是否存在，禁止假设它们的名称、ID、路径或内容。".to_string(),
        "禁止编造工具返回中不存在的信息，包括节点 ID、路径、文档正文、操作结果。不确定就先用工具查询，绝不猜测或杜撰。".to_string(),
        "只有在收到工具成功返回后，才能声称某个操作已完成。工具报错、超时或返回为空时，如实告知用户失败原因并停下询问，禁止假装成功，禁止在没有新信息的情况下反复重试同一操作。".to_string(),
        "任何会修改数据的操作（创建、移动、改写、保存、删除、覆盖），除非用户当前消息已明确、具体地指示要这样做，否则必须先用简洁的语言说明你将要做什么，并等待用户确认后再执行，不要替用户擅自决定执行方案。多步骤、破坏性、跨项目/文档的复杂操作，必须先给出简洁的执行计划并等待用户确认。指令含糊、缺少必要信息时先提问澄清，不要用假设填补空白后直接执行。".to_string(),
        "## 技能优先（强制）".to_string(),
        "处理任何具体任务前，必须先判断是否有可用技能覆盖该任务；如果技能可能适用，先读取并严格遵循技能手册。只有在没有匹配技能、技能明确不适用或技能无法完成时，才改用通用工具或直接回答。".to_string(),
        "技能手册是操作细节的强约束来源；禁止凭记忆猜测工作区方法名、参数或流程，禁止自造、简写、改写或用自然语言替代手册中的工具调用方法。".to_string(),
        "涉及 MarkFlow 工作区、项目、文档树、编辑器、附件、页面状态或文档正文写入时，必须先读取 markflow-manual 技能并按其中方法执行，不要自己猜命令、参数或流程。文档编写、追加、整篇替换必须通过 execute_browser_javascript 调用技能手册列出的 markflow 方法完成；如果不能确认方法，先重新读取技能。".to_string(),
        "## 当前状态（禁止假设）".to_string(),
        "你不知道当前处于哪个页面、打开了哪个项目或文档、编辑器是否可用或有无未保存修改。这些状态随时会因你自己的操作而改变，绝不能凭记忆或假设判断，需要时必须实时获取并以真实结果为准。".to_string(),
    ];

    // 动态注入技能索引：扫描 skills 目录读取每个 SKILL.md 的 description。
    // 系统提示词只暴露"有哪些技能、各自用途"，具体如何执行由模型自行读取技能内容决定。
    if let Some(index) = skills::build_skills_index(skills_root) {
        lines.push("## 可用技能".to_string());
        lines.push("遇到与下列技能相关的任务时，先读取对应技能的内容，再按其中说明执行；名称与用途如下：".to_string());
        lines.push(index);
    }


    lines.join("\n")
}

pub async fn chat_stream(
    Extension(db): Extension<Arc<Database>>,
    Extension(runtime_config): Extension<Arc<crate::BackendRuntimeConfig>>,
    Extension(mcp_manager): Extension<Arc<crate::mcp::McpConnectionManager>>,
    headers: HeaderMap,
    Json(payload): Json<AgentChatStreamRequest>,
) -> Result<Sse<ReceiverStream<Result<Event, Infallible>>>, Response> {
    let user = auth::require_user(&db, &headers).await?;
    if payload.provider.model.trim().is_empty() {
        return Err((StatusCode::BAD_REQUEST, Json(json!({"error": "缺少模型名称"}))).into_response());
    }
    let has_messages = payload
        .messages
        .iter()
        .any(|message| !message.content.trim().is_empty() || !message.attachments.is_empty());
    if !has_messages {
        return Err((StatusCode::BAD_REQUEST, Json(json!({"error": "消息不能为空"}))).into_response());
    }

    let provider = provider::find_user_provider(&db, user.id, payload.provider.provider_id).await?;
    let api_key = crypto::decrypt_api_key(&provider.api_key_ciphertext).map_err(|err| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": format!("读取供应商密钥失败: {}", err)})),
        )
            .into_response()
    })?;

    // 解析供应商协议与模型级配置（模型行可能不存在，此时用默认 config）。
    let model_name = payload.provider.model.clone();
    let kind = provider::ProviderKind::parse(&provider.kind);
    let provider_api = provider::OpenAiApi::parse(&provider.api);
    let config = provider::find_model_by_name(&db, provider.id, &model_name)
        .await
        .ok()
        .flatten()
        .map(|row| row.parsed_config())
        .unwrap_or_default();
    let base_url = provider.base_url.clone();

    let (tx, rx) = mpsc::channel::<Result<Event, Infallible>>(256);
    let tools_enabled = config.tools_enabled();
    let user_id = user.id;
    let db = db.clone();
    let runtime_config = runtime_config.clone();
    let mcp_manager = mcp_manager.clone();

    tokio::spawn(async move {
        let mcp_registry = build_chat_mcp_registry(
            &db,
            user_id,
            tools_enabled,
            &runtime_config,
            &mcp_manager,
            &tx,
        )
        .await;

        let conversation = match build_rig_conversation(&db, user_id, &payload).await {
            Ok(conversation) => conversation,
            Err(message) => {
                let _ = send_json_event(
                    &tx,
                    "error_event",
                    json!({ "scope": "attachments", "message": message.clone() }),
                )
                .await;
                let _ = send_json_event(&tx, "error", json!({ "error": message })).await;
                return;
            }
        };

        let skills_root = skills::resolve_skills_root(&runtime_config.skills_root_dir);
        let web_fetch_config = web_fetch::WebFetchRuntimeConfig {
            default_proxy: runtime_config.web_fetch_proxy.clone(),
            default_timeout_secs: runtime_config.web_fetch_timeout_secs,
            default_max_response_bytes: runtime_config.web_fetch_max_response_bytes,
        };
        let system_prompt = build_loop_system_prompt(&skills_root);

        let outcome = match kind {
            provider::ProviderKind::OpenAi => {
                run_openai_chat(
                    &api_key,
                    &base_url,
                    provider_api,
                    &model_name,
                    system_prompt,
                    conversation,
                    mcp_registry,
                    &config,
                    &payload,
                    skills_root.clone(),
                    web_fetch_config.clone(),
                    &tx,
                )
                .await
            }
            provider::ProviderKind::Anthropic => {
                run_anthropic_chat(
                    &api_key,
                    &base_url,
                    &provider,
                    &model_name,
                    system_prompt,
                    conversation,
                    mcp_registry,
                    &config,
                    skills_root,
                    web_fetch_config,
                    &tx,
                )
                .await
            }
        };

        if let Err(message) = outcome {
            let _ = send_json_event(
                &tx,
                "error_event",
                json!({ "scope": "runtime", "message": message.clone() }),
            )
            .await;
            let _ = send_json_event(&tx, "error", json!({ "error": message })).await;
        }
    });

    Ok(Sse::new(ReceiverStream::new(rx)).keep_alive(
        KeepAlive::new()
            .interval(Duration::from_secs(15))
            .text("keep-alive"),
    ))
}

/// 构建本轮对话的 MCP 工具注册表。tools 关闭时直接返回空注册表。
async fn build_chat_mcp_registry(
    db: &Database,
    user_id: i64,
    tools_enabled: bool,
    runtime_config: &Arc<crate::BackendRuntimeConfig>,
    mcp_manager: &Arc<crate::mcp::McpConnectionManager>,
    tx: &mpsc::Sender<Result<Event, Infallible>>,
) -> Arc<mcp_chat::McpChatToolRegistry> {
    if !tools_enabled {
        if matches!(mcp::load_mcp_settings(db, user_id).await, Ok(Some(settings)) if settings.enabled == 1) {
            let _ = send_json_event(
                tx,
                "warning_event",
                json!({ "scope": "mcp", "message": "当前模型已禁用 tools，已跳过 MCP 工具注入" }),
            )
            .await;
        }
        return Arc::new(mcp_chat::McpChatToolRegistry::empty());
    }

    match mcp::load_mcp_settings(db, user_id).await {
        Ok(Some(settings)) if settings.enabled == 1 => match mcp::list_mcp_servers(db, user_id).await {
            Ok(rows) => {
                let enabled_rows = rows.into_iter().filter(|row| row.enabled == 1).collect::<Vec<_>>();
                let build_result =
                    mcp_chat::build_mcp_chat_registry(mcp_manager, runtime_config, &enabled_rows).await;
                for warning in &build_result.warnings {
                    let _ = send_json_event(tx, "warning_event", json!({ "scope": "mcp", "message": warning })).await;
                }
                Arc::new(build_result.registry)
            }
            Err(_) => {
                let _ = send_json_event(
                    tx,
                    "warning_event",
                    json!({ "scope": "mcp", "message": "读取 MCP 服务列表失败，已跳过 MCP 工具注入" }),
                )
                .await;
                Arc::new(mcp_chat::McpChatToolRegistry::empty())
            }
        },
        Ok(_) => Arc::new(mcp_chat::McpChatToolRegistry::empty()),
        Err(_) => {
            let _ = send_json_event(
                tx,
                "warning_event",
                json!({ "scope": "mcp", "message": "读取 MCP 全局设置失败，已跳过 MCP 工具注入" }),
            )
            .await;
            Arc::new(mcp_chat::McpChatToolRegistry::empty())
        }
    }
}

#[allow(clippy::too_many_arguments)]
async fn run_openai_chat(
    api_key: &str,
    base_url: &str,
    provider_api: provider::OpenAiApi,
    model_name: &str,
    system_prompt: String,
    conversation: Vec<RigMessage>,
    mcp_registry: Arc<mcp_chat::McpChatToolRegistry>,
    config: &provider::AgentModelConfig,
    payload: &AgentChatStreamRequest,
    skills_root: Option<std::path::PathBuf>,
    web_fetch_config: web_fetch::WebFetchRuntimeConfig,
    tx: &mpsc::Sender<Result<Event, Infallible>>,
) -> Result<(), String> {
    let client = openai::Client::builder()
        .api_key(api_key)
        .base_url(base_url)
        .build()
        .map_err(|err| format!("初始化 OpenAI provider 失败: {err}"))?;

    let effective_api = runtime::resolve_openai_api(provider_api, config);
    let use_responses = matches!(effective_api, provider::OpenAiApi::Responses);
    let additional_params = params::build_openai_additional_params(payload, config, use_responses);

    if use_responses {
        let model_handle = client.completion_model(model_name);
        runtime::run_agent_stream(
            model_handle,
            system_prompt,
            conversation,
            mcp_registry,
            model_name,
            config,
            additional_params,
            skills_root.clone(),
            web_fetch_config.clone(),
            tx,
        )
        .await
    } else {
        let model_handle = client.completions_api().completion_model(model_name);
        runtime::run_agent_stream(
            model_handle,
            system_prompt,
            conversation,
            mcp_registry,
            model_name,
            config,
            additional_params,
            skills_root.clone(),
            web_fetch_config,
            tx,
        )
        .await
    }
}

#[allow(clippy::too_many_arguments)]
async fn run_anthropic_chat(
    api_key: &str,
    base_url: &str,
    provider: &AgentProvider,
    model_name: &str,
    system_prompt: String,
    conversation: Vec<RigMessage>,
    mcp_registry: Arc<mcp_chat::McpChatToolRegistry>,
    config: &provider::AgentModelConfig,
    skills_root: Option<std::path::PathBuf>,
    web_fetch_config: web_fetch::WebFetchRuntimeConfig,
    tx: &mpsc::Sender<Result<Event, Infallible>>,
) -> Result<(), String> {
    let mut builder = anthropic::Client::builder().api_key(api_key).base_url(base_url);
    if let Some(version) = provider
        .anthropic_version
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        builder = builder.anthropic_version(version);
    }
    let client = builder
        .build()
        .map_err(|err| format!("初始化 Anthropic provider 失败: {err}"))?;
    let model_handle: anthropic::completion::CompletionModel<reqwest::Client> =
        client.completion_model(model_name);
    let additional_params = params::build_anthropic_additional_params(config);
    runtime::run_agent_stream(
        model_handle,
        system_prompt,
        conversation,
        mcp_registry,
        model_name,
        config,
        additional_params,
        skills_root,
        web_fetch_config,
        tx,
    )
    .await
}


pub async fn submit_tool_callback(
    Extension(db): Extension<Arc<Database>>,
    headers: HeaderMap,
    Json(payload): Json<AgentToolCallbackRequest>,
) -> Result<Json<Value>, Response> {
    let _user = auth::require_user(&db, &headers).await?;
    if payload.run_id.trim().is_empty() || payload.call_id.trim().is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "run_id 和 call_id 不能为空"})),
        )
            .into_response());
    }

    if frontend_tool_broker()
        .resolve(payload.run_id.trim(), payload.call_id.trim(), payload.output)
        .await
    {
        Ok(Json(json!({ "ok": true })))
    } else {
        Err((
            StatusCode::NOT_FOUND,
            Json(json!({"error": "未找到对应的前端工具请求或请求已过期"})),
        )
            .into_response())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chat_stream_request_deserializes_tool_outputs() {
        let payload = serde_json::json!({
            "provider": {
                "provider_id": 1,
                "model": "qwen3.5-plus"
            },
            "messages": [
                { "role": "assistant", "content": "上一轮已经写入文档。" }
            ],
            "tool_outputs": [
                {
                    "call_id": "call_action",
                    "name": "execute_browser_javascript",
                    "arguments": "{\"code\":\"return true\"}",
                    "output": {
                        "ok": true,
                        "tool": "execute_browser_javascript",
                        "result": { "success": true }
                    }
                }
            ]
        });

        let request: AgentChatStreamRequest =
            serde_json::from_value(payload).expect("payload should deserialize");

        assert_eq!(request.tool_outputs.len(), 1);
        assert_eq!(
            request.tool_outputs[0].name.as_deref(),
            Some("execute_browser_javascript")
        );
    }

    #[test]
    fn chat_stream_request_treats_null_tool_outputs_as_empty_vec() {
        let payload = serde_json::json!({
            "provider": {
                "provider_id": 1,
                "model": "qwen3.5-plus"
            },
            "messages": [
                { "role": "user", "content": "继续" }
            ],
            "tool_outputs": null
        });

        let request: AgentChatStreamRequest =
            serde_json::from_value(payload).expect("payload should deserialize");

        assert!(request.tool_outputs.is_empty());
    }

    #[test]
    fn loop_system_prompt_contains_core_disciplines() {
        let prompt = build_loop_system_prompt(&None);
        assert!(prompt.contains("MarkFlow"));
        assert!(prompt.contains("技能优先（强制）"));
        assert!(prompt.contains("markflow-manual"));
    }
}
