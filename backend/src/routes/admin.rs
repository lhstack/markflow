use axum::{
    body::Body,
    extract::{Extension, Path},
    http::{header, HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use base64::{engine::general_purpose, Engine as _};
use serde::Deserialize;
use serde_json::json;
use std::{path::PathBuf, sync::Arc};
use tokio::fs;

use crate::{
    auth,
    db::Database,
    models::{
        AdminUserResponse, DocNode, Project, Share, SystemSettingsResponse, UploadAsset, User,
    },
};

fn json_error(status: StatusCode, message: &str) -> Response {
    (status, Json(json!({ "error": message }))).into_response()
}

fn upload_root() -> PathBuf {
    std::env::var("UPLOAD_DIR")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("uploads"))
}

async fn load_managed_user(db: &Database, user_id: i64) -> Result<User, Response> {
    let user =
        match sqlx::query_as::<_, User>("SELECT * FROM users WHERE id = ? AND is_super_admin = 0")
            .bind(user_id)
            .fetch_optional(&db.pool)
            .await
        {
            Ok(Some(user)) => user,
            Ok(None) => return Err(json_error(StatusCode::NOT_FOUND, "User not found")),
            Err(err) => {
                tracing::error!("load managed user failed: {}", err);
                return Err(json_error(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "Failed to load user",
                ));
            }
        };

    Ok(user)
}

#[derive(Deserialize)]
pub struct CreateUserRequest {
    pub username: String,
    pub password: String,
    pub is_active: Option<bool>,
}

pub async fn create_user(
    Extension(db): Extension<Arc<Database>>,
    headers: HeaderMap,
    Json(body): Json<CreateUserRequest>,
) -> impl IntoResponse {
    if let Err(resp) = auth::require_super_admin(&db, &headers).await {
        return resp;
    }

    let username = body.username.trim();
    if username.len() < 3 || username.len() > 32 {
        return json_error(StatusCode::BAD_REQUEST, "Username length must be 3-32");
    }
    if username.eq_ignore_ascii_case("admin") {
        return json_error(StatusCode::BAD_REQUEST, "Username admin is reserved");
    }
    if body.password.trim().len() < 6 {
        return json_error(
            StatusCode::BAD_REQUEST,
            "Password length must be at least 6",
        );
    }

    let exists: bool =
        match sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM users WHERE username = ?)")
            .bind(username)
            .fetch_one(&db.pool)
            .await
        {
            Ok(exists) => exists,
            Err(err) => {
                tracing::error!("check user exists failed: {}", err);
                return json_error(StatusCode::INTERNAL_SERVER_ERROR, "Failed to create user");
            }
        };

    if exists {
        return json_error(StatusCode::CONFLICT, "Username already exists");
    }

    let password_hash = match bcrypt::hash(body.password.trim(), 10) {
        Ok(hash) => hash,
        Err(err) => {
            tracing::error!("hash create user password failed: {}", err);
            return json_error(StatusCode::INTERNAL_SERVER_ERROR, "Failed to create user");
        }
    };

    let created_id = match sqlx::query(
        "INSERT INTO users (username, password_hash, is_super_admin, is_active)
         VALUES (?, ?, 0, ?)",
    )
    .bind(username)
    .bind(password_hash)
    .bind(if body.is_active.unwrap_or(true) { 1 } else { 0 })
    .execute(&db.pool)
    .await
    {
        Ok(result) => result.last_insert_rowid(),
        Err(err) => {
            tracing::error!("create user failed: {}", err);
            return json_error(StatusCode::INTERNAL_SERVER_ERROR, "Failed to create user");
        }
    };

    match load_managed_user(&db, created_id).await {
        Ok(user) => (
            StatusCode::CREATED,
            Json(json!({ "user": AdminUserResponse::from(user) })),
        )
            .into_response(),
        Err(resp) => resp,
    }
}

pub async fn get_public_settings(Extension(db): Extension<Arc<Database>>) -> impl IntoResponse {
    match db.get_system_settings().await {
        Ok(settings) => Json(json!({
            "settings": SystemSettingsResponse::from(settings)
        }))
        .into_response(),
        Err(err) => {
            tracing::error!("get public settings failed: {}", err);
            json_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "Failed to load public settings",
            )
        }
    }
}

