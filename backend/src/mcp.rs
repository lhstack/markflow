use std::{
    collections::{BTreeMap, HashMap},
    future::Future,
    process::Stdio,
    sync::Arc,
    time::Duration,
};

use aes_gcm::{
    aead::{Aead, KeyInit},
    Aes256Gcm, Nonce,
};
use axum::http::{
    header::{ACCEPT, CACHE_CONTROL, CONTENT_TYPE},
    HeaderName, HeaderValue,
};
use base64::{engine::general_purpose, Engine as _};
use futures_util::StreamExt;
use reqwest::Url;
use rmcp::{
    service::{RoleClient, RunningService, ServerSink, TxJsonRpcMessage},
    transport::{
        streamable_http_client::StreamableHttpClientTransportConfig, ConfigureCommandExt,
        StreamableHttpClientTransport, TokioChildProcess, Transport,
    },
    ServiceExt,
};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use tokio::sync::{mpsc, Mutex, Notify, RwLock};

use crate::{
    models::AgentMcpServer,
    routes::agent::{
        McpHttpAuthDetailResponse, McpHttpAuthEditRequest, McpHttpAuthPayload,
        McpSecretEntryResponse, McpSecretState, McpSecretValueResponse, McpServerUpsertRequest,
    },
    BackendRuntimeConfig,
};

const MCP_CONNECT_TIMEOUT: Duration = Duration::from_secs(12);
const MCP_DISCOVERY_TIMEOUT: Duration = Duration::from_secs(8);
const LEGACY_SSE_ENDPOINT_TIMEOUT: Duration = Duration::from_secs(10);
const MCP_CLOSE_TIMEOUT: Duration = Duration::from_secs(2);

fn mcp_secret() -> String {
    std::env::var("SHARE_PASSWORD_SECRET")
        .or_else(|_| std::env::var("JWT_SECRET"))
        .unwrap_or_else(|_| "markflow_dev_secret_change_in_production".to_string())
}

fn mcp_cipher() -> Aes256Gcm {
    let digest = Sha256::digest(mcp_secret().as_bytes());
    Aes256Gcm::new_from_slice(&digest).expect("mcp key length should be valid")
}

pub fn encrypt_secret_json<T: Serialize>(value: &T) -> anyhow::Result<String> {
    let cipher = mcp_cipher();
    let nonce_bytes: [u8; 12] = rand::random();
    let nonce = Nonce::from_slice(&nonce_bytes);
    let plaintext = serde_json::to_vec(value)?;
    let ciphertext = cipher
        .encrypt(nonce, plaintext.as_ref())
        .map_err(|_| anyhow::anyhow!("mcp secret encryption failed"))?;

    let mut payload = Vec::with_capacity(nonce_bytes.len() + ciphertext.len());
    payload.extend_from_slice(&nonce_bytes);
    payload.extend_from_slice(&ciphertext);
    Ok(general_purpose::STANDARD.encode(payload))
}

pub fn decrypt_secret_json<T: DeserializeOwned>(ciphertext: &str) -> anyhow::Result<T> {
    let decoded = general_purpose::STANDARD.decode(ciphertext)?;
    if decoded.len() < 13 {
        anyhow::bail!("invalid mcp secret ciphertext");
    }
    let (nonce_bytes, body) = decoded.split_at(12);
    let cipher = mcp_cipher();
    let plaintext = cipher
        .decrypt(Nonce::from_slice(nonce_bytes), body)
        .map_err(|_| anyhow::anyhow!("mcp secret decryption failed"))?;
    Ok(serde_json::from_slice(&plaintext)?)
}

pub fn normalize_transport(value: &str) -> Option<String> {
    match value.trim().to_ascii_lowercase().replace('_', "-").as_str() {
        "sse" => Some("sse".to_string()),
        "streamable-http" => Some("streamable-http".to_string()),
        "stdio" => Some("stdio".to_string()),
        _ => None,
    }
}

pub fn normalize_http_mcp_url(value: &str) -> anyhow::Result<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        anyhow::bail!("HTTP MCP URL is required");
    }

    let normalized = trimmed.trim_end_matches('/').to_string();
    let scheme_offset = normalized.find("://").map(|index| index + 3).unwrap_or(0);
    let authority_end = normalized[scheme_offset..]
        .find(['/', '?', '#'])
        .map(|index| scheme_offset + index)
        .unwrap_or_else(|| normalized.len());
    let authority = &normalized[scheme_offset..authority_end];
    if authority.contains('@') {
        anyhow::bail!("HTTP MCP URL must not contain credentials");
    }

    Ok(normalized)
}

fn validate_transport_url_shape(transport: &str, normalized_url: &str) -> anyhow::Result<()> {
    if transport != "streamable-http" {
        return Ok(());
    }

    let parsed = Url::parse(normalized_url)
        .map_err(|err| anyhow::anyhow!("invalid HTTP MCP URL: {err}"))?;
    if parsed.path().trim_end_matches('/').ends_with("/sse") {
        anyhow::bail!(
            "streamable-http transport should not use a legacy SSE /sse endpoint; please switch to the streamable HTTP endpoint URL"
        );
    }

    Ok(())
}

