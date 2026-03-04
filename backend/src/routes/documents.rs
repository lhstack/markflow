use axum::{
    extract::{Extension, Path},
    http::{StatusCode, HeaderMap},
    Json,
    response::IntoResponse,
};
use serde::Deserialize;
use serde_json::json;
use std::sync::Arc;
use uuid::Uuid;

use crate::{db::Database, models::{DocNode, DocNodeResponse}, auth};

async fn user_exists(db: &Database, user_id: &str) -> bool {
    sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM users WHERE id = ?)")
        .bind(user_id)
        .fetch_one(&db.pool)
        .await
        .unwrap_or(false)
}

fn build_tree(nodes: Vec<DocNode>, parent_id: Option<&str>) -> Vec<DocNodeResponse> {
    let mut result = vec![];
    for node in nodes.iter() {
        let matches = match parent_id {
            Some(pid) => node.parent_id.as_deref() == Some(pid),
            None => node.parent_id.is_none(),
        };
        if matches {
            let mut resp = DocNodeResponse::from_node(node.clone());
            resp.children = build_tree(nodes.clone(), Some(&node.id));
            // Sort children: dirs first, then by sort_order
            resp.children.sort_by(|a, b| {
                if a.node_type != b.node_type {
                    if a.node_type == "dir" { std::cmp::Ordering::Less } else { std::cmp::Ordering::Greater }
                } else {
                    a.sort_order.cmp(&b.sort_order)
                }
            });
            result.push(resp);
        }
    }
    result.sort_by(|a, b| {
        if a.node_type != b.node_type {
            if a.node_type == "dir" { std::cmp::Ordering::Less } else { std::cmp::Ordering::Greater }
        } else {
            a.sort_order.cmp(&b.sort_order)
        }
    });
    result
}

pub async fn list_tree(
    Extension(db): Extension<Arc<Database>>,
    headers: HeaderMap,
) -> impl IntoResponse {
    let claims = match auth::extract_user_id(&headers) {
        Some(c) => c,
        None => return (StatusCode::UNAUTHORIZED, Json(json!({"error": "Unauthorized"}))).into_response(),
    };
    if !user_exists(&db, &claims.sub).await {
        return (StatusCode::UNAUTHORIZED, Json(json!({"error": "User does not exist"}))).into_response();
    }

    let nodes: Vec<DocNode> = sqlx::query_as(
        "SELECT * FROM doc_nodes WHERE user_id = ? ORDER BY sort_order ASC, created_at ASC"
    )
    .bind(&claims.sub)
    .fetch_all(&db.pool)
    .await
    .unwrap_or_default();

    let tree = build_tree(nodes, None);
    Json(json!({"tree": tree})).into_response()
}

#[derive(Deserialize)]
pub struct CreateNodeRequest {
    pub parent_id: Option<String>,
    pub name: String,
    pub node_type: String,
    pub content: Option<String>,
}