pub async fn get_system_settings(
    Extension(db): Extension<Arc<Database>>,
    headers: HeaderMap,
) -> impl IntoResponse {
    if let Err(resp) = auth::require_super_admin(&db, &headers).await {
        return resp;
    }

    match db.get_system_settings().await {
        Ok(settings) => Json(json!({
            "settings": SystemSettingsResponse::from(settings)
        }))
        .into_response(),
        Err(err) => {
            tracing::error!("get system settings failed: {}", err);
            json_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "Failed to load system settings",
            )
        }
    }
}

#[derive(Deserialize)]
pub struct UpdateSystemSettingsRequest {
    pub registration_enabled: Option<bool>,
    pub upload_max_mb: Option<i64>,
}

pub async fn update_system_settings(
    Extension(db): Extension<Arc<Database>>,
    headers: HeaderMap,
    Json(body): Json<UpdateSystemSettingsRequest>,
) -> impl IntoResponse {
    if let Err(resp) = auth::require_super_admin(&db, &headers).await {
        return resp;
    }

    let current = match db.get_system_settings().await {
        Ok(settings) => settings,
        Err(err) => {
            tracing::error!("load current system settings failed: {}", err);
            return json_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "Failed to load current settings",
            );
        }
    };

    let upload_max_mb = body
        .upload_max_mb
        .unwrap_or(current.upload_max_bytes / 1024 / 1024);
    if !(1..=1024).contains(&upload_max_mb) {
        return json_error(
            StatusCode::BAD_REQUEST,
            "Upload size limit must be between 1MB and 1024MB",
        );
    }

    match db
        .update_system_settings(
            body.registration_enabled
                .unwrap_or(current.registration_enabled == 1),
            upload_max_mb * 1024 * 1024,
        )
        .await
    {
        Ok(settings) => Json(json!({
            "settings": SystemSettingsResponse::from(settings)
        }))
        .into_response(),
        Err(err) => {
            tracing::error!("update system settings failed: {}", err);
            json_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "Failed to update system settings",
            )
        }
    }
}

pub async fn list_users(
    Extension(db): Extension<Arc<Database>>,
    headers: HeaderMap,
) -> impl IntoResponse {
    if let Err(resp) = auth::require_super_admin(&db, &headers).await {
        return resp;
    }

    let users = match sqlx::query_as::<_, User>(
        "SELECT * FROM users WHERE is_super_admin = 0 ORDER BY created_at DESC, id DESC",
    )
    .fetch_all(&db.pool)
    .await
    {
        Ok(users) => users,
        Err(err) => {
            tracing::error!("list users failed: {}", err);
            return json_error(StatusCode::INTERNAL_SERVER_ERROR, "Failed to load users");
        }
    };

    let users: Vec<AdminUserResponse> = users.into_iter().map(AdminUserResponse::from).collect();
    Json(json!({ "users": users })).into_response()
}

pub async fn delete_user(
    Extension(db): Extension<Arc<Database>>,
    headers: HeaderMap,
    Path(id): Path<i64>,
) -> impl IntoResponse {
    if let Err(resp) = auth::require_super_admin(&db, &headers).await {
        return resp;
    }

    let user = match load_managed_user(&db, id).await {
        Ok(user) => user,
        Err(resp) => return resp,
    };

    let user_upload_dir = upload_root().join(user.id.to_string());

    let mut tx = match db.pool.begin().await {
        Ok(tx) => tx,
        Err(err) => {
            tracing::error!("begin delete user tx failed: {}", err);
            return json_error(StatusCode::INTERNAL_SERVER_ERROR, "Failed to delete user");
        }
    };

    if let Err(err) = sqlx::query("DELETE FROM users WHERE id = ? AND is_super_admin = 0")
        .bind(user.id)
        .execute(&mut *tx)
        .await
    {
        tracing::error!("delete user failed: {}", err);
        return json_error(StatusCode::INTERNAL_SERVER_ERROR, "Failed to delete user");
    }

    if let Err(err) = tx.commit().await {
        tracing::error!("commit delete user failed: {}", err);
        return json_error(StatusCode::INTERNAL_SERVER_ERROR, "Failed to delete user");
    }

    if user_upload_dir.exists() {
        if let Err(err) = fs::remove_dir_all(&user_upload_dir).await {
            tracing::warn!(
                "delete user upload directory failed for {}: {}",
                user_upload_dir.display(),
                err
            );
        }
    }

    Json(json!({ "message": "User deleted" })).into_response()
}

