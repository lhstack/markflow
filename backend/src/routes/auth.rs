use axum::{
    extract::Extension,
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    Json,
};
use base64::{engine::general_purpose, Engine as _};
use serde::Deserialize;
use serde_json::json;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::Mutex;
use uuid::Uuid;

use crate::{
    auth,
    db::Database,
    models::{User, UserInfo},
};

lazy_static::lazy_static! {
    static ref CAPTCHA_STORE: Mutex<HashMap<String, (String, u64)>> = Mutex::new(HashMap::new());
    static ref TWO_FA_LOGIN_STORE: Mutex<HashMap<String, (i64, u64)>> = Mutex::new(HashMap::new());
}

fn current_ts() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs()
}

async fn create_twofa_challenge(user_id: i64) -> String {
    let challenge_id = Uuid::new_v4().to_string();
    let now = current_ts();
    let mut store = TWO_FA_LOGIN_STORE.lock().await;
    store.retain(|_, (_, ts)| now - *ts < 300);
    store.insert(challenge_id.clone(), (user_id, now));
    challenge_id
}

async fn get_twofa_challenge_user_id(challenge_id: &str) -> Option<i64> {
    let now = current_ts();
    let mut store = TWO_FA_LOGIN_STORE.lock().await;
    store.retain(|_, (_, ts)| now - *ts < 300);
    store.get(challenge_id).map(|(user_id, _)| *user_id)
}

async fn clear_twofa_challenge(challenge_id: &str) {
    let mut store = TWO_FA_LOGIN_STORE.lock().await;
    store.remove(challenge_id);
}

pub async fn get_captcha() -> Json<serde_json::Value> {
    let (answer, question, captcha_id) = {
        use rand::Rng;
        let mut rng = rand::thread_rng();
        let a: u32 = rng.gen_range(0..10);
        let b: u32 = rng.gen_range(0..10);
        let answer = (a + b).to_string();
        let question = format!("{} + {} = ?", a, b);
        let captcha_id = Uuid::new_v4().to_string();
        (answer, question, captcha_id)
    };

    let mut store = CAPTCHA_STORE.lock().await;
    let now = current_ts();
    store.retain(|_, (_, ts)| now - *ts < 300);
    store.insert(captcha_id.clone(), (answer, now));

    let svg = format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="120" height="40"><rect width="120" height="40" rx="6" fill="#1c2128"/><line x1="0" y1="15" x2="120" y2="20" stroke="#444" stroke-width="0.8"/><text x="60" y="27" font-family="monospace" font-size="17" fill="#58a6ff" font-weight="bold" text-anchor="middle">{}</text></svg>"##,
        question
    );
    let encoded = general_purpose::STANDARD.encode(svg.as_bytes());
    Json(json!({
        "captcha_id": captcha_id,
        "image": format!("data:image/svg+xml;base64,{}", encoded)
    }))
}

async fn verify_captcha(id: &str, answer: &str) -> bool {
    let mut store = CAPTCHA_STORE.lock().await;
    if let Some((expected, ts)) = store.remove(id) {
        return current_ts() - ts < 300 && expected == answer.trim();
    }
    false
}

fn verify_totp(secret_encoded: &str, code: &str) -> bool {
    use totp_rs::{Algorithm, Secret, TOTP};

    let bytes = match Secret::Encoded(secret_encoded.to_string()).to_bytes() {
        Ok(b) => b,
        Err(_) => return false,
    };

    match TOTP::new(
        Algorithm::SHA1,
        6,
        1,
        30,
        bytes,
        None,
        "markflow".to_string(),
    ) {
        Ok(totp) => totp.check_current(code).unwrap_or(false),
        Err(_) => false,
    }
}

#[derive(Deserialize)]
pub struct RegisterRequest {
    pub username: String,
    pub password: String,
    pub avatar: Option<String>,
}

