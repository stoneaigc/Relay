//! 统一登录入口:一个地址登录,按账号身份签发对应端的令牌。
//! - 凭据匹配配置管理员(`RELAY_ADMIN__*`)→ 签发 admin 令牌(role=admin)
//! - 否则按门户用户校验(数据库)→ 签发 portal 令牌(role=user)
//!
//! 防爆破:独立滑动窗口(key 前缀 `uni:`),不与两端原有计数混用。
//! 前端约定:登录页按 role 把令牌写入对应应用的 localStorage 键
//! (`relay_admin_token` / `relay_portal_token`),两套 API 鉴权零改动。

use std::net::SocketAddr;
use std::sync::Arc;

use axum::{
    extract::{ConnectInfo, State},
    response::{Html, IntoResponse, Response},
    Json,
};
use serde::Deserialize;

use crate::error::ApiError;
use crate::state::AppState;

const LOGIN_PAGE: &str = include_str!("login.html");

/// GET /login(与 / 共用):统一登录页(单文件静态页,相对路径请求,天然兼容子路径反代)。
pub async fn login_page() -> Response {
    Html(LOGIN_PAGE).into_response()
}

#[derive(Deserialize)]
pub struct UniLogin {
    pub username: String,
    pub password: String,
}

/// POST /api/auth/login —— 统一登录。
/// 先比对配置管理员;未命中再走门户用户校验。失败统一 InvalidCredentials,
/// 不区分「账号不存在 / 密码错误 / 是否管理员」,避免枚举探测。
pub async fn login(
    State(state): State<Arc<AppState>>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    Json(body): Json<UniLogin>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let ip_key = format!("uni:{}", addr.ip());
    if let Some(secs) = state.login_guard.locked_for(&ip_key) {
        return Err(ApiError::TooManyAttempts(format!(
            "失败次数过多,请 {secs} 秒后再试"
        )));
    }
    let fail = |state: &AppState| state.login_guard.record_failure(&ip_key);
    let (secret, ttl) = {
        let cfg = state.config();
        (cfg.auth.jwt_secret.clone(), cfg.auth.session_ttl_secs)
    };
    let username = body.username.trim();

    // ① 配置管理员(default.toml / RELAY_ADMIN__* / 后台改密后的 settings.admin.*)
    let admin_hit = {
        let cfg = state.config();
        // 口令校验:后台改过密(argon2 哈希)优先;否则 env/config 明文。
        let pass_ok = match cfg.admin.password_hash.as_deref() {
            Some(hash) => crate::storage::verify_password(hash, &body.password).0,
            None => body.password == cfg.admin.password,
        };
        username == cfg.admin.username && pass_ok
    };
    if admin_hit {
        state.login_guard.clear(&ip_key);
        let token = crate::jwt::issue(&secret, username, "admin", ttl)?;
        return Ok(Json(serde_json::json!({ "role": "admin", "token": token })));
    }

    // ② 门户用户(数据库)
    let (id, pwhash) = match crate::storage::find_user_by_username(&state.db, username)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?
    {
        Some(v) => v,
        None => {
            fail(&state);
            return Err(ApiError::InvalidCredentials);
        }
    };
    let Some(stored) = pwhash.as_deref() else {
        fail(&state);
        return Err(ApiError::InvalidCredentials);
    };
    let (ok, legacy) = crate::storage::verify_password(stored, &body.password);
    if !ok {
        fail(&state);
        return Err(ApiError::InvalidCredentials);
    }
    // 旧版无盐 sha256 哈希登录成功后透明升级为 argon2(失败仅记日志,不阻断登录)。
    if legacy {
        let new_hash = crate::storage::hash_password(&body.password);
        if let Err(e) = crate::storage::upgrade_password_hash(&state.db, id, &new_hash).await {
            tracing::warn!("密码哈希升级失败(user {id}): {e}");
        }
    }
    if let Some(u) = state.user(&id) {
        if !u.is_active() {
            return Err(ApiError::AccountDisabled);
        }
    }
    state.login_guard.clear(&ip_key);
    let token = crate::jwt::issue(&secret, &id.to_string(), "portal", ttl)?;
    Ok(Json(serde_json::json!({ "role": "user", "token": token })))
}