pub fn parse_custom_header_lines(lines: &[String]) -> anyhow::Result<BTreeMap<String, String>> {
    let mut headers = BTreeMap::new();
    let mut normalized_names = std::collections::HashSet::new();

    for raw_line in lines {
        let line = raw_line.trim();
        if line.is_empty() {
            continue;
        }

        let (name, value) = line
            .split_once(':')
            .ok_or_else(|| anyhow::anyhow!("invalid custom header line: {line}"))?;
        let name = name.trim();
        if name.is_empty() {
            anyhow::bail!("invalid custom header line: {line}");
        }
        let normalized_name = name.to_ascii_lowercase();
        if !normalized_names.insert(normalized_name.clone()) {
            anyhow::bail!("duplicate custom header name: {name}");
        }

        headers.insert(name.to_string(), value.trim().to_string());
    }

    Ok(headers)
}

pub fn auth_derived_header_name(payload: &McpHttpAuthPayload) -> Option<String> {
    match payload {
        McpHttpAuthPayload::Bearer { .. } | McpHttpAuthPayload::Basic { .. } => {
            Some("authorization".to_string())
        }
        McpHttpAuthPayload::Header { name, .. } => {
            let normalized = name.trim().to_ascii_lowercase();
            if normalized.is_empty() {
                None
            } else {
                Some(normalized)
            }
        }
        McpHttpAuthPayload::Query { .. } => None,
    }
}

pub fn validate_mcp_header_collisions(
    auth: Option<&McpHttpAuthPayload>,
    custom_headers: &BTreeMap<String, String>,
) -> anyhow::Result<()> {
    let Some(auth) = auth else {
        return Ok(());
    };

    let Some(auth_header) = auth_derived_header_name(auth) else {
        return Ok(());
    };

    let collision = custom_headers
        .keys()
        .find(|header_name| header_name.trim().to_ascii_lowercase() == auth_header);

    if let Some(header_name) = collision {
        anyhow::bail!(
            "custom header collides with auth-derived header: {}",
            header_name
        );
    }

    Ok(())
}

pub fn parse_stdio_env_lines(lines: &[String]) -> anyhow::Result<BTreeMap<String, String>> {
    let mut env = BTreeMap::new();

    for raw_line in lines {
        let line = raw_line.trim();
        if line.is_empty() {
            continue;
        }

        let (name, value) = line
            .split_once('=')
            .ok_or_else(|| anyhow::anyhow!("invalid stdio env line: {line}"))?;
        let name = name.trim();
        if name.is_empty() {
            anyhow::bail!("invalid stdio env line: {line}");
        }
        env.insert(name.to_string(), value.trim().to_string());
    }

    Ok(env)
}

fn resolve_required_secret<T: Clone>(
    state: &McpSecretState<T>,
    existing: Option<T>,
    label: &str,
) -> anyhow::Result<T> {
    match state {
        McpSecretState::Keep => existing.ok_or_else(|| anyhow::anyhow!("missing existing {label}")),
        McpSecretState::Replace { value } => Ok(value.clone()),
        McpSecretState::Clear => anyhow::bail!("{label} cannot be cleared"),
    }
}

pub fn resolve_http_auth_update(
    existing: Option<McpHttpAuthPayload>,
    state: &McpSecretState<McpHttpAuthEditRequest>,
) -> anyhow::Result<Option<McpHttpAuthPayload>> {
    match state {
        McpSecretState::Keep => Ok(existing),
        McpSecretState::Clear => Ok(None),
        McpSecretState::Replace { value } => {
            let existing = existing.as_ref();
            let resolved = match value {
                McpHttpAuthEditRequest::Bearer { scheme, token } => {
                    let existing_bearer = match existing {
                        Some(McpHttpAuthPayload::Bearer { token, scheme }) => {
                            Some((token.clone(), scheme.clone()))
                        }
                        _ => None,
                    };
                    let token = resolve_required_secret(
                        token,
                        existing_bearer.as_ref().map(|(token, _)| token.clone()),
                        "bearer token",
                    )?;
                    let scheme = scheme
                        .clone()
                        .or_else(|| existing_bearer.as_ref().and_then(|(_, scheme)| scheme.clone()));
                    McpHttpAuthPayload::Bearer { token, scheme }
                }
                McpHttpAuthEditRequest::Basic { username, password } => {
                    let existing_password = match existing {
                        Some(McpHttpAuthPayload::Basic { password, .. }) => Some(password.clone()),
                        _ => None,
                    };
                    let password =
                        resolve_required_secret(password, existing_password, "basic password")?;
                    McpHttpAuthPayload::Basic {
                        username: username.trim().to_string(),
                        password,
                    }
                }
                McpHttpAuthEditRequest::Header { name, value } => {
                    let existing_value = match existing {
                        Some(McpHttpAuthPayload::Header { value, .. }) => Some(value.clone()),
                        _ => None,
                    };
                    let name = name.trim();
                    if name.is_empty() {
                        anyhow::bail!("header name is required");
                    }
                    let value = resolve_required_secret(value, existing_value, "header value")?;
                    McpHttpAuthPayload::Header {
                        name: name.to_string(),
                        value,
                    }
                }
                McpHttpAuthEditRequest::Query { name, value } => {
                    let existing_value = match existing {
                        Some(McpHttpAuthPayload::Query { value, .. }) => Some(value.clone()),
                        _ => None,
                    };
                    let value = resolve_required_secret(value, existing_value, "query value")?;
                    McpHttpAuthPayload::Query {
                        name: name.trim().to_string(),
                        value,
                    }
                }
            };

            Ok(Some(resolved))
        }
    }
}

