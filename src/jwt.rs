use std::time::{SystemTime, UNIX_EPOCH};

use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};

use crate::error::ApiError;

/// 会话令牌声明。`kind` 区分门户面 / 管理面,二者不可混用。
#[derive(Debug, Serialize, Deserialize)]
pub struct Claims {
    pub sub: String,  // portal: user_id;admin: username
    pub kind: String, // "portal" | "admin"
    pub exp: usize,
}

fn now() -> usize {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs() as usize
}

pub fn issue(secret: &str, sub: &str, kind: &str, ttl_secs: u64) -> Result<String, ApiError> {
    let claims = Claims {
        sub: sub.to_string(),
        kind: kind.to_string(),
        exp: now() + ttl_secs as usize,
    };
    encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(secret.as_bytes()),
    )
    .map_err(|e| ApiError::Internal(format!("jwt encode: {e}")))
}

pub fn verify(secret: &str, token: &str, expected_kind: &str) -> Result<Claims, ApiError> {
    let data = decode::<Claims>(
        token,
        &DecodingKey::from_secret(secret.as_bytes()),
        &Validation::default(),
    )
    .map_err(|_| ApiError::Unauthorized)?;
    if data.claims.kind != expected_kind {
        return Err(ApiError::Unauthorized);
    }
    Ok(data.claims)
}

/// 从 `Authorization: Bearer <jwt>` 取并校验。
pub fn from_headers(
    secret: &str,
    headers: &axum::http::HeaderMap,
    expected_kind: &str,
) -> Result<Claims, ApiError> {
    let token = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.strip_prefix("Bearer ").or_else(|| s.strip_prefix("bearer ")))
        .ok_or(ApiError::Unauthorized)?;
    verify(secret, token.trim(), expected_kind)
}
