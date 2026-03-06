use axum::{
    extract::Multipart,
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    Json,
};
use chrono::Local;
use serde_json::json;
use std::{
    path::{Path, PathBuf},
    str::FromStr,
};
use tokio::fs;
use uuid::Uuid;

use crate::auth;

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

fn extension_from_filename(name: &str) -> Option<String> {
    Path::new(name)
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

pub async fn upload_file(headers: HeaderMap, mut multipart: Multipart) -> impl IntoResponse {
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

    let mut kind = "file".to_string();
    let mut file_name: Option<String> = None;
    let mut content_type: Option<String> = None;
    let mut file_bytes: Option<Vec<u8>> = None;

    loop {
        let next_field = match multipart.next_field().await {
            Ok(field) => field,
            Err(err) => {
                tracing::warn!("upload multipart parse failed: {}", err);
                return (
                    StatusCode::BAD_REQUEST,
                    Json(json!({"error": "Invalid multipart payload"})),
                )
                    .into_response();
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
                    return (
                        StatusCode::BAD_REQUEST,
                        Json(json!({"error": "Invalid upload kind"})),
                    )
                        .into_response();
                }
            }
            continue;
        }

        if field_name == "file" {
            file_name = field.file_name().map(str::to_string);
            content_type = field.content_type().map(str::to_string);

            match field.bytes().await {
                Ok(bytes) => file_bytes = Some(bytes.to_vec()),
                Err(err) => {
                    tracing::warn!("upload file read failed: {}", err);
                    return (
                        StatusCode::BAD_REQUEST,
                        Json(json!({"error": "Failed to read upload"})),
                    )
                        .into_response();
                }
            }
        }
    }

    let file_bytes = match file_bytes {
        Some(bytes) if !bytes.is_empty() => bytes,
        _ => {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({"error": "No file uploaded"})),
            )
                .into_response();
        }
    };

    if matches!(kind.as_str(), "avatar" | "project-background")
        && !content_type_is_image(content_type.as_deref())
    {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "Only image files are allowed for this upload kind"})),
        )
            .into_response();
    }

    let extension = file_name
        .as_deref()
        .and_then(extension_from_filename)
        .or_else(|| extension_from_content_type(content_type.as_deref()))
        .unwrap_or_else(|| "bin".to_string());

    let date_segment = Local::now().format("%Y%m%d").to_string();
    let relative_path = format!(
        "{}/{}/{}-{}.{}",
        claims.sub,
        date_segment,
        kind,
        Uuid::new_v4().simple(),
        extension
    );

    let root = upload_root();
    let target_path = root.join(&relative_path);
    let parent_dir = match target_path.parent() {
        Some(dir) => dir,
        None => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "Invalid upload path"})),
            )
                .into_response();
        }
    };

    if let Err(err) = fs::create_dir_all(parent_dir).await {
        tracing::error!("create upload dir failed: {}", err);
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "Failed to prepare upload directory"})),
        )
            .into_response();
    }

    if let Err(err) = fs::write(&target_path, &file_bytes).await {
        tracing::error!("write upload file failed: {}", err);
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "Failed to save upload"})),
        )
            .into_response();
    }

    let public_path = format!("/uploads/{}", relative_path.replace('\\', "/"));

    Json(json!({
        "url": public_path,
        "path": relative_path.replace('\\', "/"),
        "kind": kind,
        "size": file_bytes.len(),
        "content_type": content_type
    }))
    .into_response()
}