pub async fn normalize_mcp_server_upsert_request(
    request: &McpServerUpsertRequest,
    runtime: &BackendRuntimeConfig,
    existing_http_url: Option<&str>,
    require_complete_connection_fields: bool,
) -> anyhow::Result<()> {
    let transport = normalize_transport(&request.transport)
        .ok_or_else(|| anyhow::anyhow!("unsupported MCP transport"))?;

    if transport == "stdio" && !runtime.mcp_stdio_enabled {
        anyhow::bail!("stdio transport is disabled by backend runtime");
    }

    if transport == "stdio" && !runtime.mcp_stdio_allowed_commands.is_empty() {
        let command = request.command.as_deref().unwrap_or_default().trim();
        if !runtime
            .mcp_stdio_allowed_commands
            .iter()
            .any(|allowed| allowed == command)
        {
            anyhow::bail!("stdio command is not allowlisted");
        }
    }

    if transport == "stdio" && require_complete_connection_fields {
        let command = request.command.as_deref().unwrap_or_default().trim();
        if command.is_empty() {
            anyhow::bail!("stdio command is required");
        }
    } else if transport != "stdio" {
        if let Some(url) = request.url.as_deref() {
            let normalized = url.trim();
            if normalized.is_empty() {
                anyhow::bail!("HTTP MCP URL is required");
            }
            let normalized = normalize_http_mcp_url(normalized)?;
            validate_transport_url_shape(&transport, &normalized)?;
        } else if existing_http_url.is_none() && require_complete_connection_fields {
            anyhow::bail!("HTTP MCP URL is required");
        } else if let Some(existing_url) = existing_http_url {
            let normalized = normalize_http_mcp_url(existing_url)?;
            validate_transport_url_shape(&transport, &normalized)?;
        }
    }

    Ok(())
}

pub fn auth_payload_kind(payload: &McpHttpAuthPayload) -> &'static str {
    match payload {
        McpHttpAuthPayload::Bearer { .. } => "bearer",
        McpHttpAuthPayload::Basic { .. } => "basic",
        McpHttpAuthPayload::Header { .. } => "header",
        McpHttpAuthPayload::Query { .. } => "query",
    }
}

pub fn reveal_auth_payload(payload: &McpHttpAuthPayload) -> McpHttpAuthDetailResponse {
    match payload {
        McpHttpAuthPayload::Bearer { scheme, token } => McpHttpAuthDetailResponse {
            auth_type: "bearer".to_string(),
            scheme: scheme.clone(),
            username: None,
            header_name: None,
            query_name: None,
            token: McpSecretValueResponse {
                has_value: true,
                value: Some(token.clone()),
            },
            password: McpSecretValueResponse {
                has_value: false,
                value: None,
            },
            value: McpSecretValueResponse {
                has_value: false,
                value: None,
            },
        },
        McpHttpAuthPayload::Basic { username, password } => McpHttpAuthDetailResponse {
            auth_type: "basic".to_string(),
            scheme: None,
            username: Some(username.clone()),
            header_name: None,
            query_name: None,
            token: McpSecretValueResponse {
                has_value: false,
                value: None,
            },
            password: McpSecretValueResponse {
                has_value: true,
                value: Some(password.clone()),
            },
            value: McpSecretValueResponse {
                has_value: false,
                value: None,
            },
        },
        McpHttpAuthPayload::Header { name, value } => McpHttpAuthDetailResponse {
            auth_type: "header".to_string(),
            scheme: None,
            username: None,
            header_name: Some(name.clone()),
            query_name: None,
            token: McpSecretValueResponse {
                has_value: false,
                value: None,
            },
            password: McpSecretValueResponse {
                has_value: false,
                value: None,
            },
            value: McpSecretValueResponse {
                has_value: true,
                value: Some(value.clone()),
            },
        },
        McpHttpAuthPayload::Query { name, value } => McpHttpAuthDetailResponse {
            auth_type: "query".to_string(),
            scheme: None,
            username: None,
            header_name: None,
            query_name: Some(name.clone()),
            token: McpSecretValueResponse {
                has_value: false,
                value: None,
            },
            password: McpSecretValueResponse {
                has_value: false,
                value: None,
            },
            value: McpSecretValueResponse {
                has_value: true,
                value: Some(value.clone()),
            },
        },
    }
}

