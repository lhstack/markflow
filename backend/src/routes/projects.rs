use axum::{
    extract::{Extension, Path},
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    Json,
};
use serde::Deserialize;
use serde_json::json;
use std::sync::Arc;

use crate::{
    auth,
    db::Database,
    models::{Project, ProjectResponse},
};

async fn user_exists(db: &Database, user_id: i64) -> bool {
    sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM users WHERE id = ?)")
        .bind(user_id)
        .fetch_one(&db.pool)
        .await
        .unwrap_or(false)
}

async fn project_name_exists(
    db: &Database,
    user_id: i64,
    name: &str,
    exclude_id: Option<i64>,
) -> bool {
    match exclude_id {
        Some(project_id) => sqlx::query_scalar(
            "SELECT EXISTS(
                    SELECT 1 FROM projects
                    WHERE user_id = ?
                      AND id != ?
                      AND name = ? COLLATE NOCASE
                )",
        )
        .bind(user_id)
        .bind(project_id)
        .bind(name)
        .fetch_one(&db.pool)
        .await
        .unwrap_or(false),
        None => sqlx::query_scalar(
            "SELECT EXISTS(
                    SELECT 1 FROM projects
                    WHERE user_id = ?
                      AND name = ? COLLATE NOCASE
                )",
        )
        .bind(user_id)
        .bind(name)
        .fetch_one(&db.pool)
        .await
        .unwrap_or(false),
    }
}

pub async fn list_projects(
    Extension(db): Extension<Arc<Database>>,
    headers: HeaderMap,
) -> impl IntoResponse {
    let user = match auth::require_user(&db, &headers).await {
        Ok(user) => user,
        Err(resp) => return resp,
    };

    if !user_exists(&db, user.id).await {
        return (
            StatusCode::UNAUTHORIZED,
            Json(json!({"error": "User does not exist"})),
        )
            .into_response();
    }

    let projects: Vec<Project> = sqlx::query_as(
        "SELECT * FROM projects WHERE user_id = ? ORDER BY sort_order ASC, created_at ASC",
    )
    .bind(user.id)
    .fetch_all(&db.pool)
    .await
    .unwrap_or_default();

    let responses: Vec<ProjectResponse> = projects.into_iter().map(ProjectResponse::from).collect();
    Json(json!({"projects": responses})).into_response()
}

#[derive(Deserialize)]
pub struct CreateProjectRequest {
    pub name: String,
    pub description: Option<String>,
    pub background_image: Option<String>,
}

pub async fn create_project(
    Extension(db): Extension<Arc<Database>>,
    headers: HeaderMap,
    Json(body): Json<CreateProjectRequest>,
) -> impl IntoResponse {
    let user = match auth::require_user(&db, &headers).await {
        Ok(user) => user,
        Err(resp) => return resp,
    };

    if !user_exists(&db, user.id).await {
        return (
            StatusCode::UNAUTHORIZED,
            Json(json!({"error": "User does not exist"})),
        )
            .into_response();
    }

    let name = body.name.trim();
    if name.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "项目名称不能为空"})),
        )
            .into_response();
    }
    if project_name_exists(&db, user.id, name, None).await {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "项目名称已存在"})),
        )
            .into_response();
    }

    let max_order: i64 =
        sqlx::query_scalar("SELECT COALESCE(MAX(sort_order), -1) FROM projects WHERE user_id = ?")
            .bind(user.id)
            .fetch_one(&db.pool)
            .await
            .unwrap_or(-1);

    let description = body.description.unwrap_or_default();
    let background_image = body
        .background_image
        .as_ref()
        .map(|url| url.trim())
        .filter(|url| !url.is_empty())
        .map(|url| url.to_string());

    let project_id = match sqlx::query(
        "INSERT INTO projects (user_id, name, description, background_image, sort_order)
         VALUES (?, ?, ?, ?, ?)",
    )
    .bind(user.id)
    .bind(name)
    .bind(&description)
    .bind(background_image)
    .bind(max_order + 1)
    .execute(&db.pool)
    .await
    {
        Ok(result) => result.last_insert_rowid(),
        Err(e) => {
            tracing::error!("create_project failed: {}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "创建项目失败"})),
            )
                .into_response();
        }
    };

    let project: Project = match sqlx::query_as("SELECT * FROM projects WHERE id = ?")
        .bind(project_id)
        .fetch_one(&db.pool)
        .await
    {
        Ok(p) => p,
        Err(e) => {
            tracing::error!("create_project fetch failed: {}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "读取项目失败"})),
            )
                .into_response();
        }
    };

    (
        StatusCode::CREATED,
        Json(json!({"project": ProjectResponse::from(project)})),
    )
        .into_response()
}

#[derive(Deserialize)]
pub struct UpdateProjectRequest {
    pub name: Option<String>,
    pub description: Option<String>,
    pub background_image: Option<String>,
}

