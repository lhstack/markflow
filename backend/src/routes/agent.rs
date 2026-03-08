use std::{convert::Infallible, sync::Arc, time::Duration};

use async_openai::{
    config::OpenAIConfig,
    types::{
        responses::{
            CreateResponseArgs, EasyInputContent, EasyInputMessage, InputItem, InputParam,
            MessageType, ReasoningArgs, ReasoningEffort, ReasoningSummary, ResponseStreamEvent,
            Role,
        },
    },
    Client,
};
use axum::{
    extract::Extension,
    http::{HeaderMap, StatusCode},
    response::{
        sse::{Event, KeepAlive, Sse},
        IntoResponse, Response,
    },
    Json,
};
use futures_util::StreamExt;
use serde::Deserialize;
use serde_json::json;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;

use crate::{auth, db::Database};

#[derive(Debug, Deserialize, Clone)]
pub struct AgentProviderPayload {
    pub api_key: String,
    pub base_url: Option<String>,
    pub model: String,
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
}

fn truncate_chars(raw: &str, max_chars: usize) -> String {
    let truncated: String = raw.chars().take(max_chars).collect();
    if raw.chars().count() > max_chars {
        format!("{}\n\n[内容已截断]", truncated)
    } else {
        truncated
    }
}

fn normalize_base_url(value: Option<&str>) -> String {
    match value.map(str::trim).filter(|v| !v.is_empty()) {
        Some(url) => url.trim_end_matches('/').to_string(),
        None => "https://api.openai.com/v1".to_string(),
    }
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
    if let Some(project_catalog) = ctx.project_catalog.as_deref().filter(|value| !value.trim().is_empty()) {
        parts.push(format!("当前可见项目列表：{}", project_catalog));
    }
    if let Some(current_node_catalog) = ctx.current_node_catalog.as_deref().filter(|value| !value.trim().is_empty()) {
        parts.push(format!("当前项目可见目录/文档：{}", current_node_catalog));
    }

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
        }
        _ => {
            parts.push("你必须在最终正文开头只选择一个动作标记：[[ACTION:chat]]、[[ACTION:append]]、[[ACTION:replace]]。".to_string());
            parts.push("如果只是回答问题或解释，使用 [[ACTION:chat]]。如果用户要求继续完善现有文档，使用 [[ACTION:append]]。如果用户要求整体改写、重写、整理当前文档，使用 [[ACTION:replace]]。".to_string());
            parts.push("动作标记后面紧接正文，不要解释你选择了什么动作。".to_string());
            parts.push("如果用户明确要求切换页面、进入项目概览、进入某个项目或打开当前项目中的某个文档，你可以在最前面额外输出一个路由标记，然后紧跟动作标记。".to_string());
            parts.push("可用路由标记格式只有三种：[[ROUTE:overview]]、[[ROUTE:project:项目名]]、[[ROUTE:doc:文档名]]。".to_string());
            parts.push("只有在你能从上下文中确认目标名称时才输出路由标记；不要编造项目名或文档名。".to_string());
        }
    }

    if matches!(mode, "auto" | "write") {
        if let Some(content) = ctx.doc_content.as_deref().filter(|value| !value.trim().is_empty()) {
            parts.push("以下是当前文档正文摘要，可用于参考：".to_string());
            parts.push(truncate_chars(content, 6000));
        }
    }

    parts.join("\n\n")
}