pub fn reveal_headers(headers: &BTreeMap<String, String>) -> Vec<McpSecretEntryResponse> {
    headers
        .iter()
        .map(|(name, value)| McpSecretEntryResponse {
            name: name.clone(),
            has_value: true,
            value: Some(value.clone()),
        })
        .collect()
}

pub fn reveal_stdio_env(env: &BTreeMap<String, String>) -> Vec<McpSecretEntryResponse> {
    env.iter()
        .map(|(name, value)| McpSecretEntryResponse {
            name: name.clone(),
            has_value: true,
            value: Some(value.clone()),
        })
        .collect()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedMcpServerConfig {
    pub id: i64,
    pub user_id: i64,
    pub name: String,
    pub enabled: bool,
    pub transport: String,
    pub url: Option<String>,
    pub command: Option<String>,
    pub args: Vec<String>,
    pub auth_type: String,
    pub auth_payload: Option<McpHttpAuthPayload>,
    pub custom_headers: BTreeMap<String, String>,
    pub stdio_env: BTreeMap<String, String>,
    pub config_version: i64,
}

impl ResolvedMcpServerConfig {
    pub fn connection_key(&self) -> String {
        format!("{}:{}:{}", self.user_id, self.id, self.config_version)
    }
}

pub fn load_runtime_server_config(row: &AgentMcpServer) -> anyhow::Result<ResolvedMcpServerConfig> {
    let auth_payload = match row.auth_config_ciphertext.as_deref() {
        Some(ciphertext) if !ciphertext.trim().is_empty() => {
            Some(decrypt_secret_json::<McpHttpAuthPayload>(ciphertext)?)
        }
        _ => None,
    };

    let custom_headers = match row.custom_headers_ciphertext.as_deref() {
        Some(ciphertext) if !ciphertext.trim().is_empty() => {
            decrypt_secret_json::<BTreeMap<String, String>>(ciphertext)?
        }
        _ => BTreeMap::new(),
    };

    let stdio_env = match row.env_ciphertext.as_deref() {
        Some(ciphertext) if !ciphertext.trim().is_empty() => {
            decrypt_secret_json::<BTreeMap<String, String>>(ciphertext)?
        }
        _ => BTreeMap::new(),
    };

    Ok(ResolvedMcpServerConfig {
        id: row.id,
        user_id: row.user_id,
        name: row.name.trim().to_string(),
        enabled: row.enabled == 1,
        transport: normalize_transport(&row.transport)
            .ok_or_else(|| anyhow::anyhow!("unsupported MCP transport"))?,
        url: row
            .url
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(|value| value.to_string()),
        command: row
            .command
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(|value| value.to_string()),
        args: serde_json::from_str::<Vec<String>>(&row.args_json).unwrap_or_default(),
        auth_type: auth_payload
            .as_ref()
            .map(|payload| auth_payload_kind(payload).to_string())
            .unwrap_or_else(|| row.auth_type.trim().to_ascii_lowercase()),
        auth_payload,
        custom_headers,
        stdio_env,
        config_version: row.config_version,
    })
}

#[derive(Debug, Clone)]
pub struct ResolvedHttpTransportRequest {
    pub url: String,
    pub headers: HashMap<HeaderName, HeaderValue>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedStdioLaunchConfig {
    pub command: String,
    pub args: Vec<String>,
    pub env: BTreeMap<String, String>,
}

pub fn build_http_transport_request(
    config: &ResolvedMcpServerConfig,
) -> anyhow::Result<ResolvedHttpTransportRequest> {
    let raw_url = config
        .url
        .as_deref()
        .ok_or_else(|| anyhow::anyhow!("HTTP MCP URL is required"))?;
    let mut url = Url::parse(raw_url)?;
    let mut headers = HashMap::new();

    if let Some(auth) = config.auth_payload.as_ref() {
        match auth {
            McpHttpAuthPayload::Bearer { token, scheme } => {
                let scheme = scheme
                    .as_deref()
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .unwrap_or("Bearer");
                headers.insert(
                    HeaderName::from_static("authorization"),
                    HeaderValue::from_str(&format!("{scheme} {}", token.trim()))?,
                );
            }
            McpHttpAuthPayload::Basic { username, password } => {
                let token = general_purpose::STANDARD
                    .encode(format!("{}:{}", username.trim(), password));
                headers.insert(
                    HeaderName::from_static("authorization"),
                    HeaderValue::from_str(&format!("Basic {token}"))?,
                );
            }
            McpHttpAuthPayload::Header { name, value } => {
                headers.insert(
                    HeaderName::from_bytes(name.trim().as_bytes())?,
                    HeaderValue::from_str(value.trim())?,
                );
            }
            McpHttpAuthPayload::Query { name, value } => {
                let name = name.trim();
                if name.is_empty() {
                    anyhow::bail!("query parameter name is required");
                }
                url.query_pairs_mut().append_pair(name, value.trim());
            }
        }
    }

    for (name, value) in &config.custom_headers {
        headers.insert(
            HeaderName::from_bytes(name.trim().as_bytes())?,
            HeaderValue::from_str(value.trim())?,
        );
    }

    Ok(ResolvedHttpTransportRequest {
        url: url.to_string(),
        headers,
    })
}

pub fn build_streamable_http_transport_config(
    config: &ResolvedMcpServerConfig,
) -> anyhow::Result<StreamableHttpClientTransportConfig> {
    let request = build_http_transport_request(config)?;
    Ok(StreamableHttpClientTransportConfig::with_uri(request.url).custom_headers(request.headers))
}

pub fn build_stdio_launch_config(
    config: &ResolvedMcpServerConfig,
    runtime: &BackendRuntimeConfig,
) -> anyhow::Result<ResolvedStdioLaunchConfig> {
    let command = config
        .command
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| anyhow::anyhow!("stdio command is required"))?
        .to_string();

    if !runtime.mcp_stdio_enabled {
        anyhow::bail!("stdio transport is disabled by backend runtime");
    }

    if !runtime.mcp_stdio_allowed_commands.is_empty()
        && !runtime
            .mcp_stdio_allowed_commands
            .iter()
            .any(|allowed| allowed == &command)
    {
        anyhow::bail!("stdio command is not allowlisted");
    }

    Ok(ResolvedStdioLaunchConfig {
        command,
        args: config
            .args
            .iter()
            .map(|value| value.trim())
            .filter(|value| !value.is_empty())
            .map(|value| value.to_string())
            .collect(),
        env: config.stdio_env.clone(),
    })
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpCapabilitySnapshot {
    pub tools: Value,
    pub resources: Value,
    pub prompts: Value,
}

impl Default for McpCapabilitySnapshot {
    fn default() -> Self {
        Self {
            tools: json!([]),
            resources: json!([]),
            prompts: json!([]),
        }
    }
}

fn serialize_or_empty<T: Serialize>(value: &T) -> Value {
    serde_json::to_value(value).unwrap_or_else(|_| json!([]))
}

fn short_mcp_error(err: &dyn std::fmt::Display) -> String {
    let raw = err.to_string().replace('\n', " ");
    let trimmed = raw.trim();
    if trimmed.chars().count() <= 180 {
        trimmed.to_string()
    } else {
        trimmed.chars().take(180).collect::<String>()
    }
}

#[derive(Debug)]
pub enum LegacySseClientTransportError {
    Request(reqwest::Error),
    Json(serde_json::Error),
    MissingEndpoint,
}

impl std::fmt::Display for LegacySseClientTransportError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Request(err) => write!(f, "{err}"),
            Self::Json(err) => write!(f, "{err}"),
            Self::MissingEndpoint => write!(f, "legacy SSE endpoint was not announced in time"),
        }
    }
}

