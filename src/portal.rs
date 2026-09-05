use std::sync::Arc;

use axum::{extract::State, http::HeaderMap, Json};
use serde::Deserialize;
use serde_json::{json, Value};
use uuid::Uuid;

use crate::error::ApiError;
use crate::state::{AppState, KeyEntry, UserState};
use crate::admin::{PageQuery, normalize_page, total_pages};
use crate::{jwt, storage};

/// 校验门户会话,返回 user_id。
fn portal_user(state: &AppState, headers: &HeaderMap) -> Result<Uuid, ApiError> {
    let cfg = state.config();
    let claims = jwt::from_headers(&cfg.auth.jwt_secret, headers, "portal")?;
    Uuid::parse_str(&claims.sub).map_err(|_| ApiError::Unauthorized)
}

fn now_secs() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs() as i64
}

#[derive(Deserialize)]
pub struct SendCode {
    pub email: String,
}

/// POST /portal/auth/email/send_code —— 发送注册验证码到邮箱。
pub async fn send_email_code(
    State(state): State<Arc<AppState>>,
    Json(body): Json<SendCode>,
) -> Result<Json<Value>, ApiError> {
    let email = body.email.trim().to_lowercase();
    if !email.contains('@') || email.len() < 3 {
        return Err(ApiError::BadRequest("邮箱格式不正确".into()));
    }
    // 限频:60 秒内不重复发。缓存值为 "{发送时刻}:{code哈希}"。
    let code_key = format!("email_code:{email}");
    if let Some(v) = state.cache.get(&code_key).await.map_err(|e| ApiError::Internal(e.to_string()))? {
        let sent_at = v.split_once(':').and_then(|(t, _)| t.parse::<i64>().ok()).unwrap_or(0);
        if now_secs() - sent_at < 60 {
            return Err(ApiError::BadRequest("请求过于频繁,请稍后再试".into()));
        }
    }

    let code: String = {
        use rand::Rng;
        format!("{:06}", rand::thread_rng().gen_range(0..1_000_000))
    };
    let val = format!("{}:{}", now_secs(), storage::hash_password(&code));
    state
        .cache
        .set_ex(&code_key, &val, 300)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    let cfg_email = state.config().email.clone();
    if cfg_email.enabled() {
        crate::email::send_code(&cfg_email, &email, &code)
            .await
            .map_err(|e| ApiError::Internal(format!("邮件发送失败: {e}")))?;
        Ok(Json(json!({ "sent": true })))
    } else {
        // 未配置 SMTP:开发模式,把验证码打到日志(并在响应里返回,方便联调)。
        tracing::warn!("[DEV] 邮箱验证码 {email}: {code}");
        Ok(Json(json!({ "sent": false, "dev_code": code })))
    }
}

#[derive(Deserialize)]
pub struct Register {
    pub email: String,
    pub code: String,
    pub password: String,
}

/// POST /portal/auth/register —— 邮箱注册(需验证码),返回会话 JWT。
pub async fn register(
    State(state): State<Arc<AppState>>,
    Json(body): Json<Register>,
) -> Result<Json<Value>, ApiError> {
    let email = body.email.trim().to_lowercase();
    if !email.contains('@') || email.len() < 3 {
        return Err(ApiError::BadRequest("邮箱格式不正确".into()));
    }
    if body.password.len() < 6 {
        return Err(ApiError::BadRequest("密码至少 6 位".into()));
    }
    // 校验验证码。过期由缓存 TTL 负责;值格式 "{发送时刻}:{code哈希}"。
    let code_key = format!("email_code:{email}");
    match state.cache.get(&code_key).await.map_err(|e| ApiError::Internal(e.to_string()))? {
        Some(v) if v.split_once(':').map(|(_, h)| h) == Some(storage::hash_password(&body.code).as_str()) => {
            let _ = state.cache.del(&code_key).await;
        }
        _ => return Err(ApiError::BadRequest("验证码错误或已过期".into())),
    }
    let email = email.as_str();
    let (grant, limit, rpm, tpm, secret, ttl) = {
        let cfg = state.config();
        (cfg.defaults.signup_grant_tokens, cfg.defaults.concurrency_limit, cfg.defaults.rpm_limit, cfg.defaults.tpm_limit, cfg.auth.jwt_secret.clone(), cfg.auth.session_ttl_secs)
    };
    let pwhash = storage::hash_password(&body.password);
    // 注册默认绑定到激活(默认)模型组;无激活组则不绑定。
    let active_group = storage::active_group_id(&state.db)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    let id = storage::admin_create_user(&state.db, email, &pwhash, Some(email), None, grant, active_group, "email")
        .await
        .map_err(|_| ApiError::BadRequest("该邮箱已注册".into()))?;
    let us = Arc::new(UserState::new(id, grant, limit, 0, 1.0, active_group.unwrap_or(0)));
    us.set_limits(rpm, tpm);
    state.users.insert(id, us);
    let token = jwt::issue(&secret, &id.to_string(), "portal", ttl)?;
    Ok(Json(json!({ "token": token, "user": { "id": id, "email": email } })))
}

