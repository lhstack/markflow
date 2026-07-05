//! 对话运行时：基于 rig 高层 `Agent` 的多轮流式执行。
//!
//! 设计要点（完全重构）：
//! - 用 `client.agent(model).preamble(..).tools(..).build()` 构建高层 Agent，
//!   由 rig 框架自身管理多轮工具循环与历史，替代此前手写的 `stream_rig_agent_loop`。
//! - 前端工具 / MCP 工具都以 `Box<dyn ToolDyn>` 注入（见 `tools.rs`），
//!   rig 在轮次间调用 `ToolDyn::call`，其内部通过 broker 往返或本地执行。
//! - 文档编写、追加、替换统一通过 execute_browser_javascript 调用 markflow 方法完成，
//!   不再使用文本流写入协议。

use std::{convert::Infallible, sync::Arc};

use axum::response::sse::Event;
use futures_util::StreamExt;
use rig::{
    agent::{AgentBuilder, MultiTurnStreamItem, StreamingError},
    completion::{CompletionModel, GetTokenUsage, Message as RigMessage},
    message::{AssistantContent as RigAssistantContent, UserContent as RigUserContent},
    streaming::{StreamedAssistantContent, StreamedUserContent, StreamingPrompt},
};
use serde_json::{json, Value};
use tokio::sync::mpsc;
use uuid::Uuid;

use crate::mcp_chat::McpChatToolRegistry;

use super::provider::{AgentModelConfig, OpenAiApi};
use super::tools::build_agent_tools;
use super::web_fetch::WebFetchRuntimeConfig;
use super::send_json_event;

const MAX_AGENT_TURNS: usize = 1000;

