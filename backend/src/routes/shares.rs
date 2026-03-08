use aes_gcm::{
    aead::{Aead, KeyInit},
    Aes256Gcm, Nonce,
};
use axum::{
    extract::{Extension, Path},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use base64::{engine::general_purpose, Engine as _};
use chrono::Utc;
use rand::{distributions::Alphanumeric, Rng};
use serde::Deserialize;
use serde_json::json;
use sha2::{Digest, Sha256};
use std::sync::Arc;

use crate::{
    auth,
    db::Database,
    models::{DocNode, DocNodeResponse, Share, ShareResponse},
};

fn generate_token() -> String {
    rand::thread_rng()
        .sample_iter(&Alphanumeric)
        .take(20)
        .map(char::from)
        .collect()
}

fn share_secret() -> String {
    std::env::var("SHARE_PASSWORD_SECRET")
        .or_else(|_| std::env::var("JWT_SECRET"))
        .unwrap_or_else(|_| "markflow_dev_secret_change_in_production".to_string())
}

fn share_cipher() -> Aes256Gcm {
    let digest = Sha256::digest(share_secret().as_bytes());
    Aes256Gcm::new_from_slice(&digest).expect("share password key length should be valid")
}

fn encrypt_share_password(password: &str) -> anyhow::Result<String> {
    let cipher = share_cipher();
    let nonce_bytes: [u8; 12] = rand::random();
    let nonce = Nonce::from_slice(&nonce_bytes);
    let ciphertext = cipher
        .encrypt(nonce, password.as_bytes())
        .map_err(|_| anyhow::anyhow!("share password encryption failed"))?;

    let mut payload = Vec::with_capacity(nonce_bytes.len() + ciphertext.len());
    payload.extend_from_slice(&nonce_bytes);
    payload.extend_from_slice(&ciphertext);
    Ok(general_purpose::STANDARD.encode(payload))
}

fn decrypt_share_password(ciphertext: &str) -> anyhow::Result<String> {
    let decoded = general_purpose::STANDARD.decode(ciphertext)?;
    if decoded.len() < 13 {
        anyhow::bail!("invalid share password ciphertext");
    }
    let (nonce_bytes, body) = decoded.split_at(12);
    let cipher = share_cipher();
    let plaintext = cipher
        .decrypt(Nonce::from_slice(nonce_bytes), body)
        .map_err(|_| anyhow::anyhow!("share password decryption failed"))?;
    Ok(String::from_utf8(plaintext)?)
}

fn is_expired(expires_at: &Option<String>) -> bool {
    match expires_at {
        None => false,
        Some(dt) => {
            if let Ok(parsed) = chrono::DateTime::parse_from_rfc3339(dt) {
                parsed.with_timezone(&Utc) <= Utc::now()
            } else if let Ok(parsed) =
                chrono::NaiveDateTime::parse_from_str(dt, "%Y-%m-%d %H:%M:%S")
            {
                parsed <= Utc::now().naive_utc()
            } else {
                false
            }
        }
    }
}

async fn delete_share_ids(db: &Database, share_ids: &[i64]) {
    for share_id in share_ids {
        if let Err(err) = sqlx::query("DELETE FROM shares WHERE id = ?")
            .bind(share_id)
            .execute(&db.pool)
            .await
        {
            tracing::warn!("delete expired share {} failed: {}", share_id, err);
        }
    }
}

async fn cleanup_expired_doc_shares(db: &Database, user_id: i64, doc_id: i64) {
    let shares: Vec<Share> =
        sqlx::query_as("SELECT * FROM shares WHERE user_id = ? AND doc_id = ?")
            .bind(user_id)
            .bind(doc_id)
            .fetch_all(&db.pool)
            .await
            .unwrap_or_default();

    let expired_ids: Vec<i64> = shares
        .into_iter()
        .filter(|share| is_expired(&share.expires_at))
        .map(|share| share.id)
        .collect();

    delete_share_ids(db, &expired_ids).await;
}

async fn find_active_share(db: &Database, token: &str) -> Result<Share, Response> {
    let share: Option<Share> = sqlx::query_as("SELECT * FROM shares WHERE token = ?")
        .bind(token)
        .fetch_optional(&db.pool)
        .await
        .unwrap();

    let share = match share {
        Some(s) => s,
        None => {
            return Err((
                StatusCode::NOT_FOUND,
                Json(json!({"error": "Share link not found"})),
            )
                .into_response())
        }
    };

    if is_expired(&share.expires_at) {
        delete_share_ids(db, &[share.id]).await;
        return Err((
            StatusCode::GONE,
            Json(json!({"error": "Share link expired"})),
        )
            .into_response());
    }

    Ok(share)
}

fn verify_share_password_header(share: &Share, headers: &HeaderMap) -> bool {
    match &share.password_hash {
        Some(hash) => headers
            .get("X-Share-Password")
            .and_then(|v| v.to_str().ok())
            .map(|p| bcrypt::verify(p, hash).unwrap_or(false))
            .unwrap_or(false),
        None => true,
    }
}

async fn ensure_share_access(
    db: &Database,
    token: &str,
    headers: &HeaderMap,
) -> Result<Share, Response> {
    let share = find_active_share(db, token).await?;
    if !verify_share_password_header(&share, headers) {
        return Err((
            StatusCode::UNAUTHORIZED,
            Json(json!({"error": "Password required"})),
        )
            .into_response());
    }
    Ok(share)
}

async fn find_doc_by_id(db: &Database, doc_id: i64) -> Option<DocNode> {
    sqlx::query_as("SELECT * FROM doc_nodes WHERE id = ?")
        .bind(doc_id)
        .fetch_optional(&db.pool)
        .await
        .unwrap_or(None)
}

async fn node_in_shared_scope(
    db: &Database,
    root: &DocNode,
    target_id: i64,
) -> Result<Option<DocNode>, Response> {
    let target = match find_doc_by_id(db, target_id).await {
        Some(node) => node,
        None => return Ok(None),
    };

    if target.user_id != root.user_id {
        return Ok(None);
    }

    if root.node_type == "doc" {
        return Ok((target.id == root.id).then_some(target));
    }

    let mut current = Some(target.clone());
    while let Some(node) = current {
        if node.id == root.id {
            return Ok(Some(target));
        }
        current = match node.parent_id {
            Some(parent_id) => find_doc_by_id(db, parent_id).await,
            None => None,
        };
    }

    Ok(None)
}

#[derive(Deserialize)]
pub struct CreateShareRequest {
    pub doc_id: i64,
    pub password: Option<String>,
    pub expires_at: Option<String>,
}

pub async fn create_share(
    Extension(db): Extension<Arc<Database>>,
    headers: HeaderMap,
    Json(body): Json<CreateShareRequest>,
) -> impl IntoResponse {
    let user = match auth::require_user(&db, &headers).await {
        Ok(user) => user,
        Err(resp) => return resp,
    };

    let doc_exists: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM doc_nodes WHERE id = ? AND user_id = ?)")
            .bind(body.doc_id)
            .bind(user.id)
            .fetch_one(&db.pool)
            .await
            .unwrap_or(false);

    if !doc_exists {
        return (
            StatusCode::NOT_FOUND,
            Json(json!({"error": "Document not found"})),
        )
            .into_response();
    }

    cleanup_expired_doc_shares(&db, user.id, body.doc_id).await;

    let token = generate_token();
    let normalized_password = body
        .password
        .as_ref()
        .map(|p| p.trim().to_string())
        .filter(|p| !p.is_empty());
    let password_hash = normalized_password
        .as_ref()
        .map(|password| bcrypt::hash(password, 10).unwrap());
    let password_ciphertext = match normalized_password.as_deref() {
        Some(password) => match encrypt_share_password(password) {
            Ok(value) => Some(value),
            Err(err) => {
                tracing::error!("encrypt share password failed: {}", err);
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(json!({"error": "Failed to protect share password"})),
                )
                    .into_response();
            }
        },
        None => None,
    };

    let share_id = sqlx::query(
        "INSERT INTO shares (user_id, doc_id, token, password_hash, password_ciphertext, expires_at) VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(user.id)
    .bind(body.doc_id)
    .bind(&token)
    .bind(&password_hash)
    .bind(&password_ciphertext)
    .bind(&body.expires_at)
    .execute(&db.pool)
    .await
    .unwrap()
    .last_insert_rowid();

    let share: Share = sqlx::query_as("SELECT * FROM shares WHERE id = ?")
        .bind(share_id)
        .fetch_one(&db.pool)
        .await
        .unwrap();

    (
        StatusCode::CREATED,
        Json(json!({"share": ShareResponse::from(share)})),
    )
        .into_response()
}

