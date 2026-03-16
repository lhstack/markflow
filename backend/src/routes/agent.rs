use std::{
    collections::HashMap,
    convert::Infallible,
    path::PathBuf,
    sync::{Arc, OnceLock},
    time::Duration,
};

use aes_gcm::{
    aead::{Aead, KeyInit},
    Aes256Gcm, Nonce,
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
use reqwest::Client as HttpClient;
use rig::{
    client::CompletionClient as RigCompletionClient,
    completion::{
        CompletionModel as RigCompletionModel, CompletionRequest as RigCompletionRequest,
        ToolDefinition as RigToolDefinition,
    },
    message::{
        AssistantContent as RigAssistantContent,
        Document as RigDocument, DocumentMediaType as RigDocumentMediaType,
        DocumentSourceKind as RigDocumentSourceKind, ImageMediaType as RigImageMediaType,
        ImageDetail as RigImageDetail, Message as RigMessage, MimeType as RigMimeType,
        ToolChoice as RigToolChoice, ToolResultContent as RigToolResultContent,
        UserContent as RigUserContent,
    },
    providers::{anthropic, gemini, openai},
    streaming::StreamedAssistantContent,
    OneOrMany as RigOneOrMany,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use tokio::fs;
use tokio::sync::{mpsc, oneshot, Mutex};
use tokio_stream::wrappers::ReceiverStream;
use uuid::Uuid;

use crate::{agent_protocol::{default_agent_base_url, route_descriptions, route_enum_values}, auth, db::Database, models::{AgentProvider, UploadAsset}};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AgentProviderProtocol {
    OpenAi,
    Anthropic,
    Gemini,
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
    pub provider_kind: Option<String>,
    pub base_url: Option<String>,
    pub api_key: Option<String>,
    pub remote_models: Option<Vec<String>>,
    pub enabled_models: Option<Vec<String>>,
    pub custom_models: Option<Vec<String>>,
    pub model_configs: Option<std::collections::HashMap<String, AgentProviderModelConfig>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AgentProviderModelConfig {
    #[serde(default)]
    pub modalities: Vec<String>,
    #[serde(default)]
    pub thinking: Option<bool>,
    #[serde(default)]
    pub tools_enabled: Option<bool>,
    #[serde(default)]
    pub temperature: Option<f64>,
    #[serde(default)]
    pub max_output_tokens: Option<u64>,
    #[serde(default)]
    pub top_p: Option<f64>,
    #[serde(default)]
    pub top_k: Option<u64>,
    #[serde(default)]
    pub presence_penalty: Option<f64>,
    #[serde(default)]
    pub frequency_penalty: Option<f64>,
    #[serde(default)]
    pub parallel_tool_calls: Option<bool>,
    #[serde(default)]
    pub reasoning_effort: Option<String>,
    #[serde(default)]
    pub stop_sequences: Vec<String>,
    #[serde(default)]
    pub response_mime_type: Option<String>,
    #[serde(default)]
    pub additional_params: Option<Value>,
}

#[derive(Debug, Serialize)]
pub struct AgentProviderSummary {
    pub id: i64,
    pub name: String,
    pub provider_kind: String,
    pub base_url: String,
    pub remote_models: Vec<String>,
    pub enabled_models: Vec<String>,
    pub custom_models: Vec<String>,
    pub model_configs: std::collections::HashMap<String, AgentProviderModelConfig>,
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
    pub provider_kind: String,
    pub base_url: String,
    pub api_key: String,
    pub remote_models: Vec<String>,
    pub enabled_models: Vec<String>,
    pub custom_models: Vec<String>,
    pub model_configs: std::collections::HashMap<String, AgentProviderModelConfig>,
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
    #[serde(default)]
    pub attachments: Vec<AgentMessageAttachmentPayload>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct AgentMessageAttachmentPayload {
    pub upload_id: i64,
    pub kind: Option<String>,
    pub content_type: Option<String>,
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
    #[serde(default)]
    pub agent_execution: Option<AgentExecutionContextPayload>,
    #[serde(default)]
    pub last_execution: Option<AgentExecutionMemoryPayload>,
    #[serde(default)]
    pub session_memory: Option<AgentSessionMemoryPayload>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct AgentChatStreamRequest {
    pub provider: AgentProviderPayload,
    pub messages: Vec<AgentMessagePayload>,
    pub transport_mode: Option<String>,
    pub context: Option<AgentContextPayload>,
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

#[allow(dead_code)]
#[derive(Debug, Deserialize, Clone, Default)]
pub struct AgentExecutionToolCallSummaryPayload {
    pub name: Option<String>,
    pub arguments: Option<String>,
    pub output: Option<String>,
    pub ok: Option<bool>,
    pub outcome: Option<String>,
    pub stage_policy: Option<String>,
    #[serde(default)]
    pub capabilities: Vec<String>,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize, Clone, Default)]
pub struct AgentExecutionContextPayload {
    pub current_mode: Option<String>,
    pub awaiting: Option<String>,
    pub current_action_kind: Option<String>,
    pub current_action_status: Option<String>,
    pub current_action_mode: Option<String>,
    pub current_action_target: Option<String>,
    pub confirmation_required: Option<bool>,
    pub pending_plan: Option<String>,
    pub pending_plan_user_reply: Option<String>,
    pub plan_confirmation_decision: Option<String>,
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
    #[serde(default)]
    pub plan_completed_steps: Vec<String>,
    pub document_write_observed: Option<bool>,
    pub save_attempt_without_document_change: Option<bool>,
    pub last_intercept_code: Option<String>,
    pub last_intercept_message: Option<String>,
    pub last_intercept_guidance: Option<String>,
    #[serde(default)]
    pub recent_tool_calls: Vec<AgentExecutionToolCallSummaryPayload>,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize, Clone, Default)]
pub struct AgentExecutionMemoryPayload {
    pub current_mode: Option<String>,
    pub awaiting: Option<String>,
    pub current_action_kind: Option<String>,
    pub current_action_status: Option<String>,
    pub current_action_mode: Option<String>,
    pub current_action_target: Option<String>,
    pub confirmation_required: Option<bool>,
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
    #[serde(default)]
    pub plan_completed_steps: Vec<String>,
    pub document_write_observed: Option<bool>,
    pub save_attempt_without_document_change: Option<bool>,
    pub last_intercept_code: Option<String>,
    pub last_intercept_message: Option<String>,
    pub last_intercept_guidance: Option<String>,
    #[serde(default)]
    pub recent_tool_calls: Vec<AgentExecutionToolCallSummaryPayload>,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize, Clone, Default)]
pub struct AgentSessionMemoryPayload {
    pub summary: Option<String>,
    #[serde(default)]
    pub active_user_goals: Vec<String>,
    #[serde(default)]
    pub completed_facts: Vec<String>,
    #[serde(default)]
    pub open_loops: Vec<String>,
    pub updated_at: Option<String>,
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

#[derive(Clone, Default)]
struct FrontendToolBroker {
    pending: Arc<Mutex<HashMap<String, oneshot::Sender<Value>>>>,
}

impl FrontendToolBroker {
    async fn register(&self, run_id: &str, call_id: &str) -> oneshot::Receiver<Value> {
        let (tx, rx) = oneshot::channel();
        self.pending
            .lock()
            .await
            .insert(format!("{}:{}", run_id, call_id), tx);
        rx
    }

    async fn resolve(&self, run_id: &str, call_id: &str, output: Value) -> bool {
        let key = format!("{}:{}", run_id, call_id);
        let sender = self.pending.lock().await.remove(&key);
        sender.map(|tx| tx.send(output).is_ok()).unwrap_or(false)
    }

    async fn cancel_run(&self, run_id: &str) {
        let prefix = format!("{}:", run_id);
        self.pending
            .lock()
            .await
            .retain(|key, _| !key.starts_with(&prefix));
    }
}

fn frontend_tool_broker() -> &'static FrontendToolBroker {
    static BROKER: OnceLock<FrontendToolBroker> = OnceLock::new();
    BROKER.get_or_init(FrontendToolBroker::default)
}

#[derive(Debug, Clone)]
struct PendingFrontendToolCall {
    id: String,
    call_id: String,
    name: String,
    arguments: Value,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CompletedActionBlock {
    mode: String,
    body: String,
}

const ACTION_PROTOCOL_TOOL_NAME: &str = "action_protocol_write";

fn normalize_base_url(value: Option<&str>) -> String {
    match value.map(str::trim).filter(|v| !v.is_empty()) {
        Some(url) => url.trim_end_matches('/').to_string(),
        None => default_agent_base_url().to_string(),
    }
}

fn normalize_provider_kind(value: Option<&str>) -> String {
    match value.map(str::trim).filter(|v| !v.is_empty()) {
        Some(value) if value.eq_ignore_ascii_case("anthropic") || value.eq_ignore_ascii_case("claude") => {
            "anthropic".to_string()
        }
        Some(value) if value.eq_ignore_ascii_case("gemini") || value.eq_ignore_ascii_case("google") => {
            "gemini".to_string()
        }
        _ => "openai".to_string(),
    }
}

fn provider_protocol_from_kind(kind: &str) -> Option<AgentProviderProtocol> {
    match kind.trim().to_ascii_lowercase().as_str() {
        "anthropic" | "claude" => Some(AgentProviderProtocol::Anthropic),
        "gemini" | "google" => Some(AgentProviderProtocol::Gemini),
        "openai" => Some(AgentProviderProtocol::OpenAi),
        _ => None,
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

fn normalize_modalities(values: Vec<String>) -> Vec<String> {
    unique_strings(
        values
            .into_iter()
            .map(|item| item.trim().to_ascii_lowercase())
            .collect(),
    )
}

fn normalize_reasoning_effort(value: Option<&str>) -> Option<String> {
    match value.map(str::trim).filter(|item| !item.is_empty()) {
        Some(value)
            if matches!(
                value.to_ascii_lowercase().as_str(),
                "minimal" | "low" | "medium" | "high"
            ) =>
        {
            Some(value.to_ascii_lowercase())
        }
        _ => None,
    }
}

fn normalize_model_config(mut config: AgentProviderModelConfig) -> Option<AgentProviderModelConfig> {
    config.modalities = normalize_modalities(config.modalities);
    config.stop_sequences = unique_strings(config.stop_sequences);
    config.reasoning_effort = normalize_reasoning_effort(config.reasoning_effort.as_deref());
    if matches!(config.additional_params, Some(Value::Null)) {
        config.additional_params = None;
    }

    let is_empty = config.modalities.is_empty()
        && config.thinking.is_none()
        && config.tools_enabled.is_none()
        && config.temperature.is_none()
        && config.max_output_tokens.is_none()
        && config.top_p.is_none()
        && config.top_k.is_none()
        && config.presence_penalty.is_none()
        && config.frequency_penalty.is_none()
        && config.parallel_tool_calls.is_none()
        && config.reasoning_effort.is_none()
        && config.stop_sequences.is_empty()
        && config
            .response_mime_type
            .as_ref()
            .map(|item| item.trim().is_empty())
            .unwrap_or(true)
        && config.additional_params.is_none();

    if is_empty {
        None
    } else {
        if let Some(mime) = config.response_mime_type.as_mut() {
            *mime = mime.trim().to_string();
        }
        Some(config)
    }
}

fn parse_model_configs(raw: &str) -> std::collections::HashMap<String, AgentProviderModelConfig> {
    serde_json::from_str::<std::collections::HashMap<String, AgentProviderModelConfig>>(raw)
        .unwrap_or_default()
        .into_iter()
        .filter_map(|(model, config)| {
            let key = model.trim().to_string();
            if key.is_empty() {
                return None;
            }
            normalize_model_config(config).map(|config| (key, config))
        })
        .collect()
}

fn serialize_model_configs(
    values: &std::collections::HashMap<String, AgentProviderModelConfig>,
) -> String {
    let normalized = values
        .iter()
        .filter_map(|(model, config)| {
            let key = model.trim().to_string();
            if key.is_empty() {
                return None;
            }
            normalize_model_config(config.clone()).map(|config| (key, config))
        })
        .collect::<std::collections::BTreeMap<_, _>>();

    serde_json::to_string(&normalized).unwrap_or_else(|_| "{}".to_string())
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
        provider_kind: normalize_provider_kind(Some(&provider.provider_kind)),
        base_url: provider.base_url,
        remote_models: parse_json_string_array(&provider.remote_models),
        enabled_models: parse_json_string_array(&provider.enabled_models),
        custom_models: parse_json_string_array(&provider.custom_models),
        model_configs: parse_model_configs(&provider.model_configs),
        is_active: provider.is_active == 1,
        has_api_key: !provider.api_key_ciphertext.trim().is_empty(),
        created_at: provider.created_at,
        updated_at: provider.updated_at,
    }
}

async fn list_user_providers(db: &Database, user_id: i64) -> Result<Vec<AgentProvider>, Response> {
    sqlx::query_as::<_, AgentProvider>(
        "SELECT * FROM agent_providers WHERE user_id = ? ORDER BY updated_at DESC, id DESC",
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
    sqlx::query("UPDATE agent_providers SET is_active = CASE WHEN id = ? THEN 1 ELSE 0 END WHERE user_id = ?")
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

fn normalize_transport_mode(value: Option<&str>) -> String {
    match value.map(str::trim).filter(|v| !v.is_empty()) {
        Some("responses") => "responses".to_string(),
        Some("chat") => "chat".to_string(),
        _ => "auto".to_string(),
    }
}

fn resolve_openai_transport_mode(requested: Option<&str>, base_url: &str) -> &'static str {
    let normalized = base_url.trim_end_matches('/').to_ascii_lowercase();
    let is_official_openai = normalized == default_agent_base_url()
        || normalized == "https://api.openai.com/v1"
        || normalized.starts_with("https://api.openai.com/");

    if !is_official_openai {
        return "chat";
    }

    match normalize_transport_mode(requested).as_str() {
        "chat" => "chat",
        _ => "responses",
    }
}

fn detect_provider_protocol(provider: &AgentProvider, model: &str) -> AgentProviderProtocol {
    if let Some(protocol) = provider_protocol_from_kind(provider.provider_kind.as_str()) {
        return protocol;
    }

    let provider_hint = format!("{} {}", provider.name, provider.base_url).to_lowercase();
    let model_hint = model.to_lowercase();

    if provider_hint.contains("anthropic")
        || provider_hint.contains("claude")
        || (provider_hint.is_empty() && model_hint.starts_with("claude"))
    {
        AgentProviderProtocol::Anthropic
    } else if provider_hint.contains("google")
        || provider_hint.contains("gemini")
        || provider_hint.contains("generativelanguage")
        || (provider_hint.is_empty() && model_hint.starts_with("gemini"))
    {
        AgentProviderProtocol::Gemini
    } else {
        AgentProviderProtocol::OpenAi
    }
}

fn normalize_provider_base_url(protocol: AgentProviderProtocol, value: Option<&str>) -> String {
    match protocol {
        AgentProviderProtocol::OpenAi => normalize_base_url(value),
        AgentProviderProtocol::Anthropic => value
            .map(str::trim)
            .filter(|item| !item.is_empty())
            .unwrap_or("https://api.anthropic.com")
            .to_string(),
        AgentProviderProtocol::Gemini => value
            .map(str::trim)
            .filter(|item| !item.is_empty())
            .unwrap_or("https://generativelanguage.googleapis.com")
            .to_string(),
    }
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

fn detect_completed_action_block(text: &str) -> Option<CompletedActionBlock> {
    let upper = text.to_uppercase();
    let start = upper.find("[[ACTION:")?;
    let open_end = upper[start..].find("]]")? + start;
    let close_start = upper[open_end + 2..].find("[[/ACTION]]")? + open_end + 2;
    let mode = text[start + "[[ACTION:".len()..open_end].trim().to_ascii_lowercase();
    if mode.is_empty() {
        return None;
    }
    let body = text[open_end + 2..close_start].to_string();
    Some(CompletedActionBlock { mode, body })
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

fn append_action_tool_result_history(
    messages: &mut Vec<RigMessage>,
    mode: &str,
    output: Value,
) -> Result<(), String> {
    let tool_id = format!("action_protocol_write_{}", Uuid::new_v4().simple());
    let arguments = json!({ "mode": mode });
    let assistant_message = RigOneOrMany::many(vec![RigAssistantContent::tool_call_with_call_id(
        tool_id.clone(),
        tool_id.clone(),
        ACTION_PROTOCOL_TOOL_NAME.to_string(),
        arguments,
    )])
    .map_err(|_| "恢复 ACTION 工具结果历史时缺少 assistant 内容".to_string())?;
    messages.push(RigMessage::Assistant {
        id: None,
        content: assistant_message,
    });

    let output_text = serde_json::to_string(&output)
        .map_err(|err| format!("序列化 ACTION 工具结果失败: {err}"))?;
    messages.push(tool_result_user_message(
        &tool_id,
        Some(tool_id.as_str()),
        &output_text,
    ));

    Ok(())
}

fn build_action_protocol_tool_call(current_turns: usize, mode: &str) -> PendingFrontendToolCall {
    let id = format!("action_protocol_write_{}", current_turns);
    PendingFrontendToolCall {
        id: id.clone(),
        call_id: id,
        name: ACTION_PROTOCOL_TOOL_NAME.to_string(),
        arguments: json!({
            "mode": mode,
        }),
    }
}

fn find_model_config(
    provider: &AgentProvider,
    model: &str,
) -> Option<AgentProviderModelConfig> {
    parse_model_configs(&provider.model_configs)
        .remove(model.trim())
        .and_then(normalize_model_config)
}

fn merge_json_values(base: Option<Value>, extra: Option<Value>) -> Option<Value> {
    match (base, extra) {
        (Some(Value::Object(mut left)), Some(Value::Object(right))) => {
            for (key, value) in right {
                left.insert(key, value);
            }
            Some(Value::Object(left))
        }
        (Some(base), Some(extra)) => Some(match (base, extra) {
            (Value::Null, value) => value,
            (value, Value::Null) => value,
            (_, value) => value,
        }),
        (Some(value), None) | (None, Some(value)) => Some(value),
        (None, None) => None,
    }
}

fn function_tool(
    name: &str,
    description: impl Into<String>,
    parameters: serde_json::Value,
) -> RigToolDefinition {
    RigToolDefinition {
        name: name.to_string(),
        description: description.into(),
        parameters,
    }
}

fn agent_function_tools() -> Vec<RigToolDefinition> {
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
            "rewrite_document_section",
            "按章节标题替换当前 Markdown 文档中的整节内容。适用于“重写某一节”“替换某个标题下的整段正文”“局部编辑但不想整篇 replace”这类场景。调用前应先通过 read_editor_snapshot 或 read_document 确认当前文档和目标标题；content 需要提供替换后的完整章节 Markdown，通常应包含章节标题本身。可选传入项目和文档定位参数，工具会先打开目标文档再执行替换。",
            json!({
                "type": "object",
                "properties": {
                    "project_id": { "type": "integer", "description": "目标项目 ID。优先使用。" },
                    "project_name": { "type": "string", "description": "目标项目名称。只有拿不到 project_id 时再使用。" },
                    "doc_id": { "type": "integer", "description": "目标文档 ID。优先使用。" },
                    "doc_path": { "type": "string", "description": "目标文档路径。只有拿不到 doc_id 时再使用。" },
                    "doc_name": { "type": "string", "description": "目标文档名称。只有拿不到 doc_id 和 doc_path 时再使用。" },
                    "target_heading": { "type": "string", "description": "要替换的章节标题，例如 ## 结论 或 结论。" },
                    "content": { "type": "string", "description": "替换后的完整章节 Markdown，通常包含新的标题和正文。" }
                },
                "required": ["target_heading", "content"],
                "additionalProperties": false,
            }),
        ),
        function_tool(
            "replace_document_block",
            "按原文片段精确替换当前 Markdown 文档中的一段内容。适用于“把某段文字替换成新版”“互换两个相邻片段时先做精确局部替换”“需要保留其余正文不变”这类场景。调用前应先通过 read_editor_snapshot 或 read_document 读取最新正文，并确保 find 参数与原文完全匹配；否则替换不会成功。可选传入项目和文档定位参数，工具会先打开目标文档再执行替换。",
            json!({
                "type": "object",
                "properties": {
                    "project_id": { "type": "integer", "description": "目标项目 ID。优先使用。" },
                    "project_name": { "type": "string", "description": "目标项目名称。只有拿不到 project_id 时再使用。" },
                    "doc_id": { "type": "integer", "description": "目标文档 ID。优先使用。" },
                    "doc_path": { "type": "string", "description": "目标文档路径。只有拿不到 doc_id 时再使用。" },
                    "doc_name": { "type": "string", "description": "目标文档名称。只有拿不到 doc_id 和 doc_path 时再使用。" },
                    "find": { "type": "string", "description": "要匹配的原文片段，必须与当前文档中的真实内容完全一致。" },
                    "replace": { "type": "string", "description": "替换后的新片段。" }
                },
                "required": ["find", "replace"],
                "additionalProperties": false,
            }),
        ),
        function_tool(
            "replace_document_blocks",
            "在一次调用中顺序执行多段精确块替换，并只对编辑器做一次最终写入。适用于“同一轮需要做多处局部替换”“多个 replace_document_block 想合并成一次操作”“一次性完成一组相关块替换”这类场景。调用前应先通过 read_editor_snapshot 或 read_document 读取最新正文；replacements 中每一项都需要提供完整匹配的 find 和对应 replace，后续项会基于前一项已经替换后的正文继续执行。",
            json!({
                "type": "object",
                "properties": {
                    "project_id": { "type": "integer", "description": "目标项目 ID。优先使用。" },
                    "project_name": { "type": "string", "description": "目标项目名称。只有拿不到 project_id 时再使用。" },
                    "doc_id": { "type": "integer", "description": "目标文档 ID。优先使用。" },
                    "doc_path": { "type": "string", "description": "目标文档路径。只有拿不到 doc_id 时再使用。" },
                    "doc_name": { "type": "string", "description": "目标文档名称。只有拿不到 doc_id 和 doc_path 时再使用。" },
                    "replacements": {
                        "type": "array",
                        "description": "按顺序执行的局部替换列表。",
                        "items": {
                            "type": "object",
                            "properties": {
                                "find": { "type": "string", "description": "要匹配的原文片段，必须与当前执行时的正文完全一致。" },
                                "replace": { "type": "string", "description": "替换后的新片段。" }
                            },
                            "required": ["find", "replace"],
                            "additionalProperties": false
                        }
                    }
                },
                "required": ["replacements"],
                "additionalProperties": false,
            }),
        ),
        function_tool(
            "swap_document_sections",
            "按章节标题直接互换当前 Markdown 文档中的两个整节位置。适用于“把结论和重要发现互换一下”“交换两个章节顺序”“整节对调且保留各自标题与正文”这类场景。即使两个标题位于不同的 Markdown 层级，也可以使用该工具；它会整体移动两节内容并保留原有标题层级，而不是只改写标题下的正文。调用前应先通过 read_editor_snapshot 或 read_document 确认两个章节标题真实存在。可选传入项目和文档定位参数，工具会先打开目标文档再执行互换。",
            json!({
                "type": "object",
                "properties": {
                    "project_id": { "type": "integer", "description": "目标项目 ID。优先使用。" },
                    "project_name": { "type": "string", "description": "目标项目名称。只有拿不到 project_id 时再使用。" },
                    "doc_id": { "type": "integer", "description": "目标文档 ID。优先使用。" },
                    "doc_path": { "type": "string", "description": "目标文档路径。只有拿不到 doc_id 时再使用。" },
                    "doc_name": { "type": "string", "description": "目标文档名称。只有拿不到 doc_id 和 doc_path 时再使用。" },
                    "first_heading": { "type": "string", "description": "第一个要互换的章节标题，例如 结论。" },
                    "second_heading": { "type": "string", "description": "第二个要互换的章节标题，例如 重要发现与意义。" }
                },
                "required": ["first_heading", "second_heading"],
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

async fn send_json_event(
    tx: &mpsc::Sender<Result<Event, Infallible>>,
    name: &str,
    payload: serde_json::Value,
) -> bool {
    tx.send(Ok(Event::default().event(name).data(payload.to_string())))
        .await
        .is_ok()
}

fn merge_reasoning_blocks(
    accumulated_reasoning: &mut Vec<rig::message::Reasoning>,
    incoming: &rig::message::Reasoning,
) {
    let ids_match = |existing: &rig::message::Reasoning| {
        matches!(
            (&existing.id, &incoming.id),
            (Some(existing_id), Some(incoming_id)) if existing_id == incoming_id
        )
    };

    if let Some(existing) = accumulated_reasoning
        .iter_mut()
        .rev()
        .find(|existing| ids_match(existing))
    {
        existing.content.extend(incoming.content.clone());
    } else {
        accumulated_reasoning.push(incoming.clone());
    }
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

fn extend_loop_visible_text(accumulated: &mut String, turn_text: &str) {
    let visible_text = strip_completed_action_blocks(turn_text);
    if visible_text.is_empty() {
        return;
    }

    accumulated.push_str(&visible_text);
}

fn strip_completed_action_blocks(text: &str) -> String {
    let mut remaining = text;
    let mut stripped = String::new();

    loop {
        let upper = remaining.to_ascii_uppercase();
        let Some(start) = upper.find("[[ACTION:") else {
            stripped.push_str(remaining);
            break;
        };

        stripped.push_str(&remaining[..start]);
        let after_start = &remaining[start..];
        let after_start_upper = &upper[start..];
        let Some(close_relative) = after_start_upper.find("[[/ACTION]]") else {
            stripped.push_str(after_start);
            break;
        };
        let close_end = start + close_relative + "[[/ACTION]]".len();
        remaining = &remaining[close_end..];
    }

    stripped
}

fn summarize_frontend_tool_output(call: &PendingFrontendToolCall, output: &Value) -> String {
    let ok = output.get("ok").and_then(Value::as_bool);
    let result = output.get("result").and_then(Value::as_object);

    if call.name == ACTION_PROTOCOL_TOOL_NAME {
        let doc_name = result
            .and_then(|result| result.get("doc_name"))
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty());
        let wrote_document = result
            .and_then(|result| result.get("wrote_document"))
            .and_then(Value::as_bool)
            == Some(true);
        let write_completed = result
            .and_then(|result| result.get("write_completed"))
            .and_then(Value::as_bool)
            == Some(true);

        return match ok {
            Some(true) if wrote_document || write_completed => doc_name
                .map(|name| format!("已写入《{}》正文", name))
                .unwrap_or_else(|| "已完成正文写入".to_string()),
            Some(false) => "正文写入失败".to_string(),
            _ => "正文写入已返回结果".to_string(),
        };
    }

    match ok {
        Some(true) => format!("前端工具 {} 执行完成", call.name),
        Some(false) => format!("前端工具 {} 返回错误", call.name),
        None => format!("前端工具 {} 已返回结果", call.name),
    }
}

fn with_frontend_tool_summary(mut output: Value, summary: &str) -> Value {
    if summary.trim().is_empty() {
        return output;
    }

    if let Some(object) = output.as_object_mut() {
        object
            .entry("summary".to_string())
            .or_insert_with(|| Value::String(summary.to_string()));
    }

    output
}

async fn request_frontend_tool_output(
    tx: &mpsc::Sender<Result<Event, Infallible>>,
    run_id: &str,
    call: &PendingFrontendToolCall,
) -> Result<Value, String> {
    let receiver = frontend_tool_broker()
        .register(run_id, &call.call_id)
        .await;

    if !send_json_event(
        tx,
        "tool.request",
        json!({
            "run_id": run_id,
            "call_id": call.call_id,
            "name": call.name,
            "arguments": call.arguments,
        }),
    )
    .await
    {
        return Err("前端工具请求通道已关闭".to_string());
    }

    let _ = send_json_event(
        tx,
        "tool_event",
        json!({
            "tool": call.name,
            "status": "requested",
            "summary": format!("正在请求前端工具 {}", call.name),
            "call_id": call.call_id,
            "arguments": call.arguments,
        }),
    )
    .await;

    let raw_output = match tokio::time::timeout(Duration::from_secs(120), receiver).await {
        Ok(Ok(output)) => output,
        Ok(Err(_)) => return Err(format!("前端工具 {} 的结果通道已中断", call.name)),
        Err(_) => return Err(format!("等待前端工具 {} 超时", call.name)),
    };

    let summary = summarize_frontend_tool_output(call, &raw_output);
    let output = with_frontend_tool_summary(raw_output, &summary);
    let _ = send_json_event(
        tx,
        "tool_event",
        json!({
            "tool": call.name,
            "status": if output.get("ok").and_then(Value::as_bool) == Some(true) {
                "completed"
            } else {
                "failed"
            },
            "summary": summary,
            "call_id": call.call_id,
            "arguments": call.arguments,
            "output": output,
        }),
    )
    .await;

    Ok(output)
}

fn split_agent_prompt(messages: Vec<RigMessage>) -> (RigMessage, Vec<RigMessage>) {
    if let Some((prompt, history)) = messages.split_last() {
        match prompt {
            RigMessage::User { .. } => (prompt.clone(), history.to_vec()),
            _ => (RigMessage::user("继续"), messages),
        }
    } else {
        (RigMessage::user("继续"), Vec::new())
    }
}

fn non_empty_trimmed(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
}

fn summarize_recent_tool_calls(tool_calls: &[AgentExecutionToolCallSummaryPayload]) -> Vec<String> {
    tool_calls
        .iter()
        .filter_map(|call| {
            let name = non_empty_trimmed(call.name.as_deref())?;
            let outcome = non_empty_trimmed(call.outcome.as_deref())
                .unwrap_or_else(|| match call.ok {
                    Some(true) => "success".to_string(),
                    Some(false) => "error".to_string(),
                    None => "unknown".to_string(),
                });
            Some(format!("{} ({})", name, outcome))
        })
        .take(4)
        .collect()
}

fn summarize_tool_outputs(tool_outputs: &[AgentToolOutputPayload]) -> Vec<String> {
    tool_outputs
        .iter()
        .filter_map(|tool_output| {
            let tool_name = non_empty_trimmed(tool_output.name.as_deref())?;
            let payload = tool_output.output.as_object()?;
            let ok = payload.get("ok").and_then(Value::as_bool);
            let result = payload.get("result").and_then(Value::as_object);
            let wrote_document = result
                .and_then(|result| result.get("wrote_document"))
                .and_then(Value::as_bool)
                == Some(true);
            let write_completed = result
                .and_then(|result| result.get("write_completed"))
                .and_then(Value::as_bool)
                == Some(true);
            let saved = result
                .and_then(|result| result.get("saved"))
                .and_then(Value::as_bool)
                == Some(true);
            let already_saved = result
                .and_then(|result| result.get("already_saved"))
                .and_then(Value::as_bool)
                == Some(true);
            let outcome = match ok {
                Some(true) if wrote_document || write_completed => "成功，已完成正文写入",
                Some(true) if saved => "成功，已保存当前文档",
                Some(true) if already_saved => "成功，文档本来就是已保存状态",
                Some(true) => "成功",
                Some(false) => "失败",
                None => "已返回结果",
            };
            Some(format!("{}: {}", tool_name, outcome))
        })
        .take(4)
        .collect()
}

fn build_execution_context_prompt_section(ctx: &AgentContextPayload) -> Option<String> {
    let execution = ctx.agent_execution.as_ref()?;
    let mut lines = Vec::new();

    if let Some(mode) = non_empty_trimmed(execution.current_mode.as_deref()) {
        lines.push(format!("当前模式: {}", mode));
    }
    if let Some(awaiting) = non_empty_trimmed(execution.awaiting.as_deref()) {
        lines.push(format!("当前等待状态: {}", awaiting));
    }
    if let Some(summary) = non_empty_trimmed(execution.previous_assistant_summary.as_deref()) {
        lines.push(format!("上一轮助手摘要: {}", summary));
    }
    if let Some(task_kind) = non_empty_trimmed(execution.task_kind.as_deref()) {
        lines.push(format!("任务类型: {}", task_kind));
    }
    if let Some(edit_intent) = non_empty_trimmed(execution.edit_intent.as_deref()) {
        lines.push(format!("编辑意图: {}", edit_intent));
    }
    if let Some(step) = non_empty_trimmed(execution.plan_current_step.as_deref()) {
        match (execution.plan_step_index, execution.plan_total_steps) {
            (Some(index), Some(total)) => {
                lines.push(format!("当前计划步骤: 第 {}/{} 步 - {}", index, total, step));
            }
            _ => lines.push(format!("当前计划步骤: {}", step)),
        }
    }
    if !execution.plan_completed_steps.is_empty() {
        lines.push(format!(
            "已完成计划步骤: {}",
            execution.plan_completed_steps.join("；")
        ));
    }
    if execution.write_completed == Some(true) || execution.document_write_observed == Some(true) {
        lines.push("本轮之前已经完成正文写入，不要把文档写入步骤当成未执行。".to_string());
    }
    if execution.save_requested == Some(true) {
        lines.push("当前仍需保存文档。".to_string());
    }
    let recent_tools = summarize_recent_tool_calls(&execution.recent_tool_calls);
    if !recent_tools.is_empty() {
        lines.push(format!("最近工具调用: {}", recent_tools.join("；")));
    }

    if lines.is_empty() {
        None
    } else {
        Some(format!("## 当前执行状态\n{}", lines.join("\n")))
    }
}

fn build_execution_memory_prompt_section(ctx: &AgentContextPayload) -> Option<String> {
    let memory = ctx.last_execution.as_ref()?;
    let mut lines = Vec::new();

    if let Some(summary) = non_empty_trimmed(memory.assistant_summary.as_deref()) {
        lines.push(format!("上一轮完成摘要: {}", summary));
    }
    if let Some(plan) = non_empty_trimmed(memory.plan.as_deref()) {
        lines.push(format!("上一轮计划: {}", plan));
    }
    if let Some(step) = non_empty_trimmed(memory.plan_current_step.as_deref()) {
        lines.push(format!("上一轮所处步骤: {}", step));
    }
    if !memory.plan_completed_steps.is_empty() {
        lines.push(format!(
            "上一轮已完成步骤: {}",
            memory.plan_completed_steps.join("；")
        ));
    }
    if memory.write_completed == Some(true) || memory.document_write_observed == Some(true) {
        lines.push("上一轮已经确认发生正文写入。".to_string());
    }
    let recent_tools = summarize_recent_tool_calls(&memory.recent_tool_calls);
    if !recent_tools.is_empty() {
        lines.push(format!("上一轮工具结果摘要: {}", recent_tools.join("；")));
    }

    if lines.is_empty() {
        None
    } else {
        Some(format!("## 上一轮执行记忆\n{}", lines.join("\n")))
    }
}

fn build_session_memory_prompt_section(ctx: &AgentContextPayload) -> Option<String> {
    let memory = ctx.session_memory.as_ref()?;
    let mut lines = Vec::new();

    if let Some(summary) = non_empty_trimmed(memory.summary.as_deref()) {
        lines.push(format!("会话摘要: {}", summary));
    }
    if !memory.active_user_goals.is_empty() {
        lines.push(format!(
            "当前用户目标: {}",
            memory.active_user_goals.join("；")
        ));
    }
    if !memory.completed_facts.is_empty() {
        lines.push(format!(
            "已完成事实: {}",
            memory.completed_facts.join("；")
        ));
    }
    if !memory.open_loops.is_empty() {
        lines.push(format!("尚未完成事项: {}", memory.open_loops.join("；")));
    }

    if lines.is_empty() {
        None
    } else {
        Some(format!("## 会话记忆\n{}", lines.join("\n")))
    }
}

fn build_tool_outputs_prompt_section(tool_outputs: &[AgentToolOutputPayload]) -> Option<String> {
    let summaries = summarize_tool_outputs(tool_outputs);
    if summaries.is_empty() {
        None
    } else {
        Some(format!(
            "## 最近一轮工具执行回执\n{}",
            summaries.join("\n")
        ))
    }
}

fn build_loop_system_prompt(
    username: &str,
    ctx: &AgentContextPayload,
    tool_outputs: &[AgentToolOutputPayload],
) -> String {
    let mut lines = vec![
        "你是 MarkFlow 内置的工作区智能助手。".to_string(),
        "默认使用用户最新一条消息的语言回复；只有在用户明确要求时才切换到其他语言。".to_string(),
        "回答要简洁、基于事实，并以推进任务为目标。".to_string(),
        "当你需要当前工作区状态、文档内容、页面导航或执行修改时，应主动使用工具。".to_string(),
        "先分析用户意图，判断当前任务是简单操作还是复杂操作。".to_string(),
        "如果任务包含多个相互依赖的步骤、删除或覆盖等破坏性操作、跨项目或跨文档修改、创建加移动加写入这类组合流程，或任何理应先让用户确认的执行方案，都视为复杂操作。".to_string(),
        "对于复杂操作，先给出简洁的执行计划，并等待用户确认；在确认之前不要执行任意修改数据的操作操作。".to_string(),
        "对于简单操作，如果下一步明显且风险较低，你可以自行决定并直接执行。".to_string(),
        "对于文档写作,如空白文档编写,重新编写文档,或者文档末尾追加内容，使用 ACTION 协议，而不是整篇写入工具。".to_string(),
        "当需要向空文档写入首稿，或需要把内容追加到当前文档末尾时，一律使用 [[ACTION:append]]...[[/ACTION]]。空文档首稿也属于追加写入。".to_string(),
        "只有在确实需要用一份新的完整正文整体替换当前整篇文档时，才使用 [[ACTION:replace]]...[[/ACTION]]。".to_string(),
        "输出 ACTION 块时，只输出该次写入对应的动作块内容，必须正确闭合标记，并且在动作块结束前不要请求后续工具。".to_string(),
        format!("当前登录用户: {}", username),
        "## 页面上下文".to_string(),
    ];

    if let Some(scope) = ctx.page_scope.as_deref() {
        lines.push(format!("页面范围: {}", scope));
    }
    if let Some(page_state) = ctx.page_state.as_deref() {
        lines.push(format!("页面状态: {}", page_state));
    }
    if let Some(project_name) = ctx.project_name.as_deref() {
        lines.push(format!("当前项目: {}", project_name));
    }
    if let Some(doc_name) = ctx.doc_name.as_deref() {
        lines.push(format!("当前文档: {}", doc_name));
    }
    if let Some(doc_id) = ctx.doc_id {
        lines.push(format!("当前文档 ID: {}", doc_id));
    }
    if let Some(project_catalog) = ctx
        .project_catalog
        .as_deref()
        .filter(|value| !value.trim().is_empty())
    {
        lines.push(format!("当前可见项目: {}", project_catalog));
    }
    if let Some(current_node_catalog) = ctx
        .current_node_catalog
        .as_deref()
        .filter(|value| !value.trim().is_empty())
    {
        lines.push(format!("当前可见树节点: {}", current_node_catalog));
    }
    if let Some(editor_available) = ctx.editor_available {
        lines.push(format!(
            "编辑器可用: {}",
            if editor_available { "是" } else { "否" }
        ));
    }
    if let Some(snapshot_source) = ctx
        .editor_snapshot_source
        .as_deref()
        .filter(|value| !value.trim().is_empty())
    {
        lines.push(format!("编辑器快照来源: {}", snapshot_source));
    }
    if let Some(unsaved_changes) = ctx.editor_unsaved_changes {
        lines.push(format!(
            "编辑器存在未保存修改: {}",
            if unsaved_changes { "是" } else { "否" }
        ));
    }

    if let Some(section) = build_execution_context_prompt_section(ctx) {
        lines.push(section);
    }
    if let Some(section) = build_execution_memory_prompt_section(ctx) {
        lines.push(section);
    }
    if let Some(section) = build_session_memory_prompt_section(ctx) {
        lines.push(section);
    }
    if let Some(section) = build_tool_outputs_prompt_section(tool_outputs) {
        lines.push(section);
    }

    lines.join("\n")
}

fn build_manual_rig_request<M: RigCompletionModel>(
    model: &M,
    prompt: RigMessage,
    history: Vec<RigMessage>,
    system_prompt: &str,
    tools: Vec<RigToolDefinition>,
    model_config: Option<&AgentProviderModelConfig>,
    additional_params: Option<Value>,
) -> RigCompletionRequest {
    let tools_enabled = model_config
        .and_then(|config| config.tools_enabled)
        .unwrap_or(true);

    let mut builder = model
        .completion_request(prompt)
        .preamble(system_prompt.to_string())
        .messages(history)
        .temperature_opt(model_config.and_then(|config| config.temperature))
        .max_tokens_opt(model_config.and_then(|config| config.max_output_tokens))
        .tools(if tools_enabled { tools } else { Vec::new() });

    if !tools_enabled {
        builder = builder.tool_choice(RigToolChoice::None);
    }

    if let Some(params) = additional_params {
        builder = builder.additional_params(params);
    }

    builder.build()
}

async fn stream_rig_agent_loop<M>(
    model_handle: M,
    conversation: Vec<RigMessage>,
    ctx: &AgentContextPayload,
    tool_outputs: &[AgentToolOutputPayload],
    username: &str,
    model_name: &str,
    tx: &mpsc::Sender<Result<Event, Infallible>>,
    model_config: Option<&AgentProviderModelConfig>,
    additional_params: Option<Value>,
) -> Result<(), String>
where
    M: RigCompletionModel + 'static,
    M::StreamingResponse: Clone + Unpin + rig::completion::GetTokenUsage,
{
    const MAX_AGENT_TURNS: usize = 64;

    let run_id = format!("run_{}", Uuid::new_v4().simple());
    let system_prompt = build_loop_system_prompt(username, ctx, tool_outputs);
    let tools = agent_function_tools();
    let (mut current_prompt, mut chat_history) = split_agent_prompt(conversation);
    let mut current_turns = 0usize;
    let mut accumulated_text_response = String::new();

    let _ = send_json_event(
        tx,
        "message.started",
        json!({
            "model": model_name,
            "run_id": run_id,
        }),
    )
    .await;

    loop {
        if current_turns > MAX_AGENT_TURNS + 1 {
            frontend_tool_broker().cancel_run(&run_id).await;
            return Err(format!("Agent 超过最大轮次限制（{}）", MAX_AGENT_TURNS));
        }

        current_turns += 1;

        let request = build_manual_rig_request(
            &model_handle,
            current_prompt.clone(),
            chat_history.clone(),
            &system_prompt,
            tools.clone(),
            model_config,
            additional_params.clone(),
        );
        let mut stream = model_handle
            .stream(request)
            .await
            .map_err(|err| format!("初始化模型流失败: {err}"))?;

        chat_history.push(current_prompt.clone());

        let mut turn_tool_calls: Vec<PendingFrontendToolCall> = Vec::new();
        let mut turn_tool_results: Vec<(String, Option<String>, String)> = Vec::new();
        let mut accumulated_reasoning: Vec<rig::message::Reasoning> = Vec::new();
        let mut pending_reasoning_delta_text = String::new();
        let mut pending_reasoning_delta_id: Option<String> = None;
        let mut last_text_response = String::new();
        let mut saw_tool_call_this_turn = false;

        while let Some(item) = stream.next().await {
            match item {
                Ok(StreamedAssistantContent::Text(text)) => {
                    last_text_response.push_str(&text.text);
                    if !send_json_event(tx, "message.delta", json!({ "content": text.text })).await {
                        break;
                    }
                }
                Ok(StreamedAssistantContent::ReasoningDelta { reasoning, id }) => {
                    pending_reasoning_delta_text.push_str(&reasoning);
                    if pending_reasoning_delta_id.is_none() {
                        pending_reasoning_delta_id = id.clone();
                    }
                    let _ = send_json_event(tx, "reasoning.delta", json!({ "delta": reasoning })).await;
                }
                Ok(StreamedAssistantContent::Reasoning(reasoning)) => {
                    merge_reasoning_blocks(&mut accumulated_reasoning, &reasoning);
                    let _ = send_json_event(
                        tx,
                        "reasoning.delta",
                        json!({ "delta": reasoning.display_text() }),
                    )
                    .await;
                }
                Ok(StreamedAssistantContent::ToolCall { tool_call, .. }) => {
                    let call_id = tool_call
                        .call_id
                        .clone()
                        .filter(|value| !value.trim().is_empty())
                        .unwrap_or_else(|| tool_call.id.clone());
                    let arguments = tool_call.function.arguments.clone();
                    let _ = send_json_event(
                        tx,
                        "tool.call.delta",
                        json!({
                            "call_id": call_id,
                            "name": tool_call.function.name,
                            "arguments": arguments.to_string(),
                        }),
                    )
                    .await;
                    turn_tool_calls.push(PendingFrontendToolCall {
                        id: tool_call.id.clone(),
                        call_id: call_id.clone(),
                        name: tool_call.function.name.clone(),
                        arguments: arguments.clone(),
                    });
                    let output = match request_frontend_tool_output(
                        tx,
                        &run_id,
                        &PendingFrontendToolCall {
                            id: tool_call.id.clone(),
                            call_id: call_id.clone(),
                            name: tool_call.function.name.clone(),
                            arguments: arguments.clone(),
                        },
                    )
                    .await
                    {
                        Ok(output) => output,
                        Err(error) => json!({
                            "ok": false,
                            "tool": tool_call.function.name,
                            "error": error,
                        }),
                    };
                    let output_text = serde_json::to_string(&output)
                        .map_err(|err| format!("序列化工具结果失败: {err}"))?;
                    turn_tool_results.push((tool_call.id, Some(call_id), output_text));
                    saw_tool_call_this_turn = true;
                }
                Ok(StreamedAssistantContent::ToolCallDelta { .. }) | Ok(StreamedAssistantContent::Final(_)) => {}
                Err(err) => {
                    frontend_tool_broker().cancel_run(&run_id).await;
                    return Err(format!("Agent 执行失败: {err}"));
                }
            }
        }

        let response_id = stream
            .message_id
            .clone()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty());

        if accumulated_reasoning.is_empty() && !pending_reasoning_delta_text.is_empty() {
            let mut assembled = rig::message::Reasoning::new(&pending_reasoning_delta_text);
            if let Some(id) = pending_reasoning_delta_id.take() {
                assembled = assembled.with_id(id);
            }
            accumulated_reasoning.push(assembled);
        }

        let completed_action = detect_completed_action_block(&last_text_response);

        let mut assistant_items: Vec<RigAssistantContent> = accumulated_reasoning
            .into_iter()
            .map(RigAssistantContent::Reasoning)
            .collect();
        if saw_tool_call_this_turn && !last_text_response.is_empty() {
            assistant_items.push(RigAssistantContent::text(last_text_response.clone()));
        }
        assistant_items.extend(turn_tool_calls.iter().map(|call| {
            RigAssistantContent::tool_call_with_call_id(
                call.id.clone(),
                call.call_id.clone(),
                call.name.clone(),
                call.arguments.clone(),
            )
        }));

        if !assistant_items.is_empty() {
            let assistant_message = RigOneOrMany::many(assistant_items)
                .map_err(|_| "工具轮次缺少 assistant 内容".to_string())?;
            chat_history.push(RigMessage::Assistant {
                id: response_id.clone(),
                content: assistant_message,
            });
        }

        extend_loop_visible_text(&mut accumulated_text_response, &last_text_response);

        if let Some(action) = completed_action.as_ref() {
            if !saw_tool_call_this_turn && !last_text_response.is_empty() {
                chat_history.push(RigMessage::assistant(last_text_response.clone()));
            }
            let synthetic_call =
                build_action_protocol_tool_call(current_turns, action.mode.as_str());
            let output = request_frontend_tool_output(tx, &run_id, &synthetic_call)
                .await
                .unwrap_or_else(|error| {
                    json!({
                        "ok": false,
                        "tool": ACTION_PROTOCOL_TOOL_NAME,
                        "error": error,
                    })
                });
            append_action_tool_result_history(
                &mut chat_history,
                synthetic_call
                    .arguments
                    .get("mode")
                    .and_then(Value::as_str)
                    .unwrap_or("append"),
                output,
            )?;

            if !saw_tool_call_this_turn {
                current_prompt = chat_history
                    .pop()
                    .ok_or_else(|| "ACTION 工具结果后缺少后续 prompt".to_string())?;
                continue;
            }
        }

        if !saw_tool_call_this_turn {
            if completed_action.is_some() {
                current_prompt = chat_history
                    .pop()
                    .ok_or_else(|| "ACTION 工具结果后缺少后续 prompt".to_string())?;
                continue;
            }

            current_prompt = chat_history
                .pop()
                .ok_or_else(|| "工具执行后缺少后续 prompt".to_string())?;
            chat_history.push(current_prompt.clone());
            if !last_text_response.is_empty() {
                chat_history.push(RigMessage::assistant(last_text_response.clone()));
            }

            let _ = send_json_event(
                tx,
                "message.completed",
                json!({
                    "content": accumulated_text_response,
                    "run_id": run_id,
                    "response_id": response_id,
                }),
            )
            .await;
            let _ = send_json_event(tx, "done", json!({})).await;
            frontend_tool_broker().cancel_run(&run_id).await;
            return Ok(());
        }

        for (id, call_id, output_text) in turn_tool_results {
            chat_history.push(tool_result_user_message(&id, call_id.as_deref(), &output_text));
        }

        current_prompt = chat_history
            .pop()
            .ok_or_else(|| "工具执行后缺少后续 prompt".to_string())?;
    }
}

fn build_openai_additional_params(
    payload: &AgentChatStreamRequest,
    model_config: Option<&AgentProviderModelConfig>,
    use_responses_api: bool,
) -> Option<Value> {
    let mut params = serde_json::Map::new();

    if let Some(config) = model_config {
        if let Some(top_p) = config.top_p {
            params.insert("top_p".to_string(), json!(top_p));
        }
        if let Some(presence_penalty) = config.presence_penalty {
            params.insert("presence_penalty".to_string(), json!(presence_penalty));
        }
        if let Some(frequency_penalty) = config.frequency_penalty {
            params.insert("frequency_penalty".to_string(), json!(frequency_penalty));
        }
        if !config.stop_sequences.is_empty() {
            params.insert("stop".to_string(), json!(config.stop_sequences));
        }
        if use_responses_api {
            if let Some(parallel_tool_calls) = config.parallel_tool_calls {
                params.insert(
                    "parallel_tool_calls".to_string(),
                    json!(parallel_tool_calls),
                );
            }
        }
    }

    if let Some(previous_response_id) = payload
        .previous_response_id
        .as_ref()
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
    {
        if use_responses_api {
            params.insert(
                "previous_response_id".to_string(),
                Value::String(previous_response_id.to_string()),
            );
        }
    }

    let thinking = model_config.and_then(|config| config.thinking);
    if use_responses_api {
        match thinking {
            Some(true) => {
                params.insert(
                    "reasoning".to_string(),
                    json!({
                        "effort": model_config
                            .and_then(|config| config.reasoning_effort.as_deref())
                            .unwrap_or("medium"),
                        "summary": "detailed"
                    }),
                );
            }
            Some(false) => {}
            None => {
                params.insert(
                    "reasoning".to_string(),
                    json!({
                        "effort": "medium",
                        "summary": "detailed"
                    }),
                );
            }
        }
    }

    merge_json_values(
        if params.is_empty() {
            None
        } else {
            Some(Value::Object(params))
        },
        model_config.and_then(|config| config.additional_params.clone()),
    )
}

fn build_anthropic_additional_params(
    model_config: Option<&AgentProviderModelConfig>,
) -> Option<Value> {
    let Some(config) = model_config else {
        return None;
    };

    let mut params = serde_json::Map::new();
    if let Some(top_p) = config.top_p {
        params.insert("top_p".to_string(), json!(top_p));
    }
    if let Some(top_k) = config.top_k {
        params.insert("top_k".to_string(), json!(top_k));
    }
    if !config.stop_sequences.is_empty() {
        params.insert("stop_sequences".to_string(), json!(config.stop_sequences));
    }

    merge_json_values(
        if params.is_empty() {
            None
        } else {
            Some(Value::Object(params))
        },
        config.additional_params.clone(),
    )
}

fn build_gemini_additional_params(
    model_config: Option<&AgentProviderModelConfig>,
) -> Option<Value> {
    let Some(config) = model_config else {
        return None;
    };

    let mut generation_config = serde_json::Map::new();
    if let Some(top_p) = config.top_p {
        generation_config.insert("topP".to_string(), json!(top_p));
    }
    if let Some(top_k) = config.top_k {
        generation_config.insert("topK".to_string(), json!(top_k));
    }
    if let Some(presence_penalty) = config.presence_penalty {
        generation_config.insert("presencePenalty".to_string(), json!(presence_penalty));
    }
    if let Some(frequency_penalty) = config.frequency_penalty {
        generation_config.insert("frequencyPenalty".to_string(), json!(frequency_penalty));
    }
    if !config.stop_sequences.is_empty() {
        generation_config.insert("stopSequences".to_string(), json!(config.stop_sequences));
    }
    if let Some(response_mime_type) = config
        .response_mime_type
        .as_ref()
        .map(|item| item.trim())
        .filter(|item| !item.is_empty())
    {
        generation_config.insert(
            "responseMimeType".to_string(),
            json!(response_mime_type),
        );
    }
    if matches!(config.thinking, Some(true)) {
        generation_config.insert(
            "thinkingConfig".to_string(),
            json!({
                "thinkingBudget": 2048,
                "includeThoughts": true
            }),
        );
    }

    merge_json_values(
        if generation_config.is_empty() {
            None
        } else {
            Some(json!({ "generationConfig": generation_config }))
        },
        config.additional_params.clone(),
    )
}

fn join_api_path(base_url: &str, path: &str) -> String {
    format!(
        "{}/{}",
        base_url.trim_end_matches('/'),
        path.trim_start_matches('/')
    )
}

fn models_endpoint(protocol: AgentProviderProtocol, base_url: &str) -> String {
    match protocol {
        AgentProviderProtocol::OpenAi => join_api_path(base_url, "models"),
        AgentProviderProtocol::Anthropic => {
            if base_url.trim_end_matches('/').ends_with("/v1") {
                join_api_path(base_url, "models")
            } else {
                join_api_path(base_url, "v1/models")
            }
        }
        AgentProviderProtocol::Gemini => {
            if base_url.contains("/v1beta") || base_url.contains("/v1/") || base_url.ends_with("/v1") {
                join_api_path(base_url, "models")
            } else {
                join_api_path(base_url, "v1beta/models")
            }
        }
    }
}

async fn fetch_models_via_http(
    protocol: AgentProviderProtocol,
    base_url: &str,
    api_key: &str,
) -> Result<Vec<AgentModelSummary>, anyhow::Error> {
    let client = HttpClient::new();
    let url = models_endpoint(protocol, base_url);
    let mut request = client.get(url.clone());

    request = match protocol {
        AgentProviderProtocol::OpenAi => request.bearer_auth(api_key),
        AgentProviderProtocol::Anthropic => request
            .header("x-api-key", api_key)
            .header("anthropic-version", "2023-06-01"),
        AgentProviderProtocol::Gemini => client.get(format!("{url}?key={api_key}")),
    };

    let value = request
        .send()
        .await?
        .error_for_status()?
        .json::<Value>()
        .await?;

    let models = match protocol {
        AgentProviderProtocol::OpenAi => value
            .get("data")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .map(|item| AgentModelSummary {
                id: item
                    .get("id")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string(),
                owned_by: item
                    .get("owned_by")
                    .and_then(Value::as_str)
                    .unwrap_or("openai")
                    .to_string(),
                created: item
                    .get("created")
                    .and_then(Value::as_u64)
                    .unwrap_or_default() as u32,
            })
            .collect::<Vec<_>>(),
        AgentProviderProtocol::Anthropic => value
            .get("data")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .map(|item| AgentModelSummary {
                id: item
                    .get("id")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string(),
                owned_by: "anthropic".to_string(),
                created: 0,
            })
            .collect::<Vec<_>>(),
        AgentProviderProtocol::Gemini => value
            .get("models")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .map(|item| {
                let raw_name = item
                    .get("name")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .trim_start_matches("models/")
                    .to_string();
                AgentModelSummary {
                    id: raw_name,
                    owned_by: "google".to_string(),
                    created: 0,
                }
            })
            .collect::<Vec<_>>(),
    };

    Ok(models
        .into_iter()
        .filter(|model| !model.id.trim().is_empty())
        .collect())
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

    let provider_kind = normalize_provider_kind(payload.provider_kind.as_deref());
    let protocol = provider_protocol_from_kind(&provider_kind).unwrap_or(AgentProviderProtocol::OpenAi);
    let base_url = normalize_provider_base_url(protocol, payload.base_url.as_deref());
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
    let model_configs = payload.model_configs.unwrap_or_default();

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
             SET name = ?, provider_kind = ?, base_url = ?, api_key_ciphertext = ?, remote_models = ?, enabled_models = ?, custom_models = ?, model_configs = ?, updated_at = datetime('now')
             WHERE id = ? AND user_id = ?",
        )
        .bind(name)
        .bind(&provider_kind)
        .bind(&base_url)
        .bind(api_key_ciphertext)
        .bind(serialize_json_string_array(&remote_models))
        .bind(serialize_json_string_array(&enabled_models))
        .bind(serialize_json_string_array(&custom_models))
        .bind(serialize_model_configs(&model_configs))
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
            "INSERT INTO agent_providers (user_id, name, provider_kind, base_url, api_key_ciphertext, remote_models, enabled_models, custom_models, model_configs, is_active)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, 0)",
        )
        .bind(user.id)
        .bind(name)
        .bind(&provider_kind)
        .bind(&base_url)
        .bind(api_key_ciphertext)
        .bind(serialize_json_string_array(&remote_models))
        .bind(serialize_json_string_array(&enabled_models))
        .bind(serialize_json_string_array(&custom_models))
        .bind(serialize_model_configs(&model_configs))
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
        provider_kind: normalize_provider_kind(Some(&provider.provider_kind)),
        base_url: provider.base_url,
        api_key,
        remote_models: parse_json_string_array(&provider.remote_models),
        enabled_models: parse_json_string_array(&provider.enabled_models),
        custom_models: parse_json_string_array(&provider.custom_models),
        model_configs: parse_model_configs(&provider.model_configs),
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
    let protocol = detect_provider_protocol(&provider, "");
    let base_url = normalize_provider_base_url(protocol, Some(&provider.base_url));

    let mut models = fetch_models_via_http(protocol, &base_url, &api_key)
        .await
        .map_err(|err| {
        (
            StatusCode::BAD_GATEWAY,
            Json(json!({"error": format!("获取模型列表失败: {}", err)})),
        )
            .into_response()
    })?;
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
        .any(|message| !message.content.trim().is_empty() || !message.attachments.is_empty());
    if !has_messages {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "消息不能为空"})),
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

    let (tx, rx) = mpsc::channel::<Result<Event, Infallible>>(256);
    let model = payload.provider.model.clone();
    let protocol = detect_provider_protocol(&provider, &model);
    let base_url = normalize_provider_base_url(protocol, Some(&provider.base_url));
    let model_config = find_model_config(&provider, &model);
    let username = user.username.clone();
    let user_id = user.id;
    let db = db.clone();

    tokio::spawn(async move {
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

        let outcome = match protocol {
            AgentProviderProtocol::OpenAi => {
                let client = match openai::Client::builder()
                    .api_key(api_key.clone())
                    .base_url(&base_url)
                    .build()
                {
                    Ok(client) => client,
                    Err(err) => {
                        let message = format!("初始化 OpenAI provider 失败: {err}");
                        let _ = send_json_event(&tx, "error_event", json!({ "scope": "runtime", "message": message.clone() })).await;
                        let _ = send_json_event(&tx, "error", json!({ "error": message.clone() })).await;
                        return;
                    }
                };
                match resolve_openai_transport_mode(payload.transport_mode.as_deref(), &base_url) {
                    "chat" => {
                        let model_handle = client
                            .clone()
                            .completions_api()
                            .completion_model(model.clone());
                        let additional_params =
                            build_openai_additional_params(&payload, model_config.as_ref(), false);
                        stream_rig_agent_loop(
                            model_handle,
                            conversation,
                            &ctx,
                            &payload.tool_outputs,
                            &username,
                            &model,
                            &tx,
                            model_config.as_ref(),
                            additional_params,
                        )
                        .await
                    }
                    _ => {
                        let model_handle = client.completion_model(model.clone());
                        let additional_params =
                            build_openai_additional_params(&payload, model_config.as_ref(), true);
                        stream_rig_agent_loop(
                            model_handle,
                            conversation,
                            &ctx,
                            &payload.tool_outputs,
                            &username,
                            &model,
                            &tx,
                            model_config.as_ref(),
                            additional_params,
                        )
                        .await
                    }
                }
            }
            AgentProviderProtocol::Anthropic => {
                match anthropic::Client::builder()
                    .api_key(api_key.clone())
                    .base_url(&base_url)
                    .build()
                {
                    Ok(client) => {
                        let model_handle: anthropic::completion::CompletionModel<reqwest::Client> =
                            client.completion_model(model.clone());
                        stream_rig_agent_loop(
                            model_handle,
                            conversation,
                            &ctx,
                            &payload.tool_outputs,
                            &username,
                            &model,
                            &tx,
                            model_config.as_ref(),
                            build_anthropic_additional_params(model_config.as_ref()),
                        )
                        .await
                    }
                    Err(err) => Err(format!("初始化 Anthropic provider 失败: {err}")),
                }
            }
            AgentProviderProtocol::Gemini => {
                match gemini::Client::builder()
                    .api_key(api_key.clone())
                    .base_url(&base_url)
                    .build()
                {
                    Ok(client) => {
                        let model_handle: gemini::completion::CompletionModel<reqwest::Client> =
                            client.completion_model(model.clone());
                        stream_rig_agent_loop(
                            model_handle,
                            conversation,
                            &ctx,
                            &payload.tool_outputs,
                            &username,
                            &model,
                            &tx,
                            model_config.as_ref(),
                            build_gemini_additional_params(model_config.as_ref()),
                        )
                        .await
                    }
                    Err(err) => Err(format!("初始化 Gemini provider 失败: {err}")),
                }
            }
        };

        if let Err(message) = outcome {
                let _ = send_json_event(&tx, "error_event", json!({ "scope": "runtime", "message": message.clone() })).await;
                let _ = send_json_event(&tx, "error", json!({ "error": message })).await;
        }
    });

    Ok(Sse::new(ReceiverStream::new(rx)).keep_alive(
        KeepAlive::new()
            .interval(Duration::from_secs(15))
            .text("keep-alive"),
    ))
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
    fn chat_stream_request_deserializes_execution_context_and_tool_outputs() {
        let payload = serde_json::json!({
            "provider": {
                "provider_id": 1,
                "model": "qwen3.5-plus"
            },
            "messages": [
                { "role": "assistant", "content": "上一轮已经写入文档。" }
            ],
            "context": {
                "page_scope": "home.doc",
                "doc_id": 12,
                "doc_name": "测试文档",
                "agent_execution": {
                    "current_mode": "plan",
                    "plan_step_index": 2,
                    "plan_current_step": "补充总结",
                    "write_completed": true,
                    "document_write_observed": true
                },
                "last_execution": {
                    "assistant_summary": "已经完成正文写入",
                    "plan_current_step": "补充总结",
                    "write_completed": true,
                    "document_write_observed": true
                },
                "session_memory": {
                    "summary": "用户正在完善当前文档",
                    "completed_facts": ["已完成正文写入"],
                    "open_loops": ["保存当前文档"]
                }
            },
            "tool_outputs": [
                {
                    "call_id": "call_action",
                    "name": "action_protocol_write",
                    "arguments": "{\"mode\":\"append\"}",
                    "output": {
                        "ok": true,
                        "tool": "action_protocol_write",
                        "result": {
                            "wrote_document": true,
                            "write_completed": true
                        }
                    }
                }
            ]
        });

        let request: AgentChatStreamRequest =
            serde_json::from_value(payload).expect("payload should deserialize");

        let ctx = request.context.expect("context should exist");
        assert_eq!(
            ctx.agent_execution
                .as_ref()
                .and_then(|state| state.plan_current_step.as_deref()),
            Some("补充总结")
        );
        assert_eq!(
            ctx.last_execution
                .as_ref()
                .and_then(|state| state.assistant_summary.as_deref()),
            Some("已经完成正文写入")
        );
        assert_eq!(
            ctx.session_memory
                .as_ref()
                .map(|memory| memory.completed_facts.clone()),
            Some(vec!["已完成正文写入".to_string()])
        );
        assert_eq!(request.tool_outputs.len(), 1);
        assert_eq!(
            request.tool_outputs[0].name.as_deref(),
            Some("action_protocol_write")
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
    fn loop_system_prompt_mentions_completed_action_write_and_plan_progress() {
        let ctx = AgentContextPayload {
            page_scope: Some("home.doc".to_string()),
            doc_name: Some("测试文档".to_string()),
            agent_execution: Some(AgentExecutionContextPayload {
                current_mode: Some("plan".to_string()),
                plan_step_index: Some(2),
                plan_total_steps: Some(3),
                plan_current_step: Some("补充总结".to_string()),
                write_completed: Some(true),
                document_write_observed: Some(true),
                ..Default::default()
            }),
            last_execution: Some(AgentExecutionMemoryPayload {
                assistant_summary: Some("已经完成正文写入".to_string()),
                plan_completed_steps: vec!["完成正文首稿".to_string()],
                ..Default::default()
            }),
            session_memory: Some(AgentSessionMemoryPayload {
                completed_facts: vec!["已完成正文写入".to_string()],
                open_loops: vec!["保存当前文档".to_string()],
                ..Default::default()
            }),
            ..Default::default()
        };
        let tool_outputs = vec![AgentToolOutputPayload {
            call_id: "call_action".to_string(),
            name: Some("action_protocol_write".to_string()),
            arguments: Some("{\"mode\":\"append\"}".to_string()),
            output: serde_json::json!({
                "ok": true,
                "tool": "action_protocol_write",
                "result": {
                    "wrote_document": true,
                    "write_completed": true
                }
            }),
        }];

        let prompt = build_loop_system_prompt("tester", &ctx, &tool_outputs);

        assert!(prompt.contains("已经完成正文写入"));
        assert!(prompt.contains("不要把文档写入步骤当成未执行"));
        assert!(prompt.contains("action_protocol_write: 成功，已完成正文写入"));
        assert!(prompt.contains("当前计划步骤: 第 2/3 步 - 补充总结"));
    }

    #[test]
    fn detect_completed_action_block_for_backend_loop() {
        let action = detect_completed_action_block(
            "前言\n[[ACTION:append]]## 总结\n内容\n[[/ACTION]]\n后记",
        )
        .expect("action block should be detected");

        assert_eq!(action.mode, "append");
        assert!(action.body.contains("## 总结"));
    }

    #[test]
    fn append_action_tool_result_history_replays_as_tool_round() {
        let mut messages = vec![RigMessage::user("请继续")];
        append_action_tool_result_history(
            &mut messages,
            "append",
            serde_json::json!({
                "ok": true,
                "tool": "action_protocol_write",
                "result": {
                    "wrote_document": true,
                    "write_completed": true
                }
            }),
        )
        .expect("action history should append");

        assert_eq!(messages.len(), 3);
        match &messages[1] {
            RigMessage::Assistant { .. } => {}
            other => panic!("expected assistant tool call message, got {:?}", other),
        }
        match &messages[2] {
            RigMessage::User { .. } => {}
            other => panic!("expected tool result user message, got {:?}", other),
        }
    }

    #[test]
    fn action_protocol_tool_call_arguments_do_not_repeat_document_body() {
        let call = build_action_protocol_tool_call(7, "append");

        assert_eq!(call.name, ACTION_PROTOCOL_TOOL_NAME);
        assert_eq!(call.arguments.get("mode").and_then(Value::as_str), Some("append"));
        assert!(call.arguments.get("body").is_none());
    }

    #[test]
    fn action_completion_round_preserves_assistant_action_text_before_tool_result() {
        let mut messages = vec![RigMessage::user("请继续")];
        let assistant_text = "[[ACTION:append]]## 总结\n内容\n[[/ACTION]]";
        messages.push(RigMessage::assistant(assistant_text));
        append_action_tool_result_history(
            &mut messages,
            "append",
            serde_json::json!({
                "ok": true,
                "tool": "action_protocol_write",
                "result": {
                    "wrote_document": true,
                    "write_completed": true,
                    "doc_id": 250,
                    "doc_name": "变量与数据类型"
                }
            }),
        )
        .expect("action history should append");

        assert_eq!(messages.len(), 4);
        match &messages[1] {
            RigMessage::Assistant { .. } => {}
            other => panic!("expected preserved assistant action text, got {:?}", other),
        }
        match &messages[2] {
            RigMessage::Assistant { .. } => {}
            other => panic!("expected assistant tool call message, got {:?}", other),
        }
        match &messages[3] {
            RigMessage::User { .. } => {}
            other => panic!("expected tool result user message, got {:?}", other),
        }
    }

    #[test]
    fn extend_loop_visible_text_preserves_prior_turns() {
        let mut accumulated = String::new();
        extend_loop_visible_text(&mut accumulated, "第一轮：先删除旧项目。");
        extend_loop_visible_text(&mut accumulated, "\n第二轮：已创建新项目。");

        assert_eq!(accumulated, "第一轮：先删除旧项目。\n第二轮：已创建新项目。");
    }

    #[test]
    fn extend_loop_visible_text_strips_completed_action_blocks() {
        let mut accumulated = String::new();
        extend_loop_visible_text(
            &mut accumulated,
            "先打开文档。\n[[ACTION:append]]# 标题\n正文\n[[/ACTION]]\n然后继续下一步。",
        );

        assert_eq!(accumulated, "先打开文档。\n\n然后继续下一步。");
    }

    #[test]
    fn summarize_frontend_tool_output_uses_document_write_summary_for_action_protocol() {
        let call = PendingFrontendToolCall {
            id: "tool_1".to_string(),
            call_id: "call_1".to_string(),
            name: ACTION_PROTOCOL_TOOL_NAME.to_string(),
            arguments: serde_json::json!({ "mode": "append" }),
        };

        let summary = summarize_frontend_tool_output(
            &call,
            &serde_json::json!({
                "ok": true,
                "tool": "action_protocol_write",
                "result": {
                    "wrote_document": true,
                    "write_completed": true,
                    "doc_name": "变量与数据类型"
                }
            }),
        );

        assert_eq!(summary, "已写入《变量与数据类型》正文");
    }

    #[test]
    fn with_frontend_tool_summary_injects_summary_into_action_output() {
        let output = with_frontend_tool_summary(
            serde_json::json!({
                "ok": true,
                "tool": "action_protocol_write",
                "result": {
                    "wrote_document": true,
                    "write_completed": true,
                    "doc_name": "变量与数据类型"
                }
            }),
            "已写入《变量与数据类型》正文",
        );

        assert_eq!(
            output.get("summary").and_then(Value::as_str),
            Some("已写入《变量与数据类型》正文")
        );
    }
}
