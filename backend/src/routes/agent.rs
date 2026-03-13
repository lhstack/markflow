use std::{
    convert::Infallible,
    sync::Arc,
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
            ChatCompletionTool, ChatCompletionTools, CreateChatCompletionRequestArgs, FinishReason,
            FunctionCall as ChatFunctionCall, FunctionObject,
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
use chrono::Utc;
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use serde_json::json;
use sha2::{Digest, Sha256};
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use uuid::Uuid;

use crate::{agent_protocol::{control_close_marker, control_open_marker, control_phases, default_agent_base_url, route_descriptions, route_enum_values, task_analysis_complexities, task_analysis_intents, task_analysis_modes, task_analysis_write_scopes, write_action_markers, write_action_modes, write_action_payload_formats}, auth, db::Database, models::AgentProvider};

enum AgentStreamError {
    Retryable(String),
    Fatal(String),
}

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

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct AgentExecutionToolCallSummaryPayload {
    pub name: String,
    pub arguments: Option<String>,
    pub output: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct AgentExecutionContextPayload {
    pub pending_plan: Option<String>,
    pub pending_plan_user_reply: Option<String>,
    pub composite_write_then_save: Option<bool>,
    pub semantic_continuation: Option<bool>,
    pub semantic_continuation_round: Option<i64>,
    pub previous_assistant_summary: Option<String>,
    pub task_kind: Option<String>,
    pub edit_intent: Option<String>,
    pub edit_stage: Option<String>,
    pub save_requested: Option<bool>,
    pub write_completed: Option<bool>,
    pub plan_step_index: Option<i64>,
    pub plan_total_steps: Option<i64>,
    pub plan_current_step: Option<String>,
    pub plan_completed_steps: Option<Vec<String>>,
    pub document_write_observed: Option<bool>,
    pub save_attempt_without_document_change: Option<bool>,
    pub recent_tool_calls: Option<Vec<AgentExecutionToolCallSummaryPayload>>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct AgentExecutionMemoryPayload {
    pub plan: Option<String>,
    pub assistant_summary: Option<String>,
    pub control_phase: Option<String>,
    pub task_kind: Option<String>,
    pub edit_intent: Option<String>,
    pub edit_stage: Option<String>,
    pub save_requested: Option<bool>,
    pub write_completed: Option<bool>,
    pub plan_step_index: Option<i64>,
    pub plan_total_steps: Option<i64>,
    pub plan_current_step: Option<String>,
    pub plan_completed_steps: Option<Vec<String>>,
    pub document_write_observed: Option<bool>,
    pub save_attempt_without_document_change: Option<bool>,
    pub recent_tool_calls: Option<Vec<AgentExecutionToolCallSummaryPayload>>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct AgentSessionMemoryPayload {
    pub summary: Option<String>,
    pub active_user_goals: Option<Vec<String>>,
    pub completed_facts: Option<Vec<String>>,
    pub open_loops: Option<Vec<String>>,
    pub updated_at: Option<String>,
}

#[derive(Debug, Deserialize, Default, Clone)]
pub struct AgentContextPayload {
    pub page_scope: Option<String>,
    pub page_state: Option<String>,
    pub project_name: Option<String>,
    pub doc_id: Option<i64>,
    pub doc_name: Option<String>,
    pub project_catalog: Option<String>,
    pub current_node_catalog: Option<String>,
    pub editor_available: Option<bool>,
    pub editor_snapshot_source: Option<String>,
    pub editor_unsaved_changes: Option<bool>,
    pub agent_execution: Option<AgentExecutionContextPayload>,
    pub last_execution: Option<AgentExecutionMemoryPayload>,
    pub session_memory: Option<AgentSessionMemoryPayload>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct AgentChatStreamRequest {
    pub provider: AgentProviderPayload,
    pub messages: Vec<AgentMessagePayload>,
    pub mode: Option<String>,
    pub transport_mode: Option<String>,
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

#[derive(Debug, Serialize, Clone)]
pub struct RuntimeTargetResourcesPayload {
    pub project_ids: Vec<String>,
    pub folder_ids: Vec<String>,
    pub doc_ids: Vec<String>,
}

#[derive(Debug, Serialize, Clone)]
pub struct RuntimePlanStepPayload {
    pub id: String,
    pub title: String,
    pub kind: String,
    pub description: String,
    pub status: String,
    pub tool_hints: Vec<String>,
    pub requires_confirmation: bool,
}

#[derive(Debug, Serialize, Clone)]
pub struct RuntimeTaskAnalysisPayload {
    pub intent: String,
    pub complexity: String,
    pub mode: String,
    pub requires_tools: bool,
    pub requires_user_confirmation: bool,
    pub write_scope: Option<String>,
    pub preferred_write_action: Option<String>,
    pub target_resources: RuntimeTargetResourcesPayload,
    pub deliverable: Option<String>,
    pub steps: Vec<RuntimePlanStepPayload>,
}

#[derive(Debug, Serialize, Clone)]
pub struct RuntimePlanPayload {
    pub id: String,
    pub goal: String,
    pub summary: Option<String>,
    pub status: String,
    pub steps: Vec<RuntimePlanStepPayload>,
    pub created_at: String,
    pub updated_at: String,
}

enum AgentStreamOutcome {
    Message {
        text: String,
        response_id: Option<String>,
    },
    ToolCalls {
        response_id: String,
        text: String,
        calls: Vec<AgentToolCallRequest>,
    },
}

fn normalize_base_url(value: Option<&str>) -> String {
    match value.map(str::trim).filter(|v| !v.is_empty()) {
        Some(url) => url.trim_end_matches('/').to_string(),
        None => default_agent_base_url().to_string(),
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

fn normalize_transport_mode(value: Option<&str>) -> String {
    match value.map(str::trim).filter(|v| !v.is_empty()) {
        Some("responses") => "responses".to_string(),
        Some("chat") => "chat".to_string(),
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

fn latest_user_message(messages: &[AgentMessagePayload]) -> String {
    messages
        .iter()
        .rev()
        .find(|message| message.role.trim().eq_ignore_ascii_case("user") && !message.content.trim().is_empty())
        .map(|message| message.content.trim().to_string())
        .unwrap_or_default()
}

fn plan_lines_from_text(plan_text: &str) -> Vec<String> {
    plan_text
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(|line| {
            line.trim_start_matches(|ch: char| {
                ch.is_ascii_digit() || matches!(ch, '.' | '、' | ')' | '）' | '-' | ' ')
            })
            .trim()
            .to_string()
        })
        .filter(|line| !line.is_empty())
        .collect()
}

fn current_plan_text(ctx: &AgentContextPayload) -> Option<String> {
    ctx.agent_execution
        .as_ref()
        .and_then(|state| state.pending_plan.clone())
        .or_else(|| {
            ctx.last_execution
                .as_ref()
                .and_then(|memory| memory.plan.clone())
        })
        .map(|plan| plan.trim().to_string())
        .filter(|plan| !plan.is_empty())
}

fn current_plan_progress(
    ctx: &AgentContextPayload,
) -> (Option<usize>, Option<usize>, Vec<String>, Option<String>) {
    let execution = ctx.agent_execution.as_ref();
    let memory = ctx.last_execution.as_ref();
    let step_index = execution
        .and_then(|state| state.plan_step_index)
        .or_else(|| memory.and_then(|state| state.plan_step_index))
        .and_then(|value| usize::try_from(value).ok())
        .filter(|value| *value > 0);
    let total_steps = execution
        .and_then(|state| state.plan_total_steps)
        .or_else(|| memory.and_then(|state| state.plan_total_steps))
        .and_then(|value| usize::try_from(value).ok())
        .filter(|value| *value > 0);
    let completed_steps = execution
        .and_then(|state| state.plan_completed_steps.clone())
        .or_else(|| memory.and_then(|state| state.plan_completed_steps.clone()))
        .unwrap_or_default();
    let current_step = execution
        .and_then(|state| state.plan_current_step.clone())
        .or_else(|| memory.and_then(|state| state.plan_current_step.clone()))
        .map(|step| step.trim().to_string())
        .filter(|step| !step.is_empty());
    (step_index, total_steps, completed_steps, current_step)
}

fn build_structured_steps_from_current_plan(ctx: &AgentContextPayload) -> Vec<RuntimePlanStepPayload> {
    let Some(plan_text) = current_plan_text(ctx) else {
        return Vec::new();
    };

    let (current_step_index, _, completed_steps, current_step) = current_plan_progress(ctx);
    let parsed_steps = plan_lines_from_text(&plan_text);
    if parsed_steps.is_empty() {
        return Vec::new();
    }

    parsed_steps
        .into_iter()
        .enumerate()
        .map(|(index, title)| {
            let step_number = index + 1;
            let is_completed = completed_steps.iter().any(|step| step.trim() == title)
                || current_step_index.map(|value| step_number < value).unwrap_or(false);
            let is_current = current_step_index.map(|value| step_number == value).unwrap_or(false)
                || current_step
                    .as_deref()
                    .map(|step| step == title)
                    .unwrap_or(false);
            let status = if is_completed {
                "completed"
            } else if is_current {
                "in_progress"
            } else {
                "pending"
            };

            RuntimePlanStepPayload {
                id: format!("step_{}", step_number),
                title: title.clone(),
                kind: "edit".to_string(),
                description: title,
                status: status.to_string(),
                tool_hints: Vec::new(),
                requires_confirmation: false,
            }
        })
        .collect()
}

fn build_analysis_steps(mode: &str, ctx: &AgentContextPayload) -> Vec<RuntimePlanStepPayload> {
    if mode != "plan" {
        return Vec::new();
    }

    let explicit_steps = build_structured_steps_from_current_plan(ctx);
    if !explicit_steps.is_empty() {
        return explicit_steps;
    }

    let current_doc = ctx
        .doc_name
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| format!("《{}》", value))
        .unwrap_or_else(|| "当前文档".to_string());

    let current_project = ctx
        .project_name
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| format!("《{}》", value))
        .unwrap_or_else(|| "当前项目".to_string());

    let step_specs: Vec<(&str, &str, String, Vec<&str>, bool)> = if ctx.doc_id.is_some() {
        vec![
            (
                "analyze",
                "读取当前文档与目标片段",
                format!("读取 {} 的最新正文，定位本次改写所需的章节或片段。", current_doc),
                vec!["read_editor_snapshot", "read_document"],
                false,
            ),
            (
                "draft",
                "生成正文草稿",
                format!("根据用户要求为 {} 生成修改后的 Markdown 草稿。", current_doc),
                vec![],
                false,
            ),
            (
                "edit",
                "应用正文修改",
                format!("将草稿写入 {}，完成本次正文调整。", current_doc),
                vec![],
                false,
            ),
            (
                "edit",
                "保存文档",
                format!("按用户要求保存 {}。", current_doc),
                vec!["save_current_document"],
                false,
            ),
        ]
    } else if ctx
        .project_name
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .is_some()
    {
        vec![
            (
                "analyze",
                "扫描现有资源",
                format!("读取 {} 的项目结构、目录与相关文档。", current_project),
                vec!["get_project_tree", "list_projects", "read_document"],
                false,
            ),
            (
                "outline",
                "生成结构化方案",
                "根据现有资源生成结构化目录、文档或处理方案。".to_string(),
                vec![],
                false,
            ),
            (
                "draft",
                "生成草稿产物",
                "为目标目录、首页或文档生成可预览的 Markdown 草稿。".to_string(),
                vec![],
                false,
            ),
            (
                "edit",
                "执行资源变更",
                "按确认后的方案执行创建、移动、写入和保存。".to_string(),
                vec!["create_project", "create_tree_node", "open_tree_node", "save_current_document"],
                true,
            ),
        ]
    } else {
        vec![
            (
                "analyze",
                "定位目标资源",
                "确认当前请求涉及的项目、目录或文档，并补全执行所需上下文。".to_string(),
                vec!["get_current_page_state", "read_editor_snapshot", "read_document"],
                false,
            ),
            (
                "draft",
                "生成结果草稿",
                "生成本次请求需要的正文、结构或草稿结果。".to_string(),
                vec![],
                false,
            ),
            (
                "edit",
                "应用并收尾",
                "将确认后的结果写入目标资源，并完成必要的保存或状态更新。".to_string(),
                vec!["save_current_document"],
                false,
            ),
        ]
    };

    step_specs
        .into_iter()
        .enumerate()
        .map(|(index, (kind, title, description, tool_hints, requires_confirmation))| RuntimePlanStepPayload {
            id: format!("step_{}", index + 1),
            title: title.to_string(),
            kind: kind.to_string(),
            description,
            status: "pending".to_string(),
            tool_hints: tool_hints.into_iter().map(|item| item.to_string()).collect(),
            requires_confirmation,
        })
        .collect()
}

fn analyze_task_request(
    payload: &AgentChatStreamRequest,
    ctx: &AgentContextPayload,
) -> RuntimeTaskAnalysisPayload {
    let latest_user_request = latest_user_message(&payload.messages);
    let requested_mode = normalize_agent_mode(payload.mode.as_deref());
    let has_tool_outputs = payload
        .tool_outputs
        .as_ref()
        .map(|items| !items.is_empty())
        .unwrap_or(false);
    let has_pending_plan = ctx
        .agent_execution
        .as_ref()
        .and_then(|state| state.pending_plan.as_ref())
        .map(|plan| !plan.trim().is_empty())
        .unwrap_or(false);
    let has_plan_reply = ctx
        .agent_execution
        .as_ref()
        .and_then(|state| state.pending_plan_user_reply.as_ref())
        .map(|reply| !reply.trim().is_empty())
        .unwrap_or(false);
    let semantic_continuation = ctx
        .agent_execution
        .as_ref()
        .and_then(|state| state.semantic_continuation)
        .unwrap_or(false);
    let existing_plan_steps = current_plan_progress(ctx)
        .1
        .or_else(|| current_plan_text(ctx).map(|plan| plan_lines_from_text(&plan).len()))
        .unwrap_or(0);

    let mode = if has_pending_plan
        || has_plan_reply
        || semantic_continuation
        || has_tool_outputs
        || requested_mode == "auto"
        || requested_mode == "write"
    {
        "plan"
    } else {
        "chat"
    };

    let complexity = if mode != "plan" {
        "low"
    } else if existing_plan_steps >= 4 || (ctx.project_name.is_some() && ctx.doc_id.is_some()) {
        "high"
    } else {
        "medium"
    };

    let requires_tools = mode == "plan" || has_tool_outputs;
    let requires_user_confirmation =
        mode == "plan" && !has_pending_plan && !has_plan_reply && !semantic_continuation && !has_tool_outputs;
    let steps = build_analysis_steps(mode, ctx);
    let intent = ctx
        .agent_execution
        .as_ref()
        .and_then(|state| state.task_kind.clone())
        .or_else(|| {
            ctx.last_execution
                .as_ref()
                .and_then(|memory| memory.task_kind.clone())
        })
        .unwrap_or_else(|| {
            if mode == "plan" {
                "execution".to_string()
            } else {
                "conversation".to_string()
            }
        });
    RuntimeTaskAnalysisPayload {
        intent,
        complexity: complexity.to_string(),
        mode: mode.to_string(),
        requires_tools,
        requires_user_confirmation,
        write_scope: None,
        preferred_write_action: None,
        target_resources: RuntimeTargetResourcesPayload {
            project_ids: ctx
                .project_name
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(|_| vec!["current_project".to_string()])
                .unwrap_or_default(),
            folder_ids: Vec::new(),
            doc_ids: ctx
                .doc_id
                .map(|doc_id| vec![doc_id.to_string()])
                .unwrap_or_default(),
        },
        deliverable: if mode == "plan" && !latest_user_request.trim().is_empty() {
            Some(latest_user_request.clone())
        } else {
            None
        },
        steps,
    }
}

fn build_runtime_plan(
    analysis: &RuntimeTaskAnalysisPayload,
    payload: &AgentChatStreamRequest,
) -> Option<RuntimePlanPayload> {
    if analysis.mode != "plan" {
        return None;
    }

    let goal = latest_user_message(&payload.messages);
    let now = Utc::now().to_rfc3339();
    Some(RuntimePlanPayload {
        id: format!("plan_{}", Uuid::new_v4().simple()),
        goal: if goal.trim().is_empty() {
            "执行当前任务".to_string()
        } else {
            goal
        },
        summary: Some("系统根据当前请求预估的结构化执行计划。若模型生成正式计划，以模型确认版本为准。".to_string()),
        status: "pending".to_string(),
        steps: analysis.steps.clone(),
        created_at: now.clone(),
        updated_at: now,
    })
}

fn build_task_analysis_prompt_section(analysis: &RuntimeTaskAnalysisPayload) -> Option<String> {
    let allowed_modes = task_analysis_modes()
        .iter()
        .map(|item| format!("`{}`", item))
        .collect::<Vec<_>>()
        .join(" / ");
    let allowed_complexities = task_analysis_complexities()
        .iter()
        .map(|item| format!("`{}`", item))
        .collect::<Vec<_>>()
        .join(" / ");
    let allowed_intents = task_analysis_intents()
        .iter()
        .map(|item| format!("`{}`", item))
        .collect::<Vec<_>>()
        .join(" / ");
    let allowed_write_scopes = task_analysis_write_scopes()
        .iter()
        .map(|item| format!("`{}`", item))
        .collect::<Vec<_>>()
        .join(" / ");
    let write_scope = analysis
        .write_scope
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or("未指定");
    let preferred_write_action = analysis
        .preferred_write_action
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or("未指定");
    let deliverable = analysis
        .deliverable
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or("未指定");
    let mut lines = vec![
        format!("- intent: {}", analysis.intent),
        format!("- complexity: {}", analysis.complexity),
        format!("- mode: {}", analysis.mode),
        format!("- requires_tools: {}", if analysis.requires_tools { "true" } else { "false" }),
        format!(
            "- requires_user_confirmation: {}",
            if analysis.requires_user_confirmation { "true" } else { "false" }
        ),
        format!("- write_scope: {}", write_scope),
        format!("- preferred_write_action: {}", preferred_write_action),
        format!("- deliverable: {}", deliverable),
    ];
    if !analysis.steps.is_empty() {
        lines.push("- suggested_step_titles:".to_string());
        lines.extend(
            analysis
                .steps
                .iter()
                .take(8)
                .map(|step| format!("  - {}", step.title)),
        );
    }
    Some(format!(
        "## 任务分析
以下是系统基于显式运行时输入、页面上下文和已有机器状态生成的内部任务分析摘要。它不使用自然语言关键词猜测用户意图。任务分析字段的允许值来自共享协议：mode={}；complexity={}；intent={}；write_scope={}。你应优先遵循这里的 mode / complexity / requires_tools / requires_user_confirmation 运行约束；如果还需要更细的写入范围、编辑意图或动作类型，必须通过正式协议字段显式给出，而不是依赖自然语言猜测。`write_scope` 与 `preferred_write_action` 不由系统猜测，必须由你在执行协议里显式给出。
{}",
        allowed_modes,
        allowed_complexities,
        allowed_intents,
        allowed_write_scopes,
        lines.join("\n")
    ))
}

fn build_suggested_plan_prompt_section(plan: Option<&RuntimePlanPayload>) -> Option<String> {
    let plan = plan?;
    let mut lines = vec![
        format!("- goal: {}", plan.goal),
        format!(
            "- summary: {}",
            plan.summary
                .as_deref()
                .filter(|value| !value.trim().is_empty())
                .unwrap_or("未提供")
        ),
        format!("- status: {}", plan.status),
    ];
    if !plan.steps.is_empty() {
        lines.push("- candidate_steps:".to_string());
        lines.extend(
            plan.steps
                .iter()
                .take(12)
                .enumerate()
                .map(|(index, step)| format!("  {}. {}", index + 1, step.title)),
        );
    }
    Some(format!(
        "## 建议计划对象
以下是系统为当前请求生成的候选计划摘要。它只是内部参考骨架，不是用户可见输出模板。若你判断当前请求确实需要计划模式，应优先沿用这份计划的目标与步骤语义，再输出正式 `[[PLAN]]` 供用户确认；不要忽略已有步骤重新发散。
{}",
        lines.join("\n")
    ))
}

fn extract_plan_block_text(content: &str) -> Option<String> {
    let open_marker = "[[PLAN]]";
    let close_marker = "[[/PLAN]]";
    let start = content.find(open_marker)?;
    let end = content[start + open_marker.len()..].find(close_marker)?;
    let plan = &content[start + open_marker.len()..start + open_marker.len() + end];
    let trimmed = plan.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

fn structured_plan_from_text(plan_text: &str, goal: &str) -> RuntimePlanPayload {
    let now = Utc::now().to_rfc3339();
    let steps: Vec<RuntimePlanStepPayload> = plan_lines_from_text(plan_text)
        .into_iter()
        .enumerate()
        .map(|(index, line)| RuntimePlanStepPayload {
            id: format!("step_{}", index + 1),
            title: line.to_string(),
            kind: "edit".to_string(),
            description: line.to_string(),
            status: "pending".to_string(),
            tool_hints: Vec::new(),
            requires_confirmation: false,
        })
        .collect();

    RuntimePlanPayload {
        id: format!("plan_{}", Uuid::new_v4().simple()),
        goal: if goal.trim().is_empty() {
            "执行当前任务".to_string()
        } else {
            goal.trim().to_string()
        },
        summary: Some("由模型输出并等待用户确认的正式执行计划。".to_string()),
        status: "pending".to_string(),
        steps,
        created_at: now.clone(),
        updated_at: now,
    }
}


fn build_agent_execution_prompt_section(
    execution: Option<&AgentExecutionContextPayload>,
) -> Option<String> {
    let execution = execution?;
    let serialized = serde_json::to_string_pretty(execution).ok()?;
    Some(format!(
        "## 当前执行状态
以下是系统维护的结构化机器状态。它描述当前这次请求正在执行到哪里、是否仍在等待确认、是否已发生正文写入，以及最近工具调用事实。它不是聊天文本，也不是让你复述给用户看的内容；当它与自然语言历史冲突时，以它为准。
{}",
        serialized
    ))
}

fn build_last_execution_prompt_section(
    memory: Option<&AgentExecutionMemoryPayload>,
) -> Option<String> {
    let memory = memory?;
    let serialized = serde_json::to_string_pretty(memory).ok()?;
    Some(format!(
        "## 上一轮完成记录
以下是上一轮已完成执行的结构化记录。它用于回答“你刚才做了什么”“之前执行到哪里了”“你还记得吗”这类追问。若当前 `agent_execution` 为空但这里有记录，表示本轮尚未开始，不表示之前什么都没做。
{}",
        serialized
    ))
}

fn build_session_memory_prompt_section(
    memory: Option<&AgentSessionMemoryPayload>,
) -> Option<String> {
    let memory = memory?;
    let serialized = serde_json::to_string_pretty(memory).ok()?;
    Some(format!(
        "## 会话记忆
以下是系统维护的当前会话结构化记忆。它用于补充长期对话中的稳定目标、最近完成事实和仍待继续的事项。它不是执行状态真值，但比自然语言旧消息更可靠；若与较早聊天内容冲突，优先依据这里和实时上下文继续工作。
{}",
        serialized
    ))
}

fn build_protocol_prompt_section() -> String {
    let write_action_pairs = write_action_modes()
        .into_iter()
        .zip(write_action_markers())
        .map(|(mode, marker)| format!("`{}` -> `{}`", mode, marker))
        .collect::<Vec<_>>()
        .join("；");
    let write_action_payloads = write_action_payload_formats().join("\n");
    let control_phase_summary = control_phases()
        .iter()
        .map(|phase| format!("`{}`", phase))
        .collect::<Vec<_>>()
        .join(" / ");
    let control_example = serde_json::to_string_pretty(&json!({
        "phase": "completed",
        "pending_plan": false,
        "auto_continue": false,
        "needs_save": false
    }))
    .unwrap_or_else(|_| "{\"phase\":\"completed\"}".to_string());
    format!(
        "## 协议
正文动作协议：{}。
正文动作 payload 规范：
{}
控制协议：`{}` + JSON + `{}`。
控制阶段枚举：{}。
每轮响应结尾都必须输出一个控制块；最小合法示例：
{}",
        write_action_pairs,
        write_action_payloads,
        control_open_marker(),
        control_close_marker(),
        control_phase_summary,
        control_example
    )
}

fn build_context_priority_prompt_section() -> String {
    "## 上下文优先级
1. `context.agent_execution` 是当前轮机器状态，优先用于判断是否等待确认、是否继续执行、是否已写正文、最近工具是否真的发生。
2. `context.last_execution` 是上一轮已完成记录，优先用于回答“你刚才做了什么”“之前执行到哪里了”。
3. `context.session_memory` 是当前会话的结构化记忆，用于补充稳定目标、最近完成事实和待继续事项，但不替代执行状态真值。
4. 当前页面、项目、文档、编辑器未保存状态属于实时环境事实。
5. 普通消息历史只作为补充语义，不作为状态真值。
6. 若高优先级机器状态与旧 assistant 总结、计划文案或自然语言回忆冲突，以机器状态为准；不要把旧聊天中的口头承诺当成已经执行的事实。".to_string()
}

fn build_execution_rules_prompt_section() -> String {
    let control_phase_summary = control_phases()
        .iter()
        .map(|phase| format!("`{}`", phase))
        .collect::<Vec<_>>()
        .join(" / ");
    format!(
        r#"## 执行规则
1. 只有“会改变页面、项目、目录、文档或需要实际执行步骤”的执行型请求，才必须先给出完整执行计划，再等待用户确认；在用户确认之前，禁止调用任何工具、禁止写入正文、禁止保存、禁止声称已经开始执行。纯问答、解释、回忆、状态说明这类不改变状态的请求，不需要先列执行计划，应直接回答。
2. 对执行型请求，初次响应或重新规划时，必须把计划完整包在 `[[PLAN]]` 与 `[[/PLAN]]` 之间；`[[PLAN]]` 内只写计划本身，不写额外解释。给出计划的同一轮，控制块必须使用 `phase="await_user_confirmation"` 且 `pending_plan=true`。对纯问答请求，不输出 `[[PLAN]]`。
2.1 `任务分析`、`建议计划对象`、`当前执行状态`、`上一轮完成记录`、`会话记忆` 都是内部机器上下文，不是用户可见模板。禁止把这些内部对象原样回显给用户，尤其禁止直接输出 JSON / 对象字面量 / 字段列表来代替正式答复或正式 `[[PLAN]]`。
2.2 用户可见的计划只能以自然语言步骤列表写在 `[[PLAN]]...[[/PLAN]]` 中；不要把 `id`、`status`、`tool_hints`、`created_at`、`updated_at` 等内部字段展示给用户，除非用户明确要求查看原始机器状态。
3. 对同一确认点只问一次。若同时存在高风险确认与语义澄清，合并成一条确认消息一次问完；不要多轮重复追问。
4. 只有真实执行过的工具结果或正文协议写入，才算“已完成”。不要把口头说明、计划文案或未落地的承诺说成已经做完。
4.1 一旦用户已经确认你提出的执行计划，就视为对该计划内普通步骤的统一授权。除非出现新的高风险操作、真实冲突或关键缺失信息，否则执行过程中不得再次追问“是否继续”“需要我继续吗”“还要不要继续执行”“是否现在开始下一篇”等重复确认。
4.2 如果用户原请求已经明确包含保存（如“最后保存”“分别保存”“逐一保存”），这本身就是保存授权；执行过程中不得在每篇文档之间再次追问是否保存，也不得在计划尚未完成时因为某篇文档暂未保存就停下来二次确认。
5. 回答“你刚才做了什么”“你还记得吗”时，优先依据 `context.last_execution`；若其中已有工具调用、计划、正文写入记录，就不能回答成“什么都没做”。
6. 正文只能通过 assistant 文本流里的正文协议写入；工具只负责定位、创建空节点、读取、保存、重命名、导航、校验。禁止在 `create_tree_node` 里直接塞完整正文。
7. 正文协议只允许使用 `[[ACTION:append]]`、`[[ACTION:replace]]`、`[[ACTION:rewrite_section]]`、`[[ACTION:replace_block]]` 四种之一，并以 `[[/ACTION]]` 结束。标记内写入编辑器，标记外显示在聊天面板。
7.0 `append` 与 `replace` 直接输出 Markdown 正文；`rewrite_section` 必须输出 `[[TARGET]]...[[/TARGET]]` 和 `[[CONTENT]]...[[/CONTENT]]`；`replace_block` 必须输出 `[[FIND]]...[[/FIND]]` 和 `[[REPLACE]]...[[/REPLACE]]`。这些内层标签都是强制字段，不能省略、不能用自然语言解释代替。
7.1 协议闭合是强约束，不是建议。只要输出了 `[[ACTION:...]]`，就必须在同一轮输出中补上 `[[/ACTION]]`；只要输出了 `[[PLAN]]`，就必须补上 `[[/PLAN]]`；只要输出了 `[[CONTROL]]`，就必须补上 `[[/CONTROL]]`。禁止输出半截标记、禁止漏掉结束标记、禁止把结束标记省略给系统“自行理解”。
7.2 输出前先自检一遍协议完整性：检查 `ACTION / PLAN / CONTROL` 是否成对闭合、`[[CONTROL]]` 内是否是完整合法 JSON。若你已经写出开头但还没写完结尾，不要提交这一轮，先把协议补完整再结束响应。
7.3 若本轮无法产出完整闭合的协议块，就不要输出该协议块开头；宁可先输出普通说明，也不要留下半截 `[[ACTION:...]]` 或半截 `[[CONTROL]]`。
8. 选择动作时按影响范围最小化原则：局部新增优先 `append`；整篇改写或大范围重排用 `replace`；按标题整节改写用 `rewrite_section`；替换指定片段用 `replace_block`。局部改写后必须删除旧内容，最终只保留一份。
8.1 对涉及文档写入的任务，你必须在 `[[CONTROL]]` 中显式输出 `write_scope` 与 `preferred_write_action`。不要让前端或系统根据自然语言猜测写入范围。
8.2 当 `write_scope="partial"` 时，禁止使用 `[[ACTION:replace]]`；局部编辑只能使用 `append`、`rewrite_section` 或 `replace_block`。只有 `write_scope="full"` 时才允许 `replace`。
9. 若某个后续动作依赖正文已经产生，就先完成正文协议写入，再执行保存、校验、重命名或下一步；不要只在聊天区承诺“接下来处理”就停止。
9.1 聊天区可见文本只描述“本轮已经完成了什么”和“紧接着将自动处理什么”，不要先输出纯预告式文案再在下一轮重复总结。同一篇文档不要先说“下面写入《X》正文”，下一轮又说“已写入《X》正文”；如果本轮实际完成写入，应直接写“已写入《X》正文。下一步处理《Y》”。若本轮尚未真正写入，就不要提前宣称正在写入。
10. 工具调用按需、一次到位。上下文已足够时不要重复读取同一信息；若连续两次工具调用都没有新增关键信息，立即停止探测并给出当前最佳结果或一次性最小澄清。
11. 读取当前文档时：若已知存在未保存修改或用户刚改过内容，优先 `read_editor_snapshot`；否则 `read_document`。当前文档未保存时，不要只依赖 `read_document` 做续写或替换判断。
11.1 对于“改写/互换/补充正文”这类计划，`read_editor_snapshot` 或上下文里出现 `unsaved_changes=false`，只表示当前文档还没有新的未保存修改，或当前编辑器内容与已保存版本一致；这不代表任务已经完成，也不代表无需继续改写。你仍应基于返回的最新内容继续执行当前正文修改步骤。
11.2 只有在本轮真实请求了 `read_editor_snapshot` 后，才允许说“等待快照返回后继续执行”；只有在收到了对应工具结果后，才允许说“已读取当前快照”“正在根据快照继续处理”。禁止口头宣称等待或已读取快照，但实际上没有发生工具调用或尚未收到结果。
11.3 若 `read_editor_snapshot` 返回 `source="saved_document"`、`freshness="saved_fallback"`、`editor_ready=false` 或等价含义，说明这次没有拿到实时编辑器快照，只拿到了已保存正文回退版本。此时不要把它当成“最新未保存内容”；若任务依赖实时改动，应先承认当前只拿到了已保存版本，再决定是否继续读取、等待编辑器就绪或调整方案。
12. 保存默认需要用户明确授权；除非用户明确要求保存，否则生成未保存草稿，不得擅自声称“已保存”。
12.1 如果用户要求修改头像，而上下文或用户消息里已经提供了可直接使用的图片 URL，应直接调用 `update_profile` 并传 `avatar=<url>`；不要错误声称“当前无法调用头像更新工具”或要求用户必须手动进入个人设置页面上传。
12.2 只有在用户没有提供可用图片 URL、也没有可复用的现有图片附件时，才说明当前缺少头像来源；此时应明确缺的是“可用图片 URL 或可用附件”，而不是否认 `update_profile` 工具存在。
12.3 若用户要求“随便找一张图片帮我换头像”，但当前没有真实可用的图片 URL 来源，就应坦诚说明缺少可直接设置的 URL；不要把“暂时没有 URL”说成“不能更新头像”。
13. 调用 `save_current_document` 之前，必须先确认当前文档确实存在新的未保存修改，或者本轮已经实际完成正文写入。若当前只是读取、定位、检查上下文，禁止提前保存。
13.1 当 `save_current_document` 返回 `saved=false`、`already_saved=true`、`unsaved_changes_before_save=false` 或等价含义时，说明当前文档在调用前就是已保存状态，这次没有形成新的保存动作。不要把它说成“已保存成功”，也不要继续重复保存；应重新判断是否其实还没写入正文，或当前步骤根本还停留在读取/规划阶段。
13.2 若已确认计划中的当前步骤是“改写/互换/补充正文”，就必须先输出正文协议完成该步骤，再决定是否保存。禁止在 `read_editor_snapshot -> save_current_document -> read_editor_snapshot` 这类只读/只存循环里空转。
14. 每轮末尾都必须输出完整控制块 `[[CONTROL]]{{...}}[[/CONTROL]]`，且 `phase` 只能取这些值：{}。
15. 若当前正在执行一个已确认的多步骤计划，控制块还必须同步输出真实计划进度：`plan_step_index`、`plan_total_steps`、`plan_current_step`、`plan_completed_steps`。这些字段只反映已经实际完成或当前正在执行的步骤，不能把“准备做”“打算做”写成已完成。
16. 一致性要求：计划一旦已经得到用户确认并进入执行阶段，后续轮次必须把 `pending_plan=false`；只有真正再次停下来等待用户确认时，才能设为 `true`。
17. `needs_save=true` 只用于“当前必须停下来等待用户决定是否保存”的场景；如果同一用户请求还有后续步骤要继续自动执行，或者用户已经明确要求最终保存，则不要因为当前文档暂时未保存就提前输出 `needs_save=true`。
18. `phase="await_user_confirmation"` 表示等待用户确认，且 `pending_plan=true`；`phase="auto_continue"` 或 `phase="in_progress"` 表示当前请求还要继续自动推进，且 `auto_continue=true`；`phase="needs_save"` 表示正文已写入但仍需保存决策，且 `needs_save=true`；全部完成且无需继续时使用 `phase="completed"`。
19. 不要输出 `[[CONTINUE]]`，也不要依赖自然语言让前端猜状态；前端只读取控制块。
20. 对于已经确认并开始执行的计划，只有两种情况下允许输出 `phase="await_user_confirmation"`：出现新的高风险操作，或真实缺少继续所必需的信息。禁止仅因为中途切换到下一篇文档、某篇文档刚保存完成、或你想让用户“确认继续”就切回等待确认。
21. 计划确认后必须严格按计划顺序推进。若当前步骤尚未真正完成，不要跳到下一步，也不要把“准备做”“打算做”写成“已完成”。
22. 收尾时保持精简：单行动给出本次结果和必要的下一步；多行动给出已完成、未完成或受阻项，以及下一步。"#,
        control_phase_summary
    )
}

fn build_system_prompt(
    _mode: &str,
    username: &str,
    ctx: &AgentContextPayload,
    analysis: Option<&RuntimeTaskAnalysisPayload>,
    suggested_plan: Option<&RuntimePlanPayload>,
) -> String {
    let mut parts = vec![
        "你是 MarkFlow 内置的智能文档助手。请使用中文，回答直接、可执行、少废话。".to_string(),
        "如果用户要求输出 Markdown 文档内容，优先输出结构清晰的 Markdown。".to_string(),
        format!("当前登录用户：{}", username),
        build_protocol_prompt_section(),
        build_context_priority_prompt_section(),
    ];

    let mut runtime_context = vec!["## 当前页面上下文".to_string()];
    if let Some(scope) = ctx.page_scope.as_deref() {
        runtime_context.push(format!("页面作用域：{}", scope));
    }
    if let Some(page_state) = ctx.page_state.as_deref() {
        runtime_context.push(format!("页面状态：{}", page_state));
    }
    if let Some(project_name) = ctx.project_name.as_deref() {
        runtime_context.push(format!("当前项目：{}", project_name));
    }
    if let Some(doc_name) = ctx.doc_name.as_deref() {
        runtime_context.push(format!("当前文档：{}", doc_name));
    }
    if let Some(doc_id) = ctx.doc_id {
        runtime_context.push(format!("当前文档 ID：{}", doc_id));
    }
    if let Some(project_catalog) = ctx
        .project_catalog
        .as_deref()
        .filter(|value| !value.trim().is_empty())
    {
        runtime_context.push(format!("当前可见项目列表：{}", project_catalog));
    }
    if let Some(current_node_catalog) = ctx
        .current_node_catalog
        .as_deref()
        .filter(|value| !value.trim().is_empty())
    {
        runtime_context.push(format!("当前项目可见目录/文档：{}", current_node_catalog));
    }
    if let Some(editor_available) = ctx.editor_available {
        runtime_context.push(format!(
            "编辑器是否可用：{}",
            if editor_available { "是" } else { "否" }
        ));
    }
    if let Some(snapshot_source) = ctx
        .editor_snapshot_source
        .as_deref()
        .filter(|value| !value.trim().is_empty())
    {
        runtime_context.push(format!("编辑器快照来源：{}", snapshot_source));
    }
    if let Some(unsaved_changes) = ctx.editor_unsaved_changes {
        runtime_context.push(format!(
            "当前文档未保存修改：{}",
            if unsaved_changes { "是" } else { "否" }
        ));
    }
    if runtime_context.len() > 1 {
        parts.push(runtime_context.join("\n"));
    }

    if let Some(execution_section) = build_agent_execution_prompt_section(ctx.agent_execution.as_ref()) {
        parts.push(execution_section);
    }
    if let Some(last_execution_section) = build_last_execution_prompt_section(ctx.last_execution.as_ref()) {
        parts.push(last_execution_section);
    }
    if let Some(session_memory_section) = build_session_memory_prompt_section(ctx.session_memory.as_ref()) {
        parts.push(session_memory_section);
    }
    if let Some(task_analysis_section) = analysis.and_then(build_task_analysis_prompt_section) {
        parts.push(task_analysis_section);
    }
    if let Some(suggested_plan_section) = build_suggested_plan_prompt_section(suggested_plan) {
        parts.push(suggested_plan_section);
    }

    parts.push(build_execution_rules_prompt_section());

    parts.join("\n\n")
}

fn build_response_input(payload: &AgentChatStreamRequest) -> Vec<InputItem> {
    if let Some(outputs) = payload
        .tool_outputs
        .as_ref()
        .filter(|items| !items.is_empty())
    {
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
                output
                    .name
                    .as_deref()
                    .map(str::trim)
                    .filter(|value| !value.is_empty()),
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

fn function_tool(name: &str, description: impl Into<String>, parameters: serde_json::Value) -> Tool {
    FunctionTool {
        name: name.to_string(),
        description: Some(description.into()),
        parameters: Some(parameters),
        strict: Some(false),
    }
    .into()
}

fn agent_function_tools() -> Vec<Tool> {
    let route_description_summary = route_descriptions().join("；");
    let route_enum_values = route_enum_values();

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
            format!("列出当前前端应用支持的所有页面路由、每个路由的用途说明、典型使用场景和参数要求。适用于模型需要回答“你知道哪些页面”“如何跳转到某个页面”时使用。当前协议路由包括：{}。", route_description_summary),
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
                        "description": format!("目标页面路由。只有 route 明确时才使用此工具。协议路由包括：{}。", route_description_summary),
                        "enum": route_enum_values
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
            "update_profile",
            "更新当前登录用户的个人资料。当前主要用于修改或清空头像。可以直接传 avatar URL，也可以先通过 list_uploads 找到一个已有图片附件，再用 upload_id 把它设为头像；clear_avatar=true 表示清空头像。",
            json!({
                "type": "object",
                "properties": {
                    "avatar": { "type": "string", "description": "新的头像 URL。适用于用户明确给出 URL 时。" },
                    "upload_id": { "type": "integer", "description": "已有附件 ID。适用于先通过 list_uploads 找到图片附件，再把该附件 URL 设为头像。" },
                    "clear_avatar": { "type": "boolean", "description": "是否清空当前头像。true 时不要再传 avatar 或 upload_id。" }
                },
                "additionalProperties": false,
            }),
        ),
        function_tool(
            "list_uploads",
            "列出当前用户的附件，并支持按附件类型、名称关键字、是否未被引用、附件 ID 等条件筛选。适用于回答“有哪些附件”“未引用的附件有哪些”“找出某类附件”“删除附件前先确认目标”这类场景。",
            json!({
                "type": "object",
                "properties": {
                    "upload_ids": {
                        "type": "array",
                        "description": "要筛选的附件 ID 列表。适用于只查看一批已知附件。",
                        "items": { "type": "integer" }
                    },
                    "kind": {
                        "type": "string",
                        "description": "单个附件类型筛选。",
                        "enum": ["avatar", "project-background", "doc-image", "doc-file"]
                    },
                    "kinds": {
                        "type": "array",
                        "description": "多个附件类型筛选。",
                        "items": { "type": "string", "enum": ["avatar", "project-background", "doc-image", "doc-file"] }
                    },
                    "name_query": { "type": "string", "description": "按附件原始文件名做模糊搜索。" },
                    "unused_only": { "type": "boolean", "description": "是否只返回当前未被引用的附件。" },
                    "limit": { "type": "integer", "description": "限制返回条数，适合先抽样查看结果。" }
                },
                "additionalProperties": false,
            }),
        ),
        function_tool(
            "delete_uploads",
            "删除一个或多个附件，支持按附件 ID、类型、名称关键字、是否未被引用等条件筛选后批量删除。删除前应先用 list_uploads 确认目标，避免误删；删除已被引用的附件会导致相关链接失效。",
            json!({
                "type": "object",
                "properties": {
                    "upload_ids": {
                        "type": "array",
                        "description": "要删除的附件 ID 列表。批量删除时优先使用，最稳定。",
                        "items": { "type": "integer" }
                    },
                    "kind": {
                        "type": "string",
                        "description": "按单个附件类型批量删除。",
                        "enum": ["avatar", "project-background", "doc-image", "doc-file"]
                    },
                    "kinds": {
                        "type": "array",
                        "description": "按多个附件类型批量删除。",
                        "items": { "type": "string", "enum": ["avatar", "project-background", "doc-image", "doc-file"] }
                    },
                    "name_query": { "type": "string", "description": "按附件原始文件名模糊筛选后删除。" },
                    "unused_only": { "type": "boolean", "description": "是否只删除未被引用的附件。" }
                },
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
            "保存当前正在编辑的 Markdown 文档。适用于用户明确要求“保存”“提交修改”“应用更改”“确认保存”时调用。调用前应先确认当前文档确实存在新的未保存修改；若工具返回 `already_saved=true`、`unsaved_changes_before_save=false` 或等价含义，表示当前文档本来就是已保存状态，这次不需要执行保存，也不应把它说成“已保存成功”。可选传入项目和文档定位参数，工具会先打开目标文档再执行保存；如果编辑器尚未初始化完成会报错，不应在仅需生成草稿时调用。",
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
            "删除一个或多个文档或目录。适用于明确删除文档/目录的场景，支持批量删除。删除前应先通过 get_project_tree 确认目标路径和节点，避免误删；优先使用 node_ids，路径其次；如果当前项目树中按名称能唯一精确匹配，也可直接传 node_names 删除。",
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
                    },
                    "node_names": {
                        "type": "array",
                        "description": "要删除的节点名称列表。仅当当前项目内能唯一精确匹配时使用；若存在重名，应改用 node_ids 或 node_paths。",
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

    if let Some(outputs) = payload
        .tool_outputs
        .as_ref()
        .filter(|items| !items.is_empty())
    {
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

    let needs_semantic_continuation_prompt = payload
        .context
        .as_ref()
        .and_then(|ctx| ctx.agent_execution.as_ref())
        .map(|execution| execution.semantic_continuation.unwrap_or(false))
        .unwrap_or(false);
    let has_tool_outputs = payload
        .tool_outputs
        .as_ref()
        .map(|items| !items.is_empty())
        .unwrap_or(false);

    if needs_semantic_continuation_prompt && !has_tool_outputs {
        messages.push(
            ChatCompletionRequestUserMessage::from(
                "继续执行当前同一用户请求的剩余步骤。不要重复已完成内容，直接进入下一步；若仍需自动续轮，请继续按控制协议输出。"
                    .to_string(),
            )
            .into(),
        );
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
            }
            Ok(ResponseStreamEvent::ResponseOutputTextDelta(ev)) => {
                if !ev.delta.is_empty() {
                    final_text.push_str(&ev.delta);
                    if !send_json_event(tx, "message.delta", json!({ "content": ev.delta })).await {
                        return Err(AgentStreamError::Fatal("客户端连接已关闭".to_string()));
                    }
                    if !send_json_event(tx, "assistant_text_delta", json!({ "delta": ev.delta })).await {
                        return Err(AgentStreamError::Fatal("客户端连接已关闭".to_string()));
                    }
                }
            }
            Ok(ResponseStreamEvent::ResponseReasoningTextDelta(ev)) => {
                if !ev.delta.is_empty() {
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
            }
            Ok(ResponseStreamEvent::ResponseFailed(ev)) => {
                let message = ev
                    .response
                    .error
                    .as_ref()
                    .map(|error| error.message.clone())
                    .unwrap_or_else(|| "Responses 调用失败".to_string());
                return if final_text.is_empty() {
                    Err(AgentStreamError::Retryable(message))
                } else {
                    Err(AgentStreamError::Fatal(message))
                };
            }
            Ok(ResponseStreamEvent::ResponseError(ev)) => {
                return if final_text.is_empty() {
                    Err(AgentStreamError::Retryable(ev.message))
                } else {
                    Err(AgentStreamError::Fatal(ev.message))
                };
            }
            Ok(_) => {}
            Err(err) => {
                let message = format!("Responses 流式调用失败: {}", err);
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
            text: final_text,
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
                }
                for choice in chunk.choices {
                    if let Some(content) = choice.delta.content {
                        if !content.is_empty() {
                            final_text.push_str(&content);
                            if !send_json_event(tx, "message.delta", json!({ "content": content }))
                                .await
                            {
                                return Err(AgentStreamError::Fatal(
                                    "客户端连接已关闭".to_string(),
                                ));
                            }
                            if !send_json_event(tx, "assistant_text_delta", json!({ "delta": content }))
                                .await
                            {
                                return Err(AgentStreamError::Fatal(
                                    "客户端连接已关闭".to_string(),
                                ));
                            }
                        }
                    }

                    if let Some(refusal) = choice.delta.refusal {
                        if !refusal.is_empty() {
                            final_text.push_str(&refusal);
                            if !send_json_event(tx, "message.delta", json!({ "content": refusal }))
                                .await
                            {
                                return Err(AgentStreamError::Fatal(
                                    "客户端连接已关闭".to_string(),
                                ));
                            }
                            if !send_json_event(tx, "assistant_text_delta", json!({ "delta": refusal }))
                                .await
                            {
                                return Err(AgentStreamError::Fatal(
                                    "客户端连接已关闭".to_string(),
                                ));
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
                            return Ok(AgentStreamOutcome::ToolCalls {
                                response_id: if response_id.trim().is_empty() {
                                    model.to_string()
                                } else {
                                    response_id
                                },
                                text: final_text,
                                calls,
                            });
                        }
                    }
                }
            }
            Err(err) => {
                return Err(AgentStreamError::Fatal(format!(
                    "Chat Completions 流式调用失败: {}",
                    err
                )));
            }
        }
    }

    Ok(AgentStreamOutcome::Message {
        text: final_text,
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
    let has_messages = payload
        .messages
        .iter()
        .any(|message| !message.content.trim().is_empty());
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
    let transport_mode = normalize_transport_mode(payload.transport_mode.as_deref());
    let write_mode = normalize_write_mode(payload.write_mode.as_deref());
    let username = user.username.clone();
    let task_analysis = analyze_task_request(&payload, &ctx);
    let suggested_plan = build_runtime_plan(&task_analysis, &payload);
    let latest_request = latest_user_message(&payload.messages);
    let responses_prompt = build_system_prompt(&mode, &username, &ctx, Some(&task_analysis), suggested_plan.as_ref());
    let fallback_prompt = build_system_prompt(&mode, &username, &ctx, Some(&task_analysis), suggested_plan.as_ref());

    tokio::spawn(async move {
        let _ = send_json_event(
            &tx,
            "message.started",
            json!({
                "model": model,
                "mode": mode,
                "transport_mode": transport_mode,
                "write_mode": write_mode,
                "user": username,
            }),
        )
        .await;
        let _ = send_json_event(&tx, "task_analysis", serde_json::to_value(&task_analysis).unwrap_or_else(|_| json!({}))).await;
        let outcome = match transport_mode.as_str() {
            "chat" => {
                let _ = send_json_event(
                    &tx,
                    "agent.transport",
                    json!({
                        "mode": "chat",
                        "tools_available": true,
                        "requested_mode": transport_mode,
                    }),
                )
                .await;
                stream_via_chat_completions(&client, &payload, &fallback_prompt, &model, &tx)
                    .await
                    .map_err(|chat_error| match chat_error {
                        AgentStreamError::Retryable(chat_message) | AgentStreamError::Fatal(chat_message) => chat_message,
                    })
            }
            "responses" => match stream_via_responses(&client, &payload, &responses_prompt, &model, &tx)
                .await
            {
                Ok(outcome) => {
                    let _ = send_json_event(
                        &tx,
                        "agent.transport",
                        json!({
                            "mode": "responses",
                            "tools_available": true,
                            "requested_mode": transport_mode,
                        }),
                    )
                    .await;
                    Ok(outcome)
                }
                Err(AgentStreamError::Retryable(message)) | Err(AgentStreamError::Fatal(message)) => Err(message),
            },
            _ => match stream_via_responses(&client, &payload, &responses_prompt, &model, &tx)
                .await
            {
                Ok(outcome) => {
                    let _ = send_json_event(
                        &tx,
                        "agent.transport",
                        json!({
                            "mode": "responses",
                            "tools_available": true,
                            "requested_mode": transport_mode,
                        }),
                    )
                    .await;
                    Ok(outcome)
                }
                Err(AgentStreamError::Retryable(responses_error)) => {
                    let _ = send_json_event(
                        &tx,
                        "agent.transport",
                        json!({
                            "mode": "chat_fallback",
                            "tools_available": true,
                            "reason": responses_error,
                            "requested_mode": transport_mode,
                        }),
                    )
                    .await;
                    stream_via_chat_completions(&client, &payload, &fallback_prompt, &model, &tx)
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
            },
        };

        match outcome {
            Ok(AgentStreamOutcome::Message { text, response_id }) => {
                if let Some(plan_text) = extract_plan_block_text(&text) {
                    let structured_plan = structured_plan_from_text(&plan_text, &latest_request);
                    let _ = send_json_event(
                        &tx,
                        "plan_event",
                        json!({
                            "plan": structured_plan,
                            "status": "pending",
                        }),
                    )
                    .await;
                }
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
            Ok(AgentStreamOutcome::ToolCalls {
                response_id,
                text,
                calls,
            }) => {
                if let Some(plan_text) = extract_plan_block_text(&text) {
                    let structured_plan = structured_plan_from_text(&plan_text, &latest_request);
                    let _ = send_json_event(
                        &tx,
                        "plan_event",
                        json!({
                            "plan": structured_plan,
                            "status": "pending",
                        }),
                    )
                    .await;
                }
                for call in &calls {
                    let _ = send_json_event(
                        &tx,
                        "tool_event",
                        json!({
                            "tool": call.name,
                            "status": "requested",
                            "summary": format!("已请求工具 {}", call.name),
                            "call_id": call.call_id,
                        }),
                    )
                    .await;
                }
                let _ = send_json_event(
                    &tx,
                    "tool.calls.required",
                    json!({
                        "response_id": response_id,
                        "content": text,
                        "calls": calls,
                    }),
                )
                .await;
                let _ = send_json_event(&tx, "done", json!({})).await;
            }
            Err(message) => {
                let _ = send_json_event(&tx, "error_event", json!({ "scope": "runtime", "message": message.clone() })).await;
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
