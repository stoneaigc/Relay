use std::sync::Arc;

use axum::http::HeaderMap;

use crate::error::ApiError;
use crate::state::{AppState, UserState};
use crate::storage::hash_key;

pub struct AuthCtx {
    pub user: Arc<UserState>,
    /// 命中的 API Key id(用量明细按密钥维度归因)。
    pub key_id: uuid::Uuid,
    /// 该 Key 的模型白名单(None=不限;Some=仅允许列表内模型,门户 JWT 调用恒为 None)。
    pub key_scope: Option<Vec<String>>,
}

impl AuthCtx {
    /// 分级密钥管控:Key 被限制模型范围时,请求模型必须在白名单内。
    pub fn check_model(&self, model: &str) -> Result<(), ApiError> {
        if let Some(allowed) = &self.key_scope {
            if !allowed.iter().any(|m| m == model) {
                return Err(ApiError::ModelNotAllowed(model.to_string()));
            }
        }
        Ok(())
    }
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

    Ok(AuthCtx {
        user,
        key_id: entry.id,
        key_scope: entry.allowed_models.clone(),
    })
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