pub async fn list_shares(
    Extension(db): Extension<Arc<Database>>,
    headers: HeaderMap,
    Path(doc_id): Path<i64>,
) -> impl IntoResponse {
    let user = match auth::require_user(&db, &headers).await {
        Ok(user) => user,
        Err(resp) => return resp,
    };

    let shares: Vec<Share> = sqlx::query_as(
        "SELECT * FROM shares WHERE doc_id = ? AND user_id = ? ORDER BY created_at DESC",
    )
    .bind(doc_id)
    .bind(user.id)
    .fetch_all(&db.pool)
    .await
    .unwrap_or_default();

    let mut expired_ids = Vec::new();
    let mut active_shares = Vec::new();
    for share in shares {
        if is_expired(&share.expires_at) {
            expired_ids.push(share.id);
        } else {
            active_shares.push(share);
        }
    }

    delete_share_ids(&db, &expired_ids).await;

    let responses: Vec<ShareResponse> =
        active_shares.into_iter().map(ShareResponse::from).collect();
    Json(json!({"shares": responses})).into_response()
}

pub async fn delete_share(
    Extension(db): Extension<Arc<Database>>,
    headers: HeaderMap,
    Path(id): Path<i64>,
) -> impl IntoResponse {
    let user = match auth::require_user(&db, &headers).await {
        Ok(user) => user,
        Err(resp) => return resp,
    };

    let result = sqlx::query("DELETE FROM shares WHERE id = ? AND user_id = ?")
        .bind(id)
        .bind(user.id)
        .execute(&db.pool)
        .await
        .unwrap();

    if result.rows_affected() == 0 {
        return (
            StatusCode::NOT_FOUND,
            Json(json!({"error": "Share not found"})),
        )
            .into_response();
    }

    Json(json!({"message": "删除成功"})).into_response()
}

