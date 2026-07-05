//! 供应商 API Key 的对称加解密。密钥来自环境变量，与其它敏感字段共用同一套 secret。

use aes_gcm::{
    aead::{Aead, KeyInit},
    Aes256Gcm, Nonce,
};
use base64::{engine::general_purpose, Engine as _};
use sha2::{Digest, Sha256};

fn provider_secret() -> String {
    std::env::var("SHARE_PASSWORD_SECRET")
        .or_else(|_| std::env::var("JWT_SECRET"))
        .unwrap_or_else(|_| "markflow_dev_secret_change_in_production".to_string())
}

fn provider_cipher() -> Aes256Gcm {
    let digest = Sha256::digest(provider_secret().as_bytes());
    Aes256Gcm::new_from_slice(&digest).expect("agent provider key length should be valid")
}

pub(super) fn encrypt_api_key(api_key: &str) -> anyhow::Result<String> {
    let cipher = provider_cipher();
    let nonce_bytes: [u8; 12] = rand::random();
    let nonce = Nonce::from_slice(&nonce_bytes);
    let ciphertext = cipher
        .encrypt(nonce, api_key.as_bytes())
        .map_err(|_| anyhow::anyhow!("agent provider api key encryption failed"))?;

    let mut payload = Vec::with_capacity(nonce_bytes.len() + ciphertext.len());
    payload.extend_from_slice(&nonce_bytes);
    payload.extend_from_slice(&ciphertext);
    Ok(general_purpose::STANDARD.encode(payload))
}

pub(super) fn decrypt_api_key(ciphertext: &str) -> anyhow::Result<String> {
    let decoded = general_purpose::STANDARD.decode(ciphertext)?;
    if decoded.len() < 13 {
        anyhow::bail!("invalid agent provider api key ciphertext");
    }
    let (nonce_bytes, body) = decoded.split_at(12);
    let cipher = provider_cipher();
    let plaintext = cipher
        .decrypt(Nonce::from_slice(nonce_bytes), body)
        .map_err(|_| anyhow::anyhow!("agent provider api key decryption failed"))?;
    Ok(String::from_utf8(plaintext)?)
}