pub async fn register(
    Extension(db): Extension<Arc<Database>>,
    Json(body): Json<RegisterRequest>,
) -> impl IntoResponse {
    let settings = match db.get_system_settings().await {
        Ok(settings) => settings,
        Err(err) => {
            tracing::error!("load system settings for register failed: {}", err);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "Failed to load system settings"})),
            )
                .into_response();
        }
    };

    if settings.registration_enabled != 1 {
        return (
            StatusCode::FORBIDDEN,
            Json(json!({"error": "Registration is disabled"})),
        )
            .into_response();
    }

    let username = body.username.trim().to_string();
    if username.len() < 3 || username.len() > 32 {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "Username length must be 3-32"})),
        )
            .into_response();
    }
    if body.password.len() < 6 {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "Password length must be at least 6"})),
        )
            .into_response();
    }

    let exists: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM users WHERE username = ?)")
        .bind(&username)
        .fetch_one(&db.pool)
        .await
        .unwrap_or(false);

    if exists {
        return (
            StatusCode::CONFLICT,
            Json(json!({"error": "Username already exists"})),
        )
            .into_response();
    }

    let password_hash = bcrypt::hash(&body.password, 10).unwrap();
    let user_id =
        sqlx::query("INSERT INTO users (username, password_hash, avatar) VALUES (?, ?, ?)")
            .bind(&username)
            .bind(&password_hash)
            .bind(&body.avatar)
            .execute(&db.pool)
            .await
            .unwrap()
            .last_insert_rowid();

    let token = auth::create_token(user_id, &username).unwrap();
    (
        StatusCode::CREATED,
        Json(json!({
            "token": token,
            "user": {
                "id": user_id,
                "username": username,
                "avatar": body.avatar,
                "totp_enabled": false
            }
        })),
    )
        .into_response()
}

#[derive(Deserialize)]
pub struct LoginRequest {
    pub username: String,
    pub password: String,
    pub captcha_id: String,
    pub captcha_answer: String,
    pub totp_code: Option<String>,
}

pub async fn login(
    Extension(db): Extension<Arc<Database>>,
    Json(body): Json<LoginRequest>,
) -> impl IntoResponse {
    if !verify_captcha(&body.captcha_id, &body.captcha_answer).await {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "Invalid captcha"})),
        )
            .into_response();
    }

    let user: Option<User> = sqlx::query_as("SELECT * FROM users WHERE username = ?")
        .bind(&body.username)
        .fetch_optional(&db.pool)
        .await
        .unwrap();

    let user = match user {
        Some(u) => u,
        None => {
            return (
                StatusCode::UNAUTHORIZED,
                Json(json!({"error": "Invalid username or password"})),
            )
                .into_response()
        }
    };

    if user.is_active != 1 {
        return (
            StatusCode::FORBIDDEN,
            Json(json!({"error": "User is disabled"})),
        )
            .into_response();
    }

    if !bcrypt::verify(&body.password, &user.password_hash).unwrap_or(false) {
        return (
            StatusCode::UNAUTHORIZED,
            Json(json!({"error": "Invalid username or password"})),
        )
            .into_response();
    }

    if user.totp_enabled == 1 {
        let secret = user.totp_secret.clone().unwrap_or_default();
        let code = match body.totp_code.as_deref() {
            Some(c) if !c.is_empty() => c.to_string(),
            _ => {
                let challenge_id = create_twofa_challenge(user.id).await;
                return Json(json!({
                    "require_2fa": true,
                    "challenge_id": challenge_id,
                    "message": "2FA required"
                }))
                .into_response();
            }
        };

        if secret.is_empty() || !verify_totp(&secret, &code) {
            return (
                StatusCode::UNAUTHORIZED,
                Json(json!({"error": "Invalid 2FA code"})),
            )
                .into_response();
        }
    }

    let token = auth::create_token(user.id, &user.username).unwrap();
    let user_info = UserInfo::from(user);
    Json(json!({"token": token, "user": user_info})).into_response()
}

