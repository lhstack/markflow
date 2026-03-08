use anyhow::{anyhow, Result};
use axum::{
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use chrono::{Duration, Utc};
use jsonwebtoken::{decode, encode, Algorithm, DecodingKey, EncodingKey, Header, Validation};
use serde_json::json;

use crate::{
    db::Database,
    models::{Claims, User},
};

fn jwt_secret() -> String {
    std::env::var("JWT_SECRET")
        .unwrap_or_else(|_| "markflow_dev_secret_change_in_production".to_string())
}

const TOKEN_EXPIRE_HOURS: i64 = 24 * 7; // 7 days

pub fn create_token(user_id: i64, username: &str) -> Result<String> {
    let now = Utc::now();
    let exp = (now + Duration::hours(TOKEN_EXPIRE_HOURS)).timestamp() as usize;
    let iat = now.timestamp() as usize;
    let claims = Claims {
        sub: user_id,
        username: username.to_string(),
        exp,
        iat,
    };
    let secret = jwt_secret();
    let token = encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(secret.as_bytes()),
    )?;
    Ok(token)
}

pub fn verify_token(token: &str) -> Result<Claims> {
    let secret = jwt_secret();
    let data = decode::<Claims>(
        token,
        &DecodingKey::from_secret(secret.as_bytes()),
        &Validation::new(Algorithm::HS256),
    )
    .map_err(|e| anyhow!("Invalid token: {}", e))?;
    Ok(data.claims)
}

pub fn extract_user_id(headers: &HeaderMap) -> Option<Claims> {
    let auth = headers.get("Authorization")?.to_str().ok()?;
    let token = auth.strip_prefix("Bearer ")?;
    verify_token(token).ok()
}

pub async fn require_user(db: &Database, headers: &HeaderMap) -> Result<User, Response> {
    let claims = match extract_user_id(headers) {
        Some(claims) => claims,
        None => {
            return Err((
                StatusCode::UNAUTHORIZED,
                Json(json!({"error": "Unauthorized"})),
            )
                .into_response())
        }
    };

    let user = match sqlx::query_as::<_, User>("SELECT * FROM users WHERE id = ?")
        .bind(claims.sub)
        .fetch_optional(&db.pool)
        .await
    {
        Ok(Some(user)) => user,
        Ok(None) => {
            return Err((
                StatusCode::UNAUTHORIZED,
                Json(json!({"error": "User not found"})),
            )
                .into_response())
        }
        Err(err) => {
            tracing::error!("load authenticated user failed: {}", err);
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "Failed to load user"})),
            )
                .into_response());
        }
    };

    if user.is_active != 1 {
        return Err((
            StatusCode::FORBIDDEN,
            Json(json!({"error": "User is disabled"})),
        )
            .into_response());
    }

    Ok(user)
}

pub async fn require_super_admin(db: &Database, headers: &HeaderMap) -> Result<User, Response> {
    let user = require_user(db, headers).await?;
    if user.is_super_admin != 1 {
        return Err((StatusCode::FORBIDDEN, Json(json!({"error": "Forbidden"}))).into_response());
    }
    Ok(user)
}