pub async fn get_share_password(
    Extension(db): Extension<Arc<Database>>,
    headers: HeaderMap,
    Path(id): Path<i64>,
) -> impl IntoResponse {
    let user = match auth::require_user(&db, &headers).await {
        Ok(user) => user,
        Err(resp) => return resp,
    };

    let share: Option<Share> = sqlx::query_as("SELECT * FROM shares WHERE id = ? AND user_id = ?")
        .bind(id)
        .bind(user.id)
        .fetch_optional(&db.pool)
        .await
        .unwrap_or(None);

    let share = match share {
        Some(share) => share,
        None => {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({"error": "Share not found"})),
            )
                .into_response();
        }
    };

    if is_expired(&share.expires_at) {
        delete_share_ids(&db, &[share.id]).await;
        return (
            StatusCode::GONE,
            Json(json!({"error": "Share link expired"})),
        )
            .into_response();
    }

    let ciphertext = match &share.password_ciphertext {
        Some(ciphertext) => ciphertext,
        None => {
            return (
                StatusCode::CONFLICT,
                Json(json!({"error": "Share password is not recoverable"})),
            )
                .into_response();
        }
    };

    match decrypt_share_password(ciphertext) {
        Ok(password) => Json(json!({ "password": password })).into_response(),
        Err(err) => {
            tracing::error!("decrypt share password failed for {}: {}", share.id, err);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "Failed to decrypt share password"})),
            )
                .into_response()
        }
    }
}

pub async fn get_share(
    Extension(db): Extension<Arc<Database>>,
    Path(token): Path<String>,
) -> impl IntoResponse {
    let share = match find_active_share(&db, &token).await {
        Ok(share) => share,
        Err(resp) => return resp,
    };

    let doc: Option<DocNode> = sqlx::query_as("SELECT * FROM doc_nodes WHERE id = ?")
        .bind(share.doc_id)
        .fetch_optional(&db.pool)
        .await
        .unwrap();

    let doc = match doc {
        Some(d) => d,
        None => {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({"error": "Document deleted"})),
            )
                .into_response()
        }
    };

    Json(json!({
        "share": {
            "token": share.token,
            "doc_name": doc.name,
            "doc_type": doc.node_type,
            "has_password": share.password_hash.is_some(),
            "expires_at": share.expires_at
        }
    }))
    .into_response()
}

