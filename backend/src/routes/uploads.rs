use axum::{
    body::Body,
    extract::{Extension, Multipart, Path},
    http::{header, HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use chrono::Local;
use serde_json::json;
use std::{
    path::{Path as FsPath, PathBuf},
    str::FromStr,
    sync::Arc,
};
use tokio::fs;
use uuid::Uuid;

use crate::{
    auth,
    db::Database,
    models::{UploadAsset, UploadAssetResponse, UploadUsage},
};

fn upload_root() -> PathBuf {
    std::env::var("UPLOAD_DIR")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("uploads"))
}

fn sanitize_kind(raw: &str) -> String {
    let filtered: String = raw
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric() || *ch == '-' || *ch == '_')
        .collect();

    if filtered.is_empty() {
        "file".to_string()
    } else {
        filtered.to_ascii_lowercase()
    }
}

fn sanitize_extension(raw: &str) -> Option<String> {
    let trimmed = raw.trim().trim_start_matches('.');
    if trimmed.is_empty() {
        return None;
    }

    let filtered: String = trimmed
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric())
        .collect();

    if filtered.is_empty() {
        None
    } else {
        Some(filtered.to_ascii_lowercase())
    }
}

fn sanitize_original_name(name: &str) -> String {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return "file".to_string();
    }

    trimmed
        .chars()
        .map(|ch| {
            if ch.is_control() || matches!(ch, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|')
            {
                '_'
            } else {
                ch
            }
        })
        .collect()
}

fn extension_from_filename(name: &str) -> Option<String> {
    FsPath::new(name)
        .extension()
        .and_then(|ext| ext.to_str())
        .and_then(sanitize_extension)
}

fn extension_from_content_type(content_type: Option<&str>) -> Option<String> {
    let mime = content_type.and_then(|value| mime_guess::mime::Mime::from_str(value).ok())?;
    mime_guess::get_mime_extensions_str(mime.essence_str())
        .and_then(|exts| exts.first().copied())
        .and_then(sanitize_extension)
}

fn content_type_is_image(content_type: Option<&str>) -> bool {
    content_type
        .map(|value| value.to_ascii_lowercase().starts_with("image/"))
        .unwrap_or(false)
}

fn upload_public_url(id: i64) -> String {
    format!("/uploads/files/{id}")
}

fn is_image_asset(content_type: Option<&str>, original_name: &str) -> bool {
    content_type_is_image(content_type)
        || extension_from_filename(original_name)
            .map(|ext| {
                matches!(
                    ext.as_str(),
                    "png" | "jpg" | "jpeg" | "gif" | "webp" | "svg" | "bmp"
                )
            })
            .unwrap_or(false)
}

async fn build_usage(db: &Database, user_id: i64, url: &str) -> UploadUsage {
    let avatar: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM users WHERE id = ? AND avatar = ?)")
            .bind(user_id)
            .bind(url)
            .fetch_one(&db.pool)
            .await
            .unwrap_or(false);

    let project_refs: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM projects WHERE user_id = ? AND background_image = ?",
    )
    .bind(user_id)
    .bind(url)
    .fetch_one(&db.pool)
    .await
    .unwrap_or(0);

    let doc_refs: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM doc_nodes WHERE user_id = ? AND content LIKE '%' || ? || '%'",
    )
    .bind(user_id)
    .bind(url)
    .fetch_one(&db.pool)
    .await
    .unwrap_or(0);

    UploadUsage {
        avatar,
        project_refs,
        doc_refs,
    }
}

async fn to_response(db: &Database, asset: UploadAsset) -> UploadAssetResponse {
    let url = upload_public_url(asset.id);
    let usage = build_usage(db, asset.user_id, &url).await;

    UploadAssetResponse {
        id: asset.id,
        kind: asset.kind,
        original_name: asset.original_name,
        url,
        content_type: asset.content_type,
        size: asset.size,
        created_at: asset.created_at,
        updated_at: asset.updated_at,
        usage,
    }
}

struct ParsedUpload {
    kind: String,
    original_name: String,
    content_type: Option<String>,
    bytes: Vec<u8>,
    extension: String,
}