impl std::error::Error for LegacySseClientTransportError {}

impl From<reqwest::Error> for LegacySseClientTransportError {
    fn from(value: reqwest::Error) -> Self {
        Self::Request(value)
    }
}

impl From<serde_json::Error> for LegacySseClientTransportError {
    fn from(value: serde_json::Error) -> Self {
        Self::Json(value)
    }
}

#[derive(Debug)]
struct LegacySseShared {
    endpoint_url: RwLock<Option<String>>,
    endpoint_notify: Notify,
    read_task: Mutex<Option<tokio::task::JoinHandle<()>>>,
}

impl Default for LegacySseShared {
    fn default() -> Self {
        Self {
            endpoint_url: RwLock::new(None),
            endpoint_notify: Notify::new(),
            read_task: Mutex::new(None),
        }
    }
}

impl LegacySseShared {
    async fn set_endpoint(&self, url: String) {
        *self.endpoint_url.write().await = Some(url);
        self.endpoint_notify.notify_waiters();
    }

    async fn wait_for_endpoint(&self) -> Result<String, LegacySseClientTransportError> {
        let wait = async {
            loop {
                if let Some(url) = self.endpoint_url.read().await.clone() {
                    return Ok(url);
                }
                self.endpoint_notify.notified().await;
            }
        };

        tokio::time::timeout(LEGACY_SSE_ENDPOINT_TIMEOUT, wait)
            .await
            .map_err(|_| LegacySseClientTransportError::MissingEndpoint)?
    }
}

pub struct LegacySseClientTransport {
    client: reqwest::Client,
    request: ResolvedHttpTransportRequest,
    shared: Arc<LegacySseShared>,
    receiver: mpsc::Receiver<rmcp::service::RxJsonRpcMessage<RoleClient>>,
}