#[derive(Deserialize)]
pub struct ResetPassword {
    pub email: String,
    pub code: String,
    pub password: String,
}

/// POST /portal/auth/reset_password —— 邮箱验证码重置密码(忘记密码)。
pub async fn reset_password(
    State(state): State<Arc<AppState>>,
    Json(body): Json<ResetPassword>,
) -> Result<Json<Value>, ApiError> {
    let email = body.email.trim().to_lowercase();
    if !email.contains('@') || email.len() < 3 {
        return Err(ApiError::BadRequest("邮箱格式不正确".into()));
    }
    if body.password.len() < 6 {
        return Err(ApiError::BadRequest("密码至少 6 位".into()));
    }
    // 校验验证码(复用注册的邮箱验证码机制;值格式 "{发送时刻}:{code哈希}")。
    let code_key = format!("email_code:{email}");
    match state.cache.get(&code_key).await.map_err(|e| ApiError::Internal(e.to_string()))? {
        Some(v) if v.split_once(':').map(|(_, h)| h) == Some(storage::hash_password(&body.code).as_str()) => {
            let _ = state.cache.del(&code_key).await;
        }
        _ => return Err(ApiError::BadRequest("验证码错误或已过期".into())),
    }
    let id = storage::find_user_by_email(&state.db, &email)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?
        .ok_or_else(|| ApiError::BadRequest("该邮箱未注册".into()))?;
    let pwhash = storage::hash_password(&body.password);
    storage::update_user_fields(&state.db, id, None, None, None, Some(&pwhash))
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    let (secret, ttl) = {
        let cfg = state.config();
        (cfg.auth.jwt_secret.clone(), cfg.auth.session_ttl_secs)
    };
    let token = jwt::issue(&secret, &id.to_string(), "portal", ttl)?;
    Ok(Json(json!({ "token": token, "user": { "id": id } })))
}

#[derive(Deserialize)]
pub struct PwLogin {
    pub username: String,
    pub password: String,
}

/// POST /portal/auth/login —— 用户名 + 密码登录(管理端创建的用户)。
pub async fn password_login(
    State(state): State<Arc<AppState>>,
    Json(body): Json<PwLogin>,
) -> Result<Json<Value>, ApiError> {
    let (secret, ttl) = {
        let cfg = state.config();
        (cfg.auth.jwt_secret.clone(), cfg.auth.session_ttl_secs)
    };
    let (id, pwhash) = storage::find_user_by_username(&state.db, body.username.trim())
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?
        .ok_or(ApiError::Unauthorized)?;
    if pwhash.as_deref() != Some(storage::hash_password(&body.password).as_str()) {
        return Err(ApiError::Unauthorized);
    }
    if let Some(u) = state.user(&id) {
        if !u.is_active() {
            return Err(ApiError::AccountDisabled);
        }
    }
    let token = jwt::issue(&secret, &id.to_string(), "portal", ttl)?;
    Ok(Json(json!({ "token": token, "user": { "id": id } })))
}