async fn parse_multipart(mut multipart: Multipart) -> Result<ParsedUpload, Response> {
    let mut kind = "file".to_string();
    let mut file_name: Option<String> = None;
    let mut content_type: Option<String> = None;
    let mut file_bytes: Option<Vec<u8>> = None;

    loop {
        let next_field = match multipart.next_field().await {
            Ok(field) => field,
            Err(err) => {
                tracing::warn!("upload multipart parse failed: {}", err);
                return Err((
                    StatusCode::BAD_REQUEST,
                    Json(json!({"error": "Invalid multipart payload"})),
                )
                    .into_response());
            }
        };

        let Some(field) = next_field else {
            break;
        };

        let field_name = field.name().unwrap_or_default().to_string();
        if field_name == "kind" {
            match field.text().await {
                Ok(value) => kind = sanitize_kind(&value),
                Err(err) => {
                    tracing::warn!("upload kind field parse failed: {}", err);
                    return Err((
                        StatusCode::BAD_REQUEST,
                        Json(json!({"error": "Invalid upload kind"})),
                    )
                        .into_response());
                }
            }
            continue;
        }

        if field_name == "file" {
            file_name = field.file_name().map(|name| sanitize_original_name(name));
            content_type = field.content_type().map(str::to_string);

            match field.bytes().await {
                Ok(bytes) => file_bytes = Some(bytes.to_vec()),
                Err(err) => {
                    tracing::warn!("upload file read failed: {}", err);
                    return Err((
                        StatusCode::BAD_REQUEST,
                        Json(json!({"error": "Failed to read upload"})),
                    )
                        .into_response());
                }
            }
        }
    }

    let bytes = match file_bytes {
        Some(bytes) if !bytes.is_empty() => bytes,
        _ => {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(json!({"error": "No file uploaded"})),
            )
                .into_response());
        }
    };

    let original_name = file_name.unwrap_or_else(|| "file".to_string());
    let extension = extension_from_filename(&original_name)
        .or_else(|| extension_from_content_type(content_type.as_deref()))
        .unwrap_or_else(|| "bin".to_string());

    Ok(ParsedUpload {
        kind,
        original_name,
        content_type,
        bytes,
        extension,
    })
}

async fn store_upload_record(
    db: &Database,
    user_id: i64,
    parsed: ParsedUpload,
    existing_id: Option<i64>,
) -> Result<UploadAsset, Response> {
    if matches!(
        parsed.kind.as_str(),
        "avatar" | "project-background" | "doc-image"
    ) && !is_image_asset(parsed.content_type.as_deref(), &parsed.original_name)
    {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "Only image files are allowed for this upload kind"})),
        )
            .into_response());
    }

    let root = upload_root();
    let date_segment = Local::now().format("%Y%m%d").to_string();

    let asset_id = if let Some(id) = existing_id {
        id
    } else {
        match sqlx::query(
            "INSERT INTO uploads (user_id, kind, original_name, stored_path, content_type, size)
             VALUES (?, ?, ?, '', ?, 0)",
        )
        .bind(user_id)
        .bind(&parsed.kind)
        .bind(&parsed.original_name)
        .bind(&parsed.content_type)
        .execute(&db.pool)
        .await
        {
            Ok(result) => result.last_insert_rowid(),
            Err(err) => {
                tracing::error!("create upload record failed: {}", err);
                return Err((
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(json!({"error": "Failed to create upload record"})),
                )
                    .into_response());
            }
        }
    };

    let relative_path = format!(
        "{}/{}/{}-{}-{}.{}",
        user_id,
        date_segment,
        parsed.kind,
        asset_id,
        Uuid::new_v4().simple(),
        parsed.extension
    );
    let target_path = root.join(&relative_path);
    let parent_dir = target_path.parent().ok_or_else(|| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "Invalid upload path"})),
        )
            .into_response()
    })?;

    if let Err(err) = fs::create_dir_all(parent_dir).await {
        tracing::error!("create upload dir failed: {}", err);
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "Failed to prepare upload directory"})),
        )
            .into_response());
    }

    if let Err(err) = fs::write(&target_path, &parsed.bytes).await {
        tracing::error!("write upload file failed: {}", err);
        if existing_id.is_none() {
            let _ = sqlx::query("DELETE FROM uploads WHERE id = ?")
                .bind(asset_id)
                .execute(&db.pool)
                .await;
        }
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "Failed to save upload"})),
        )
            .into_response());
    }

    let previous_path: Option<String> = if existing_id.is_some() {
        sqlx::query_scalar("SELECT stored_path FROM uploads WHERE id = ? AND user_id = ?")
            .bind(asset_id)
            .bind(user_id)
            .fetch_optional(&db.pool)
            .await
            .unwrap_or(None)
    } else {
        None
    };

    if let Err(err) = sqlx::query(
        "UPDATE uploads
         SET kind = ?, original_name = ?, stored_path = ?, content_type = ?, size = ?, updated_at = datetime('now')
         WHERE id = ? AND user_id = ?",
    )
    .bind(&parsed.kind)
    .bind(&parsed.original_name)
    .bind(relative_path.replace('\\', "/"))
    .bind(&parsed.content_type)
    .bind(parsed.bytes.len() as i64)
    .bind(asset_id)
    .bind(user_id)
    .execute(&db.pool)
    .await
    {
        tracing::error!("update upload record failed: {}", err);
        let _ = fs::remove_file(&target_path).await;
        if existing_id.is_none() {
            let _ = sqlx::query("DELETE FROM uploads WHERE id = ?")
                .bind(asset_id)
                .execute(&db.pool)
                .await;
        }
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "Failed to update upload record"})),
        )
            .into_response());
    }

    if let Some(previous_path) = previous_path {
        if !previous_path.trim().is_empty()
            && previous_path.replace('\\', "/") != relative_path.replace('\\', "/")
        {
            let old_path = root.join(previous_path);
            let _ = fs::remove_file(old_path).await;
        }
    }

    match sqlx::query_as("SELECT * FROM uploads WHERE id = ? AND user_id = ?")
        .bind(asset_id)
        .bind(user_id)
        .fetch_one(&db.pool)
        .await
    {
        Ok(asset) => Ok(asset),
        Err(err) => {
            tracing::error!("fetch upload record failed: {}", err);
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "Failed to fetch upload record"})),
            )
                .into_response())
        }
    }
}