#[derive(Deserialize)]
pub struct VerifyShareRequest {
    pub password: String,
}

pub async fn verify_share(
    Extension(db): Extension<Arc<Database>>,
    Path(token): Path<String>,
    Json(body): Json<VerifyShareRequest>,
) -> impl IntoResponse {
    let share: Option<Share> = sqlx::query_as("SELECT * FROM shares WHERE token = ?")
        .bind(&token)
        .fetch_optional(&db.pool)
        .await
        .unwrap();

    let share = match share {
        Some(s) => s,
        None => {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({"error": "Share link not found"})),
            )
                .into_response()
        }
    };

    if is_expired(&share.expires_at) {
        delete_share_ids(&db, &[share.id]).await;
        return (
            StatusCode::GONE,
            Json(json!({"error": "Share link expired"})),
        )
            .into_response();
    }

    match &share.password_hash {
        None => Json(json!({"verified": true})).into_response(),
        Some(hash) => {
            if bcrypt::verify(&body.password, hash).unwrap_or(false) {
                Json(json!({"verified": true})).into_response()
            } else {
                (
                    StatusCode::UNAUTHORIZED,
                    Json(json!({"error": "Invalid password", "verified": false})),
                )
                    .into_response()
            }
        }
    }
}

pub async fn get_share_content(
    Extension(db): Extension<Arc<Database>>,
    Path(token): Path<String>,
    headers: HeaderMap,
) -> impl IntoResponse {
    let share = match ensure_share_access(&db, &token, &headers).await {
        Ok(share) => share,
        Err(resp) => return resp,
    };

    let doc = find_doc_by_id(&db, share.doc_id).await;

    match doc {
        Some(d) => {
            if d.node_type == "dir" {
                let children: Vec<DocNode> = sqlx::query_as(
                    "SELECT * FROM doc_nodes WHERE user_id = ? ORDER BY sort_order ASC",
                )
                .bind(d.user_id)
                .fetch_all(&db.pool)
                .await
                .unwrap_or_default();

                let tree = build_subtree(children, d.id);
                let mut resp = DocNodeResponse::from_node_meta(d);
                resp.children = tree;
                Json(json!({"node": resp})).into_response()
            } else {
                Json(json!({"node": DocNodeResponse::from_node(d)})).into_response()
            }
        }
        None => (
            StatusCode::NOT_FOUND,
            Json(json!({"error": "Document deleted"})),
        )
            .into_response(),
    }
}

fn build_subtree(all_nodes: Vec<DocNode>, root_id: i64) -> Vec<DocNodeResponse> {
    let mut result = vec![];
    for node in all_nodes.iter() {
        if node.parent_id == Some(root_id) {
            let mut resp = DocNodeResponse::from_node_meta(node.clone());
            resp.children = build_subtree(all_nodes.clone(), node.id);
            result.push(resp);
        }
    }
    result.sort_by(|a, b| {
        if a.node_type != b.node_type {
            if a.node_type == "dir" {
                std::cmp::Ordering::Less
            } else {
                std::cmp::Ordering::Greater
            }
        } else {
            a.sort_order.cmp(&b.sort_order)
        }
    });
    result
}

pub async fn get_share_node_content(
    Extension(db): Extension<Arc<Database>>,
    Path((token, node_id)): Path<(String, i64)>,
    headers: HeaderMap,
) -> impl IntoResponse {
    let share = match ensure_share_access(&db, &token, &headers).await {
        Ok(share) => share,
        Err(resp) => return resp,
    };

    let root = match find_doc_by_id(&db, share.doc_id).await {
        Some(node) => node,
        None => {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({"error": "Document deleted"})),
            )
                .into_response();
        }
    };

    let node = match node_in_shared_scope(&db, &root, node_id).await {
        Ok(Some(node)) => node,
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({"error": "Document not found in share"})),
            )
                .into_response();
        }
        Err(resp) => return resp,
    };

    Json(json!({"node": DocNodeResponse::from_node(node)})).into_response()
}