pub async fn export_user_data(
    Extension(db): Extension<Arc<Database>>,
    headers: HeaderMap,
    Path(id): Path<i64>,
) -> impl IntoResponse {
    if let Err(resp) = auth::require_super_admin(&db, &headers).await {
        return resp;
    }

    let user = match load_managed_user(&db, id).await {
        Ok(user) => user,
        Err(resp) => return resp,
    };

    let projects: Vec<Project> = match sqlx::query_as(
        "SELECT * FROM projects WHERE user_id = ? ORDER BY sort_order ASC, created_at ASC",
    )
    .bind(user.id)
    .fetch_all(&db.pool)
    .await
    {
        Ok(rows) => rows,
        Err(err) => {
            tracing::error!("export user projects failed: {}", err);
            return json_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "Failed to export user data",
            );
        }
    };

    let doc_nodes: Vec<DocNode> = match sqlx::query_as(
        "SELECT * FROM doc_nodes WHERE user_id = ? ORDER BY sort_order ASC, created_at ASC, id ASC",
    )
    .bind(user.id)
    .fetch_all(&db.pool)
    .await
    {
        Ok(rows) => rows,
        Err(err) => {
            tracing::error!("export user docs failed: {}", err);
            return json_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "Failed to export user data",
            );
        }
    };

    let shares: Vec<Share> = match sqlx::query_as(
        "SELECT * FROM shares WHERE user_id = ? ORDER BY created_at ASC, id ASC",
    )
    .bind(user.id)
    .fetch_all(&db.pool)
    .await
    {
        Ok(rows) => rows,
        Err(err) => {
            tracing::error!("export user shares failed: {}", err);
            return json_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "Failed to export user data",
            );
        }
    };

    let uploads: Vec<UploadAsset> = match sqlx::query_as(
        "SELECT * FROM uploads WHERE user_id = ? ORDER BY created_at ASC, id ASC",
    )
    .bind(user.id)
    .fetch_all(&db.pool)
    .await
    {
        Ok(rows) => rows,
        Err(err) => {
            tracing::error!("export user uploads failed: {}", err);
            return json_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "Failed to export user data",
            );
        }
    };

    let root = upload_root();
    let mut upload_payloads = Vec::with_capacity(uploads.len());
    for upload in uploads {
        let file_path = root.join(upload.stored_path.replace('\\', "/"));
        let file_bytes = match fs::read(&file_path).await {
            Ok(bytes) => Some(general_purpose::STANDARD.encode(bytes)),
            Err(err) => {
                tracing::warn!(
                    "read export upload file failed {}: {}",
                    file_path.display(),
                    err
                );
                None
            }
        };

        upload_payloads.push(json!({
            "id": upload.id,
            "kind": upload.kind,
            "original_name": upload.original_name,
            "stored_path": upload.stored_path,
            "content_type": upload.content_type,
            "size": upload.size,
            "created_at": upload.created_at,
            "updated_at": upload.updated_at,
            "file_base64": file_bytes,
        }));
    }

    let payload = json!({
        "exported_at": chrono::Utc::now().to_rfc3339(),
        "user": {
            "id": user.id,
            "username": user.username,
            "avatar": user.avatar,
            "totp_enabled": user.totp_enabled == 1,
            "has_totp_secret": user.totp_secret.as_deref().map(|v| !v.trim().is_empty()).unwrap_or(false),
            "is_active": user.is_active == 1,
            "created_at": user.created_at,
            "updated_at": user.updated_at,
        },
        "projects": projects,
        "doc_nodes": doc_nodes,
        "shares": shares,
        "uploads": upload_payloads,
    });

    let file_name = format!("markflow-user-{}-export.json", user.username);
    match serde_json::to_vec_pretty(&payload) {
        Ok(bytes) => Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, "application/json; charset=utf-8")
            .header(
                header::CONTENT_DISPOSITION,
                format!("attachment; filename=\"{}\"", file_name.replace('"', "_")),
            )
            .body(Body::from(bytes))
            .unwrap(),
        Err(err) => {
            tracing::error!("serialize export user data failed: {}", err);
            json_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "Failed to export user data",
            )
        }
    }
}

