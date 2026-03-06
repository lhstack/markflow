use axum::{
    body::Body,
    http::{header, StatusCode, Uri},
    response::{IntoResponse, Response},
};
use rust_embed::RustEmbed;
use std::path::{Component, PathBuf};

#[derive(RustEmbed)]
#[folder = "../frontend/dist/"]
struct Assets;

pub async fn static_handler(uri: Uri) -> impl IntoResponse {
    let request_path = uri.path();
    if request_path.starts_with("/uploads/") {
        return serve_uploaded_file(request_path).await;
    }

    let path = request_path.trim_start_matches('/');

    // Try exact path first
    if let Some(content) = Assets::get(path) {
        let mime = mime_guess::from_path(path).first_or_octet_stream();
        return Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, mime.as_ref())
            .body(Body::from(content.data.to_vec()))
            .unwrap();
    }

    // Try with index.html appended for dirs
    let index_path = if path.is_empty() {
        "index.html".to_string()
    } else {
        format!("{}/index.html", path)
    };
    if let Some(content) = Assets::get(&index_path) {
        let mime = mime_guess::from_path(&index_path).first_or_octet_stream();
        return Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, mime.as_ref())
            .body(Body::from(content.data.to_vec()))
            .unwrap();
    }

    // SPA fallback: serve index.html for all unmatched routes
    if let Some(content) = Assets::get("index.html") {
        return Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, "text/html; charset=utf-8")
            .body(Body::from(content.data.to_vec()))
            .unwrap();
    }

    Response::builder()
        .status(StatusCode::NOT_FOUND)
        .body(Body::from("Not Found"))
        .unwrap()
}

async fn serve_uploaded_file(request_path: &str) -> Response {
    let upload_root = std::env::var("UPLOAD_DIR")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("uploads"));

    let relative = request_path.trim_start_matches("/uploads/");
    let mut safe_path = PathBuf::new();
    for component in PathBuf::from(relative).components() {
        match component {
            Component::Normal(part) => safe_path.push(part),
            _ => {
                return Response::builder()
                    .status(StatusCode::BAD_REQUEST)
                    .body(Body::from("Invalid path"))
                    .unwrap();
            }
        }
    }

    let file_path = upload_root.join(safe_path);
    let content = match tokio::fs::read(&file_path).await {
        Ok(content) => content,
        Err(_) => {
            return Response::builder()
                .status(StatusCode::NOT_FOUND)
                .body(Body::from("Not Found"))
                .unwrap();
        }
    };

    let mime = mime_guess::from_path(&file_path).first_or_octet_stream();
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, mime.as_ref())
        .body(Body::from(content))
        .unwrap()
}
