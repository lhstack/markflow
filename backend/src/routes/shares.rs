use axum::{
    extract::{Extension, Path},
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    Json,
};
use chrono::Utc;
use rand::{distributions::Alphanumeric, Rng};
use serde::Deserialize;
use serde_json::json;
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
    let shares: Vec<Share> = sqlx::query_as("SELECT * FROM shares WHERE user_id = ? AND doc_id = ?")
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
    let password_hash = body.password.as_ref().map(|p| bcrypt::hash(p, 10).unwrap());

    let share_id = sqlx::query(
        "INSERT INTO shares (user_id, doc_id, token, password_hash, expires_at) VALUES (?, ?, ?, ?, ?)",
    )
    .bind(user.id)
    .bind(body.doc_id)
    .bind(&token)
    .bind(&password_hash)
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

    let responses: Vec<ShareResponse> = active_shares.into_iter().map(ShareResponse::from).collect();
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

pub async fn get_share(
    Extension(db): Extension<Arc<Database>>,
    Path(token): Path<String>,
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

    if share.password_hash.is_some() {
        let verified = headers
            .get("X-Share-Password")
            .and_then(|v| v.to_str().ok())
            .map(|p| bcrypt::verify(p, share.password_hash.as_ref().unwrap()).unwrap_or(false))
            .unwrap_or(false);

        if !verified {
            return (
                StatusCode::UNAUTHORIZED,
                Json(json!({"error": "Password required"})),
            )
                .into_response();
        }
    }

    let doc: Option<DocNode> = sqlx::query_as("SELECT * FROM doc_nodes WHERE id = ?")
        .bind(share.doc_id)
        .fetch_optional(&db.pool)
        .await
        .unwrap();

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
                let mut resp = DocNodeResponse::from_node(d);
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
            let mut resp = DocNodeResponse::from_node(node.clone());
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
