//! 供应商 + 模型两表数据层与 CRUD。
//!
//! 设计：`agent_providers` 只承载供应商级别配置（协议、base_url、密钥、
//! OpenAI 传输模式、anthropic 版本、远程模型快照）。`agent_models` 每行一个
//! 已保存模型，全部运行参数收敛进单个 `config` JSON blob（见 [`AgentModelConfig`]）。
//! 只支持 openai / anthropic 两种协议，不做任何兼容降级。

use std::sync::Arc;

use axum::{
    extract::{Extension, Path},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::{
    auth,
    db::Database,
    models::{AgentModel, AgentProvider},
};

use super::crypto::{decrypt_api_key, encrypt_api_key};

/// 供应商协议。只保留 openai / anthropic。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderKind {
    OpenAi,
    Anthropic,
}

impl ProviderKind {
    pub fn as_str(self) -> &'static str {
        match self {
            ProviderKind::OpenAi => "openai",
            ProviderKind::Anthropic => "anthropic",
        }
    }

    pub fn parse(value: &str) -> ProviderKind {
        match value.trim().to_ascii_lowercase().as_str() {
            "anthropic" | "claude" => ProviderKind::Anthropic,
            _ => ProviderKind::OpenAi,
        }
    }

    pub fn default_base_url(self) -> &'static str {
        match self {
            ProviderKind::OpenAi => "https://api.openai.com/v1",
            ProviderKind::Anthropic => "https://api.anthropic.com",
        }
    }
}

/// OpenAI 传输模式。anthropic 忽略此字段。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpenAiApi {
    Responses,
    Completions,
}

impl OpenAiApi {
    pub fn as_str(self) -> &'static str {
        match self {
            OpenAiApi::Responses => "responses",
            OpenAiApi::Completions => "completions",
        }
    }

    pub fn parse(value: &str) -> OpenAiApi {
        match value.trim().to_ascii_lowercase().as_str() {
            "responses" => OpenAiApi::Responses,
            _ => OpenAiApi::Completions,
        }
    }
}

/// 单个已保存模型的完整运行参数。序列化后存进 `agent_models.config`。
///
/// 这是 markflow 自有的 schema（不复用 awake），字段覆盖 OpenAI
/// responses / completions 与 Anthropic 三套协议所需参数；运行时按协议
/// 各取所需拼装 additional_params。全部字段可选，缺省即沿用上游默认。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AgentModelConfig {
    /// 模型级 OpenAI 传输模式覆盖：responses|completions；None 表示沿用供应商。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub api: Option<String>,
    /// 是否允许工具调用。缺省 true。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tools_enabled: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub top_p: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub top_k: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_window: Option<u64>,
    /// 是否开启推理/思考。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reasoning_enabled: Option<bool>,
    /// OpenAI reasoning effort：minimal|low|medium|high 等。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reasoning_effort: Option<String>,
    /// OpenAI reasoning summary：auto|concise|detailed。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reasoning_summary: Option<String>,
    /// Anthropic thinking budget tokens。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thinking_budget_tokens: Option<u64>,
    /// 停止序列。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub stop_sequences: Vec<String>,
    /// 输入模态提示。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub modalities: Vec<String>,
    /// 高级透传参数，原样并入 additional_params。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub additional_params: Option<Value>,
}

impl AgentModelConfig {
    pub(super) fn parse(raw: &str) -> AgentModelConfig {
        serde_json::from_str(raw).unwrap_or_default()
    }

    fn serialize(&self) -> String {
        serde_json::to_string(self).unwrap_or_else(|_| "{}".to_string())
    }

    /// 解析模型级 OpenAI 传输模式覆盖。
    pub fn api_override(&self) -> Option<OpenAiApi> {
        self.api.as_deref().map(OpenAiApi::parse)
    }

    /// 是否允许工具调用，缺省 true。
    pub fn tools_enabled(&self) -> bool {
        self.tools_enabled.unwrap_or(true)
    }
}