pub async fn create_node(
    Extension(db): Extension<Arc<Database>>,
    headers: HeaderMap,
    Json(body): Json<CreateNodeRequest>,
) -> impl IntoResponse {
    let claims = match auth::extract_user_id(&headers) {
        Some(c) => c,
        None => return (StatusCode::UNAUTHORIZED, Json(json!({"error": "Unauthorized"}))).into_response(),
    };
    if !user_exists(&db, &claims.sub).await {
        return (StatusCode::UNAUTHORIZED, Json(json!({"error": "User does not exist"}))).into_response();
    }

    if body.node_type != "dir" && body.node_type != "doc" {
        return (StatusCode::BAD_REQUEST, Json(json!({"error": "节点类型无效"}))).into_response();
    }
    if body.name.trim().is_empty() {
        return (StatusCode::BAD_REQUEST, Json(json!({"error": "名称不能为空"}))).into_response();
    }

    // Verify parent belongs to user
    if let Some(ref pid) = body.parent_id {
        let parent_exists: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM doc_nodes WHERE id = ? AND user_id = ? AND node_type = 'dir')"
        )
        .bind(pid)
        .bind(&claims.sub)
        .fetch_one(&db.pool)
        .await
        .unwrap_or(false);
        
        if !parent_exists {
            return (StatusCode::BAD_REQUEST, Json(json!({"error": "父目录不存在"}))).into_response();
        }
    }

    // Get max sort_order
    let max_order: i64 = sqlx::query_scalar(
        "SELECT COALESCE(MAX(sort_order), -1) FROM doc_nodes WHERE user_id = ? AND parent_id IS ?"
    )
    .bind(&claims.sub)
    .bind(&body.parent_id)
    .fetch_one(&db.pool)
    .await
    .unwrap_or(-1);

    let node_id = Uuid::new_v4().to_string();

    let insert_result = sqlx::query(
        "INSERT INTO doc_nodes (id, user_id, parent_id, name, node_type, content, sort_order) VALUES (?, ?, ?, ?, ?, ?, ?)"
    )
    .bind(&node_id)
    .bind(&claims.sub)
    .bind(&body.parent_id)
    .bind(&body.name)
    .bind(&body.node_type)
    .bind(&body.content)
    .bind(max_order + 1)
    .execute(&db.pool)
    .await;

    if let Err(e) = insert_result {
        if e.to_string().contains("FOREIGN KEY constraint failed") {
            return (StatusCode::BAD_REQUEST, Json(json!({"error": "Invalid parent or user reference"}))).into_response();
        }
        tracing::error!("create_node insert failed: {}", e);
        return (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": "Failed to create node"}))).into_response();
    }

    let node: DocNode = match sqlx::query_as("SELECT * FROM doc_nodes WHERE id = ?")
        .bind(&node_id)
        .fetch_one(&db.pool)
        .await
    {
        Ok(node) => node,
        Err(e) => {
            tracing::error!("create_node fetch failed: {}", e);
            return (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": "Failed to fetch created node"}))).into_response();
        }
    };

    let mut resp = DocNodeResponse::from_node(node);
    resp.children = vec![];

    (StatusCode::CREATED, Json(json!({"node": resp}))).into_response()
}

pub async fn get_node(
    Extension(db): Extension<Arc<Database>>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> impl IntoResponse {
    let claims = match auth::extract_user_id(&headers) {
        Some(c) => c,
        None => return (StatusCode::UNAUTHORIZED, Json(json!({"error": "Unauthorized"}))).into_response(),
    };

    let node: Option<DocNode> = sqlx::query_as(
        "SELECT * FROM doc_nodes WHERE id = ? AND user_id = ?"
    )
    .bind(&id)
    .bind(&claims.sub)
    .fetch_optional(&db.pool)
    .await
    .unwrap();

    match node {
        Some(n) => {
            if n.node_type == "dir" {
                // Return dir stats
                let doc_count: i64 = sqlx::query_scalar(
                    "SELECT COUNT(*) FROM doc_nodes WHERE user_id = ? AND node_type = 'doc' AND parent_id = ?"
                )
                .bind(&claims.sub)
                .bind(&id)
                .fetch_one(&db.pool)
                .await
                .unwrap_or(0);
                
                let dir_count: i64 = sqlx::query_scalar(
                    "SELECT COUNT(*) FROM doc_nodes WHERE user_id = ? AND node_type = 'dir' AND parent_id = ?"
                )
                .bind(&claims.sub)
                .bind(&id)
                .fetch_one(&db.pool)
                .await
                .unwrap_or(0);

                let children: Vec<DocNode> = sqlx::query_as(
                    "SELECT * FROM doc_nodes WHERE user_id = ? AND parent_id = ? ORDER BY sort_order ASC, created_at ASC"
                )
                .bind(&claims.sub)
                .bind(&id)
                .fetch_all(&db.pool)
                .await
                .unwrap_or_default();

                let mut child_responses: Vec<DocNodeResponse> = children
                    .into_iter()
                    .map(DocNodeResponse::from_node)
                    .collect();

                child_responses.sort_by(|a, b| {
                    if a.node_type != b.node_type {
                        if a.node_type == "dir" { std::cmp::Ordering::Less } else { std::cmp::Ordering::Greater }
                    } else {
                        a.sort_order.cmp(&b.sort_order)
                    }
                });

                let mut node_resp = DocNodeResponse::from_node(n);
                node_resp.children = child_responses;

                Json(json!({
                    "node": node_resp,
                    "stats": {
                        "doc_count": doc_count,
                        "dir_count": dir_count
                    }
                })).into_response()
            } else {
                Json(json!({"node": DocNodeResponse::from_node(n)})).into_response()
            }
        }
        None => (StatusCode::NOT_FOUND, Json(json!({"error": "文档不存在"}))).into_response(),
    }
}

#[derive(Deserialize)]
pub struct UpdateNodeRequest {
    pub name: Option<String>,
    pub content: Option<String>,
}

pub async fn update_node(
    Extension(db): Extension<Arc<Database>>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(body): Json<UpdateNodeRequest>,
) -> impl IntoResponse {
    let claims = match auth::extract_user_id(&headers) {
        Some(c) => c,
        None => return (StatusCode::UNAUTHORIZED, Json(json!({"error": "Unauthorized"}))).into_response(),
    };

    let node: Option<DocNode> = sqlx::query_as(
        "SELECT * FROM doc_nodes WHERE id = ? AND user_id = ?"
    )
    .bind(&id)
    .bind(&claims.sub)
    .fetch_optional(&db.pool)
    .await
    .unwrap();

    let node = match node {
        Some(n) => n,
        None => return (StatusCode::NOT_FOUND, Json(json!({"error": "文档不存在"}))).into_response(),
    };

    let new_name = body.name.unwrap_or(node.name);
    let new_content = body.content.or(node.content);

    sqlx::query(
        "UPDATE doc_nodes SET name = ?, content = ?, updated_at = datetime('now') WHERE id = ?"
    )
    .bind(&new_name)
    .bind(&new_content)
    .bind(&id)
    .execute(&db.pool)
    .await
    .unwrap();

    let updated: DocNode = sqlx::query_as("SELECT * FROM doc_nodes WHERE id = ?")
        .bind(&id)
        .fetch_one(&db.pool)
        .await
        .unwrap();

    Json(json!({"node": DocNodeResponse::from_node(updated)})).into_response()
}

pub async fn delete_node(
    Extension(db): Extension<Arc<Database>>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> impl IntoResponse {
    let claims = match auth::extract_user_id(&headers) {
        Some(c) => c,
        None => return (StatusCode::UNAUTHORIZED, Json(json!({"error": "Unauthorized"}))).into_response(),
    };

    let result = sqlx::query(
        "DELETE FROM doc_nodes WHERE id = ? AND user_id = ?"
    )
    .bind(&id)
    .bind(&claims.sub)
    .execute(&db.pool)
    .await
    .unwrap();

    if result.rows_affected() == 0 {
        return (StatusCode::NOT_FOUND, Json(json!({"error": "文档不存在"}))).into_response();
    }

    Json(json!({"message": "删除成功"})).into_response()
}

#[derive(Deserialize)]
pub struct MoveNodeRequest {
    pub parent_id: Option<String>,
    pub sort_order: Option<i64>,
}

pub async fn move_node(
    Extension(db): Extension<Arc<Database>>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(body): Json<MoveNodeRequest>,
) -> impl IntoResponse {
    let claims = match auth::extract_user_id(&headers) {
        Some(c) => c,
        None => return (StatusCode::UNAUTHORIZED, Json(json!({"error": "Unauthorized"}))).into_response(),
    };
    if !user_exists(&db, &claims.sub).await {
        return (StatusCode::UNAUTHORIZED, Json(json!({"error": "User does not exist"}))).into_response();
    }

    // Prevent moving to a child of itself
    if let Some(ref new_parent) = body.parent_id {
        if new_parent == &id {
            return (StatusCode::BAD_REQUEST, Json(json!({"error": "Cannot move a node under itself"}))).into_response();
        }

        let parent_exists: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM doc_nodes WHERE id = ? AND user_id = ? AND node_type = 'dir')"
        )
        .bind(new_parent)
        .bind(&claims.sub)
        .fetch_one(&db.pool)
        .await
        .unwrap_or(false);

        if !parent_exists {
            return (StatusCode::BAD_REQUEST, Json(json!({"error": "Target parent does not exist"}))).into_response();
        }

        // Check the new parent isn't a descendant
        let is_descendant: bool = sqlx::query_scalar(
            "WITH RECURSIVE desc(id) AS (
                SELECT id FROM doc_nodes WHERE parent_id = ?
                UNION ALL
                SELECT n.id FROM doc_nodes n JOIN desc d ON n.parent_id = d.id
            )
            SELECT EXISTS(SELECT 1 FROM desc WHERE id = ?)"
        )
        .bind(&id)
        .bind(new_parent)
        .fetch_one(&db.pool)
        .await
        .unwrap_or(false);
        
        if is_descendant {
            return (StatusCode::BAD_REQUEST, Json(json!({"error": "不能移动到子目录"}))).into_response();
        }
    }

    let sort_order = body.sort_order.unwrap_or(0);

    let update_result = sqlx::query(
        "UPDATE doc_nodes SET parent_id = ?, sort_order = ?, updated_at = datetime('now') WHERE id = ? AND user_id = ?"
    )
    .bind(&body.parent_id)
    .bind(sort_order)
    .bind(&id)
    .bind(&claims.sub)
    .execute(&db.pool)
    .await;

    match update_result {
        Ok(result) => {
            if result.rows_affected() == 0 {
                return (StatusCode::NOT_FOUND, Json(json!({"error": "Node not found"}))).into_response();
            }
        }
        Err(e) => {
            if e.to_string().contains("FOREIGN KEY constraint failed") {
                return (StatusCode::BAD_REQUEST, Json(json!({"error": "Invalid parent reference"}))).into_response();
            }
            tracing::error!("move_node update failed: {}", e);
            return (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": "Failed to move node"}))).into_response();
        }
    }

    Json(json!({"message": "移动成功"})).into_response()
}