/// 构建并运行一次对话流。`model_handle` 由调用方按供应商协议实例化。
///
/// - `system_prompt`：完整系统提示（含技能优先与页面上下文）。
/// - `conversation`：完整消息序列，最后一条 user 消息作为 prompt，其余作为历史。
/// - `mcp_registry`：本轮启用的 MCP 工具。
#[allow(clippy::too_many_arguments)]
pub async fn run_agent_stream<M>(
    model_handle: M,
    system_prompt: String,
    conversation: Vec<RigMessage>,
    mcp_registry: Arc<McpChatToolRegistry>,
    model_name: &str,
    config: &AgentModelConfig,
    additional_params: Option<Value>,
    skills_root: Option<std::path::PathBuf>,
    web_fetch_config: WebFetchRuntimeConfig,
    tx: &mpsc::Sender<Result<Event, Infallible>>,
) -> Result<(), String>
where
    M: CompletionModel + 'static,
    M::StreamingResponse: GetTokenUsage,
{
    let run_id = format!("run_{}", Uuid::new_v4().simple());

    let (prompt, history) = split_prompt(conversation);
    // 按上下文窗口裁剪历史：预留输出 token，扣除系统提示与当前 prompt 固定开销，
    // 按“tool_call 与其配对 tool_result”成组从最新往旧填充，超预算即止且至少保留一组。
    let history = trim_history_for_context(
        history,
        &system_prompt,
        &prompt,
        config.context_window,
        config.max_tokens,
    );

    // 组装工具：前端工具（broker 往返）+ MCP 工具（本地执行）。
    let tools = if config.tools_enabled() {
        build_agent_tools(
            &run_id,
            tx,
            Arc::clone(&mcp_registry),
            skills_root.clone(),
            web_fetch_config.clone(),
        )
    } else {
        Vec::new()
    };

    // 构建高层 Agent。返回 Self 的参数 setter 先做条件赋值，
    // `.tools()` 会切换 typestate（NoToolConfig -> WithBuilderTools），放最后一次性调用。
    let mut builder = AgentBuilder::new(model_handle)
        .preamble(&system_prompt)
        .default_max_turns(MAX_AGENT_TURNS);
    if let Some(temperature) = config.temperature {
        builder = builder.temperature(temperature);
    }
    if let Some(max_tokens) = config.max_tokens {
        builder = builder.max_tokens(max_tokens);
    }
    if let Some(params) = additional_params {
        builder = builder.additional_params(params);
    }
    let agent = builder.tools(tools).build();

    let _ = send_json_event(
        tx,
        "message.started",
        json!({ "model": model_name, "run_id": run_id }),
    )
    .await;

    let mut stream = agent.stream_prompt(prompt).with_history(history).await;

    let mut accumulated_text = String::new();
    let response_id: Option<String> = None;
    let mut usage_json: Option<Value> = None;

    while let Some(item) = stream.next().await {
        let item = match item {
            Ok(item) => item,
            Err(err) => {
                if let StreamingError::Prompt(prompt_err) = &err {
                    if matches!(
                        **prompt_err,
                        rig::completion::PromptError::MaxTurnsError { .. }
                    ) {
                        break;
                    }
                }
                frontend_cancel(&run_id).await;
                return Err(format!("Agent 执行失败: {err}"));
            }
        };

        match item {
            MultiTurnStreamItem::StreamAssistantItem(StreamedAssistantContent::Text(text)) => {
                accumulated_text.push_str(&text.text);
                if !send_json_event(tx, "message.delta", json!({ "content": text.text })).await {
                    break;
                }
            }
            MultiTurnStreamItem::StreamAssistantItem(StreamedAssistantContent::ReasoningDelta {
                reasoning,
                ..
            }) => {
                if !reasoning.trim().is_empty() {
                    let _ = send_json_event(tx, "reasoning.delta", json!({ "delta": reasoning }))
                        .await;
                }
            }
            MultiTurnStreamItem::StreamAssistantItem(StreamedAssistantContent::Reasoning(
                reasoning,
            )) => {
                let text = reasoning.display_text();
                if !text.trim().is_empty() {
                    let _ = send_json_event(tx, "reasoning.delta", json!({ "delta": text })).await;
                }
            }
            MultiTurnStreamItem::StreamAssistantItem(StreamedAssistantContent::ToolCall {
                tool_call,
                ..
            }) => {
                let call_id = tool_call
                    .call_id
                    .clone()
                    .filter(|value| !value.trim().is_empty())
                    .unwrap_or_else(|| tool_call.id.clone());
                let _ = send_json_event(
                    tx,
                    "tool.call.delta",
                    json!({
                        "call_id": call_id,
                        "name": tool_call.function.name,
                        "arguments": tool_call.function.arguments.to_string(),
                    }),
                )
                .await;
            }
            MultiTurnStreamItem::StreamUserItem(StreamedUserContent::ToolResult { .. }) => {}
            MultiTurnStreamItem::FinalResponse(response) => {
                // 部分 provider（如通过兼容网关的 qwq 系列）不发逐字 Text delta，
                // 正文只在 FinalResponse.response() 里整体给出。若前面没累积到任何
                // 文本，则用最终响应文本补发一次 delta 并累积，避免正文丢失。
                if accumulated_text.trim().is_empty() {
                    let final_text = response.response();
                    if !final_text.trim().is_empty() {
                        accumulated_text.push_str(final_text);
                        let _ = send_json_event(tx, "message.delta", json!({ "content": final_text })).await;
                    }
                }
                // 聚合整个多轮 turn 的 token 消耗，随 message.completed 一并下发。
                let usage = response.usage();
                usage_json = Some(json!({
                    "input_tokens": usage.input_tokens,
                    "output_tokens": usage.output_tokens,
                    "total_tokens": usage.total_tokens,
                    "cached_input_tokens": usage.cached_input_tokens,
                    "cache_creation_input_tokens": usage.cache_creation_input_tokens,
                    "tool_use_prompt_tokens": usage.tool_use_prompt_tokens,
                    "reasoning_tokens": usage.reasoning_tokens,
                }));
            }
            _ => {}
        }
    }

    let _ = send_json_event(
        tx,
        "message.completed",
        json!({
            "content": accumulated_text,
            "run_id": run_id,
            "response_id": response_id,
            "usage": usage_json,
        }),
    )
    .await;
    let _ = send_json_event(tx, "done", json!({})).await;
    frontend_cancel(&run_id).await;
    Ok(())
}

async fn frontend_cancel(run_id: &str) {
    super::broker::frontend_tool_broker().cancel_run(run_id).await;
}