// ---------------------------------------------------------------------------
// 请求 / 响应 DTO
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct ProviderUpsertRequest {
    pub id: Option<i64>,
    pub name: String,
    /// openai | anthropic
    pub kind: Option<String>,
    /// openai 传输模式: responses | completions
    pub api: Option<String>,
    pub base_url: Option<String>,
    pub api_key: Option<String>,
    pub anthropic_version: Option<String>,
    #[serde(default)]
    pub remote_models: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
pub struct ModelUpsertRequest {
    pub id: Option<i64>,
    pub provider_id: i64,
    pub alias: String,
    pub model_id: String,
    pub display_name: Option<String>,
    #[serde(default)]
    pub config: AgentModelConfig,
}

#[derive(Debug, Serialize)]
pub struct ModelSummary {
    pub id: i64,
    pub provider_id: i64,
    pub alias: String,
    pub model_id: String,
    pub display_name: Option<String>,
    pub config: AgentModelConfig,
    pub created_at: String,
    pub updated_at: String,
}

impl ModelSummary {
    fn from_row(row: AgentModel) -> ModelSummary {
        ModelSummary {
            id: row.id,
            provider_id: row.provider_id,
            alias: row.alias,
            model_id: row.model_id,
            display_name: row.display_name,
            config: AgentModelConfig::parse(&row.config),
            created_at: row.created_at,
            updated_at: row.updated_at,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct ProviderSummary {
    pub id: i64,
    pub name: String,
    pub kind: String,
    pub api: String,
    pub base_url: String,
    pub anthropic_version: Option<String>,
    pub remote_models: Vec<String>,
    pub models: Vec<ModelSummary>,
    pub is_active: bool,
    pub has_api_key: bool,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Serialize)]
pub struct ProvidersResponse {
    pub providers: Vec<ProviderSummary>,
    pub active_provider_id: Option<i64>,
}

#[derive(Debug, Serialize)]
pub struct ProviderDetailResponse {
    pub id: i64,
    pub name: String,
    pub kind: String,
    pub api: String,
    pub base_url: String,
    pub anthropic_version: Option<String>,
    pub api_key: String,
    pub remote_models: Vec<String>,
    pub models: Vec<ModelSummary>,
    pub is_active: bool,
}

// ---------------------------------------------------------------------------
// 辅助
// ---------------------------------------------------------------------------

fn json_error(status: StatusCode, message: impl Into<String>) -> Response {
    (status, Json(json!({ "error": message.into() }))).into_response()
}

fn parse_string_array(raw: &str) -> Vec<String> {
    serde_json::from_str::<Vec<String>>(raw)
        .unwrap_or_default()
        .into_iter()
        .map(|item| item.trim().to_string())
        .filter(|item| !item.is_empty())
        .collect()
}

fn serialize_string_array(values: &[String]) -> String {
    let mut seen = std::collections::HashSet::new();
    let unique: Vec<&String> = values
        .iter()
        .filter(|value| !value.trim().is_empty() && seen.insert(value.trim().to_string()))
        .collect();
    serde_json::to_string(&unique).unwrap_or_else(|_| "[]".to_string())
}

fn normalize_base_url(kind: ProviderKind, value: Option<&str>) -> String {
    let trimmed = value.map(str::trim).filter(|item| !item.is_empty());
    match trimmed {
        Some(url) => url.trim_end_matches('/').to_string(),
        None => kind.default_base_url().to_string(),
    }
}

// ---------------------------------------------------------------------------
// 数据访问
// ---------------------------------------------------------------------------

pub async fn list_user_providers(
    db: &Database,
    user_id: i64,
) -> Result<Vec<AgentProvider>, Response> {
    sqlx::query_as::<_, AgentProvider>(
        "SELECT * FROM agent_providers WHERE user_id = ? ORDER BY updated_at DESC, id DESC",
    )
    .bind(user_id)
    .fetch_all(&db.pool)
    .await
    .map_err(|err| {
        json_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("获取供应商列表失败: {err}"),
        )
    })
}

pub async fn find_user_provider(
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
            json_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("查询供应商失败: {err}"),
            )
        })?
        .ok_or_else(|| json_error(StatusCode::NOT_FOUND, "供应商不存在"))
}

async fn list_provider_models(
    db: &Database,
    provider_id: i64,
) -> Result<Vec<AgentModel>, Response> {
    sqlx::query_as::<_, AgentModel>(
        "SELECT * FROM agent_models WHERE provider_id = ? ORDER BY sort_order ASC, id ASC",
    )
    .bind(provider_id)
    .fetch_all(&db.pool)
    .await
    .map_err(|err| {
        json_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("获取模型列表失败: {err}"),
        )
    })
}

