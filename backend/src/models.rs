use serde::{Deserialize, Serialize};
use sqlx::FromRow;

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct User {
    pub id: String,
    pub username: String,
    pub password_hash: String,
    pub avatar: Option<String>,
    pub totp_secret: Option<String>,
    pub totp_enabled: i64,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct DocNode {
    pub id: String,
    pub user_id: String,
    pub parent_id: Option<String>,
    pub name: String,
    pub node_type: String,
    pub content: Option<String>,
    pub sort_order: i64,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Share {
    pub id: String,
    pub user_id: String,
    pub doc_id: String,
    pub token: String,
    pub password_hash: Option<String>,
    pub expires_at: Option<String>,
    pub created_at: String,
}

// API response types
#[derive(Debug, Serialize, Deserialize)]
pub struct UserInfo {
    pub id: String,
    pub username: String,
    pub avatar: Option<String>,
    pub totp_enabled: bool,
}

impl From<User> for UserInfo {
    fn from(u: User) -> Self {
        Self {
            id: u.id,
            username: u.username,
            avatar: u.avatar,
            totp_enabled: u.totp_enabled == 1,
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct DocNodeResponse {
    pub id: String,
    pub parent_id: Option<String>,
    pub name: String,
    pub node_type: String,
    pub content: Option<String>,
    pub sort_order: i64,
    pub created_at: String,
    pub updated_at: String,
    pub children: Vec<DocNodeResponse>,
}

impl DocNodeResponse {
    pub fn from_node(node: DocNode) -> Self {
        Self {
            id: node.id,
            parent_id: node.parent_id,
            name: node.name,
            node_type: node.node_type,
            content: node.content,
            sort_order: node.sort_order,
            created_at: node.created_at,
            updated_at: node.updated_at,
            children: vec![],
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ShareResponse {
    pub id: String,
    pub doc_id: String,
    pub token: String,
    pub has_password: bool,
    pub expires_at: Option<String>,
    pub created_at: String,
}

impl From<Share> for ShareResponse {
    fn from(s: Share) -> Self {
        Self {
            id: s.id,
            doc_id: s.doc_id,
            token: s.token,
            has_password: s.password_hash.is_some(),
            expires_at: s.expires_at,
            created_at: s.created_at,
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Claims {
    pub sub: String, // user_id
    pub username: String,
    pub exp: usize,
    pub iat: usize,
}