pub async fn update_project(
    Extension(db): Extension<Arc<Database>>,
    headers: HeaderMap,
    Path(id): Path<i64>,
    Json(body): Json<UpdateProjectRequest>,
) -> impl IntoResponse {
    let user = match auth::require_user(&db, &headers).await {
        Ok(user) => user,
        Err(resp) => return resp,
    };

    let existing: Option<Project> =
        sqlx::query_as("SELECT * FROM projects WHERE id = ? AND user_id = ?")
            .bind(id)
            .bind(user.id)
            .fetch_optional(&db.pool)
            .await
            .unwrap_or(None);

    let existing = match existing {
        Some(p) => p,
        None => {
            return (StatusCode::NOT_FOUND, Json(json!({"error": "项目不存在"}))).into_response();
        }
    };

    let name = body
        .name
        .as_ref()
        .map(|v| v.trim().to_string())
        .unwrap_or(existing.name);
    if name.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "项目名称不能为空"})),
        )
            .into_response();
    }
    if project_name_exists(&db, user.id, &name, Some(id)).await {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "项目名称已存在"})),
        )
            .into_response();
    }

    let description = body.description.unwrap_or(existing.description);
    let background_image = if let Some(v) = body.background_image {
        let trimmed = v.trim().to_string();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed)
        }
    } else {
        existing.background_image
    };

    if let Err(e) = sqlx::query(
        "UPDATE projects
         SET name = ?, description = ?, background_image = ?, updated_at = datetime('now')
         WHERE id = ? AND user_id = ?",
    )
    .bind(&name)
    .bind(&description)
    .bind(&background_image)
    .bind(id)
    .bind(user.id)
    .execute(&db.pool)
    .await
    {
        tracing::error!("update_project failed: {}", e);
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "更新项目失败"})),
        )
            .into_response();
    }

    let project: Project = match sqlx::query_as("SELECT * FROM projects WHERE id = ?")
        .bind(id)
        .fetch_one(&db.pool)
        .await
    {
        Ok(p) => p,
        Err(e) => {
            tracing::error!("update_project fetch failed: {}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "读取项目失败"})),
            )
                .into_response();
        }
    };

    Json(json!({"project": ProjectResponse::from(project)})).into_response()
}

pub async fn delete_project(
    Extension(db): Extension<Arc<Database>>,
    headers: HeaderMap,
    Path(id): Path<i64>,
) -> impl IntoResponse {
    let user = match auth::require_user(&db, &headers).await {
        Ok(user) => user,
        Err(resp) => return resp,
    };

    let project_exists: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM projects WHERE id = ? AND user_id = ?)")
            .bind(id)
            .bind(user.id)
            .fetch_one(&db.pool)
            .await
            .unwrap_or(false);

    if !project_exists {
        return (StatusCode::NOT_FOUND, Json(json!({"error": "项目不存在"}))).into_response();
    }

    let project_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM projects WHERE user_id = ?")
        .bind(user.id)
        .fetch_one(&db.pool)
        .await
        .unwrap_or(0);

    if project_count <= 1 {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "至少保留一个项目"})),
        )
            .into_response();
    }

    let mut tx = match db.pool.begin().await {
        Ok(tx) => tx,
        Err(e) => {
            tracing::error!("delete_project begin tx failed: {}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "删除项目失败"})),
            )
                .into_response();
        }
    };

    let fallback_project_id: i64 = match sqlx::query_scalar(
        "SELECT id FROM projects
         WHERE user_id = ? AND id != ?
         ORDER BY sort_order ASC, created_at ASC
         LIMIT 1",
    )
    .bind(user.id)
    .bind(id)
    .fetch_one(&mut *tx)
    .await
    {
        Ok(v) => v,
        Err(e) => {
            tracing::error!("delete_project fallback query failed: {}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "删除项目失败"})),
            )
                .into_response();
        }
    };

    if let Err(e) = sqlx::query(
        "UPDATE doc_nodes SET project_id = ?, updated_at = datetime('now')
         WHERE user_id = ? AND project_id = ?",
    )
    .bind(fallback_project_id)
    .bind(user.id)
    .bind(id)
    .execute(&mut *tx)
    .await
    {
        tracing::error!("delete_project move docs failed: {}", e);
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "删除项目失败"})),
        )
            .into_response();
    }

    if let Err(e) = sqlx::query("DELETE FROM projects WHERE id = ? AND user_id = ?")
        .bind(id)
        .bind(user.id)
        .execute(&mut *tx)
        .await
    {
        tracing::error!("delete_project delete failed: {}", e);
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "删除项目失败"})),
        )
            .into_response();
    }

    if let Err(e) = tx.commit().await {
        tracing::error!("delete_project commit failed: {}", e);
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "删除项目失败"})),
        )
            .into_response();
    }

    Json(json!({
        "message": "删除成功",
        "fallback_project_id": fallback_project_id
    }))
    .into_response()
}