pub async fn find_provider_model(
    db: &Database,
    provider_id: i64,
    model_id: i64,
) -> Result<AgentModel, Response> {
    sqlx::query_as::<_, AgentModel>("SELECT * FROM agent_models WHERE id = ? AND provider_id = ?")
        .bind(model_id)
        .bind(provider_id)
        .fetch_optional(&db.pool)
        .await
        .map_err(|err| {
            json_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("查询模型失败: {err}"),
            )
        })?
        .ok_or_else(|| json_error(StatusCode::NOT_FOUND, "模型不存在"))
}

/// 按模型名（alias 或 model_id）在指定供应商下定位一条已保存模型行。
/// chat_stream 收到的 payload 只有模型字符串，需要据此还原完整配置。
pub async fn find_model_by_name(
    db: &Database,
    provider_id: i64,
    name: &str,
) -> Result<Option<AgentModel>, Response> {
    let name = name.trim();
    sqlx::query_as::<_, AgentModel>(
        "SELECT * FROM agent_models WHERE provider_id = ? AND (alias = ? OR model_id = ?)          ORDER BY sort_order ASC, id ASC LIMIT 1",
    )
    .bind(provider_id)
    .bind(name)
    .bind(name)
    .fetch_optional(&db.pool)
    .await
    .map_err(|err| {
        json_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("查询模型失败: {err}"),
        )
    })
}

impl AgentModel {
    /// 解析该行的运行参数配置。
    pub fn parsed_config(&self) -> AgentModelConfig {
        AgentModelConfig::parse(&self.config)
    }
}

async fn set_active_provider(
    db: &Database,
    user_id: i64,
    provider_id: i64,
) -> Result<(), Response> {
    sqlx::query(
        "UPDATE agent_providers SET is_active = CASE WHEN id = ? THEN 1 ELSE 0 END WHERE user_id = ?",
    )
    .bind(provider_id)
    .bind(user_id)
    .execute(&db.pool)
    .await
    .map_err(|err| {
        json_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("激活供应商失败: {err}"),
        )
    })?;
    Ok(())
}

async fn provider_summary(
    db: &Database,
    provider: AgentProvider,
) -> Result<ProviderSummary, Response> {
    let models = list_provider_models(db, provider.id)
        .await?
        .into_iter()
        .map(ModelSummary::from_row)
        .collect();
    Ok(ProviderSummary {
        id: provider.id,
        name: provider.name,
        kind: ProviderKind::parse(&provider.kind).as_str().to_string(),
        api: OpenAiApi::parse(&provider.api).as_str().to_string(),
        base_url: provider.base_url,
        anthropic_version: provider.anthropic_version,
        remote_models: parse_string_array(&provider.remote_models),
        models,
        is_active: provider.is_active == 1,
        has_api_key: !provider.api_key_ciphertext.trim().is_empty(),
        created_at: provider.created_at,
        updated_at: provider.updated_at,
    })
}

async fn build_providers_response(
    db: &Database,
    user_id: i64,
) -> Result<ProvidersResponse, Response> {
    let providers = list_user_providers(db, user_id).await?;
    let active_provider_id = providers
        .iter()
        .find(|provider| provider.is_active == 1)
        .map(|provider| provider.id);

    let mut summaries = Vec::with_capacity(providers.len());
    for provider in providers {
        summaries.push(provider_summary(db, provider).await?);
    }

    Ok(ProvidersResponse {
        providers: summaries,
        active_provider_id,
    })
}

// ---------------------------------------------------------------------------
// Provider CRUD handlers
// ---------------------------------------------------------------------------

pub async fn list_providers(
    Extension(db): Extension<Arc<Database>>,
    headers: HeaderMap,
) -> Result<Json<ProvidersResponse>, Response> {
    let user = auth::require_user(&db, &headers).await?;
    Ok(Json(build_providers_response(&db, user.id).await?))
}