#[derive(Deserialize)]
pub struct Login2FARequest {
    pub challenge_id: String,
    pub totp_code: String,
}

pub async fn login_2fa(
    Extension(db): Extension<Arc<Database>>,
    Json(body): Json<Login2FARequest>,
) -> impl IntoResponse {
    let code = body.totp_code.trim();
    if code.len() != 6 {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "Invalid 2FA code"})),
        )
            .into_response();
    }

    let user_id = match get_twofa_challenge_user_id(&body.challenge_id).await {
        Some(id) => id,
        None => {
            return (
                StatusCode::UNAUTHORIZED,
                Json(json!({"error": "2FA challenge expired"})),
            )
                .into_response()
        }
    };

    let user: Option<User> = sqlx::query_as("SELECT * FROM users WHERE id = ?")
        .bind(&user_id)
        .fetch_optional(&db.pool)
        .await
        .unwrap();

    let user = match user {
        Some(u) => u,
        None => {
            return (
                StatusCode::UNAUTHORIZED,
                Json(json!({"error": "User not found"})),
            )
                .into_response()
        }
    };

    if user.is_active != 1 {
        clear_twofa_challenge(&body.challenge_id).await;
        return (
            StatusCode::FORBIDDEN,
            Json(json!({"error": "User is disabled"})),
        )
            .into_response();
    }

    if user.totp_enabled != 1 {
        clear_twofa_challenge(&body.challenge_id).await;
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "2FA is not enabled"})),
        )
            .into_response();
    }

    let secret = user.totp_secret.clone().unwrap_or_default();
    if secret.is_empty() || !verify_totp(&secret, code) {
        return (
            StatusCode::UNAUTHORIZED,
            Json(json!({"error": "Invalid 2FA code"})),
        )
            .into_response();
    }

    clear_twofa_challenge(&body.challenge_id).await;

    let token = auth::create_token(user.id, &user.username).unwrap();
    let user_info = UserInfo::from(user);
    Json(json!({"token": token, "user": user_info})).into_response()
}

pub async fn me(Extension(db): Extension<Arc<Database>>, headers: HeaderMap) -> impl IntoResponse {
    match auth::require_user(&db, &headers).await {
        Ok(user) => Json(json!({"user": UserInfo::from(user)})).into_response(),
        Err(resp) => resp,
    }
}

#[derive(Deserialize)]
pub struct UpdateProfileRequest {
    pub avatar: Option<String>,
}

pub async fn update_profile(
    Extension(db): Extension<Arc<Database>>,
    headers: HeaderMap,
    Json(body): Json<UpdateProfileRequest>,
) -> impl IntoResponse {
    let user = match auth::require_user(&db, &headers).await {
        Ok(user) => user,
        Err(resp) => return resp,
    };

    sqlx::query("UPDATE users SET avatar = ?, updated_at = datetime('now') WHERE id = ?")
        .bind(&body.avatar)
        .bind(user.id)
        .execute(&db.pool)
        .await
        .unwrap();

    let user: User = sqlx::query_as("SELECT * FROM users WHERE id = ?")
        .bind(user.id)
        .fetch_one(&db.pool)
        .await
        .unwrap();

    Json(json!({"user": UserInfo::from(user)})).into_response()
}

#[derive(Deserialize)]
pub struct ChangePasswordRequest {
    pub old_password: String,
    pub new_password: String,
}