/// GET /portal/me
pub async fn me(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Json<Value>, ApiError> {
    let uid = portal_user(&state, &headers)?;
    let row = storage::get_user(&state.db, uid)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?
        .ok_or(ApiError::InvalidKey)?;
    let balance = state.user(&uid).map(|u| u.balance()).unwrap_or(row.token_balance);
    Ok(Json(json!({
        "id": row.id,
        "phone": row.phone,
        "balance": balance,
        "used_total": row.token_used_total,
    })))
}

/// GET /portal/balance —— 顶部额度卡片数据(读内存)。
pub async fn balance(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Json<Value>, ApiError> {
    let uid = portal_user(&state, &headers)?;
    let bal = state.user(&uid).map(|u| u.balance()).unwrap_or(0);
    Ok(Json(json!({ "balance": bal })))
}

/// GET /portal/summary —— 总量 / 已用 / 余额。
pub async fn summary(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Json<Value>, ApiError> {
    let uid = portal_user(&state, &headers)?;
    let balance = state.user(&uid).map(|u| u.balance()).unwrap_or(0);
    let used = storage::user_used(&state.db, uid)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    Ok(Json(json!({ "granted": used + balance, "used": used, "balance": balance })))
}

/// GET /portal/keys —— 我的 Key(仅掩码)。
pub async fn list_keys(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Json<Value>, ApiError> {
    let uid = portal_user(&state, &headers)?;
    let rows = storage::list_keys(&state.db, uid)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    let data: Vec<Value> = rows
        .iter()
        .map(|k| {
            json!({
                "id": k.id,
                "interface_kind": k.interface_kind,
                "key_prefix": k.key_prefix,
                "revoked": k.revoked != 0,
                "created_at": k.created_at,
            })
        })
        .collect();
    Ok(Json(json!({ "data": data })))
}

#[derive(Deserialize)]
pub struct KeyReq {
    pub interface_kind: String,
}

fn valid_interface(k: &str) -> bool {
    k == "openai" || k == "anthropic"
}

/// POST /portal/keys —— 创建 Key,响应含明文(仅此一次)。
pub async fn create_key(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(body): Json<KeyReq>,
) -> Result<Json<Value>, ApiError> {
    let uid = portal_user(&state, &headers)?;
    if !valid_interface(&body.interface_kind) {
        return Err(ApiError::BadRequest("interface_kind must be openai|anthropic".into()));
    }

    // 每接口仅允许一把有效 Key。
    let existing = storage::list_keys(&state.db, uid)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    if existing
        .iter()
        .any(|k| k.revoked == 0 && k.interface_kind == body.interface_kind)
    {
        return Err(ApiError::BadRequest(
            "active key already exists for this interface; use rotate".into(),
        ));
    }

    let (plaintext, hash, row) = storage::create_key(&state.db, uid, &body.interface_kind)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    state.keys.insert(hash, KeyEntry { id: row.id, user_id: uid });

    Ok(Json(json!({
        "id": row.id,
        "interface_kind": row.interface_kind,
        "key": plaintext,            // 明文,仅此一次
        "key_prefix": row.key_prefix,
    })))
}

/// POST /portal/keys/rotate —— 轮换:旧 Key 失效,返回新明文(仅此一次)。
pub async fn rotate_key(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(body): Json<KeyReq>,
) -> Result<Json<Value>, ApiError> {
    let uid = portal_user(&state, &headers)?;
    if !valid_interface(&body.interface_kind) {
        return Err(ApiError::BadRequest("interface_kind must be openai|anthropic".into()));
    }

    let revoked = storage::revoke_active_keys(&state.db, uid, &body.interface_kind)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    for h in revoked {
        state.keys.remove(&h);
    }

    let (plaintext, hash, row) = storage::create_key(&state.db, uid, &body.interface_kind)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    state.keys.insert(hash, KeyEntry { id: row.id, user_id: uid });

    Ok(Json(json!({
        "id": row.id,
        "interface_kind": row.interface_kind,
        "key": plaintext,
        "key_prefix": row.key_prefix,
    })))
}

/// GET /portal/models —— 当前用户(按其模型组)可用的对外模型名。
pub async fn models(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Json<Value>, ApiError> {
    let uid = portal_user(&state, &headers)?;
    let group = state.user(&uid).map(|u| u.group()).unwrap_or(0);
    let data = state.routing.load().group_model_names(group);
    Ok(Json(json!({ "data": data })))
}

/// POST /portal/chat —— 门户对话客户端入口(JWT 鉴权,复用数据面管线)。
pub async fn chat(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Result<axum::response::Response, ApiError> {
    let uid = portal_user(&state, &headers)?;
    let user = state.user(&uid).ok_or(ApiError::InvalidKey)?;
    // 门户对话走 JWT 鉴权,无 API Key,密钥维度记 None。
    crate::handlers::run_chat(Arc::clone(&state), user, None, body).await
}

/// GET /portal/series —— 当前用户最近 30 天每日消耗(曲线)。
pub async fn series(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Json<Value>, ApiError> {
    let uid = portal_user(&state, &headers)?;
    let tz = state.config().defaults.tz_offset_hours;
    let data = storage::user_daily_series(&state.db, uid, 30, tz)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    Ok(Json(json!({ "data": data })))
}

/// GET /portal/rewards —— 后台配置的奖励任务(仅启用)+ 我的申领记录。
pub async fn rewards(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Json<Value>, ApiError> {
    let uid = portal_user(&state, &headers)?;
    let tasks = storage::list_reward_tasks(&state.db, true)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    let claims = storage::list_user_reward_claims(&state.db, uid)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    Ok(Json(json!({ "tasks": tasks, "claims": claims })))
}

#[derive(Deserialize)]
pub struct ClaimReward {
    pub task_id: i64,
    #[serde(default)]
    pub evidence: Option<String>,
}

/// POST /portal/rewards —— 对某奖励任务提交申领,待后台人工审核。
pub async fn claim_reward(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(body): Json<ClaimReward>,
) -> Result<Json<Value>, ApiError> {
    let uid = portal_user(&state, &headers)?;
    let task = storage::get_reward_task(&state.db, body.task_id)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?
        .ok_or_else(|| ApiError::BadRequest("奖励任务不存在".into()))?;
    if !task.enabled {
        return Err(ApiError::BadRequest("该奖励任务已下线".into()));
    }

    // 按任务的证明类型要求 evidence:截图 / 链接 / 文本;none 无需证明。
    let evidence = body.evidence.as_deref().map(str::trim).filter(|s| !s.is_empty());
    match task.evidence_type.as_str() {
        "screenshot" if evidence.is_none() => return Err(ApiError::BadRequest("请上传截图以便审核".into())),
        "link" if evidence.is_none() => return Err(ApiError::BadRequest("请填写链接以便审核".into())),
        "text" if evidence.is_none() => return Err(ApiError::BadRequest("请填写申领内容".into())),
        _ => {}
    }
    if let Some(e) = evidence {
        // 截图以 base64 data URL 存入 evidence;限制大小,避免库膨胀(前端已压缩)。
        if e.len() > 8_000_000 {
            return Err(ApiError::BadRequest("提交内容过大,请压缩后重试".into()));
        }
        if task.evidence_type == "text" {
            let chars = e.chars().count();
            if chars < 10 {
                return Err(ApiError::BadRequest("内容太短,请至少写 10 个字".into()));
            }
            if chars > 5000 {
                return Err(ApiError::BadRequest("内容过长(最多 5000 字)".into()));
            }
        }
    }

    if storage::has_open_reward_claim(&state.db, uid, task.id)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?
    {
        return Err(ApiError::BadRequest("你已提交过该奖励申领,请勿重复提交".into()));
    }

    // 区间类任务申领时先记 0,通过时由管理员评定;固定类记任务额度。
    let reward_tokens = if task.variable { 0 } else { task.reward_tokens };
    let id = storage::create_reward_claim(&state.db, uid, task.id, evidence, reward_tokens)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    Ok(Json(json!({ "id": id, "status": 0, "reward_tokens": reward_tokens })))
}

/// GET /portal/usage?page=&page_size= —— 当前登录用户自己的用量(分页)。
pub async fn usage(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    axum::extract::Query(q): axum::extract::Query<PageQuery>,
) -> Result<Json<Value>, ApiError> {
    let uid = portal_user(&state, &headers)?;
    let (page, page_size, offset) = normalize_page(q.page, q.page_size);
    let (rows, total) = storage::usage_rows_page(&state.db, Some(uid), page_size as i64, offset as i64)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    Ok(Json(json!({
        "data": rows,
        "page": page,
        "page_size": page_size,
        "total": total,
        "total_pages": total_pages(total, page_size),
    })))
}