pub async fn save_provider(
    Extension(db): Extension<Arc<Database>>,
    headers: HeaderMap,
    Json(payload): Json<ProviderUpsertRequest>,
) -> Result<Json<ProvidersResponse>, Response> {
    let user = auth::require_user(&db, &headers).await?;
    let name = payload.name.trim();
    if name.is_empty() {
        return Err(json_error(StatusCode::BAD_REQUEST, "请填写供应商名称"));
    }

    let kind = ProviderKind::parse(payload.kind.as_deref().unwrap_or("openai"));
    let api = OpenAiApi::parse(payload.api.as_deref().unwrap_or("responses"));
    let base_url = normalize_base_url(kind, payload.base_url.as_deref());
    let anthropic_version = payload
        .anthropic_version
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    let remote_models = serialize_string_array(&payload.remote_models.unwrap_or_default());

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
            encrypt_api_key(&next_api_key).map_err(|err| {
                json_error(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("供应商 API Key 加密失败: {err}"),
                )
            })?
        };

        sqlx::query(
            "UPDATE agent_providers
             SET name = ?, kind = ?, api = ?, base_url = ?, api_key_ciphertext = ?,
                 anthropic_version = ?, remote_models = ?, updated_at = datetime('now')
             WHERE id = ? AND user_id = ?",
        )
        .bind(name)
        .bind(kind.as_str())
        .bind(api.as_str())
        .bind(&base_url)
        .bind(api_key_ciphertext)
        .bind(&anthropic_version)
        .bind(&remote_models)
        .bind(provider_id)
        .bind(user.id)
        .execute(&db.pool)
        .await
        .map_err(|err| {
            json_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("更新供应商失败: {err}"),
            )
        })?;

        provider_id
    } else {
        let api_key = payload.api_key.unwrap_or_default();
        if api_key.trim().is_empty() {
            return Err(json_error(StatusCode::BAD_REQUEST, "请填写 API Key"));
        }
        let api_key_ciphertext = encrypt_api_key(api_key.trim()).map_err(|err| {
            json_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("供应商 API Key 加密失败: {err}"),
            )
        })?;

        sqlx::query(
            "INSERT INTO agent_providers
                (user_id, name, kind, base_url, api_key_ciphertext, api, anthropic_version, remote_models, is_active)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, 0)",
        )
        .bind(user.id)
        .bind(name)
        .bind(kind.as_str())
        .bind(&base_url)
        .bind(api_key_ciphertext)
        .bind(api.as_str())
        .bind(&anthropic_version)
        .bind(&remote_models)
        .execute(&db.pool)
        .await
        .map_err(|err| {
            json_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("创建供应商失败: {err}"),
            )
        })?
        .last_insert_rowid()
    };

    if existing_active.is_none() {
        set_active_provider(&db, user.id, saved_id).await?;
    }

    Ok(Json(build_providers_response(&db, user.id).await?))
}

pub async fn get_provider(
    Extension(db): Extension<Arc<Database>>,
    headers: HeaderMap,
    Path(provider_id): Path<i64>,
) -> Result<Json<ProviderDetailResponse>, Response> {
    let user = auth::require_user(&db, &headers).await?;
    let provider = find_user_provider(&db, user.id, provider_id).await?;
    let api_key = decrypt_api_key(&provider.api_key_ciphertext).map_err(|err| {
        json_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("读取供应商密钥失败: {err}"),
        )
    })?;
    let models = list_provider_models(&db, provider.id)
        .await?
        .into_iter()
        .map(ModelSummary::from_row)
        .collect();

    Ok(Json(ProviderDetailResponse {
        id: provider.id,
        name: provider.name,
        kind: ProviderKind::parse(&provider.kind).as_str().to_string(),
        api: OpenAiApi::parse(&provider.api).as_str().to_string(),
        base_url: provider.base_url,
        anthropic_version: provider.anthropic_version,
        api_key,
        remote_models: parse_string_array(&provider.remote_models),
        models,
        is_active: provider.is_active == 1,
    }))
}

pub async fn activate_provider(
    Extension(db): Extension<Arc<Database>>,
    headers: HeaderMap,
    Path(provider_id): Path<i64>,
) -> Result<Json<ProvidersResponse>, Response> {
    let user = auth::require_user(&db, &headers).await?;
    let _ = find_user_provider(&db, user.id, provider_id).await?;
    set_active_provider(&db, user.id, provider_id).await?;
    Ok(Json(build_providers_response(&db, user.id).await?))
}