fn build_response_input(payload: &AgentChatStreamRequest) -> Vec<InputItem> {
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

async fn send_json_event(
    tx: &mpsc::Sender<Result<Event, Infallible>>,
    name: &str,
    payload: serde_json::Value,
) -> bool {
    tx.send(Ok(Event::default().event(name).data(payload.to_string())))
        .await
        .is_ok()
}

pub async fn chat_stream(
    Extension(db): Extension<Arc<Database>>,
    headers: HeaderMap,
    Json(payload): Json<AgentChatStreamRequest>,
) -> Result<Sse<ReceiverStream<Result<Event, Infallible>>>, Response> {
    let user = auth::require_user(&db, &headers).await?;

    if payload.provider.api_key.trim().is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "缺少供应商 API Key"})),
        )
            .into_response());
    }
    if payload.provider.model.trim().is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "缺少模型名称"})),
        )
            .into_response());
    }
    if payload.messages.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "消息不能为空"})),
        )
            .into_response());
    }

    let ctx = payload.context.clone().unwrap_or_default();
    let input_items = build_response_input(&payload);
    let config = OpenAIConfig::new()
        .with_api_key(payload.provider.api_key.clone())
        .with_api_base(normalize_base_url(payload.provider.base_url.as_deref()));
    let client = Client::with_config(config);

    let (tx, rx) = mpsc::channel::<Result<Event, Infallible>>(256);
    let model = payload.provider.model.clone();
    let mode = normalize_agent_mode(payload.mode.as_deref());
    let write_mode = normalize_write_mode(payload.write_mode.as_deref());
    let username = user.username.clone();

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

        let reasoning = match ReasoningArgs::default()
            .effort(ReasoningEffort::Medium)
            .summary(ReasoningSummary::Detailed)
            .build()
        {
            Ok(reasoning) => reasoning,
            Err(err) => {
                let _ = send_json_event(
                    &tx,
                    "error",
                    json!({ "error": format!("Reasoning 参数构造失败: {}", err) }),
                )
                .await;
                return;
            }
        };

        let request = match CreateResponseArgs::default()
            .model(model.clone())
            .input(InputParam::Items(input_items))
            .instructions(build_system_prompt(&mode, write_mode.as_deref(), &username, &ctx))
            .reasoning(reasoning)
            .stream(true)
            .build()
        {
            Ok(request) => request,
            Err(err) => {
                let _ = send_json_event(
                    &tx,
                    "error",
                    json!({ "error": format!("Responses 请求构造失败: {}", err) }),
                )
                .await;
                return;
            }
        };

        let mut stream = match client.responses().create_stream(request).await {
            Ok(stream) => stream,
            Err(err) => {
                let _ = send_json_event(
                    &tx,
                    "error",
                    json!({ "error": format!("创建流式会话失败: {}", err) }),
                )
                .await;
                return;
            }
        };

        let mut final_text = String::new();

        while let Some(event) = stream.next().await {
            match event {
                Ok(ResponseStreamEvent::ResponseOutputTextDelta(ev)) => {
                    final_text.push_str(&ev.delta);
                    if !send_json_event(
                        &tx,
                        "message.delta",
                        json!({ "content": ev.delta }),
                    )
                    .await
                    {
                        return;
                    }
                }
                Ok(ResponseStreamEvent::ResponseReasoningTextDelta(ev)) => {
                    if !send_json_event(
                        &tx,
                        "reasoning.delta",
                        json!({ "item_id": ev.item_id, "delta": ev.delta }),
                    )
                    .await
                    {
                        return;
                    }
                }
                Ok(ResponseStreamEvent::ResponseReasoningSummaryTextDelta(ev)) => {
                    if !send_json_event(
                        &tx,
                        "reasoning.delta",
                        json!({ "item_id": ev.item_id, "delta": ev.delta }),
                    )
                    .await
                    {
                        return;
                    }
                }
                Ok(ResponseStreamEvent::ResponseFailed(ev)) => {
                    let message = ev
                        .response
                        .error
                        .as_ref()
                        .map(|error| error.message.clone())
                        .unwrap_or_else(|| "Responses 调用失败".to_string());
                    let _ = send_json_event(&tx, "error", json!({ "error": message })).await;
                    return;
                }
                Ok(ResponseStreamEvent::ResponseError(ev)) => {
                    let _ = send_json_event(&tx, "error", json!({ "error": ev.message })).await;
                    return;
                }
                Ok(_) => {}
                Err(err) => {
                    let _ = send_json_event(
                        &tx,
                        "error",
                        json!({ "error": format!("流式调用失败: {}", err) }),
                    )
                    .await;
                    return;
                }
            }
        }

        let _ = send_json_event(
            &tx,
            "message.completed",
            json!({ "content": final_text }),
        )
        .await;
        let _ = send_json_event(&tx, "done", json!({})).await;
    });

    Ok(Sse::new(ReceiverStream::new(rx)).keep_alive(
        KeepAlive::new()
            .interval(Duration::from_secs(15))
            .text("keep-alive"),
    ))
}
