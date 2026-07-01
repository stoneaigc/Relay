use std::sync::Arc;

use axum::http::HeaderMap;

use crate::error::ApiError;
use crate::state::{AppState, UserState};
use crate::storage::hash_key;

pub struct AuthCtx {
    pub user: Arc<UserState>,
}

/// 从请求头取 API Key 并在内存中校验(不查库)。
/// 兼容 OpenAI 的 `Authorization: Bearer` 与 Anthropic 的 `x-api-key`。
pub fn authenticate(state: &AppState, headers: &HeaderMap) -> Result<AuthCtx, ApiError> {
    let raw = extract_key(headers).ok_or(ApiError::Unauthorized)?;
    let hash = hash_key(&raw);

    let entry = state.keys.get(&hash).ok_or(ApiError::InvalidKey)?;
    let user = state
        .user(&entry.user_id)
        .ok_or(ApiError::InvalidKey)?;

    if !user.is_active() {
        return Err(ApiError::AccountDisabled);
    }

    Ok(AuthCtx { user })
}

fn extract_key(headers: &HeaderMap) -> Option<String> {
    if let Some(v) = headers.get("authorization") {
        if let Ok(s) = v.to_str() {
            if let Some(token) = s.strip_prefix("Bearer ").or_else(|| s.strip_prefix("bearer ")) {
                return Some(token.trim().to_string());
            }
        }
    }
    if let Some(v) = headers.get("x-api-key") {
        if let Ok(s) = v.to_str() {
            return Some(s.trim().to_string());
        }
    }
    None
}