pub async fn delete_provider(
    Extension(db): Extension<Arc<Database>>,
    headers: HeaderMap,
    Path(provider_id): Path<i64>,
) -> Result<Json<ProvidersResponse>, Response> {
    let user = auth::require_user(&db, &headers).await?;
    let provider = find_user_provider(&db, user.id, provider_id).await?;

    sqlx::query("DELETE FROM agent_providers WHERE id = ? AND user_id = ?")
        .bind(provider.id)
        .bind(user.id)
        .execute(&db.pool)
        .await
        .map_err(|err| {
            json_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("删除供应商失败: {err}"),
            )
        })?;

    // 被删的是激活供应商时，回落到剩余第一个。
    if provider.is_active == 1 {
        if let Some(next) = list_user_providers(&db, user.id).await?.first() {
            set_active_provider(&db, user.id, next.id).await?;
        }
    }

    Ok(Json(build_providers_response(&db, user.id).await?))
}

// ---------------------------------------------------------------------------
// Model CRUD handlers
// ---------------------------------------------------------------------------

pub async fn save_model(
    Extension(db): Extension<Arc<Database>>,
    headers: HeaderMap,
    Json(payload): Json<ModelUpsertRequest>,
) -> Result<Json<ProvidersResponse>, Response> {
    let user = auth::require_user(&db, &headers).await?;
    // 校验供应商归属。
    let _ = find_user_provider(&db, user.id, payload.provider_id).await?;

    let alias = payload.alias.trim();
    let model_id = payload.model_id.trim();
    if alias.is_empty() || model_id.is_empty() {
        return Err(json_error(StatusCode::BAD_REQUEST, "请填写模型别名和模型 ID"));
    }
    let display_name = payload
        .display_name
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    let config = payload.config.serialize();

    if let Some(model_row_id) = payload.id {
        // 校验模型归属。
        let _ = find_provider_model(&db, payload.provider_id, model_row_id).await?;
        sqlx::query(
            "UPDATE agent_models
             SET alias = ?, model_id = ?, display_name = ?, config = ?, updated_at = datetime('now')
             WHERE id = ? AND provider_id = ?",
        )
        .bind(alias)
        .bind(model_id)
        .bind(&display_name)
        .bind(&config)
        .bind(model_row_id)
        .bind(payload.provider_id)
        .execute(&db.pool)
        .await
        .map_err(|err| {
            json_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("更新模型失败: {err}"),
            )
        })?;
    } else {
        let next_sort: i64 = sqlx::query_scalar(
            "SELECT COALESCE(MAX(sort_order), 0) + 1 FROM agent_models WHERE provider_id = ?",
        )
        .bind(payload.provider_id)
        .fetch_one(&db.pool)
        .await
        .unwrap_or(0);

        sqlx::query(
            "INSERT INTO agent_models (provider_id, alias, model_id, display_name, config, sort_order)
             VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(payload.provider_id)
        .bind(alias)
        .bind(model_id)
        .bind(&display_name)
        .bind(&config)
        .bind(next_sort)
        .execute(&db.pool)
        .await
        .map_err(|err| {
            json_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("创建模型失败: {err}"),
            )
        })?;
    }

    Ok(Json(build_providers_response(&db, user.id).await?))
}

#[derive(Debug, Deserialize)]
pub struct ModelDeleteRequest {
    pub provider_id: i64,
    pub model_id: i64,
}

pub async fn delete_model(
    Extension(db): Extension<Arc<Database>>,
    headers: HeaderMap,
    Json(payload): Json<ModelDeleteRequest>,
) -> Result<Json<ProvidersResponse>, Response> {
    let user = auth::require_user(&db, &headers).await?;
    let _ = find_user_provider(&db, user.id, payload.provider_id).await?;
    let model = find_provider_model(&db, payload.provider_id, payload.model_id).await?;

    sqlx::query("DELETE FROM agent_models WHERE id = ? AND provider_id = ?")
        .bind(model.id)
        .bind(payload.provider_id)
        .execute(&db.pool)
        .await
        .map_err(|err| {
            json_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("删除模型失败: {err}"),
            )
        })?;

    Ok(Json(build_providers_response(&db, user.id).await?))
}