impl LegacySseClientTransport {
    pub async fn connect(
        request: ResolvedHttpTransportRequest,
    ) -> Result<Self, LegacySseClientTransportError> {
        let client = reqwest::Client::new();
        let mut connect_request = client
            .get(&request.url)
            .header(ACCEPT, "text/event-stream")
            .header(CACHE_CONTROL, "no-cache");
        for (name, value) in &request.headers {
            connect_request = connect_request.header(name, value);
        }

        let response = connect_request.send().await?.error_for_status()?;
        let (tx, rx) = mpsc::channel(128);
        let shared = Arc::new(LegacySseShared::default());

        let task = tokio::spawn(run_legacy_sse_reader(
            response,
            request.url.clone(),
            shared.clone(),
            tx,
        ));
        *shared.read_task.lock().await = Some(task);

        Ok(Self {
            client,
            request,
            shared,
            receiver: rx,
        })
    }
}

async fn run_legacy_sse_reader(
    response: reqwest::Response,
    base_url: String,
    shared: Arc<LegacySseShared>,
    sender: mpsc::Sender<rmcp::service::RxJsonRpcMessage<RoleClient>>,
) {
    let mut stream = response.bytes_stream();
    let mut buffer = String::new();
    let mut event_name: Option<String> = None;
    let mut data_lines: Vec<String> = Vec::new();

    while let Some(chunk) = stream.next().await {
        let chunk = match chunk {
            Ok(value) => value,
            Err(err) => {
                tracing::warn!("legacy SSE stream read failed: {}", err);
                break;
            }
        };

        buffer.push_str(&String::from_utf8_lossy(&chunk));

        while let Some(index) = buffer.find('\n') {
            let line = buffer[..index].trim_end_matches('\r').to_string();
            buffer.drain(..=index);

            if line.is_empty() {
                let event = event_name.take().unwrap_or_else(|| "message".to_string());
                let data = data_lines.join("\n");
                data_lines.clear();
                handle_legacy_sse_event(&base_url, &event, &data, &shared, &sender).await;
                continue;
            }

            if line.starts_with(':') {
                continue;
            }

            let (field, value) = line
                .split_once(':')
                .map(|(field, value)| (field, value.trim_start()))
                .unwrap_or((line.as_str(), ""));

            match field {
                "event" => event_name = Some(value.to_string()),
                "data" => data_lines.push(value.to_string()),
                _ => {}
            }
        }
    }
}

async fn handle_legacy_sse_event(
    base_url: &str,
    event: &str,
    data: &str,
    shared: &Arc<LegacySseShared>,
    sender: &mpsc::Sender<rmcp::service::RxJsonRpcMessage<RoleClient>>,
) {
    match event {
        "endpoint" => {
            if data.trim().is_empty() {
                return;
            }
            let resolved = Url::parse(data)
                .or_else(|_| Url::parse(base_url).and_then(|base| base.join(data)))
                .map(|url| url.to_string());
            match resolved {
                Ok(url) => shared.set_endpoint(url).await,
                Err(err) => tracing::warn!("legacy SSE endpoint resolve failed: {}", err),
            }
        }
        "message" | "" => {
            if data.trim().is_empty() {
                return;
            }
            match serde_json::from_str::<rmcp::service::RxJsonRpcMessage<RoleClient>>(data) {
            Ok(message) => {
                let _ = sender.send(message).await;
            }
            Err(err) => tracing::warn!("legacy SSE message parse failed: {}", err),
            }
        }
        _ => {}
    }
}

impl Transport<RoleClient> for LegacySseClientTransport {
    type Error = LegacySseClientTransportError;

    fn send(
        &mut self,
        item: TxJsonRpcMessage<RoleClient>,
    ) -> impl Future<Output = Result<(), Self::Error>> + Send + 'static {
        let client = self.client.clone();
        let shared = self.shared.clone();
        let request = self.request.clone();
        async move {
            let endpoint = shared.wait_for_endpoint().await?;
            let mut post_request = client
                .post(endpoint)
                .header(CONTENT_TYPE, "application/json");
            for (name, value) in &request.headers {
                post_request = post_request.header(name, value);
            }

            post_request
                .json(&item)
                .send()
                .await?
                .error_for_status()?;
            Ok(())
        }
    }

    async fn receive(&mut self) -> Option<rmcp::service::RxJsonRpcMessage<RoleClient>> {
        self.receiver.recv().await
    }

    fn close(&mut self) -> impl Future<Output = Result<(), Self::Error>> + Send {
        let shared = self.shared.clone();
        async move {
            if let Some(task) = shared.read_task.lock().await.take() {
                task.abort();
            }
            Ok(())
        }
    }
}

type SharedRunningClient = Arc<Mutex<RunningService<RoleClient, ()>>>;

pub struct CachedMcpConnection {
    pub key: String,
    pub server_id: i64,
    pub peer: ServerSink,
    running: SharedRunningClient,
}

impl CachedMcpConnection {
    async fn close(&self) {
        let mut running = self.running.lock().await;
        let _ = running.close_with_timeout(MCP_CLOSE_TIMEOUT).await;
    }
}

#[derive(Default)]
pub struct McpConnectionManager {
    connections: Mutex<HashMap<String, Arc<CachedMcpConnection>>>,
}