pub async fn upload_file(
    Extension(db): Extension<Arc<Database>>,
    headers: HeaderMap,
    multipart: Multipart,
) -> impl IntoResponse {
    let claims = match auth::extract_user_id(&headers) {
        Some(c) => c,
        None => {
            return (
                StatusCode::UNAUTHORIZED,
                Json(json!({"error": "Unauthorized"})),
            )
                .into_response();
        }
    };

    let parsed = match parse_multipart(multipart).await {
        Ok(parsed) => parsed,
        Err(resp) => return resp,
    };

    let asset = match store_upload_record(&db, claims.sub, parsed, None).await {
        Ok(asset) => asset,
        Err(resp) => return resp,
    };

    let response = to_response(&db, asset).await;
    (StatusCode::CREATED, Json(json!({ "upload": response }))).into_response()
}

pub async fn replace_upload(
    Extension(db): Extension<Arc<Database>>,
    headers: HeaderMap,
    Path(id): Path<i64>,
    multipart: Multipart,
) -> impl IntoResponse {
    let claims = match auth::extract_user_id(&headers) {
        Some(c) => c,
        None => {
            return (
                StatusCode::UNAUTHORIZED,
                Json(json!({"error": "Unauthorized"})),
            )
                .into_response();
        }
    };

    let existing: Option<UploadAsset> =
        sqlx::query_as("SELECT * FROM uploads WHERE id = ? AND user_id = ?")
            .bind(id)
            .bind(claims.sub)
            .fetch_optional(&db.pool)
            .await
            .unwrap_or(None);

    let Some(existing) = existing else {
        return (
            StatusCode::NOT_FOUND,
            Json(json!({"error": "Upload not found"})),
        )
            .into_response();
    };

    let parsed = match parse_multipart(multipart).await {
        Ok(mut parsed) => {
            parsed.kind = existing.kind.clone();
            parsed
        }
        Err(resp) => return resp,
    };

    let asset = match store_upload_record(&db, claims.sub, parsed, Some(id)).await {
        Ok(asset) => asset,
        Err(resp) => return resp,
    };

    let response = to_response(&db, asset).await;
    Json(json!({ "upload": response })).into_response()
}

