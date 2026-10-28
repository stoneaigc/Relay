use std::net::SocketAddr;
use std::sync::Arc;

use axum::{
    extract::{ConnectInfo, State},
    http::HeaderMap,
    Json,
};
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
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    Json(body): Json<SendCode>,
) -> Result<Json<Value>, ApiError> {
    let email = body.email.trim().to_lowercase();
    if !email.contains('@') || email.len() < 3 {
        return Err(ApiError::BadRequest("邮箱格式不正确".into()));
    }
    // 防轰炸:IP 与邮箱双标识,每次发码均计数(发码消耗资源,成功也不清零)。
    let ip_key = format!("code:{}", addr.ip());
    let email_key = format!("codee:{email}");
    for k in [&ip_key, &email_key] {
        if let Some(secs) = state.login_guard.locked_for(k) {
            return Err(ApiError::TooManyAttempts(format!(
                "请求过于频繁,请 {secs} 秒后再试"
            )));
        }
    }
    state.login_guard.record_failure(&ip_key);
    state.login_guard.record_failure(&email_key);
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
    let val = format!("{}:{}", now_secs(), storage::hash_key(&code));
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
pub struct SendPhoneCode {
    pub phone: String,
}

/// 校验大陆手机号格式。
fn valid_phone(p: &str) -> bool {
    p.len() == 11 && p.starts_with('1') && p.chars().skip(1).all(|c| c.is_ascii_digit())
}

/// POST /portal/auth/phone/send_code —— 发送手机号注册验证码(经短信网关)。
/// 未配置 sms.gateway_url 时为开发模式:验证码随响应回显并打日志。
pub async fn send_phone_code(
    State(state): State<Arc<AppState>>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    Json(body): Json<SendPhoneCode>,
) -> Result<Json<Value>, ApiError> {
    // 开关:后台「设置→门户」可关(关闭后前端隐藏手机号注册,接口一并拒绝)。
    let kv = storage::load_settings(&state.db, crate::settings::PORTAL_PREFIX)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    if kv.get(crate::settings::K_PORTAL_PHONE_REGISTER).map(|v| v != "true").unwrap_or(false) {
        return Err(ApiError::BadRequest("手机号注册未开放".into()));
    }
    let phone = body.phone.trim().to_string();
    if !valid_phone(&phone) {
        return Err(ApiError::BadRequest("手机号格式不正确".into()));
    }
    // 防轰炸:IP 与手机号双标识(发码消耗资源,成功也不清零;上限 5 次/15 分钟)。
    let ip_key = format!("code:{}", addr.ip());
    let phone_key = format!("codep:{phone}");
    for k in [&ip_key, &phone_key] {
        if let Some(secs) = state.login_guard.locked_for(k) {
            return Err(ApiError::TooManyAttempts(format!(
                "请求过于频繁,请 {secs} 秒后再试"
            )));
        }
    }
    state.login_guard.record_failure(&ip_key);
    state.login_guard.record_failure(&phone_key);
    // 限频:60 秒内不重复发。缓存值 "{发送时刻}:{code哈希}"。
    let code_key = format!("phone_code:{phone}");
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
    let val = format!("{}:{}", now_secs(), storage::hash_key(&code));
    state
        .cache
        .set_ex(&code_key, &val, 300)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    let gateway = crate::sms::load_gateway(&state.db).await;
    if gateway.is_dev() {
        // 开发模式:验证码打日志并随响应回显(与邮箱开发模式一致),本地联调可用。
        tracing::warn!("[DEV] 手机验证码 {phone}: {code}");
        Ok(Json(json!({ "sent": false, "dev_code": code })))
    } else {
        crate::sms::send(&state.http, &gateway, &phone, &code)
            .await
            .map_err(ApiError::BadRequest)?;
        Ok(Json(json!({ "sent": true })))
    }
}

#[derive(Deserialize)]
pub struct Register {
    /// 注册渠道:"email"(默认,兼容旧请求)| "phone"。
    #[serde(default)]
    pub kind: Option<String>,
    pub email: Option<String>,
    pub phone: Option<String>,
    pub code: String,
    pub password: String,
}

/// 校验验证码并消费(通用:邮箱/手机号共用 cache 键规则 "{kind}_code:{id}")。
async fn verify_code(state: &AppState, kind: &str, id: &str, code: &str) -> Result<(), ApiError> {
    let code_key = format!("{kind}_code:{id}");
    match state.cache.get(&code_key).await.map_err(|e| ApiError::Internal(e.to_string()))? {
        Some(v) if v.split_once(':').map(|(_, h)| h) == Some(storage::hash_key(code).as_str()) => {
            let _ = state.cache.del(&code_key).await;
            Ok(())
        }
        _ => Err(ApiError::BadRequest("验证码错误或已过期".into())),
    }
}

/// POST /portal/auth/register —— 双渠道注册(邮箱验证码 / 手机短信验证码),返回会话 JWT。
pub async fn register(
    State(state): State<Arc<AppState>>,
    Json(body): Json<Register>,
) -> Result<Json<Value>, ApiError> {
    if body.password.len() < 8 {
        return Err(ApiError::BadRequest("密码至少 8 位".into()));
    }
    let kind = body.kind.as_deref().unwrap_or("email");
    let pwhash = storage::hash_password(&body.password);
    // 新用户默认挂 default 用户组(P4-2);不存在则 NULL(回落全局默认)。
    let default_ug = storage::default_user_group_id(&state.db)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    let (grant, limit, rpm, tpm, secret, ttl) = {
        let cfg = state.config();
        (cfg.defaults.signup_grant_tokens, cfg.defaults.concurrency_limit, cfg.defaults.rpm_limit, cfg.defaults.tpm_limit, cfg.auth.jwt_secret.clone(), cfg.auth.session_ttl_secs)
    };
    // 运行时限额必须立即走「用户组 > 全局」解析链,不能只在 DB 记录组归属。
    let default_group = match default_ug {
        Some(gid) => storage::get_user_group(&state.db, gid)
            .await
            .map_err(|e| ApiError::Internal(e.to_string()))?,
        None => None,
    };
    let effective_cl = default_group.as_ref().and_then(|g| g.max_concurrency).unwrap_or(limit as i64).max(0) as u32;
    let effective_cl = if effective_cl == 0 { u32::MAX } else { effective_cl };
    let effective_rpm = default_group.as_ref().and_then(|g| g.rpm_limit).unwrap_or(rpm as i64).max(0) as u32;
    let effective_tpm = default_group.as_ref().and_then(|g| g.tpm_limit).unwrap_or(tpm as i64).max(0) as u32;
    // 注册默认绑定到激活(默认)模型组;无激活组则不绑定。
    let active_group = storage::active_group_id(&state.db)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    let (id, display) = match kind {
        "phone" => {
            let phone = body.phone.as_deref().map(str::trim).unwrap_or_default();
            if !valid_phone(phone) {
                return Err(ApiError::BadRequest("手机号格式不正确".into()));
            }
            let kv = storage::load_settings(&state.db, crate::settings::PORTAL_PREFIX)
                .await
                .map_err(|e| ApiError::Internal(e.to_string()))?;
            if kv.get(crate::settings::K_PORTAL_PHONE_REGISTER).map(|v| v != "true").unwrap_or(false) {
                return Err(ApiError::BadRequest("手机号注册未开放".into()));
            }
            verify_code(&state, "phone", phone, &body.code).await?;
            let id = storage::admin_create_user(
                &state.db,
                phone,
                &pwhash,
                None,
                Some(phone),
                grant,
                active_group,
                "phone",
                default_ug,
                None,
            )
            .await
            .map_err(|_| ApiError::BadRequest("该手机号已注册".into()))?;
            (id, phone.to_string())
        }
        _ => {
            let email = body.email.as_deref().map(str::trim).unwrap_or_default().to_lowercase();
            if !email.contains('@') || email.len() < 3 {
                return Err(ApiError::BadRequest("邮箱格式不正确".into()));
            }
            // kind 传 "email":verify_code 内部拼 "{kind}_code:{id}" → 与发码写入的
            // "email_code:{email}" 对齐(此前误传 "email_code" 导致查 email_code_code: 永不相等,注册必然失败)。
            verify_code(&state, "email", &email, &body.code).await?;
            let id = storage::admin_create_user(
                &state.db,
                &email,
                &pwhash,
                Some(&email),
                None,
                grant,
                active_group,
                "email",
                default_ug,
                None,
            )
            .await
            .map_err(|_| ApiError::BadRequest("该邮箱已注册".into()))?;
            (id, email)
        }
    };
    let us = Arc::new(UserState::new(id, grant, 0, effective_cl, 0, 1.0, active_group.unwrap_or(0), default_ug.unwrap_or(0)));
    us.set_limits(effective_rpm, effective_tpm);
    us.set_budgets(
        default_group.as_ref().and_then(|g| g.budget_daily).unwrap_or(0).max(0),
        default_group.as_ref().and_then(|g| g.budget_monthly).unwrap_or(0).max(0),
    );
    state.users.insert(id, us);
    let token = jwt::issue(&secret, &id.to_string(), "portal", ttl)?;
    Ok(Json(json!({ "token": token, "user": { "id": id, "account": display } })))
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
    if body.password.len() < 8 {
        return Err(ApiError::BadRequest("密码至少 8 位".into()));
    }
    // 校验验证码(复用注册的邮箱验证码机制;值格式 "{发送时刻}:{code哈希}")。
    let code_key = format!("email_code:{email}");
    match state.cache.get(&code_key).await.map_err(|e| ApiError::Internal(e.to_string()))? {
        Some(v) if v.split_once(':').map(|(_, h)| h) == Some(storage::hash_key(&body.code).as_str()) => {
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
    // 自助重置 = 用户已知新密码,标记已改密(管理员端不再展示初始密码)。
    storage::mark_password_changed(&state.db, id)
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
pub struct ChangePassword {
    pub old_password: String,
    pub new_password: String,
}

/// POST /portal/auth/change_password —— 已登录用户自助修改密码。
pub async fn change_password(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(body): Json<ChangePassword>,
) -> Result<Json<Value>, ApiError> {
    let uid = portal_user(&state, &headers)?;
    if body.new_password.len() < 8 {
        return Err(ApiError::BadRequest("新密码至少 8 位".into()));
    }
    // 旧密码校验(与登录同口径;无哈希账号视为校验失败)。
    let Some(stored) = storage::get_user_password_hash(&state.db, uid)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?
    else {
        return Err(ApiError::BadRequest("该账号未设置密码".into()));
    };
    let (ok, _legacy) = storage::verify_password(&stored, &body.old_password);
    if !ok {
        return Err(ApiError::InvalidCredentials);
    }
    let new_hash = storage::hash_password(&body.new_password);
    storage::update_user_fields(&state.db, uid, None, None, None, Some(&new_hash))
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    // 自助改密后管理员端不再展示初始密码。
    storage::mark_password_changed(&state.db, uid)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    Ok(Json(json!({ "ok": true })))
}

#[derive(Deserialize)]
pub struct PwLogin {
    pub username: String,
    pub password: String,
}

/// POST /portal/auth/login —— 用户名 + 密码登录(管理端创建的用户)。
pub async fn password_login(
    State(state): State<Arc<AppState>>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    Json(body): Json<PwLogin>,
) -> Result<Json<Value>, ApiError> {
    // 防爆破:IP 与用户名双标识,5 次失败锁 15 分钟(用户名不存在也计数,防枚举)。
    let ip_key = format!("pw:{}", addr.ip());
    let user_key = format!("pwu:{}", body.username.trim().to_lowercase());
    for k in [&ip_key, &user_key] {
        if let Some(secs) = state.login_guard.locked_for(k) {
            return Err(ApiError::TooManyAttempts(format!(
                "失败次数过多,请 {secs} 秒后再试"
            )));
        }
    }
    let fail = |state: &AppState| {
        state.login_guard.record_failure(&ip_key);
        state.login_guard.record_failure(&user_key);
    };
    let (secret, ttl) = {
        let cfg = state.config();
        (cfg.auth.jwt_secret.clone(), cfg.auth.session_ttl_secs)
    };
    let (id, pwhash) = match storage::find_user_by_username(&state.db, body.username.trim())
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
    let (ok, legacy) = storage::verify_password(stored, &body.password);
    if !ok {
        fail(&state);
        return Err(ApiError::InvalidCredentials);
    }
    // 旧版无盐 sha256 哈希登录成功后透明升级为 argon2(失败仅记日志,不阻断登录)。
    if legacy {
        let new_hash = storage::hash_password(&body.password);
        if let Err(e) = storage::upgrade_password_hash(&state.db, id, &new_hash).await {
            tracing::warn!("密码哈希升级失败(user {id}): {e}");
        }
    }
    if let Some(u) = state.user(&id) {
        if !u.is_active() {
            return Err(ApiError::AccountDisabled);
        }
    }
    state.login_guard.clear(&ip_key);
    state.login_guard.clear(&user_key);
    let token = jwt::issue(&secret, &id.to_string(), "portal", ttl)?;
    Ok(Json(json!({ "token": token, "user": { "id": id } })))
}

/// GET /portal/config -- 门户级配置(无需登录)。
pub async fn config(
    State(state): State<Arc<AppState>>,
) -> Result<Json<Value>, ApiError> {
    let kv = storage::load_settings(&state.db, crate::settings::PORTAL_PREFIX)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    let rewards_enabled = kv
        .get(crate::settings::K_PORTAL_REWARDS_ENABLED)
        .map(|v| v == "true")
        .unwrap_or(false);
    Ok(Json(json!({ "rewards_enabled": rewards_enabled })))
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

/// GET /portal/quota —— 我的套餐与实时用量(并发/RPM/TPM/日·月预算)。
/// limit 为 null = 不限;数值口径与解析链一致(用户显式 > 用户组 > 全局默认)。
pub async fn quota(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Json<Value>, ApiError> {
    let uid = portal_user(&state, &headers)?;
    let u = state.user(&uid).ok_or(ApiError::InvalidKey)?;
    let row = storage::get_user(&state.db, uid)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?
        .ok_or(ApiError::InvalidKey)?;
    let group = match row.user_group_id {
        Some(g) => storage::get_user_group(&state.db, g)
            .await
            .map_err(|e| ApiError::Internal(e.to_string()))?,
        None => None,
    };
    let cl = u.concurrency_limit.load(std::sync::atomic::Ordering::Relaxed);
    let conc_limit = if cl == u32::MAX { Value::Null } else { json!(cl) };
    let rpm_limit = u.rpm_limit.load(std::sync::atomic::Ordering::Relaxed);
    let tpm_limit = u.tpm_limit.load(std::sync::atomic::Ordering::Relaxed);
    let bd = u.budget_daily.load(std::sync::atomic::Ordering::Relaxed);
    let bm = u.budget_monthly.load(std::sync::atomic::Ordering::Relaxed);
    Ok(Json(json!({
        "plan": group.as_ref().map(|g| json!({ "id": g.id, "name": g.name })),
        "balance": u.balance(),
        "concurrency": {
            "limit": conc_limit,
            "used": u.in_flight,
        },
        "rpm": {
            "limit": if rpm_limit == 0 { Value::Null } else { json!(rpm_limit) },
            "used": u.rpm_used(),
        },
        "tpm": {
            "limit": if tpm_limit == 0 { Value::Null } else { json!(tpm_limit) },
            "used": u.tpm_used(),
        },
        "budget": {
            "daily": {
                "limit": if bd == 0 { Value::Null } else { json!(bd) },
                "used": u.used_daily.load(std::sync::atomic::Ordering::Relaxed),
            },
            "monthly": {
                "limit": if bm == 0 { Value::Null } else { json!(bm) },
                "used": u.used_monthly.load(std::sync::atomic::Ordering::Relaxed),
            },
        },
    })))
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
                // 完整 Key(解密后)仅返回给 Key 属主本人,门户端随时复制;换过 jwt_secret 的旧 Key 为 null。
                "key_plain": k.key_plain,
                "revoked": k.revoked != 0,
                "status": k.status,
                "pending_approval": k.revoked == 0 && k.status == 1,
                "allowed_models": k.allowed_models,
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

/// 读门户 Key 审批开关(缺省 false:Key 创建即用)。
async fn key_approval_required(state: &AppState) -> Result<bool, ApiError> {
    let kv = storage::load_settings(&state.db, crate::settings::PORTAL_PREFIX)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    Ok(kv.get(crate::settings::K_PORTAL_KEY_APPROVAL).map(|v| v == "true").unwrap_or(false))
}

/// POST /portal/keys —— 创建 Key,响应含明文(仅此一次)。
/// 审批开关开启时新 Key 为「待审批」状态,管理员批准后才会生效。
pub async fn create_key(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(body): Json<KeyReq>,
) -> Result<Json<Value>, ApiError> {
    let uid = portal_user(&state, &headers)?;
    if !valid_interface(&body.interface_kind) {
        return Err(ApiError::BadRequest("interface_kind must be openai|anthropic".into()));
    }

    // 每接口仅允许一把有效 Key(待审批也算占位)。
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

    let approval = key_approval_required(&state).await?;
    let status: i64 = if approval { 1 } else { 0 };
    let (plaintext, hash, row) = storage::create_key(&state.db, uid, &body.interface_kind, status)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    // 待审批不进内存 = 等效不可用;批准后由管理端热插。
    if status == 0 {
        state.keys.insert(hash, KeyEntry { id: row.id, user_id: uid, allowed_models: None });
    }

    Ok(Json(json!({
        "id": row.id,
        "interface_kind": row.interface_kind,
        "key": plaintext,            // 明文,仅此一次
        "key_prefix": row.key_prefix,
        "status": status,
        "pending_approval": status == 1,
    })))
}

/// POST /portal/keys/rotate —— 轮换:旧 Key 失效,返回新明文(仅此一次)。
/// 审批开关开启时新 Key 同样需要审批;审批关闭时新 Key 立即生效。
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

    let approval = key_approval_required(&state).await?;
    let status: i64 = if approval { 1 } else { 0 };
    let (plaintext, hash, row) = storage::create_key(&state.db, uid, &body.interface_kind, status)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    if status == 0 {
        state.keys.insert(hash, KeyEntry { id: row.id, user_id: uid, allowed_models: None });
    }

    Ok(Json(json!({
        "id": row.id,
        "interface_kind": row.interface_kind,
        "key": plaintext,
        "key_prefix": row.key_prefix,
        "status": status,
        "pending_approval": status == 1,
    })))
}

/// GET /portal/models —— 当前用户(按其模型组)可用的模型广场卡片。
pub async fn models(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Json<Value>, ApiError> {
    let uid = portal_user(&state, &headers)?;
    let routing = state.routing.load();
    let gids = state.user(&uid).map(|u| u.resolve_group_ids(&routing)).unwrap_or_default();
    let cards = routing.model_cards_union(&gids);
    let data: Vec<Value> = cards
        .iter()
        .map(|c| {
            // 与两级寻址一致:取首个提供该对外模型的模型组;0 表示不限。
            let group_id = gids.iter().find(|gid| {
                routing.groups.get(*gid).and_then(|g| g.get(&c.name)).is_some()
            });
            let group_concurrency = group_id
                .and_then(|gid| routing.group_concurrency.get(gid).copied())
                .unwrap_or(0);
            json!({
                "name": c.name,
                "group_concurrency": group_concurrency,
                "labels": c.labels,
                "tags": c.tags,
                "context_length": c.context_length,
                "multiplier": c.multiplier,
                "cache": c.cache,
                "upstreams": c.upstreams,
                "input_price": if c.input_price.is_finite() { json!(c.input_price) } else { Value::Null },
                "output_price": if c.output_price.is_finite() { json!(c.output_price) } else { Value::Null },
            })
        })
        .collect();
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
    // 门户对话走 JWT 鉴权,无 API Key,密钥维度记 None;门户入口不提供缓存逃生口。
    crate::handlers::run_chat(Arc::clone(&state), user, None, body, false).await
}

/// GET /portal/series?days= —— 当前用户最近 N 天每日消耗(曲线,默认 30,7~90)。
#[derive(Deserialize)]
pub struct SeriesDaysQuery {
    #[serde(default)]
    pub days: Option<u32>,
}

pub async fn series(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    axum::extract::Query(q): axum::extract::Query<SeriesDaysQuery>,
) -> Result<Json<Value>, ApiError> {
    let uid = portal_user(&state, &headers)?;
    let tz = state.config().defaults.tz_offset_hours;
    let days = q.days.unwrap_or(30).clamp(7, 90);
    let data = storage::user_daily_series(&state.db, uid, days as i64, tz)
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
    let (rows, total) = storage::usage_rows_page(&state.db, Some(uid), page_size as i64, offset as i64, None, None, None, None)
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
