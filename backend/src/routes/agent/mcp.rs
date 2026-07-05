//! MCP 服务管理：设置、服务 CRUD、连通性测试、能力快照刷新。
//! 逻辑与重构前一致，仅从巨石模块中拆分为独立模块。

use std::collections::BTreeMap;
use std::sync::Arc;

use axum::{
    extract::{Extension, Path},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::auth;
use crate::db::Database;
use crate::mcp;
use crate::models::{AgentMcpServer, AgentMcpSettings};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "mode", rename_all = "snake_case")]
pub enum McpSecretState<T> {
    Keep,
    Replace { value: T },
    Clear,
}

impl<T> Default for McpSecretState<T> {
    fn default() -> Self {
        Self::Keep
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum McpHttpAuthPayload {
    Bearer {
        token: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        scheme: Option<String>,
    },
    Basic {
        username: String,
        password: String,
    },
    Header {
        name: String,
        value: String,
    },
    Query {
        name: String,
        value: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum McpHttpAuthEditRequest {
    Bearer {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        scheme: Option<String>,
        #[serde(default)]
        token: McpSecretState<String>,
    },
    Basic {
        username: String,
        #[serde(default)]
        password: McpSecretState<String>,
    },
    Header {
        name: String,
        #[serde(default)]
        value: McpSecretState<String>,
    },
    Query {
        name: String,
        #[serde(default)]
        value: McpSecretState<String>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpSettingsUpsertRequest {
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpSettingsResponse {
    pub enabled: bool,
    pub updated_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServerUpsertRequest {
    pub id: Option<i64>,
    pub name: String,
    #[serde(default)]
    pub enabled: Option<bool>,
    pub transport: String,
    pub url: Option<String>,
    pub command: Option<String>,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub auth: McpSecretState<McpHttpAuthEditRequest>,
    #[serde(default)]
    pub custom_headers: McpSecretState<Vec<String>>,
    #[serde(default)]
    pub stdio_env: McpSecretState<Vec<String>>,
}

#[derive(Debug, Clone)]
struct ResolvedDraftMcpRequest {
    existing_row: Option<AgentMcpServer>,
    normalized: NormalizedMcpServerState,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpSecretValueResponse {
    pub has_value: bool,
    pub value: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpSecretEntryResponse {
    pub name: String,
    pub has_value: bool,
    pub value: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpHttpAuthDetailResponse {
    pub auth_type: String,
    pub scheme: Option<String>,
    pub username: Option<String>,
    pub header_name: Option<String>,
    pub query_name: Option<String>,
    pub token: McpSecretValueResponse,
    pub password: McpSecretValueResponse,
    pub value: McpSecretValueResponse,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServerSummaryResponse {
    pub id: i64,
    pub name: String,
    pub enabled: bool,
    pub transport: String,
    pub url: Option<String>,
    pub command: Option<String>,
    pub auth_type: String,
    pub last_status: Option<String>,
    pub last_error: Option<String>,
    pub config_version: i64,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServerDetailResponse {
    pub id: i64,
    pub name: String,
    pub enabled: bool,
    pub transport: String,
    pub url: Option<String>,
    pub command: Option<String>,
    pub args: Vec<String>,
    pub auth: Option<McpHttpAuthDetailResponse>,
    pub custom_headers: Vec<McpSecretEntryResponse>,
    pub stdio_env: Vec<McpSecretEntryResponse>,
    pub last_status: Option<String>,
    pub last_error: Option<String>,
    pub tools_snapshot: Value,
    pub resources_snapshot: Value,
    pub prompts_snapshot: Value,
    pub last_sync_at: Option<String>,
    pub config_version: i64,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServersResponse {
    pub servers: Vec<McpServerSummaryResponse>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct McpRuntimeCapabilities {
    pub transports: Vec<String>,
    pub stdio_enabled: bool,
    pub stdio_allowed_commands: Vec<String>,
}

fn runtime_mcp_capabilities(
    runtime_config: &crate::BackendRuntimeConfig,
) -> McpRuntimeCapabilities {
    let mut transports = vec!["sse".to_string(), "streamable-http".to_string()];
    if runtime_config.mcp_stdio_enabled {
        transports.push("stdio".to_string());
    }

    McpRuntimeCapabilities {
        transports,
        stdio_enabled: runtime_config.mcp_stdio_enabled,
        stdio_allowed_commands: runtime_config.mcp_stdio_allowed_commands.clone(),
    }
}

pub async fn get_runtime_capabilities(
    Extension(db): Extension<Arc<Database>>,
    headers: HeaderMap,
    Extension(runtime_config): Extension<Arc<crate::BackendRuntimeConfig>>,
) -> Result<Json<McpRuntimeCapabilities>, Response> {
    let _user = auth::require_user(&db, &headers).await?;
    Ok(Json(runtime_mcp_capabilities(&runtime_config)))
}

fn mcp_json_error(status: StatusCode, message: &str) -> Response {
    (status, Json(json!({ "error": message }))).into_response()
}

fn trim_optional(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| value.to_string())
}

fn normalize_mcp_args(values: &[String]) -> Vec<String> {
    values
        .iter()
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
        .map(|value| value.to_string())
        .collect()
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct NormalizedMcpServerState {
    name: String,
    enabled: bool,
    transport: String,
    url: Option<String>,
    command: Option<String>,
    args: Vec<String>,
    auth_type: String,
    auth_payload: Option<McpHttpAuthPayload>,
    custom_headers: BTreeMap<String, String>,
    stdio_env: BTreeMap<String, String>,
}

fn parse_mcp_secret_lines(
    state: &McpSecretState<Vec<String>>,
    existing: Option<BTreeMap<String, String>>,
    parser: fn(&[String]) -> anyhow::Result<BTreeMap<String, String>>,
) -> anyhow::Result<BTreeMap<String, String>> {
    match state {
        McpSecretState::Keep => Ok(existing.unwrap_or_default()),
        McpSecretState::Clear => Ok(BTreeMap::new()),
        McpSecretState::Replace { value } => parser(value),
    }
}

fn load_mcp_server_state(
    row: &AgentMcpServer,
) -> Result<NormalizedMcpServerState, Response> {
    let auth_payload = match row.auth_config_ciphertext.as_deref() {
        Some(ciphertext) => match mcp::decrypt_secret_json::<McpHttpAuthPayload>(ciphertext) {
            Ok(value) => Some(value),
            Err(err) => {
                tracing::error!("decrypt MCP auth payload failed for {}: {}", row.id, err);
                return Err(mcp_json_error(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "读取 MCP 认证配置失败",
                ));
            }
        },
        None => None,
    };

    let custom_headers = match row.custom_headers_ciphertext.as_deref() {
        Some(ciphertext) => match mcp::decrypt_secret_json::<BTreeMap<String, String>>(ciphertext)
        {
            Ok(value) => value,
            Err(err) => {
                tracing::error!("decrypt MCP headers failed for {}: {}", row.id, err);
                return Err(mcp_json_error(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "读取 MCP 自定义请求头失败",
                ));
            }
        },
        None => BTreeMap::new(),
    };

    let stdio_env = match row.env_ciphertext.as_deref() {
        Some(ciphertext) => match mcp::decrypt_secret_json::<BTreeMap<String, String>>(ciphertext)
        {
            Ok(value) => value,
            Err(err) => {
                tracing::error!("decrypt MCP stdio env failed for {}: {}", row.id, err);
                return Err(mcp_json_error(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "读取 MCP stdio 环境变量失败",
                ));
            }
        },
        None => BTreeMap::new(),
    };

    let auth_type = if let Some(payload) = &auth_payload {
        mcp::auth_payload_kind(payload).to_string()
    } else {
        row.auth_type.trim().to_ascii_lowercase()
    };

    Ok(NormalizedMcpServerState {
        name: row.name.trim().to_string(),
        enabled: row.enabled == 1,
        transport: mcp::normalize_transport(&row.transport).unwrap_or_else(|| row.transport.clone()),
        url: trim_optional(row.url.as_deref()),
        command: trim_optional(row.command.as_deref()),
        args: parse_json_string_array(&row.args_json),
        auth_type,
        auth_payload,
        custom_headers,
        stdio_env,
    })
}

fn normalize_mcp_server_state_from_request(
    request: &McpServerUpsertRequest,
    existing: Option<&NormalizedMcpServerState>,
    transport: &str,
) -> Result<NormalizedMcpServerState, Response> {
    let name = request.name.trim();
    if name.is_empty() {
        return Err(mcp_json_error(
            StatusCode::BAD_REQUEST,
            "MCP 服务名称不能为空",
        ));
    }

    let enabled = match existing {
        Some(existing) => request.enabled.unwrap_or(existing.enabled),
        None => request.enabled.unwrap_or(true),
    };
    let args = if transport == "stdio" {
        normalize_mcp_args(&request.args)
    } else {
        Vec::new()
    };

    let url = if transport == "stdio" {
        None
    } else {
        match request.url.as_deref() {
            Some(url) => match mcp::normalize_http_mcp_url(url) {
                Ok(value) => Some(value),
                Err(err) => {
                    return Err(mcp_json_error(StatusCode::BAD_REQUEST, &err.to_string()));
                }
            },
            None => {
                if existing.is_some() {
                    existing.and_then(|state| state.url.clone())
                } else {
                    None
                }
            }
        }
    };

    let command = if transport == "stdio" {
        trim_optional(request.command.as_deref())
    } else {
        None
    };

    let auth_payload = if transport == "stdio" {
        None
    } else {
        match mcp::resolve_http_auth_update(
            existing.and_then(|state| state.auth_payload.clone()),
            &request.auth,
        ) {
            Ok(value) => value,
            Err(err) => {
                return Err(mcp_json_error(StatusCode::BAD_REQUEST, &err.to_string()));
            }
        }
    };

    let custom_headers = if transport == "stdio" {
        BTreeMap::new()
    } else {
        parse_mcp_secret_lines(
            &request.custom_headers,
            existing.map(|state| state.custom_headers.clone()),
            mcp::parse_custom_header_lines,
        )
        .map_err(|err| mcp_json_error(StatusCode::BAD_REQUEST, &err.to_string()))?
    };

    let stdio_env = if transport == "stdio" {
        parse_mcp_secret_lines(
            &request.stdio_env,
            existing.map(|state| state.stdio_env.clone()),
            mcp::parse_stdio_env_lines,
        )
        .map_err(|err| mcp_json_error(StatusCode::BAD_REQUEST, &err.to_string()))?
    } else {
        BTreeMap::new()
    };

    if let Err(err) = mcp::validate_mcp_header_collisions(auth_payload.as_ref(), &custom_headers) {
        return Err(mcp_json_error(StatusCode::BAD_REQUEST, &err.to_string()));
    }

    let auth_type = auth_payload
        .as_ref()
        .map(|payload| mcp::auth_payload_kind(payload).to_string())
        .unwrap_or_else(|| "none".to_string());

    Ok(NormalizedMcpServerState {
        name: name.to_string(),
        enabled,
        transport: transport.to_string(),
        url,
        command,
        args,
        auth_type,
        auth_payload,
        custom_headers,
        stdio_env,
    })
}

async fn resolve_draft_mcp_request(
    db: &Database,
    runtime_config: &crate::BackendRuntimeConfig,
    user_id: i64,
    request: &McpServerUpsertRequest,
    force_require_complete_connection_fields: bool,
) -> Result<ResolvedDraftMcpRequest, Response> {
    let existing = if let Some(id) = request.id {
        match load_existing_mcp_server_state(db, user_id, id).await {
            Ok((row, state)) => Some((row, state)),
            Err(resp) if resp.status() == StatusCode::NOT_FOUND => {
                return Err(resp);
            }
            Err(resp) => return Err(resp),
        }
    } else {
        None
    };

    let effective_enabled = request.enabled.unwrap_or_else(|| {
        existing
            .as_ref()
            .map(|(_, state)| state.enabled)
            .unwrap_or(true)
    });
    let require_complete_connection_fields =
        force_require_complete_connection_fields || effective_enabled;

    if let Err(err) = mcp::normalize_mcp_server_upsert_request(
        request,
        runtime_config,
        existing
            .as_ref()
            .and_then(|(_, state)| state.url.as_deref()),
        require_complete_connection_fields,
    )
    .await
    {
        return Err(mcp_json_error(StatusCode::BAD_REQUEST, &err.to_string()));
    }

    let transport = match mcp::normalize_transport(&request.transport) {
        Some(value) => value,
        None => {
            return Err(mcp_json_error(
                StatusCode::BAD_REQUEST,
                "不支持的 MCP 传输方式",
            ));
        }
    };

    let existing_row = existing.as_ref().map(|(row, _)| row);
    let existing_state = existing.as_ref().map(|(_, state)| state);
    let normalized = normalize_mcp_server_state_from_request(request, existing_state, &transport)?;

    Ok(ResolvedDraftMcpRequest {
        existing_row: existing_row.cloned(),
        normalized,
    })
}

fn draft_runtime_server_config(
    user_id: i64,
    existing_row: Option<&AgentMcpServer>,
    normalized: &NormalizedMcpServerState,
) -> mcp::ResolvedMcpServerConfig {
    let persisted_id = existing_row.map(|row| row.id).unwrap_or(0);
    let persisted_version = existing_row.map(|row| row.config_version).unwrap_or(0);

    mcp::ResolvedMcpServerConfig {
        id: if persisted_id > 0 { -persisted_id } else { i64::MIN + user_id.abs() },
        user_id,
        name: normalized.name.clone(),
        enabled: normalized.enabled,
        transport: normalized.transport.clone(),
        url: normalized.url.clone(),
        command: normalized.command.clone(),
        args: normalized.args.clone(),
        auth_type: normalized.auth_type.clone(),
        auth_payload: normalized.auth_payload.clone(),
        custom_headers: normalized.custom_headers.clone(),
        stdio_env: normalized.stdio_env.clone(),
        config_version: persisted_version + 1,
    }
}

fn server_summary_response(row: &AgentMcpServer) -> McpServerSummaryResponse {
    McpServerSummaryResponse {
        id: row.id,
        name: row.name.clone(),
        enabled: row.enabled == 1,
        transport: row.transport.clone(),
        url: row.url.clone(),
        command: row.command.clone(),
        auth_type: row.auth_type.clone(),
        last_status: row.last_status.clone(),
        last_error: row.last_error.clone(),
        config_version: row.config_version,
        created_at: row.created_at.clone(),
        updated_at: row.updated_at.clone(),
    }
}

fn parse_snapshot(raw: &str) -> Value {
    serde_json::from_str(raw).unwrap_or_else(|_| json!([]))
}

fn server_detail_response(row: &AgentMcpServer) -> Result<McpServerDetailResponse, Response> {
    let auth = match row.auth_config_ciphertext.as_deref() {
        Some(ciphertext) => {
            let payload = mcp::decrypt_secret_json::<McpHttpAuthPayload>(ciphertext).map_err(
                |err| {
                    tracing::error!("decrypt MCP auth payload failed for {}: {}", row.id, err);
                    mcp_json_error(
                        StatusCode::INTERNAL_SERVER_ERROR,
                        "读取 MCP 认证配置失败",
                    )
                },
            )?;
            Some(mcp::reveal_auth_payload(&payload))
        }
        None => None,
    };

    let custom_headers = match row.custom_headers_ciphertext.as_deref() {
        Some(ciphertext) => {
            let headers = mcp::decrypt_secret_json::<BTreeMap<String, String>>(ciphertext)
                .map_err(|err| {
                    tracing::error!("decrypt MCP headers failed for {}: {}", row.id, err);
                    mcp_json_error(
                        StatusCode::INTERNAL_SERVER_ERROR,
                        "读取 MCP 自定义请求头失败",
                    )
                })?;
            mcp::reveal_headers(&headers)
        }
        None => Vec::new(),
    };

    let stdio_env = match row.env_ciphertext.as_deref() {
        Some(ciphertext) => {
            let env = mcp::decrypt_secret_json::<BTreeMap<String, String>>(ciphertext).map_err(
                |err| {
                    tracing::error!("decrypt MCP stdio env failed for {}: {}", row.id, err);
                    mcp_json_error(
                        StatusCode::INTERNAL_SERVER_ERROR,
                        "读取 MCP stdio 环境变量失败",
                    )
                },
            )?;
            mcp::reveal_stdio_env(&env)
        }
        None => Vec::new(),
    };

    let args = parse_json_string_array(&row.args_json);

    Ok(McpServerDetailResponse {
        id: row.id,
        name: row.name.clone(),
        enabled: row.enabled == 1,
        transport: row.transport.clone(),
        url: row.url.clone(),
        command: row.command.clone(),
        args,
        auth,
        custom_headers,
        stdio_env,
        last_status: row.last_status.clone(),
        last_error: row.last_error.clone(),
        tools_snapshot: parse_snapshot(&row.tools_snapshot),
        resources_snapshot: parse_snapshot(&row.resources_snapshot),
        prompts_snapshot: parse_snapshot(&row.prompts_snapshot),
        last_sync_at: row.last_sync_at.clone(),
        config_version: row.config_version,
        created_at: row.created_at.clone(),
        updated_at: row.updated_at.clone(),
    })
}

fn draft_server_detail_response(
    existing_row: Option<&AgentMcpServer>,
    normalized: &NormalizedMcpServerState,
    status: Option<&str>,
    error_message: Option<&str>,
    snapshot: Option<&mcp::McpCapabilitySnapshot>,
) -> McpServerDetailResponse {
    let auth = normalized
        .auth_payload
        .as_ref()
        .map(mcp::reveal_auth_payload);
    let custom_headers = mcp::reveal_headers(&normalized.custom_headers);
    let stdio_env = mcp::reveal_stdio_env(&normalized.stdio_env);
    let now = chrono::Utc::now().to_rfc3339();
    let (tools_snapshot, resources_snapshot, prompts_snapshot) = match snapshot {
        Some(value) => (
            value.tools.clone(),
            value.resources.clone(),
            value.prompts.clone(),
        ),
        None => (json!([]), json!([]), json!([])),
    };

    McpServerDetailResponse {
        id: existing_row.map(|row| row.id).unwrap_or(0),
        name: normalized.name.clone(),
        enabled: normalized.enabled,
        transport: normalized.transport.clone(),
        url: normalized.url.clone(),
        command: normalized.command.clone(),
        args: normalized.args.clone(),
        auth,
        custom_headers,
        stdio_env,
        last_status: status.map(|value| value.to_string()),
        last_error: error_message.map(|value| value.to_string()),
        tools_snapshot,
        resources_snapshot,
        prompts_snapshot,
        last_sync_at: snapshot.map(|_| now.clone()),
        config_version: existing_row.map(|row| row.config_version).unwrap_or(0),
        created_at: existing_row
            .map(|row| row.created_at.clone())
            .unwrap_or_else(|| now.clone()),
        updated_at: now,
    }
}

pub(super) async fn load_mcp_settings(
    db: &Database,
    user_id: i64,
) -> Result<Option<AgentMcpSettings>, Response> {
    sqlx::query_as::<_, AgentMcpSettings>("SELECT * FROM agent_mcp_settings WHERE user_id = ?")
        .bind(user_id)
        .fetch_optional(&db.pool)
        .await
        .map_err(|err| {
            tracing::error!("load MCP settings failed: {}", err);
            mcp_json_error(StatusCode::INTERNAL_SERVER_ERROR, "读取 MCP 设置失败")
        })
}

pub(super) async fn list_mcp_servers(
    db: &Database,
    user_id: i64,
) -> Result<Vec<AgentMcpServer>, Response> {
    sqlx::query_as::<_, AgentMcpServer>(
        "SELECT * FROM agent_mcp_servers WHERE user_id = ? ORDER BY updated_at DESC, id DESC",
    )
    .bind(user_id)
    .fetch_all(&db.pool)
    .await
    .map_err(|err| {
        tracing::error!("load MCP servers failed: {}", err);
        mcp_json_error(StatusCode::INTERNAL_SERVER_ERROR, "读取 MCP 服务列表失败")
    })
}

async fn load_mcp_server(
    db: &Database,
    user_id: i64,
    server_id: i64,
) -> Result<AgentMcpServer, Response> {
    sqlx::query_as::<_, AgentMcpServer>(
        "SELECT * FROM agent_mcp_servers WHERE id = ? AND user_id = ?",
    )
    .bind(server_id)
    .bind(user_id)
    .fetch_optional(&db.pool)
    .await
    .map_err(|err| {
        tracing::error!("load MCP server failed: {}", err);
        mcp_json_error(StatusCode::INTERNAL_SERVER_ERROR, "读取 MCP 服务失败")
    })?
    .ok_or_else(|| mcp_json_error(StatusCode::NOT_FOUND, "MCP 服务不存在"))
}

pub async fn get_mcp_settings(
    Extension(db): Extension<Arc<Database>>,
    headers: HeaderMap,
) -> impl IntoResponse {
    let user = match auth::require_user(&db, &headers).await {
        Ok(user) => user,
        Err(resp) => return resp,
    };

    match load_mcp_settings(&db, user.id).await {
        Ok(Some(settings)) => Json(json!({
            "settings": McpSettingsResponse {
                enabled: settings.enabled == 1,
                updated_at: Some(settings.updated_at),
            }
        }))
        .into_response(),
        Ok(None) => Json(json!({
            "settings": McpSettingsResponse {
                enabled: false,
                updated_at: None,
            }
        }))
        .into_response(),
        Err(resp) => resp,
    }
}

pub async fn update_mcp_settings(
    Extension(db): Extension<Arc<Database>>,
    headers: HeaderMap,
    Json(body): Json<McpSettingsUpsertRequest>,
) -> impl IntoResponse {
    let user = match auth::require_user(&db, &headers).await {
        Ok(user) => user,
        Err(resp) => return resp,
    };

    if let Err(err) = sqlx::query(
        r#"
        INSERT INTO agent_mcp_settings (user_id, enabled)
        VALUES (?, ?)
        ON CONFLICT(user_id) DO UPDATE SET
            enabled = excluded.enabled,
            updated_at = datetime('now')
        "#,
    )
    .bind(user.id)
    .bind(if body.enabled { 1 } else { 0 })
    .execute(&db.pool)
    .await
    {
        tracing::error!("save MCP settings failed: {}", err);
        return mcp_json_error(StatusCode::INTERNAL_SERVER_ERROR, "保存 MCP 设置失败");
    }

    match load_mcp_settings(&db, user.id).await {
        Ok(Some(settings)) => Json(json!({
            "settings": McpSettingsResponse {
                enabled: settings.enabled == 1,
                updated_at: Some(settings.updated_at),
            }
        }))
        .into_response(),
        Ok(None) => mcp_json_error(StatusCode::INTERNAL_SERVER_ERROR, "读取 MCP 设置失败"),
        Err(resp) => resp,
    }
}

pub async fn list_mcp_servers_handler(
    Extension(db): Extension<Arc<Database>>,
    headers: HeaderMap,
) -> impl IntoResponse {
    let user = match auth::require_user(&db, &headers).await {
        Ok(user) => user,
        Err(resp) => return resp,
    };

    match list_mcp_servers(&db, user.id).await {
        Ok(servers) => {
            let servers = servers
                .into_iter()
                .map(|server| server_summary_response(&server))
                .collect::<Vec<_>>();
            Json(McpServersResponse { servers }).into_response()
        }
        Err(resp) => resp,
    }
}

pub async fn get_mcp_server(
    Extension(db): Extension<Arc<Database>>,
    headers: HeaderMap,
    Path(id): Path<i64>,
) -> impl IntoResponse {
    let user = match auth::require_user(&db, &headers).await {
        Ok(user) => user,
        Err(resp) => return resp,
    };

    let row = match load_mcp_server(&db, user.id, id).await {
        Ok(row) => row,
        Err(resp) => return resp,
    };

    match server_detail_response(&row) {
        Ok(server) => Json(json!({ "server": server })).into_response(),
        Err(resp) => resp,
    }
}

async fn resolve_mcp_server_state_from_row(
    row: &AgentMcpServer,
) -> Result<NormalizedMcpServerState, Response> {
    load_mcp_server_state(row)
}

async fn load_existing_mcp_server_state(
    db: &Database,
    user_id: i64,
    server_id: i64,
) -> Result<(AgentMcpServer, NormalizedMcpServerState), Response> {
    let row = load_mcp_server(db, user_id, server_id).await?;
    let state = resolve_mcp_server_state_from_row(&row).await?;
    Ok((row, state))
}

async fn persist_mcp_server_state(
    db: &Database,
    user_id: i64,
    existing_row: Option<&AgentMcpServer>,
    state: &NormalizedMcpServerState,
) -> Result<i64, Response> {
    let auth_ciphertext = match &state.auth_payload {
        Some(payload) => Some(mcp::encrypt_secret_json(payload).map_err(|err| {
            tracing::error!("encrypt MCP auth payload failed: {}", err);
            mcp_json_error(StatusCode::INTERNAL_SERVER_ERROR, "保存 MCP 认证配置失败")
        })?),
        None => None,
    };

    let custom_headers_ciphertext = if state.custom_headers.is_empty() {
        None
    } else {
        Some(mcp::encrypt_secret_json(&state.custom_headers).map_err(|err| {
            tracing::error!("encrypt MCP headers failed: {}", err);
            mcp_json_error(StatusCode::INTERNAL_SERVER_ERROR, "保存 MCP 自定义请求头失败")
        })?)
    };

    let stdio_env_ciphertext = if state.stdio_env.is_empty() {
        None
    } else {
        Some(mcp::encrypt_secret_json(&state.stdio_env).map_err(|err| {
            tracing::error!("encrypt MCP stdio env failed: {}", err);
            mcp_json_error(StatusCode::INTERNAL_SERVER_ERROR, "保存 MCP stdio 环境变量失败")
        })?)
    };

    let args_json = serialize_json_string_array(&state.args);
    let transport = state.transport.as_str();
    let created_or_updated_id = if let Some(existing) = existing_row {
        let changed = {
            let existing_state = load_mcp_server_state(existing)?;
            existing_state != *state
        };

        if !changed {
            return Ok(existing.id);
        }

        let query = sqlx::query(
            r#"
            UPDATE agent_mcp_servers
               SET name = ?,
                   enabled = ?,
                   transport = ?,
                   url = ?,
                   command = ?,
                   args_json = ?,
                   env_ciphertext = ?,
                   auth_type = ?,
                   auth_config_ciphertext = ?,
                   custom_headers_ciphertext = ?,
                   last_status = NULL,
                   last_error = NULL,
                   tools_snapshot = '[]',
                   resources_snapshot = '[]',
                   prompts_snapshot = '[]',
                   last_sync_at = NULL,
                   config_version = config_version + 1,
                   updated_at = datetime('now')
             WHERE id = ? AND user_id = ?
        "#,
        );
        query
            .bind(&state.name)
            .bind(if state.enabled { 1 } else { 0 })
            .bind(transport)
            .bind(&state.url)
            .bind(&state.command)
            .bind(&args_json)
            .bind(&stdio_env_ciphertext)
            .bind(&state.auth_type)
            .bind(&auth_ciphertext)
            .bind(&custom_headers_ciphertext)
            .bind(existing.id)
            .bind(user_id)
            .execute(&db.pool)
            .await
            .map_err(|err| {
                tracing::error!("update MCP server failed: {}", err);
                mcp_json_error(StatusCode::INTERNAL_SERVER_ERROR, "保存 MCP 服务失败")
            })?;

        existing.id
    } else {
        let result = sqlx::query(
            r#"
            INSERT INTO agent_mcp_servers (
                user_id, name, enabled, transport, url, command, args_json,
                env_ciphertext, auth_type, auth_config_ciphertext,
                custom_headers_ciphertext, tools_snapshot, resources_snapshot,
                prompts_snapshot, last_status, last_error, last_sync_at, config_version
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, '[]', '[]', '[]', NULL, NULL, NULL, 1)
        "#,
        )
        .bind(user_id)
        .bind(&state.name)
        .bind(if state.enabled { 1 } else { 0 })
        .bind(transport)
        .bind(&state.url)
        .bind(&state.command)
        .bind(&args_json)
        .bind(&stdio_env_ciphertext)
        .bind(&state.auth_type)
        .bind(&auth_ciphertext)
        .bind(&custom_headers_ciphertext)
        .execute(&db.pool)
        .await
        .map_err(|err| {
            tracing::error!("create MCP server failed: {}", err);
            mcp_json_error(StatusCode::INTERNAL_SERVER_ERROR, "创建 MCP 服务失败")
        })?;

        result.last_insert_rowid()
    };

    Ok(created_or_updated_id)
}

fn mcp_server_body_response(
    row: &AgentMcpServer,
) -> Result<Json<serde_json::Value>, Response> {
    let server = server_detail_response(row)?;
    Ok(Json(json!({ "server": server })))
}

pub async fn save_mcp_server(
    Extension(db): Extension<Arc<Database>>,
    headers: HeaderMap,
    Extension(runtime_config): Extension<Arc<crate::BackendRuntimeConfig>>,
    Extension(mcp_manager): Extension<Arc<crate::mcp::McpConnectionManager>>,
    Json(body): Json<McpServerUpsertRequest>,
) -> impl IntoResponse {
    let user = match auth::require_user(&db, &headers).await {
        Ok(user) => user,
        Err(resp) => return resp,
    };

    let resolved = match resolve_draft_mcp_request(
        &db,
        &runtime_config,
        user.id,
        &body,
        false,
    )
    .await
    {
        Ok(value) => value,
        Err(resp) => return resp,
    };
    let existing_row = resolved.existing_row.as_ref();
    let normalized = resolved.normalized;

    let server_id = match persist_mcp_server_state(&db, user.id, existing_row, &normalized)
    .await
    {
        Ok(id) => id,
        Err(resp) => return resp,
    };

    if existing_row.is_some() {
        mcp_manager.invalidate_server(user.id, server_id).await;
    }

    let row = match load_mcp_server(&db, user.id, server_id).await {
        Ok(row) => row,
        Err(resp) => return resp,
    };

    match mcp_server_body_response(&row) {
        Ok(body) => {
            let status = if existing_row.is_some() {
                StatusCode::OK
            } else {
                StatusCode::CREATED
            };
            (status, body).into_response()
        }
        Err(resp) => resp,
    }
}

async fn run_mcp_draft_action(
    db: &Database,
    runtime_config: &crate::BackendRuntimeConfig,
    user_id: i64,
    body: &McpServerUpsertRequest,
) -> Result<McpServerDetailResponse, Response> {
    let resolved = resolve_draft_mcp_request(db, runtime_config, user_id, body, true).await?;
    let runtime = draft_runtime_server_config(user_id, resolved.existing_row.as_ref(), &resolved.normalized);

    match mcp::discover_mcp_capabilities_once(runtime_config, &runtime).await {
        Ok(snapshot) => Ok(draft_server_detail_response(
            resolved.existing_row.as_ref(),
            &resolved.normalized,
            Some("ready"),
            None,
            Some(&snapshot),
        )),
        Err(err) => {
            let error_message = err.to_string();
            tracing::error!(
                draft_server_id = body.id,
                draft_server_name = %resolved.normalized.name,
                transport = %runtime.transport,
                url = ?runtime.url,
                command = ?runtime.command,
                "MCP draft refresh failed: {}",
                error_message
            );
            Err((
                StatusCode::BAD_GATEWAY,
                Json(json!({
                    "error": error_message,
                    "server": draft_server_detail_response(
                        resolved.existing_row.as_ref(),
                        &resolved.normalized,
                        Some("error"),
                        Some(&error_message),
                        None,
                    ),
                })),
            )
                .into_response())
        }
    }
}

pub async fn delete_mcp_server(
    Extension(db): Extension<Arc<Database>>,
    Extension(mcp_manager): Extension<Arc<crate::mcp::McpConnectionManager>>,
    headers: HeaderMap,
    Path(id): Path<i64>,
) -> impl IntoResponse {
    let user = match auth::require_user(&db, &headers).await {
        Ok(user) => user,
        Err(resp) => return resp,
    };

    let result = sqlx::query("DELETE FROM agent_mcp_servers WHERE id = ? AND user_id = ?")
        .bind(id)
        .bind(user.id)
        .execute(&db.pool)
        .await;

    match result {
        Ok(result) if result.rows_affected() > 0 => {
            mcp_manager.invalidate_server(user.id, id).await;
            Json(json!({"message": "删除成功"})).into_response()
        }
        Ok(_) => mcp_json_error(StatusCode::NOT_FOUND, "MCP 服务不存在"),
        Err(err) => {
            tracing::error!("delete MCP server failed: {}", err);
            mcp_json_error(StatusCode::INTERNAL_SERVER_ERROR, "删除 MCP 服务失败")
        }
    }
}

async fn persist_mcp_refresh_success(
    db: &Database,
    user_id: i64,
    server_id: i64,
    snapshot: &mcp::McpCapabilitySnapshot,
) -> Result<(), Response> {
    let (tools_snapshot, resources_snapshot, prompts_snapshot) =
        mcp::capability_snapshot_json(snapshot);
    sqlx::query(
        r#"
        UPDATE agent_mcp_servers
           SET last_status = 'ready',
               last_error = NULL,
               tools_snapshot = ?,
               resources_snapshot = ?,
               prompts_snapshot = ?,
               last_sync_at = datetime('now'),
               updated_at = datetime('now')
         WHERE id = ? AND user_id = ?
        "#,
    )
    .bind(tools_snapshot)
    .bind(resources_snapshot)
    .bind(prompts_snapshot)
    .bind(server_id)
    .bind(user_id)
    .execute(&db.pool)
    .await
    .map_err(|err| {
        tracing::error!("persist MCP refresh success failed: {}", err);
        mcp_json_error(StatusCode::INTERNAL_SERVER_ERROR, "保存 MCP 能力快照失败")
    })?;
    Ok(())
}

async fn persist_mcp_refresh_failure(
    db: &Database,
    user_id: i64,
    server_id: i64,
    error_message: &str,
) -> Result<(), Response> {
    sqlx::query(
        r#"
        UPDATE agent_mcp_servers
           SET last_status = 'error',
               last_error = ?,
               updated_at = datetime('now')
         WHERE id = ? AND user_id = ?
        "#,
    )
    .bind(error_message)
    .bind(server_id)
    .bind(user_id)
    .execute(&db.pool)
    .await
    .map_err(|err| {
        tracing::error!("persist MCP refresh failure failed: {}", err);
        mcp_json_error(StatusCode::INTERNAL_SERVER_ERROR, "保存 MCP 刷新状态失败")
    })?;
    Ok(())
}

async fn run_mcp_refresh_action(
    db: &Database,
    runtime_config: &crate::BackendRuntimeConfig,
    mcp_manager: &crate::mcp::McpConnectionManager,
    user_id: i64,
    server_id: i64,
) -> Result<AgentMcpServer, Response> {
    let row = load_mcp_server(db, user_id, server_id).await?;
    let config = mcp::load_runtime_server_config(&row)
        .map_err(|err| mcp_json_error(StatusCode::INTERNAL_SERVER_ERROR, &format!("读取 MCP 配置失败: {err}")))?;

    match mcp::refresh_mcp_capabilities(mcp_manager, runtime_config, &config, true).await {
        Ok(snapshot) => {
            persist_mcp_refresh_success(db, user_id, server_id, &snapshot).await?;
        }
        Err(err) => {
            let error_message = err.to_string();
            tracing::error!(
                server_id = server_id,
                server_name = %row.name,
                transport = %config.transport,
                url = ?config.url,
                command = ?config.command,
                "MCP refresh failed: {}",
                error_message
            );
            mcp_manager.invalidate_server(user_id, server_id).await;
            persist_mcp_refresh_failure(db, user_id, server_id, &error_message).await?;
            let server = load_mcp_server(db, user_id, server_id).await.ok();
            let server = server
                .as_ref()
                .and_then(|row| server_detail_response(row).ok());
            return Err((
                StatusCode::BAD_GATEWAY,
                Json(json!({
                    "error": error_message,
                    "server": server,
                })),
            )
                .into_response());
        }
    }

    load_mcp_server(db, user_id, server_id).await
}

pub async fn test_mcp_server(
    Extension(db): Extension<Arc<Database>>,
    Extension(runtime_config): Extension<Arc<crate::BackendRuntimeConfig>>,
    Extension(mcp_manager): Extension<Arc<crate::mcp::McpConnectionManager>>,
    headers: HeaderMap,
    Path(id): Path<i64>,
) -> impl IntoResponse {
    let user = match auth::require_user(&db, &headers).await {
        Ok(user) => user,
        Err(resp) => return resp,
    };

    match run_mcp_refresh_action(&db, &runtime_config, &mcp_manager, user.id, id).await {
        Ok(row) => match server_detail_response(&row) {
            Ok(server) => Json(json!({ "server": server })).into_response(),
            Err(resp) => resp,
        },
        Err(resp) => resp,
    }
}

pub async fn test_mcp_server_draft(
    Extension(db): Extension<Arc<Database>>,
    Extension(runtime_config): Extension<Arc<crate::BackendRuntimeConfig>>,
    headers: HeaderMap,
    Json(body): Json<McpServerUpsertRequest>,
) -> impl IntoResponse {
    let user = match auth::require_user(&db, &headers).await {
        Ok(user) => user,
        Err(resp) => return resp,
    };

    match run_mcp_draft_action(&db, &runtime_config, user.id, &body).await {
        Ok(server) => Json(json!({ "server": server })).into_response(),
        Err(resp) => resp,
    }
}

pub async fn refresh_mcp_server(
    Extension(db): Extension<Arc<Database>>,
    Extension(runtime_config): Extension<Arc<crate::BackendRuntimeConfig>>,
    Extension(mcp_manager): Extension<Arc<crate::mcp::McpConnectionManager>>,
    headers: HeaderMap,
    Path(id): Path<i64>,
) -> impl IntoResponse {
    let user = match auth::require_user(&db, &headers).await {
        Ok(user) => user,
        Err(resp) => return resp,
    };

    match run_mcp_refresh_action(&db, &runtime_config, &mcp_manager, user.id, id).await {
        Ok(row) => match server_detail_response(&row) {
            Ok(server) => Json(json!({ "server": server })).into_response(),
            Err(resp) => resp,
        },
        Err(resp) => resp,
    }
}

pub async fn refresh_mcp_server_draft(
    Extension(db): Extension<Arc<Database>>,
    Extension(runtime_config): Extension<Arc<crate::BackendRuntimeConfig>>,
    headers: HeaderMap,
    Json(body): Json<McpServerUpsertRequest>,
) -> impl IntoResponse {
    let user = match auth::require_user(&db, &headers).await {
        Ok(user) => user,
        Err(resp) => return resp,
    };

    match run_mcp_draft_action(&db, &runtime_config, user.id, &body).await {
        Ok(server) => Json(json!({ "server": server })).into_response(),
        Err(resp) => resp,
    }
}

// --- JSON 字符串数组 helper（MCP args 序列化用） ---

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

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;
    use axum::{
        body::{to_bytes, Body},
        http::{Request, StatusCode},
        routing::get,
        Router,
    };
    use tower::ServiceExt;

    #[test]
    fn runtime_mcp_capabilities_defaults_to_sse_and_streamable_http() {
        let config = crate::BackendRuntimeConfig {
            mcp_stdio_enabled: false,
            mcp_stdio_allowed_commands: vec![],
            skills_root_dir: String::new(),
            ..Default::default()
        };

        let capabilities = runtime_mcp_capabilities(&config);

        assert_eq!(
            capabilities.transports,
            vec!["sse".to_string(), "streamable-http".to_string()]
        );
        assert!(!capabilities.stdio_enabled);
        assert!(capabilities.stdio_allowed_commands.is_empty());
    }

    #[test]
    fn runtime_mcp_capabilities_includes_stdio_when_enabled() {
        let config = crate::BackendRuntimeConfig {
            mcp_stdio_enabled: true,
            mcp_stdio_allowed_commands: vec!["git".to_string(), "node".to_string()],
            skills_root_dir: String::new(),
            ..Default::default()
        };

        let capabilities = runtime_mcp_capabilities(&config);

        assert_eq!(
            capabilities.transports,
            vec![
                "sse".to_string(),
                "streamable-http".to_string(),
                "stdio".to_string()
            ]
        );
        assert!(capabilities.stdio_enabled);
        assert_eq!(
            capabilities.stdio_allowed_commands,
            vec!["git".to_string(), "node".to_string()]
        );
    }

    #[test]
    fn mcp_draft_runtime_config_uses_ephemeral_identity() {
        let row = AgentMcpServer {
            id: 42,
            user_id: 7,
            name: "saved".to_string(),
            enabled: 1,
            transport: "sse".to_string(),
            url: Some("https://example.com/sse".to_string()),
            command: None,
            args_json: "[]".to_string(),
            env_ciphertext: None,
            auth_type: "none".to_string(),
            auth_config_ciphertext: None,
            custom_headers_ciphertext: None,
            last_status: Some("ready".to_string()),
            last_error: None,
            tools_snapshot: "[]".to_string(),
            resources_snapshot: "[]".to_string(),
            prompts_snapshot: "[]".to_string(),
            last_sync_at: None,
            config_version: 9,
            created_at: "2026-03-21T00:00:00Z".to_string(),
            updated_at: "2026-03-21T00:00:00Z".to_string(),
        };
        let normalized = NormalizedMcpServerState {
            name: "draft".to_string(),
            enabled: true,
            transport: "streamable-http".to_string(),
            url: Some("https://example.com/messages".to_string()),
            command: None,
            args: Vec::new(),
            auth_type: "none".to_string(),
            auth_payload: None,
            custom_headers: BTreeMap::new(),
            stdio_env: BTreeMap::new(),
        };

        let runtime = draft_runtime_server_config(7, Some(&row), &normalized);

        assert_eq!(runtime.id, -42);
        assert_eq!(runtime.config_version, 10);
        assert_eq!(runtime.transport, "streamable-http");
        assert_eq!(runtime.url.as_deref(), Some("https://example.com/messages"));
    }

    #[test]
    fn mcp_draft_detail_response_reflects_current_form_values() {
        let normalized = NormalizedMcpServerState {
            name: "draft".to_string(),
            enabled: true,
            transport: "stdio".to_string(),
            url: None,
            command: Some("npx".to_string()),
            args: vec!["-y".to_string(), "@demo/mcp".to_string()],
            auth_type: "none".to_string(),
            auth_payload: None,
            custom_headers: BTreeMap::new(),
            stdio_env: BTreeMap::from([("API_KEY".to_string(), "secret".to_string())]),
        };
        let snapshot = mcp::McpCapabilitySnapshot {
            tools: json!([{ "name": "search" }]),
            resources: json!([]),
            prompts: json!([]),
        };

        let detail = draft_server_detail_response(None, &normalized, Some("ready"), None, Some(&snapshot));

        assert_eq!(detail.id, 0);
        assert_eq!(detail.transport, "stdio");
        assert_eq!(detail.command.as_deref(), Some("npx"));
        assert_eq!(detail.args, vec!["-y".to_string(), "@demo/mcp".to_string()]);
        assert_eq!(detail.last_status.as_deref(), Some("ready"));
        assert_eq!(detail.stdio_env.len(), 1);
        assert_eq!(detail.tools_snapshot, json!([{ "name": "search" }]));
    }

    #[test]
    fn mcp_payload_secret_state_serializes_keep_replace_clear() {
        let keep = serde_json::to_value(McpSecretState::<String>::Keep)
            .expect("keep should serialize");
        assert_eq!(keep, json!({"mode": "keep"}));

        let replace = serde_json::to_value(McpSecretState::Replace {
            value: "secret".to_string(),
        })
        .expect("replace should serialize");
        assert_eq!(replace, json!({"mode": "replace", "value": "secret"}));

        let clear = serde_json::to_value(McpSecretState::<String>::Clear)
            .expect("clear should serialize");
        assert_eq!(clear, json!({"mode": "clear"}));
    }

    #[test]
    fn mcp_payload_transport_normalization_accepts_streamable_http_aliases() {
        let normalized = crate::mcp::normalize_transport("  Streamable_HTTP  ")
            .expect("transport should normalize");
        assert_eq!(normalized, "streamable-http");
    }

    #[test]
    fn mcp_payload_duplicate_custom_headers_are_rejected() {
        let err = crate::mcp::parse_custom_header_lines(&[
            "X-Test: one".to_string(),
            "x-test: two".to_string(),
        ])
        .expect_err("duplicate header names should be rejected");
        assert!(err.to_string().contains("duplicate"));
    }

    #[test]
    fn mcp_payload_rejects_auth_custom_header_collisions_after_normalization() {
        let auth = Some(McpHttpAuthPayload::Bearer {
            token: "secret-token".to_string(),
            scheme: None,
        });
        let headers = BTreeMap::from([("Authorization".to_string(), "custom".to_string())]);

        let err = crate::mcp::validate_mcp_header_collisions(auth.as_ref(), &headers)
            .expect_err("authorization collisions should be rejected");
        assert!(err.to_string().contains("Authorization"));
    }

    #[test]
    fn mcp_payload_auth_edit_can_keep_secret_values_while_updating_non_secret_fields() {
        let existing = Some(McpHttpAuthPayload::Bearer {
            token: "old-token".to_string(),
            scheme: Some("Bearer".to_string()),
        });
        let update = McpSecretState::Replace {
            value: McpHttpAuthEditRequest::Bearer {
                scheme: Some("Token".to_string()),
                token: McpSecretState::Keep,
            },
        };

        let resolved = crate::mcp::resolve_http_auth_update(existing, &update)
            .expect("auth edit should resolve");

        assert_eq!(
            resolved,
            Some(McpHttpAuthPayload::Bearer {
                token: "old-token".to_string(),
                scheme: Some("Token".to_string()),
            })
        );
    }

    #[test]
    fn mcp_payload_auth_header_name_cannot_be_empty_after_trim() {
        let existing = Some(McpHttpAuthPayload::Header {
            name: "X-Auth".to_string(),
            value: "old-value".to_string(),
        });
        let update = McpSecretState::Replace {
            value: McpHttpAuthEditRequest::Header {
                name: "   ".to_string(),
                value: McpSecretState::Keep,
            },
        };

        let err = crate::mcp::resolve_http_auth_update(existing, &update)
            .expect_err("blank header names should be rejected");
        assert!(err.to_string().contains("header"));
    }

    #[tokio::test]
    async fn mcp_payload_rejects_credential_bearing_http_urls() {
        let db = crate::db::Database::new("sqlite::memory:")
            .await
            .expect("db should open");
        db.migrate().await.expect("db should migrate");

        sqlx::query("INSERT INTO users (username, password_hash) VALUES (?, ?)")
            .bind("url-user")
            .bind("hash")
            .execute(&db.pool)
            .await
            .expect("user insert should succeed");
        let user_id: i64 = sqlx::query_scalar("SELECT id FROM users WHERE username = ?")
            .bind("url-user")
            .fetch_one(&db.pool)
            .await
            .expect("user id should be queryable");
        let token = crate::auth::create_token(user_id, "url-user")
            .expect("token should be created");

        let app = Router::new()
            .nest(
                "/api",
                Router::new()
                    .route("/agent/mcps", get(list_mcp_servers_handler).post(save_mcp_server)),
            )
            .layer(axum::Extension(Arc::new(db)))
            .layer(axum::Extension(Arc::new(crate::BackendRuntimeConfig::default())))
            .layer(axum::Extension(Arc::new(crate::mcp::McpConnectionManager::new())));

        let payload = serde_json::json!({
            "name": "url server",
            "transport": "sse",
            "url": "https://user:pass@example.com/mcp"
        });

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/agent/mcps")
                    .header("Authorization", format!("Bearer {token}"))
                    .header("Content-Type", "application/json")
                    .body(Body::from(payload.to_string()))
                    .expect("request should build"),
            )
            .await
            .expect("router call should succeed");

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn mcp_payload_update_preserves_existing_enabled_state_when_omitted() {
        let db = crate::db::Database::new("sqlite::memory:")
            .await
            .expect("db should open");
        db.migrate().await.expect("db should migrate");

        sqlx::query("INSERT INTO users (username, password_hash) VALUES (?, ?)")
            .bind("enabled-user")
            .bind("hash")
            .execute(&db.pool)
            .await
            .expect("user insert should succeed");
        let user_id: i64 = sqlx::query_scalar("SELECT id FROM users WHERE username = ?")
            .bind("enabled-user")
            .fetch_one(&db.pool)
            .await
            .expect("user id should be queryable");
        let token = crate::auth::create_token(user_id, "enabled-user")
            .expect("token should be created");

        let app = Router::new()
            .nest(
                "/api",
                Router::new()
                    .route("/agent/mcps", get(list_mcp_servers_handler).post(save_mcp_server))
                    .route("/agent/mcps/:id", get(get_mcp_server).delete(delete_mcp_server)),
            )
            .layer(axum::Extension(Arc::new(db)))
            .layer(axum::Extension(Arc::new(crate::BackendRuntimeConfig::default())))
            .layer(axum::Extension(Arc::new(crate::mcp::McpConnectionManager::new())));

        let create_payload = serde_json::json!({
            "name": "enabled server",
            "enabled": false,
            "transport": "sse",
            "url": "https://example.com/mcp"
        });

        let response = app.clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/agent/mcps")
                    .header("Authorization", format!("Bearer {token}"))
                    .header("Content-Type", "application/json")
                    .body(Body::from(create_payload.to_string()))
                    .expect("request should build"),
            )
            .await
            .expect("router call should succeed");
        assert_eq!(response.status(), StatusCode::CREATED);

        let body = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("response body should read");
        let json: Value = serde_json::from_slice(&body).expect("response should be JSON");
        let server_id = json
            .get("server")
            .and_then(|server| server.get("id"))
            .and_then(Value::as_i64)
            .expect("server id should be present");

        let update_payload = serde_json::json!({
            "id": server_id,
            "name": "renamed server",
            "transport": "sse",
            "url": "https://example.com/mcp"
        });

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/agent/mcps")
                    .header("Authorization", format!("Bearer {token}"))
                    .header("Content-Type", "application/json")
                    .body(Body::from(update_payload.to_string()))
                    .expect("request should build"),
            )
            .await
            .expect("router call should succeed");
        assert_eq!(response.status(), StatusCode::OK);

        let body = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("response body should read");
        let json: Value = serde_json::from_slice(&body).expect("response should be JSON");
        assert_eq!(
            json.get("server")
                .and_then(|server| server.get("enabled"))
                .and_then(Value::as_bool),
            Some(false)
        );
    }

    #[tokio::test]
    async fn mcp_payload_disabled_draft_can_be_updated_without_enabling_or_url() {
        let db = crate::db::Database::new("sqlite::memory:")
            .await
            .expect("db should open");
        db.migrate().await.expect("db should migrate");

        sqlx::query("INSERT INTO users (username, password_hash) VALUES (?, ?)")
            .bind("draft-update-user")
            .bind("hash")
            .execute(&db.pool)
            .await
            .expect("user insert should succeed");
        let user_id: i64 = sqlx::query_scalar("SELECT id FROM users WHERE username = ?")
            .bind("draft-update-user")
            .fetch_one(&db.pool)
            .await
            .expect("user id should be queryable");
        let token = crate::auth::create_token(user_id, "draft-update-user")
            .expect("token should be created");

        let app = Router::new()
            .nest(
                "/api",
                Router::new()
                    .route("/agent/mcps", get(list_mcp_servers_handler).post(save_mcp_server)),
            )
            .layer(axum::Extension(Arc::new(db)))
            .layer(axum::Extension(Arc::new(crate::BackendRuntimeConfig::default())))
            .layer(axum::Extension(Arc::new(crate::mcp::McpConnectionManager::new())));

        let create_payload = serde_json::json!({
            "name": "draft server",
            "enabled": false,
            "transport": "sse"
        });

        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/agent/mcps")
                    .header("Authorization", format!("Bearer {token}"))
                    .header("Content-Type", "application/json")
                    .body(Body::from(create_payload.to_string()))
                    .expect("request should build"),
            )
            .await
            .expect("router call should succeed");
        assert_eq!(response.status(), StatusCode::CREATED);

        let body = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("response body should read");
        let json: Value = serde_json::from_slice(&body).expect("response should be JSON");
        let server_id = json
            .get("server")
            .and_then(|server| server.get("id"))
            .and_then(Value::as_i64)
            .expect("server id should be present");

        let update_payload = serde_json::json!({
            "id": server_id,
            "name": "draft server renamed",
            "transport": "sse"
        });

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/agent/mcps")
                    .header("Authorization", format!("Bearer {token}"))
                    .header("Content-Type", "application/json")
                    .body(Body::from(update_payload.to_string()))
                    .expect("request should build"),
            )
            .await
            .expect("router call should succeed");
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn mcp_payload_update_preserves_existing_http_url_when_omitted() {
        let db = crate::db::Database::new("sqlite::memory:")
            .await
            .expect("db should open");
        db.migrate().await.expect("db should migrate");

        sqlx::query("INSERT INTO users (username, password_hash) VALUES (?, ?)")
            .bind("url-preserve-user")
            .bind("hash")
            .execute(&db.pool)
            .await
            .expect("user insert should succeed");
        let user_id: i64 = sqlx::query_scalar("SELECT id FROM users WHERE username = ?")
            .bind("url-preserve-user")
            .fetch_one(&db.pool)
            .await
            .expect("user id should be queryable");
        let token = crate::auth::create_token(user_id, "url-preserve-user")
            .expect("token should be created");

        let app = Router::new()
            .nest(
                "/api",
                Router::new()
                    .route("/agent/mcps", get(list_mcp_servers_handler).post(save_mcp_server)),
            )
            .layer(axum::Extension(Arc::new(db)))
            .layer(axum::Extension(Arc::new(crate::BackendRuntimeConfig::default())))
            .layer(axum::Extension(Arc::new(crate::mcp::McpConnectionManager::new())));

        let create_payload = serde_json::json!({
            "name": "url preserve server",
            "transport": "sse",
            "url": "https://example.com/original"
        });

        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/agent/mcps")
                    .header("Authorization", format!("Bearer {token}"))
                    .header("Content-Type", "application/json")
                    .body(Body::from(create_payload.to_string()))
                    .expect("request should build"),
            )
            .await
            .expect("router call should succeed");
        assert_eq!(response.status(), StatusCode::CREATED);

        let body = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("response body should read");
        let json: Value = serde_json::from_slice(&body).expect("response should be JSON");
        let server_id = json
            .get("server")
            .and_then(|server| server.get("id"))
            .and_then(Value::as_i64)
            .expect("server id should be present");

        let update_payload = serde_json::json!({
            "id": server_id,
            "name": "url preserve server v2",
            "transport": "sse"
        });

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/agent/mcps")
                    .header("Authorization", format!("Bearer {token}"))
                    .header("Content-Type", "application/json")
                    .body(Body::from(update_payload.to_string()))
                    .expect("request should build"),
            )
            .await
            .expect("router call should succeed");
        assert_eq!(response.status(), StatusCode::OK);

        let body = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("response body should read");
        let json: Value = serde_json::from_slice(&body).expect("response should be JSON");
        assert_eq!(
            json.get("server")
                .and_then(|server| server.get("url"))
                .and_then(Value::as_str),
            Some("https://example.com/original")
        );
    }

    #[test]
    fn mcp_payload_invalid_stdio_env_lines_are_rejected() {
        let err = crate::mcp::parse_stdio_env_lines(&[
            "GOOD=value".to_string(),
            "INVALID".to_string(),
        ])
        .expect_err("invalid env lines should be rejected");
        assert!(err.to_string().contains("env"));
    }

    #[test]
    fn mcp_payload_secret_round_trip_encrypts_auth_headers_and_env() {
        let auth = McpHttpAuthPayload::Bearer {
            token: "secret-token".to_string(),
            scheme: Some("Bearer".to_string()),
        };
        let ciphertext = crate::mcp::encrypt_secret_json(&auth)
            .expect("auth payload should encrypt");
        let decoded: McpHttpAuthPayload = crate::mcp::decrypt_secret_json(&ciphertext)
            .expect("auth payload should decrypt");
        assert_eq!(decoded, auth);

        let headers = vec!["X-Token: alpha".to_string(), "X-Trace: beta".to_string()];
        let ciphertext = crate::mcp::encrypt_secret_json(&headers)
            .expect("header lines should encrypt");
        let decoded: Vec<String> = crate::mcp::decrypt_secret_json(&ciphertext)
            .expect("header lines should decrypt");
        assert_eq!(decoded, headers);

        let env_lines = vec!["API_KEY=123".to_string(), "NODE_ENV=test".to_string()];
        let ciphertext = crate::mcp::encrypt_secret_json(&env_lines)
            .expect("env lines should encrypt");
        let decoded: Vec<String> = crate::mcp::decrypt_secret_json(&ciphertext)
            .expect("env lines should decrypt");
        assert_eq!(decoded, env_lines);
    }

    #[tokio::test]
    async fn mcp_payload_stdio_save_is_rejected_when_disabled() {
        let runtime = crate::BackendRuntimeConfig {
            mcp_stdio_enabled: false,
            mcp_stdio_allowed_commands: vec![],
            skills_root_dir: String::new(),
            ..Default::default()
        };
        let request = McpServerUpsertRequest {
            id: None,
            name: "stdio server".to_string(),
            enabled: Some(true),
            transport: "stdio".to_string(),
            url: None,
            command: Some("node".to_string()),
            args: vec!["server.js".to_string()],
            auth: McpSecretState::Clear,
            custom_headers: McpSecretState::Clear,
            stdio_env: McpSecretState::Clear,
        };

        let err = crate::mcp::normalize_mcp_server_upsert_request(&request, &runtime, None, true)
            .await
            .expect_err("stdio should be rejected when disabled");
        assert!(err.to_string().contains("stdio"));
    }

    #[tokio::test]
    async fn mcp_payload_disabled_http_draft_can_save_without_url() {
        let runtime = crate::BackendRuntimeConfig {
            mcp_stdio_enabled: false,
            mcp_stdio_allowed_commands: vec![],
            skills_root_dir: String::new(),
            ..Default::default()
        };
        let request = McpServerUpsertRequest {
            id: None,
            name: "draft server".to_string(),
            enabled: Some(false),
            transport: "sse".to_string(),
            url: None,
            command: None,
            args: vec![],
            auth: McpSecretState::Clear,
            custom_headers: McpSecretState::Clear,
            stdio_env: McpSecretState::Clear,
        };

        crate::mcp::normalize_mcp_server_upsert_request(&request, &runtime, None, false)
            .await
            .expect("disabled drafts should allow missing transport-specific fields");

        let err = crate::mcp::normalize_mcp_server_upsert_request(&request, &runtime, None, true)
            .await
            .expect_err("strict validation should still require an HTTP URL");
        assert!(err.to_string().contains("HTTP MCP URL is required"));
    }

    #[tokio::test]
    async fn mcp_payload_streamable_http_rejects_legacy_sse_endpoint_urls() {
        let runtime = crate::BackendRuntimeConfig {
            mcp_stdio_enabled: false,
            mcp_stdio_allowed_commands: vec![],
            skills_root_dir: String::new(),
            ..Default::default()
        };
        let request = McpServerUpsertRequest {
            id: None,
            name: "streamable server".to_string(),
            enabled: Some(true),
            transport: "streamable-http".to_string(),
            url: Some("https://example.com/mcp/sse".to_string()),
            command: None,
            args: vec![],
            auth: McpSecretState::Clear,
            custom_headers: McpSecretState::Clear,
            stdio_env: McpSecretState::Clear,
        };

        let err = crate::mcp::normalize_mcp_server_upsert_request(&request, &runtime, None, true)
            .await
            .expect_err("legacy SSE endpoints should be rejected for streamable-http");
        assert!(err.to_string().contains("legacy SSE /sse endpoint"));
    }

    #[tokio::test]
    async fn mcp_payload_settings_route_persists_toggle() {
        let db = crate::db::Database::new("sqlite::memory:")
            .await
            .expect("db should open");
        db.migrate().await.expect("db should migrate");

        sqlx::query("INSERT INTO users (username, password_hash) VALUES (?, ?)")
            .bind("settings-user")
            .bind("hash")
            .execute(&db.pool)
            .await
            .expect("user insert should succeed");
        let user_id: i64 = sqlx::query_scalar("SELECT id FROM users WHERE username = ?")
            .bind("settings-user")
            .fetch_one(&db.pool)
            .await
            .expect("user id should be queryable");
        let token = crate::auth::create_token(user_id, "settings-user")
            .expect("token should be created");

        let app = Router::new()
            .nest(
                "/api",
                Router::new()
                    .route("/agent/mcps/settings", get(get_mcp_settings).post(update_mcp_settings)),
            )
            .layer(axum::Extension(Arc::new(db)))
            .layer(axum::Extension(Arc::new(crate::BackendRuntimeConfig::default())))
            .layer(axum::Extension(Arc::new(crate::mcp::McpConnectionManager::new())));

        let response = app.clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/agent/mcps/settings")
                    .header("Authorization", format!("Bearer {token}"))
                    .header("Content-Type", "application/json")
                    .body(Body::from(r#"{"enabled":true}"#))
                    .expect("request should build"),
            )
            .await
            .expect("router call should succeed");
        assert_eq!(response.status(), StatusCode::OK);

        let body = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("response body should read");
        let json: Value = serde_json::from_slice(&body).expect("response should be JSON");
        assert_eq!(
            json.get("settings")
                .and_then(|value| value.get("enabled"))
                .and_then(Value::as_bool),
            Some(true)
        );

        let response = app.clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/api/agent/mcps/settings")
                    .header("Authorization", format!("Bearer {token}"))
                    .body(Body::empty())
                    .expect("request should build"),
            )
            .await
            .expect("router call should succeed");
        assert_eq!(response.status(), StatusCode::OK);

        let body = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("response body should read");
        let json: Value = serde_json::from_slice(&body).expect("response should be JSON");
        assert_eq!(
            json.get("settings")
                .and_then(|value| value.get("enabled"))
                .and_then(Value::as_bool),
            Some(true)
        );
    }

    #[tokio::test]
    async fn mcp_payload_server_crud_round_trip_masks_secrets() {
        let db = crate::db::Database::new("sqlite::memory:")
            .await
            .expect("db should open");
        db.migrate().await.expect("db should migrate");

        sqlx::query("INSERT INTO users (username, password_hash) VALUES (?, ?)")
            .bind("server-user")
            .bind("hash")
            .execute(&db.pool)
            .await
            .expect("user insert should succeed");
        let user_id: i64 = sqlx::query_scalar("SELECT id FROM users WHERE username = ?")
            .bind("server-user")
            .fetch_one(&db.pool)
            .await
            .expect("user id should be queryable");
        let token = crate::auth::create_token(user_id, "server-user")
            .expect("token should be created");

        let app = Router::new()
            .nest(
                "/api",
                Router::new()
                    .route("/agent/mcps", get(list_mcp_servers_handler).post(save_mcp_server))
                    .route("/agent/mcps/:id", get(get_mcp_server).delete(delete_mcp_server)),
            )
            .layer(axum::Extension(Arc::new(db)))
            .layer(axum::Extension(Arc::new(crate::BackendRuntimeConfig::default())))
            .layer(axum::Extension(Arc::new(crate::mcp::McpConnectionManager::new())));

        let create_payload = serde_json::json!({
            "name": "docs server",
            "enabled": true,
            "transport": "sse",
            "url": "https://example.com/mcp/",
            "auth": {
                "mode": "replace",
                "value": {
                    "type": "bearer",
                    "scheme": "Bearer",
                    "token": {
                        "mode": "replace",
                        "value": "super-secret"
                    }
                }
            },
            "custom_headers": {
                "mode": "replace",
                "value": [
                    "X-Api-Key: header-secret"
                ]
            }
        });

        let response = app.clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/agent/mcps")
                    .header("Authorization", format!("Bearer {token}"))
                    .header("Content-Type", "application/json")
                    .body(Body::from(create_payload.to_string()))
                    .expect("request should build"),
            )
            .await
            .expect("router call should succeed");
        assert_eq!(response.status(), StatusCode::CREATED);

        let body = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("response body should read");
        let json: Value = serde_json::from_slice(&body).expect("response should be JSON");
        let server = json.get("server").expect("server payload should exist");
        let server_id = server
            .get("id")
            .and_then(Value::as_i64)
            .expect("server id should be present");
        assert_eq!(
            server
                .get("auth")
                .and_then(|value| value.get("token"))
                .and_then(|value| value.get("has_value"))
                .and_then(Value::as_bool),
            Some(true)
        );
        assert_eq!(
            server
                .get("custom_headers")
                .and_then(Value::as_array)
                .and_then(|items| items.first())
                .and_then(|item| item.get("has_value"))
                .and_then(Value::as_bool),
            Some(true)
        );

        let response = app.clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/api/agent/mcps")
                    .header("Authorization", format!("Bearer {token}"))
                    .body(Body::empty())
                    .expect("request should build"),
            )
            .await
            .expect("router call should succeed");
        assert_eq!(response.status(), StatusCode::OK);

        let body = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("response body should read");
        let json: Value = serde_json::from_slice(&body).expect("response should be JSON");
        assert_eq!(
            json.get("servers")
                .and_then(Value::as_array)
                .map(|items| items.len()),
            Some(1)
        );

        let response = app.clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(format!("/api/agent/mcps/{server_id}"))
                    .header("Authorization", format!("Bearer {token}"))
                    .body(Body::empty())
                    .expect("request should build"),
            )
            .await
            .expect("router call should succeed");
        assert_eq!(response.status(), StatusCode::OK);

        let body = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("response body should read");
        let json: Value = serde_json::from_slice(&body).expect("response should be JSON");
        assert_eq!(
            json.get("server")
                .and_then(|value| value.get("url"))
                .and_then(Value::as_str),
            Some("https://example.com/mcp")
        );

        let response = app
            .oneshot(
                Request::builder()
                    .method("DELETE")
                    .uri(format!("/api/agent/mcps/{server_id}"))
                    .header("Authorization", format!("Bearer {token}"))
                    .body(Body::empty())
                    .expect("request should build"),
            )
            .await
            .expect("router call should succeed");
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn mcp_runtime_capabilities_handler_requires_auth() {
        let db = crate::db::Database::new("sqlite::memory:")
            .await
            .expect("db should open");
        db.migrate().await.expect("db should migrate");

        let response = get_runtime_capabilities(
            Extension(Arc::new(db)),
            axum::http::HeaderMap::new(),
            Extension(Arc::new(crate::BackendRuntimeConfig {
                mcp_stdio_enabled: true,
                mcp_stdio_allowed_commands: vec!["git".to_string()],
                skills_root_dir: String::new(),
                ..Default::default()
            })),
        )
        .await;

        let response = response.expect_err("missing auth should be rejected");
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn mcp_runtime_capabilities_route_returns_authenticated_json() {
        let db = crate::db::Database::new("sqlite::memory:")
            .await
            .expect("db should open");
        db.migrate().await.expect("db should migrate");

        sqlx::query("INSERT INTO users (username, password_hash) VALUES (?, ?)")
            .bind("route-user")
            .bind("hash")
            .execute(&db.pool)
            .await
            .expect("user insert should succeed");
        let user_id: i64 = sqlx::query_scalar("SELECT id FROM users WHERE username = ?")
            .bind("route-user")
            .fetch_one(&db.pool)
            .await
            .expect("user id should be queryable");
        let token = crate::auth::create_token(user_id, "route-user")
            .expect("token should be created");

        let app = Router::new()
            .nest(
                "/api",
                Router::new().route(
                    "/agent/mcps/runtime-capabilities",
                    get(get_runtime_capabilities),
                ),
            )
            .layer(axum::Extension(Arc::new(db)))
            .layer(axum::Extension(Arc::new(crate::BackendRuntimeConfig {
                mcp_stdio_enabled: true,
                mcp_stdio_allowed_commands: vec!["git".to_string(), "node".to_string()],
                skills_root_dir: String::new(),
                ..Default::default()
            })));

        let response = app
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/api/agent/mcps/runtime-capabilities")
                    .header("Authorization", format!("Bearer {token}"))
                    .body(Body::empty())
                    .expect("request should build"),
            )
            .await
            .expect("router call should succeed");

        assert_eq!(response.status(), StatusCode::OK);
        let body = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("response body should read");
        let json: Value = serde_json::from_slice(&body).expect("response should be JSON");

        assert_eq!(
            json.get("transports")
                .and_then(Value::as_array)
                .expect("transports should be an array")
                .iter()
                .map(|item| item.as_str().unwrap_or_default().to_string())
                .collect::<Vec<_>>(),
            vec![
                "sse".to_string(),
                "streamable-http".to_string(),
                "stdio".to_string()
            ]
        );
        assert_eq!(json.get("stdio_enabled").and_then(Value::as_bool), Some(true));
        assert_eq!(
            json.get("stdio_allowed_commands")
                .and_then(Value::as_array)
                .expect("allowed commands should be an array")
                .iter()
                .map(|item| item.as_str().unwrap_or_default().to_string())
                .collect::<Vec<_>>(),
            vec!["git".to_string(), "node".to_string()]
        );
    }

}