pub async fn list_uploads(
    Extension(db): Extension<Arc<Database>>,
    headers: HeaderMap,
) -> impl IntoResponse {
    let claims = match auth::extract_user_id(&headers) {
        Some(c) => c,
        None => {
            return (
                StatusCode::UNAUTHORIZED,
                Json(json!({"error": "Unauthorized"})),
            )
                .into_response();
        }
    };

    let assets: Vec<UploadAsset> = sqlx::query_as(
        "SELECT * FROM uploads WHERE user_id = ? ORDER BY updated_at DESC, created_at DESC, id DESC",
    )
    .bind(claims.sub)
    .fetch_all(&db.pool)
    .await
    .unwrap_or_default();

    let mut response_assets = Vec::with_capacity(assets.len());
    for asset in assets {
        response_assets.push(to_response(&db, asset).await);
    }

    Json(json!({ "uploads": response_assets })).into_response()
}

pub async fn delete_upload(
    Extension(db): Extension<Arc<Database>>,
    headers: HeaderMap,
    Path(id): Path<i64>,
) -> impl IntoResponse {
    let claims = match auth::extract_user_id(&headers) {
        Some(c) => c,
        None => {
            return (
                StatusCode::UNAUTHORIZED,
                Json(json!({"error": "Unauthorized"})),
            )
                .into_response();
        }
    };

    let existing: Option<UploadAsset> =
        sqlx::query_as("SELECT * FROM uploads WHERE id = ? AND user_id = ?")
            .bind(id)
            .bind(claims.sub)
            .fetch_optional(&db.pool)
            .await
            .unwrap_or(None);

    let Some(existing) = existing else {
        return (
            StatusCode::NOT_FOUND,
            Json(json!({"error": "Upload not found"})),
        )
            .into_response();
    };

    let url = upload_public_url(existing.id);
    let root = upload_root();
    let file_path = root.join(existing.stored_path.replace('\\', "/"));

    if let Err(err) = sqlx::query("UPDATE users SET avatar = NULL WHERE id = ? AND avatar = ?")
        .bind(claims.sub)
        .bind(&url)
        .execute(&db.pool)
        .await
    {
        tracing::warn!("clear avatar ref failed: {}", err);
    }

    if let Err(err) = sqlx::query(
        "UPDATE projects SET background_image = NULL, updated_at = datetime('now') WHERE user_id = ? AND background_image = ?",
    )
    .bind(claims.sub)
    .bind(&url)
    .execute(&db.pool)
    .await
    {
        tracing::warn!("clear project background refs failed: {}", err);
    }

    if let Err(err) = sqlx::query("DELETE FROM uploads WHERE id = ? AND user_id = ?")
        .bind(id)
        .bind(claims.sub)
        .execute(&db.pool)
        .await
    {
        tracing::error!("delete upload record failed: {}", err);
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "Failed to delete upload"})),
        )
            .into_response();
    }

    let _ = fs::remove_file(file_path).await;

    Json(json!({ "message": "删除成功" })).into_response()
}

pub async fn serve_upload(
    Extension(db): Extension<Arc<Database>>,
    Path(id): Path<i64>,
) -> impl IntoResponse {
    let asset: Option<UploadAsset> = sqlx::query_as("SELECT * FROM uploads WHERE id = ?")
        .bind(id)
        .fetch_optional(&db.pool)
        .await
        .unwrap_or(None);

    let Some(asset) = asset else {
        return Response::builder()
            .status(StatusCode::NOT_FOUND)
            .body(Body::from("Not Found"))
            .unwrap();
    };

    let file_path = upload_root().join(asset.stored_path.replace('\\', "/"));
    let content = match fs::read(&file_path).await {
        Ok(content) => content,
        Err(_) => {
            return Response::builder()
                .status(StatusCode::NOT_FOUND)
                .body(Body::from("Not Found"))
                .unwrap();
        }
    };

    let content_type = asset
        .content_type
        .as_deref()
        .map(|value| value.to_string())
        .unwrap_or_else(|| {
            mime_guess::from_path(&asset.original_name)
                .first_or_octet_stream()
                .to_string()
        });

    let disposition = if is_image_asset(Some(&content_type), &asset.original_name) {
        "inline".to_string()
    } else {
        format!(
            "inline; filename=\"{}\"",
            asset.original_name.replace('"', "_")
        )
    };

    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, content_type)
        .header(header::CACHE_CONTROL, "no-store, max-age=0")
        .header(header::CONTENT_DISPOSITION, disposition)
        .body(Body::from(content))
        .unwrap()
}
