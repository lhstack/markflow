//! 供应商远程模型清单拉取。仅支持 openai / anthropic 两种协议。
//!
//! 提供给「供应商管理 -> 拉取远程模型」使用：调用上游 `/models` 接口，
//! 归一化成 `RemoteModel` 列表供前端选择后保存成 `agent_models` 行。

use reqwest::Client as HttpClient;
use serde::Serialize;
use serde_json::Value;

use super::provider::ProviderKind;

/// 归一化后的远程模型条目。
#[derive(Debug, Clone, Serialize)]
pub struct RemoteModel {
    pub id: String,
    pub owned_by: String,
    pub created: u32,
}

fn join_api_path(base_url: &str, path: &str) -> String {
    format!(
        "{}/{}",
        base_url.trim_end_matches('/'),
        path.trim_start_matches('/')
    )
}

fn models_endpoint(kind: ProviderKind, base_url: &str) -> String {
    match kind {
        ProviderKind::OpenAi => join_api_path(base_url, "models"),
        ProviderKind::Anthropic => {
            if base_url.trim_end_matches('/').ends_with("/v1") {
                join_api_path(base_url, "models")
            } else {
                join_api_path(base_url, "v1/models")
            }
        }
    }
}

/// 拉取并归一化远程模型列表。
pub async fn fetch_remote_models(
    kind: ProviderKind,
    base_url: &str,
    api_key: &str,
) -> anyhow::Result<Vec<RemoteModel>> {
    let client = HttpClient::new();
    let url = models_endpoint(kind, base_url);
    let request = match kind {
        ProviderKind::OpenAi => client.get(&url).bearer_auth(api_key),
        ProviderKind::Anthropic => client
            .get(&url)
            .header("x-api-key", api_key)
            .header("anthropic-version", "2023-06-01"),
    };

    let value = request
        .send()
        .await?
        .error_for_status()?
        .json::<Value>()
        .await?;

    let items = value
        .get("data")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();

    let default_owner = match kind {
        ProviderKind::OpenAi => "openai",
        ProviderKind::Anthropic => "anthropic",
    };

    let models = items
        .into_iter()
        .map(|item| RemoteModel {
            id: item
                .get("id")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            owned_by: item
                .get("owned_by")
                .and_then(Value::as_str)
                .unwrap_or(default_owner)
                .to_string(),
            created: item.get("created").and_then(Value::as_u64).unwrap_or_default() as u32,
        })
        .filter(|model| !model.id.trim().is_empty())
        .collect::<Vec<_>>();

    Ok(models)
}

// ---------------------------------------------------------------------------
// HTTP handler
// ---------------------------------------------------------------------------

use std::sync::Arc;

use axum::{extract::Extension, http::HeaderMap, response::Response, Json};
use serde::Deserialize;
use serde_json::json;

use crate::auth;
use crate::db::Database;

use super::crypto::decrypt_api_key;
use super::provider::find_user_provider;

#[derive(Debug, Deserialize)]
pub struct ListModelsRequest {
    pub provider_id: i64,
}

/// 拉取指定供应商的远程模型清单，供前端「供应商管理」选择后保存。
pub async fn list_models(
    Extension(db): Extension<Arc<Database>>,
    headers: HeaderMap,
    Json(payload): Json<ListModelsRequest>,
) -> Result<Json<serde_json::Value>, Response> {
    use axum::{http::StatusCode, response::IntoResponse};

    let user = auth::require_user(&db, &headers).await?;
    let provider = find_user_provider(&db, user.id, payload.provider_id).await?;
    let api_key = decrypt_api_key(&provider.api_key_ciphertext).map_err(|err| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": format!("读取供应商密钥失败: {err}") })),
        )
            .into_response()
    })?;
    let kind = ProviderKind::parse(&provider.kind);

    let mut models = fetch_remote_models(kind, &provider.base_url, &api_key)
        .await
        .map_err(|err| {
            (
                StatusCode::BAD_GATEWAY,
                Json(json!({ "error": format!("获取模型列表失败: {err}") })),
            )
                .into_response()
        })?;
    models.sort_by(|a, b| a.id.cmp(&b.id));

    Ok(Json(json!({ "models": models })))
}