impl McpConnectionManager {
    pub fn new() -> Self {
        Self::default()
    }

    pub async fn get_or_connect(
        &self,
        runtime: &BackendRuntimeConfig,
        config: &ResolvedMcpServerConfig,
        force_reconnect: bool,
    ) -> anyhow::Result<Arc<CachedMcpConnection>> {
        if force_reconnect {
            self.invalidate_server(config.user_id, config.id).await;
        }

        let key = config.connection_key();
        if let Some(connection) = self.connections.lock().await.get(&key).cloned() {
            return Ok(connection);
        }

        let fresh = Arc::new(connect_mcp_client(runtime, config).await?);
        let existing = {
            let mut guard = self.connections.lock().await;
            if let Some(existing) = guard.get(&key).cloned() {
                Some(existing)
            } else {
                guard.insert(key, fresh.clone());
                None
            }
        };

        if let Some(existing) = existing {
            fresh.close().await;
            Ok(existing)
        } else {
            Ok(fresh)
        }
    }

    pub async fn invalidate_server(&self, user_id: i64, server_id: i64) {
        let removed = {
            let mut guard = self.connections.lock().await;
            let keys = guard
                .iter()
                .filter(|(_, connection)| {
                    connection.server_id == server_id && connection.key.starts_with(&format!("{user_id}:"))
                })
                .map(|(key, _)| key.clone())
                .collect::<Vec<_>>();
            keys.into_iter()
                .filter_map(|key| guard.remove(&key))
                .collect::<Vec<_>>()
        };

        for connection in removed {
            connection.close().await;
        }
    }
}

async fn connect_mcp_client(
    runtime: &BackendRuntimeConfig,
    config: &ResolvedMcpServerConfig,
) -> anyhow::Result<CachedMcpConnection> {
    let running = tokio::time::timeout(MCP_CONNECT_TIMEOUT, async {
        match config.transport.as_str() {
            "streamable-http" => {
                let transport_config = build_streamable_http_transport_config(config)?;
                let transport = StreamableHttpClientTransport::from_config(transport_config);
                ().serve(transport)
                    .await
                    .map_err(|err| anyhow::anyhow!("initialize streamable-http MCP client failed: {err}"))
            }
            "stdio" => {
                let launch = build_stdio_launch_config(config, runtime)?;
                let transport = TokioChildProcess::new(
                    tokio::process::Command::new(&launch.command).configure(|cmd| {
                        cmd.args(&launch.args);
                        for (name, value) in &launch.env {
                            cmd.env(name, value);
                        }
                        cmd.stdin(Stdio::piped())
                            .stdout(Stdio::piped())
                            .stderr(Stdio::inherit());
                    }),
                )
                .map_err(|err| anyhow::anyhow!("spawn stdio MCP process failed: {err}"))?;
                ().serve(transport)
                    .await
                    .map_err(|err| anyhow::anyhow!("initialize stdio MCP client failed: {err}"))
            }
            "sse" => {
                let request = build_http_transport_request(config)?;
                let transport = LegacySseClientTransport::connect(request)
                    .await
                    .map_err(|err| anyhow::anyhow!("connect legacy SSE MCP client failed: {err}"))?;
                ().serve(transport)
                    .await
                    .map_err(|err| anyhow::anyhow!("initialize legacy SSE MCP client failed: {err}"))
            }
            other => Err(anyhow::anyhow!("unsupported MCP transport: {other}")),
        }
    })
    .await
    .map_err(|_| anyhow::anyhow!("MCP connection timed out"))??;

    let peer = running.peer().clone();
    Ok(CachedMcpConnection {
        key: config.connection_key(),
        server_id: config.id,
        peer,
        running: Arc::new(Mutex::new(running)),
    })
}

pub async fn discover_mcp_capabilities(
    connection: &Arc<CachedMcpConnection>,
) -> anyhow::Result<McpCapabilitySnapshot> {
    let tools = tokio::time::timeout(MCP_DISCOVERY_TIMEOUT, connection.peer.list_all_tools())
        .await
        .map_err(|_| anyhow::anyhow!("list MCP tools timed out"))?
        .map_err(|err| anyhow::anyhow!("list MCP tools failed: {err}"))?;

    let resources = match tokio::time::timeout(MCP_DISCOVERY_TIMEOUT, connection.peer.list_all_resources()).await {
        Ok(Ok(value)) => value,
        Ok(Err(err)) => {
            tracing::debug!("list MCP resources skipped: {}", short_mcp_error(&err));
            Vec::new()
        }
        Err(_) => {
            tracing::debug!("list MCP resources timed out");
            Vec::new()
        }
    };

    let prompts = match tokio::time::timeout(MCP_DISCOVERY_TIMEOUT, connection.peer.list_all_prompts()).await {
        Ok(Ok(value)) => value,
        Ok(Err(err)) => {
            tracing::debug!("list MCP prompts skipped: {}", short_mcp_error(&err));
            Vec::new()
        }
        Err(_) => {
            tracing::debug!("list MCP prompts timed out");
            Vec::new()
        }
    };

    Ok(McpCapabilitySnapshot {
        tools: serialize_or_empty(&tools),
        resources: serialize_or_empty(&resources),
        prompts: serialize_or_empty(&prompts),
    })
}