/// 估算一条消息在上下文预算中占用的字符数（序列化后长度）。
fn message_cost(message: &RigMessage) -> usize {
    serde_json::to_string(message)
        .map(|value| value.len())
        .unwrap_or_default()
}

fn message_has_tool_calls(message: &RigMessage) -> bool {
    match message {
        RigMessage::Assistant { content, .. } => content
            .iter()
            .any(|item| matches!(item, RigAssistantContent::ToolCall(_))),
        _ => false,
    }
}

fn message_has_tool_results(message: &RigMessage) -> bool {
    match message {
        RigMessage::User { content } => content
            .iter()
            .any(|item| matches!(item, RigUserContent::ToolResult(_))),
        _ => false,
    }
}

/// 把历史按语义分组：tool_call 与其紧邻的 tool_result 绑成一组不可拆分，
/// 其余消息各自成组。裁剪时以组为单位保留，避免拆散工具调用配对导致上游报错。
fn group_history_for_context(history: Vec<RigMessage>) -> Vec<Vec<RigMessage>> {
    let mut groups = Vec::new();
    let mut messages = history.into_iter().peekable();
    while let Some(message) = messages.next() {
        if message_has_tool_calls(&message) {
            let Some(next) = messages.peek() else {
                continue;
            };
            if !message_has_tool_results(next) {
                continue;
            }
            let result = messages
                .next()
                .expect("peek confirmed paired tool result message");
            groups.push(vec![message, result]);
            continue;
        }
        if message_has_tool_results(&message) {
            continue;
        }
        groups.push(vec![message]);
    }
    groups
}

/// 按上下文窗口裁剪历史（移植自 awake 方案）。
///
/// - `context_window` 未设置时不裁剪，原样返回。
/// - `input_token_limit = context_window - max_tokens`（预留输出 token）。
/// - 以 1 token ≈ 4 字符估算，扣除系统提示与当前 prompt 的固定开销得到预算。
/// - 按组从最新往旧累加，超预算即停，但至少保留最新一组。
pub(super) fn trim_history_for_context(
    history: Vec<RigMessage>,
    preamble: &str,
    prompt: &RigMessage,
    context_window: Option<u64>,
    max_tokens: Option<u64>,
) -> Vec<RigMessage> {
    let Some(limit) = context_window.and_then(|value| usize::try_from(value).ok()) else {
        return history;
    };
    if limit == 0 {
        return Vec::new();
    }

    let reserved_output_tokens = max_tokens
        .and_then(|value| usize::try_from(value).ok())
        .unwrap_or_default();
    let input_token_limit = limit.saturating_sub(reserved_output_tokens);
    if input_token_limit == 0 {
        return Vec::new();
    }

    let max_chars = input_token_limit.saturating_mul(4);
    let fixed_chars = preamble.len().saturating_add(message_cost(prompt));
    let budget = max_chars.saturating_sub(fixed_chars);
    if budget == 0 {
        return Vec::new();
    }

    let mut kept_groups: Vec<Vec<RigMessage>> = Vec::new();
    let mut used = 0usize;
    for group in group_history_for_context(history).into_iter().rev() {
        let cost = group.iter().map(message_cost).sum::<usize>();
        if used.saturating_add(cost) > budget && !kept_groups.is_empty() {
            break;
        }
        if cost > budget && kept_groups.is_empty() {
            break;
        }
        used = used.saturating_add(cost);
        kept_groups.push(group);
    }
    kept_groups.reverse();
    kept_groups.into_iter().flatten().collect()
}

/// 取最后一条 user 消息作为 prompt，其余作为历史。若末尾非 user，则补一条"继续"。
fn split_prompt(mut messages: Vec<RigMessage>) -> (RigMessage, Vec<RigMessage>) {
    match messages.last() {
        Some(RigMessage::User { .. }) => {
            let prompt = messages.pop().expect("last exists");
            (prompt, messages)
        }
        _ => (RigMessage::user("继续"), messages),
    }
}

/// 供 chat_stream 判定协议时使用：把供应商 kind 与模型级覆盖解析成 OpenAI 传输模式。
pub fn resolve_openai_api(provider_api: OpenAiApi, config: &AgentModelConfig) -> OpenAiApi {
    config.api_override().unwrap_or(provider_api)
}