#[derive(Deserialize)]
pub struct UpdateUserStatusRequest {
    pub is_active: bool,
}

pub async fn update_user_status(
    Extension(db): Extension<Arc<Database>>,
    headers: HeaderMap,
    Path(id): Path<i64>,
    Json(body): Json<UpdateUserStatusRequest>,
) -> impl IntoResponse {
    if let Err(resp) = auth::require_super_admin(&db, &headers).await {
        return resp;
    }

    if let Err(resp) = load_managed_user(&db, id).await {
        return resp;
    }

    match sqlx::query(
        "UPDATE users SET is_active = ?, updated_at = datetime('now') WHERE id = ? AND is_super_admin = 0",
    )
    .bind(if body.is_active { 1 } else { 0 })
    .bind(id)
    .execute(&db.pool)
    .await
    {
        Ok(_) => {}
        Err(err) => {
            tracing::error!("update user status failed: {}", err);
            return json_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "Failed to update user status",
            );
        }
    }

    match load_managed_user(&db, id).await {
        Ok(user) => Json(json!({ "user": AdminUserResponse::from(user) })).into_response(),
        Err(resp) => resp,
    }
}

#[derive(Deserialize)]
pub struct ResetUserPasswordRequest {
    pub new_password: String,
}

pub async fn reset_user_password(
    Extension(db): Extension<Arc<Database>>,
    headers: HeaderMap,
    Path(id): Path<i64>,
    Json(body): Json<ResetUserPasswordRequest>,
) -> impl IntoResponse {
    if let Err(resp) = auth::require_super_admin(&db, &headers).await {
        return resp;
    }

    if body.new_password.trim().len() < 6 {
        return json_error(
            StatusCode::BAD_REQUEST,
            "New password must be at least 6 characters",
        );
    }

    if let Err(resp) = load_managed_user(&db, id).await {
        return resp;
    }

    let password_hash = match bcrypt::hash(body.new_password.trim(), 10) {
        Ok(hash) => hash,
        Err(err) => {
            tracing::error!("hash reset password failed: {}", err);
            return json_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "Failed to reset password",
            );
        }
    };

    match sqlx::query(
        "UPDATE users SET password_hash = ?, updated_at = datetime('now') WHERE id = ? AND is_super_admin = 0",
    )
    .bind(password_hash)
    .bind(id)
    .execute(&db.pool)
    .await
    {
        Ok(_) => Json(json!({ "message": "Password reset" })).into_response(),
        Err(err) => {
            tracing::error!("reset user password failed: {}", err);
            json_error(StatusCode::INTERNAL_SERVER_ERROR, "Failed to reset password")
        }
    }
}

#[derive(Deserialize)]
pub struct UpdateUser2FARequest {
    pub enabled: bool,
}

pub async fn update_user_2fa(
    Extension(db): Extension<Arc<Database>>,
    headers: HeaderMap,
    Path(id): Path<i64>,
    Json(body): Json<UpdateUser2FARequest>,
) -> impl IntoResponse {
    if let Err(resp) = auth::require_super_admin(&db, &headers).await {
        return resp;
    }

    let user = match load_managed_user(&db, id).await {
        Ok(user) => user,
        Err(resp) => return resp,
    };

    if body.enabled
        && user
            .totp_secret
            .as_deref()
            .map(|secret| secret.trim().is_empty())
            .unwrap_or(true)
    {
        return json_error(StatusCode::BAD_REQUEST, "User has not initialized 2FA yet");
    }

    match sqlx::query(
        "UPDATE users SET totp_enabled = ?, updated_at = datetime('now') WHERE id = ? AND is_super_admin = 0",
    )
    .bind(if body.enabled { 1 } else { 0 })
    .bind(id)
    .execute(&db.pool)
    .await
    {
        Ok(_) => {}
        Err(err) => {
            tracing::error!("update user 2fa failed: {}", err);
            return json_error(StatusCode::INTERNAL_SERVER_ERROR, "Failed to update 2FA");
        }
    }

    match load_managed_user(&db, id).await {
        Ok(user) => Json(json!({ "user": AdminUserResponse::from(user) })).into_response(),
        Err(resp) => resp,
    }
}