pub async fn refresh_mcp_capabilities(
    manager: &McpConnectionManager,
    runtime: &BackendRuntimeConfig,
    config: &ResolvedMcpServerConfig,
    force_reconnect: bool,
) -> anyhow::Result<McpCapabilitySnapshot> {
    let connection = manager.get_or_connect(runtime, config, force_reconnect).await?;
    discover_mcp_capabilities(&connection).await
}

pub async fn discover_mcp_capabilities_once(
    runtime: &BackendRuntimeConfig,
    config: &ResolvedMcpServerConfig,
) -> anyhow::Result<McpCapabilitySnapshot> {
    let connection = Arc::new(connect_mcp_client(runtime, config).await?);
    let snapshot = discover_mcp_capabilities(&connection).await;
    connection.close().await;
    snapshot
}

pub fn capability_snapshot_json(snapshot: &McpCapabilitySnapshot) -> (String, String, String) {
    (
        snapshot.tools.to_string(),
        snapshot.resources.to_string(),
        snapshot.prompts.to_string(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn runtime_with_stdio(commands: &[&str]) -> BackendRuntimeConfig {
        BackendRuntimeConfig {
            mcp_stdio_enabled: true,
            mcp_stdio_allowed_commands: commands.iter().map(|item| item.to_string()).collect(),
            skills_root_dir: String::new(),
            ..Default::default()
        }
    }

    fn http_config(auth_payload: Option<McpHttpAuthPayload>) -> ResolvedMcpServerConfig {
        ResolvedMcpServerConfig {
            id: 1,
            user_id: 7,
            name: "demo".to_string(),
            enabled: true,
            transport: "streamable-http".to_string(),
            url: Some("https://mcp.example.com/messages".to_string()),
            command: None,
            args: Vec::new(),
            auth_type: auth_payload
                .as_ref()
                .map(|payload| auth_payload_kind(payload).to_string())
                .unwrap_or_else(|| "none".to_string()),
            auth_payload,
            custom_headers: BTreeMap::new(),
            stdio_env: BTreeMap::new(),
            config_version: 3,
        }
    }

    #[test]
    fn mcp_transport_builds_streamable_http_headers_and_query_auth() {
        let mut config = http_config(Some(McpHttpAuthPayload::Query {
            name: "token".to_string(),
            value: "abc123".to_string(),
        }));
        config
            .custom_headers
            .insert("x-workspace".to_string(), "markflow".to_string());

        let request = build_http_transport_request(&config).expect("transport request should build");
        assert!(request.url.contains("token=abc123"));
        assert_eq!(
            request
                .headers
                .get(&HeaderName::from_static("x-workspace"))
                .expect("custom header should exist"),
            &HeaderValue::from_static("markflow")
        );
    }

    #[test]
    fn mcp_transport_builds_basic_auth_authorization_header() {
        let config = http_config(Some(McpHttpAuthPayload::Basic {
            username: "tester".to_string(),
            password: "secret".to_string(),
        }));

        let request = build_http_transport_request(&config).expect("transport request should build");
        let header = request
            .headers
            .get(&HeaderName::from_static("authorization"))
            .expect("authorization header should exist");
        assert_eq!(header, &HeaderValue::from_static("Basic dGVzdGVyOnNlY3JldA=="));
    }

    #[test]
    fn mcp_transport_builds_stdio_launch_config_and_enforces_allowlist() {
        let config = ResolvedMcpServerConfig {
            id: 2,
            user_id: 7,
            name: "stdio".to_string(),
            enabled: true,
            transport: "stdio".to_string(),
            url: None,
            command: Some("npx".to_string()),
            args: vec!["-y".to_string(), "@modelcontextprotocol/server-memory".to_string()],
            auth_type: "none".to_string(),
            auth_payload: None,
            custom_headers: BTreeMap::new(),
            stdio_env: BTreeMap::from([(String::from("API_KEY"), String::from("secret"))]),
            config_version: 1,
        };

        let launch = build_stdio_launch_config(&config, &runtime_with_stdio(&["npx"]))
            .expect("stdio launch config should build");
        assert_eq!(launch.command, "npx");
        assert_eq!(launch.args.len(), 2);
        assert_eq!(launch.env.get("API_KEY").map(String::as_str), Some("secret"));

        let err = build_stdio_launch_config(&config, &runtime_with_stdio(&["uv"]))
            .expect_err("non-allowlisted command should fail");
        assert!(err.to_string().contains("allowlisted"));
    }

    #[test]
    fn mcp_runtime_connection_key_uses_user_server_and_version() {
        let config = http_config(None);
        assert_eq!(config.connection_key(), "7:1:3");
    }
}
