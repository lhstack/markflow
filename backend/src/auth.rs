use anyhow::{Result, anyhow};
use axum::http::HeaderMap;
use jsonwebtoken::{encode, decode, Header, Algorithm, Validation, EncodingKey, DecodingKey};
use chrono::{Utc, Duration};

use crate::models::Claims;

fn jwt_secret() -> String {
    std::env::var("JWT_SECRET").unwrap_or_else(|_| "markflow_dev_secret_change_in_production".to_string())
}

const TOKEN_EXPIRE_HOURS: i64 = 24 * 7; // 7 days

pub fn create_token(user_id: &str, username: &str) -> Result<String> {
    let now = Utc::now();
    let exp = (now + Duration::hours(TOKEN_EXPIRE_HOURS)).timestamp() as usize;
    let iat = now.timestamp() as usize;
    let claims = Claims { sub: user_id.to_string(), username: username.to_string(), exp, iat };
    let secret = jwt_secret();
    let token = encode(&Header::default(), &claims, &EncodingKey::from_secret(secret.as_bytes()))?;
    Ok(token)
}

pub fn verify_token(token: &str) -> Result<Claims> {
    let secret = jwt_secret();
    let data = decode::<Claims>(
        token,
        &DecodingKey::from_secret(secret.as_bytes()),
        &Validation::new(Algorithm::HS256),
    ).map_err(|e| anyhow!("Invalid token: {}", e))?;
    Ok(data.claims)
}

pub fn extract_user_id(headers: &HeaderMap) -> Option<Claims> {
    let auth = headers.get("Authorization")?.to_str().ok()?;
    let token = auth.strip_prefix("Bearer ")?;
    verify_token(token).ok()
}