pub async fn change_password(
    Extension(db): Extension<Arc<Database>>,
    headers: HeaderMap,
    Json(body): Json<ChangePasswordRequest>,
) -> impl IntoResponse {
    let user = match auth::require_user(&db, &headers).await {
        Ok(user) => user,
        Err(resp) => return resp,
    };

    if !bcrypt::verify(&body.old_password, &user.password_hash).unwrap_or(false) {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "Current password is incorrect"})),
        )
            .into_response();
    }

    if body.new_password.len() < 6 {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "New password must be at least 6 characters"})),
        )
            .into_response();
    }

    let new_hash = bcrypt::hash(&body.new_password, 10).unwrap();
    sqlx::query("UPDATE users SET password_hash = ?, updated_at = datetime('now') WHERE id = ?")
        .bind(&new_hash)
        .bind(user.id)
        .execute(&db.pool)
        .await
        .unwrap();

    Json(json!({"message": "Password updated"})).into_response()
}

pub async fn setup_2fa(
    Extension(db): Extension<Arc<Database>>,
    headers: HeaderMap,
) -> impl IntoResponse {
    let user = match auth::require_user(&db, &headers).await {
        Ok(user) => user,
        Err(resp) => return resp,
    };

    use totp_rs::{Algorithm, Secret, TOTP};
    let secret = Secret::generate_secret();
    let secret_encoded = secret.to_encoded().to_string();
    let secret_bytes = match secret.to_bytes() {
        Ok(b) => b,
        Err(_) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "Failed to generate secret"})),
            )
                .into_response()
        }
    };

    let totp = match TOTP::new(
        Algorithm::SHA1,
        6,
        1,
        30,
        secret_bytes,
        Some("MarkFlow".to_string()),
        user.username.clone(),
    ) {
        Ok(t) => t,
        Err(_) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "Failed to initialize TOTP"})),
            )
                .into_response()
        }
    };

    let qr_b64 = totp.get_qr_base64().unwrap_or_default();
    let otpauth_url = totp.get_url();

    sqlx::query("UPDATE users SET totp_secret = ?, updated_at = datetime('now') WHERE id = ?")
        .bind(&secret_encoded)
        .bind(user.id)
        .execute(&db.pool)
        .await
        .unwrap();

    Json(json!({
        "secret": secret_encoded,
        "qr_code": format!("data:image/png;base64,{}", qr_b64),
        "otpauth_url": otpauth_url
    }))
    .into_response()
}

#[derive(Deserialize)]
pub struct CodeRequest {
    pub code: String,
}

pub async fn confirm_2fa(
    Extension(db): Extension<Arc<Database>>,
    headers: HeaderMap,
    Json(body): Json<CodeRequest>,
) -> impl IntoResponse {
    let user = match auth::require_user(&db, &headers).await {
        Ok(user) => user,
        Err(resp) => return resp,
    };

    let secret = user.totp_secret.clone().unwrap_or_default();
    if secret.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "Please setup 2FA first"})),
        )
            .into_response();
    }

    if !verify_totp(&secret, &body.code) {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "Invalid verification code"})),
        )
            .into_response();
    }

    sqlx::query("UPDATE users SET totp_enabled = 1, updated_at = datetime('now') WHERE id = ?")
        .bind(user.id)
        .execute(&db.pool)
        .await
        .unwrap();

    Json(json!({"message": "2FA enabled"})).into_response()
}

pub async fn disable_2fa(
    Extension(db): Extension<Arc<Database>>,
    headers: HeaderMap,
    Json(body): Json<CodeRequest>,
) -> impl IntoResponse {
    let user = match auth::require_user(&db, &headers).await {
        Ok(user) => user,
        Err(resp) => return resp,
    };

    let secret = user.totp_secret.clone().unwrap_or_default();
    if secret.is_empty() || user.totp_enabled == 0 {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "2FA is not enabled"})),
        )
            .into_response();
    }

    if !verify_totp(&secret, &body.code) {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "Invalid verification code"})),
        )
            .into_response();
    }

    sqlx::query(
        "UPDATE users SET totp_enabled = 0, totp_secret = NULL, updated_at = datetime('now') WHERE id = ?",
    )
        .bind(user.id)
        .execute(&db.pool)
        .await
        .unwrap();

    Json(json!({"message": "2FA disabled"})).into_response()
}
