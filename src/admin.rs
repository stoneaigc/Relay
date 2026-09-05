use std::sync::atomic::Ordering;
use std::sync::Arc;

use axum::{
    extract::{Path, State},
    http::HeaderMap,
    response::IntoResponse,
    Json,
};
use serde::Deserialize;
use serde_json::{json, Value};
use uuid::Uuid;

use crate::config::{EmailConfig, ProviderKind};
use crate::error::ApiError;
use crate::state::{AppState, UserState};
use crate::{jwt, storage};

/// 任何模型/组/路由改动后,从 DB 重建内存路由图。
async fn rebuild_routing(state: &AppState) -> Result<(), ApiError> {
    let r = storage::load_routing(&state.db)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    state.routing.store(Arc::new(r));
    Ok(())
}

fn admin_guard(state: &AppState, headers: &HeaderMap) -> Result<(), ApiError> {
    let cfg = state.config();
    jwt::from_headers(&cfg.auth.jwt_secret, headers, "admin").map(|_| ())
}

/// 价格合法性:可空;有值时必须是有限非负数($/1M tokens)。
fn price_ok(p: Option<f64>) -> bool {
    p.map(|v| v.is_finite() && v >= 0.0).unwrap_or(true)
}

/// 统一分页查询参数:page(默认1,>=1)、page_size(默认20,clamp到1..=100)。
#[derive(Deserialize, Default)]
pub struct PageQuery {
    #[serde(default)]
    pub page: Option<u32>,
    #[serde(default)]
    pub page_size: Option<u32>,
}

/// 规范化分页参数,返回 (page, page_size, offset)。
pub fn normalize_page(page: Option<u32>, page_size: Option<u32>) -> (u32, u32, u32) {
    let page = page.unwrap_or(1).max(1);
    let page_size = page_size.unwrap_or(20).clamp(1, 100);
    (page, page_size, (page - 1) * page_size)
}

/// 计算总页数:ceil(total / page_size)。
pub fn total_pages(total: i64, page_size: u32) -> u64 {
    if total <= 0 {
        0
    } else {
        ((total as u64) + (page_size as u64) - 1) / (page_size as u64)
    }
}

#[derive(Deserialize)]
pub struct AdminLogin {
    pub username: String,
    pub password: String,
}

/// POST /admin/auth/login
pub async fn login(
    State(state): State<Arc<AppState>>,
    Json(body): Json<AdminLogin>,
) -> Result<Json<Value>, ApiError> {
    let (ok, secret, ttl) = {
        let cfg = state.config();
        (
            body.username == cfg.admin.username && body.password == cfg.admin.password,
            cfg.auth.jwt_secret.clone(),
            cfg.auth.session_ttl_secs,
        )
    };
    if !ok {
        return Err(ApiError::Unauthorized);
    }
    let token = jwt::issue(&secret, &body.username, "admin", ttl)?;
    Ok(Json(json!({ "token": token })))
}

/// GET /admin/overview —— 概览统计(累计)。
pub async fn overview(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Json<Value>, ApiError> {
    admin_guard(&state, &headers)?;
    let data = storage::overview(&state.db)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    Ok(Json(data))
}

#[derive(Deserialize)]
pub struct SeriesQuery {
    #[serde(default)]
    pub granularity: Option<String>,
}

/// GET /admin/overview/series?granularity=day|week|month —— 全站消耗趋势。
pub async fn overview_series(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    axum::extract::Query(q): axum::extract::Query<SeriesQuery>,
) -> Result<Json<Value>, ApiError> {
    admin_guard(&state, &headers)?;
    let tz = state.config().defaults.tz_offset_hours;
    let gran = q.granularity.as_deref().unwrap_or("day");
    // 默认窗口:日 30、周 12、月 12。
    let periods = match gran {
        "week" | "month" => 12,
        _ => 30,
    };
    let data = storage::global_series(&state.db, gran, periods, tz)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    Ok(Json(json!({ "granularity": gran, "data": data })))
}

/// GET /admin/users?page=&page_size= —— 所有用户(余额取内存实时值,分页)。
pub async fn list_users(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    axum::extract::Query(q): axum::extract::Query<PageQuery>,
) -> Result<Json<Value>, ApiError> {
    admin_guard(&state, &headers)?;
    let (page, page_size, offset) = normalize_page(q.page, q.page_size);
    let (rows, total) = storage::list_users_page(&state.db, page_size as i64, offset as i64)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    let tz = state.config().defaults.tz_offset_hours;
    let used_map = storage::usage_by_user(&state.db, None).await.map_err(|e| ApiError::Internal(e.to_string()))?;
    let today_map = storage::usage_by_user(&state.db, Some(storage::today_start(tz))).await.map_err(|e| ApiError::Internal(e.to_string()))?;
    let month_map = storage::usage_by_user(&state.db, Some(storage::month_start(tz))).await.map_err(|e| ApiError::Internal(e.to_string()))?;

    let data: Vec<Value> = rows
        .iter()
        .map(|u| {
            let live = state.user(&u.id);
            let balance = live.as_ref().map(|x| x.balance()).unwrap_or(u.token_balance);
            let uid = u.id.to_string();
            let used = used_map.get(&uid).copied().unwrap_or(0);
            let today = today_map.get(&uid).copied().unwrap_or(0);
            json!({
                "id": u.id,
                "username": u.username,
                "email": u.email,
                "phone": u.phone,
                "status": u.status,
                "balance": balance,
                "used": used,
                "today": today,
                "used_month": month_map.get(&uid).copied().unwrap_or(0),
                "granted": used + balance,
                "concurrency_limit": live.as_ref().map(|x| x.concurrency_limit.load(Ordering::Relaxed) as i64).or(u.concurrency_limit),
                "group_id": live.as_ref().map(|x| x.group()).unwrap_or(u.group_id.unwrap_or(0)),
                "budget_daily_tokens": u.budget_daily_tokens,
                "budget_monthly_tokens": u.budget_monthly_tokens,
                "source": u.source,
                "created_at": u.created_at,
            })
        })
        .collect();
    Ok(Json(json!({
        "data": data,
        "page": page,
        "page_size": page_size,
        "total": total,
        "total_pages": total_pages(total, page_size),
    })))
}

#[derive(Deserialize)]
pub struct CreateUser {
    pub username: String,
    pub password: String,
    pub email: Option<String>,
    pub phone: Option<String>,
    pub group_id: Option<i64>,
    pub grant_tokens: Option<i64>,
    pub concurrency_limit: Option<i64>,
    pub rpm_limit: Option<i64>,
    pub tpm_limit: Option<i64>,
    pub budget_daily_tokens: Option<i64>,
    pub budget_monthly_tokens: Option<i64>,
}

/// POST /admin/users
pub async fn create_user(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(body): Json<CreateUser>,
) -> Result<Json<Value>, ApiError> {
    admin_guard(&state, &headers)?;
    if body.username.trim().is_empty() || body.password.is_empty() {
        return Err(ApiError::BadRequest("username and password required".into()));
    }
    let (grant, default_limit, default_rpm, default_tpm) = {
        let cfg = state.config();
        (
            body.grant_tokens.unwrap_or(cfg.defaults.signup_grant_tokens),
            cfg.defaults.concurrency_limit,
            cfg.defaults.rpm_limit,
            cfg.defaults.tpm_limit,
        )
    };
    let limit = body.concurrency_limit.map(|v| v as u32).unwrap_or(default_limit);
    let gid = body.group_id.filter(|g| *g > 0);
    let pwhash = storage::hash_password(&body.password);
    let email = body.email.as_deref().filter(|s| !s.is_empty());
    let phone = body.phone.as_deref().filter(|s| !s.is_empty());

    let id = storage::admin_create_user(&state.db, body.username.trim(), &pwhash, email, phone, grant, gid, "admin")
        .await
        .map_err(|_| ApiError::BadRequest("用户名或手机号已存在".into()))?;
    if body.concurrency_limit.is_some()
        || body.rpm_limit.is_some()
        || body.tpm_limit.is_some()
        || body.budget_daily_tokens.is_some()
        || body.budget_monthly_tokens.is_some()
    {
        storage::update_user(
            &state.db,
            id,
            body.concurrency_limit,
            None,
            None,
            body.rpm_limit,
            body.tpm_limit,
            body.budget_daily_tokens,
            body.budget_monthly_tokens,
        )
        .await
        .ok();
    }
    let us = Arc::new(UserState::new(id, grant, limit, 0, 1.0, gid.unwrap_or(0)));
    us.set_limits(
        body.rpm_limit.map(|v| v.max(0) as u32).unwrap_or(default_rpm),
        body.tpm_limit.map(|v| v.max(0) as u32).unwrap_or(default_tpm),
    );
    us.set_budgets(
        body.budget_daily_tokens.unwrap_or(0),
        body.budget_monthly_tokens.unwrap_or(0),
    );
    state.users.insert(id, us);
    Ok(Json(json!({ "id": id })))
}

/// GET /admin/users/:id
pub async fn get_user(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, ApiError> {
    admin_guard(&state, &headers)?;
    let u = storage::get_user(&state.db, id)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?
        .ok_or_else(|| ApiError::BadRequest("user not found".into()))?;
    let balance = state.user(&id).map(|x| x.balance()).unwrap_or(u.token_balance);
    Ok(Json(json!({
        "id": u.id,
        "phone": u.phone,
        "status": u.status,
        "balance": balance,
        "used_total": u.token_used_total,
        "concurrency_limit": u.concurrency_limit,
        "multiplier": u.bill_multiplier,
        "rpm_limit": u.rpm_limit,
        "tpm_limit": u.tpm_limit,
        "created_at": u.created_at,
    })))
}

#[derive(Deserialize)]
pub struct PatchUser {
    pub concurrency_limit: Option<i64>,
    pub status: Option<i64>,
    pub add_tokens: Option<i64>,
    pub bill_multiplier: Option<f64>,
    pub group_id: Option<i64>, // 绑定模型组(0 = 解绑)
    pub rpm_limit: Option<i64>,
    pub tpm_limit: Option<i64>,
    pub budget_daily_tokens: Option<i64>,
    pub budget_monthly_tokens: Option<i64>,
    pub username: Option<String>,
    pub email: Option<String>,
    pub phone: Option<String>,
    pub password: Option<String>, // 留空不改
}

/// PATCH /admin/users/:id —— 设并发/状态/增减额度(同步内存 + DB)。
pub async fn patch_user(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    Json(body): Json<PatchUser>,
) -> Result<Json<Value>, ApiError> {
    admin_guard(&state, &headers)?;

    storage::update_user(
        &state.db,
        id,
        body.concurrency_limit,
        body.status,
        body.bill_multiplier,
        body.rpm_limit,
        body.tpm_limit,
        body.budget_daily_tokens,
        body.budget_monthly_tokens,
    )
    .await
    .map_err(|e| ApiError::Internal(e.to_string()))?;
    if let Some(delta) = body.add_tokens {
        storage::add_tokens(&state.db, id, delta)
            .await
            .map_err(|e| ApiError::Internal(e.to_string()))?;
    }

    if let Some(u) = state.user(&id) {
        if let Some(cl) = body.concurrency_limit {
            u.concurrency_limit.store(cl as u32, Ordering::Relaxed);
        }
        if let Some(st) = body.status {
            u.status.store(st as u8, Ordering::Relaxed);
        }
        if let Some(delta) = body.add_tokens {
            u.token_balance.fetch_add(delta, Ordering::Relaxed);
        }
        if let Some(m) = body.bill_multiplier {
            u.set_bill_multiplier(m);
        }
        if body.rpm_limit.is_some() || body.tpm_limit.is_some() {
            let rpm = body.rpm_limit.map(|v| v.max(0) as u32).unwrap_or_else(|| u.rpm_limit.load(Ordering::Relaxed));
            let tpm = body.tpm_limit.map(|v| v.max(0) as u32).unwrap_or_else(|| u.tpm_limit.load(Ordering::Relaxed));
            u.set_limits(rpm, tpm);
        }
        if body.budget_daily_tokens.is_some() || body.budget_monthly_tokens.is_some() {
            let d = body.budget_daily_tokens.map(|v| v.max(0)).unwrap_or_else(|| u.budget_daily.load(Ordering::Relaxed));
            let m = body.budget_monthly_tokens.map(|v| v.max(0)).unwrap_or_else(|| u.budget_monthly.load(Ordering::Relaxed));
            u.set_budgets(d, m);
        }
    }

    if let Some(gid) = body.group_id {
        let g = if gid > 0 { Some(gid) } else { None };
        storage::set_user_group(&state.db, id, g)
            .await
            .map_err(|e| ApiError::Internal(e.to_string()))?;
        if let Some(u) = state.user(&id) {
            u.group_id.store(gid.max(0), Ordering::Relaxed);
        }
    }

    // 资料字段(用户名/邮箱/手机/密码,提供了才改)。
    let pwhash = body.password.as_deref().filter(|p| !p.is_empty()).map(storage::hash_password);
    if body.username.is_some() || body.email.is_some() || body.phone.is_some() || pwhash.is_some() {
        storage::update_user_fields(
            &state.db,
            id,
            body.username.as_deref().map(|s| s.trim()),
            body.email.as_deref(),
            body.phone.as_deref(),
            pwhash.as_deref(),
        )
        .await
        .map_err(|_| ApiError::BadRequest("用户名或手机号已被占用".into()))?;
    }
    Ok(Json(json!({ "ok": true })))
}

/// DELETE /admin/users/:id —— 删除用户(连同其 Key)。
pub async fn delete_user(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, ApiError> {
    admin_guard(&state, &headers)?;
    let hashes = storage::delete_user(&state.db, id)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    for h in hashes {
        state.keys.remove(&h);
    }
    state.users.remove(&id);
    Ok(Json(json!({ "ok": true })))
}

/// GET /admin/users/:id/series —— 该用户最近 N 天每日消耗(默认 30 天)。
pub async fn user_series(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, ApiError> {
    admin_guard(&state, &headers)?;
    let tz = state.config().defaults.tz_offset_hours;
    let data = storage::user_daily_series(&state.db, id, 30, tz)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    Ok(Json(json!({ "data": data })))
}

/// GET /admin/users/:id/usage?page=&page_size=
pub async fn user_usage(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    axum::extract::Query(q): axum::extract::Query<PageQuery>,
) -> Result<Json<Value>, ApiError> {
    admin_guard(&state, &headers)?;
    let (page, page_size, offset) = normalize_page(q.page, q.page_size);
    let (rows, total) = storage::usage_rows_page(&state.db, Some(id), page_size as i64, offset as i64)
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

/// GET /admin/usage?page=&page_size= —— 全局用量
pub async fn global_usage(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    axum::extract::Query(q): axum::extract::Query<PageQuery>,
) -> Result<Json<Value>, ApiError> {
    admin_guard(&state, &headers)?;
    let (page, page_size, offset) = normalize_page(q.page, q.page_size);
    let (rows, total) = storage::usage_rows_page(&state.db, None, page_size as i64, offset as i64)
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

/// GET /admin/usage/breakdown —— 用量聚合:按供应商/用户/密钥三个维度的 top10(含费用)。
pub async fn usage_breakdown(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Json<Value>, ApiError> {
    admin_guard(&state, &headers)?;
    let data = storage::usage_breakdown(&state.db)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    Ok(Json(data))
}

// ---- 奖励任务(后台配置)----

/// GET /admin/reward-tasks —— 奖励任务列表(含未启用)。
pub async fn list_reward_tasks(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Json<Value>, ApiError> {
    admin_guard(&state, &headers)?;
    let data = storage::list_reward_tasks(&state.db, false)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    Ok(Json(json!({ "data": data })))
}

#[derive(Deserialize)]
pub struct RewardTaskBody {
    pub title: String,
    #[serde(default)]
    pub description: Option<String>,
    pub evidence_type: String, // screenshot | link | text | none
    #[serde(default)]
    pub variable: bool,
    #[serde(default)]
    pub reward_tokens: i64,
    #[serde(default)]
    pub reward_min: i64,
    #[serde(default)]
    pub reward_max: i64,
    #[serde(default)]
    pub link_url: Option<String>,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub sort: i64,
}

fn default_true() -> bool {
    true
}

/// 校验并规整任务字段。
fn validate_reward_task(b: &RewardTaskBody) -> Result<(), ApiError> {
    if b.title.trim().is_empty() {
        return Err(ApiError::BadRequest("标题不能为空".into()));
    }
    if !matches!(b.evidence_type.as_str(), "screenshot" | "link" | "text" | "none") {
        return Err(ApiError::BadRequest("evidence_type 必须为 screenshot|link|text|none".into()));
    }
    if b.variable {
        if b.reward_min < 0 || b.reward_max < b.reward_min {
            return Err(ApiError::BadRequest("区间额度需满足 0 <= 下限 <= 上限".into()));
        }
    } else if b.reward_tokens <= 0 {
        return Err(ApiError::BadRequest("固定额度需大于 0".into()));
    }
    Ok(())
}

fn opt_trim(s: &Option<String>) -> Option<&str> {
    s.as_deref().map(str::trim).filter(|v| !v.is_empty())
}

/// POST /admin/reward-tasks —— 新建奖励任务。
pub async fn create_reward_task(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(body): Json<RewardTaskBody>,
) -> Result<Json<Value>, ApiError> {
    admin_guard(&state, &headers)?;
    validate_reward_task(&body)?;
    let id = storage::create_reward_task(
        &state.db,
        body.title.trim(),
        opt_trim(&body.description),
        &body.evidence_type,
        body.variable,
        body.reward_tokens,
        body.reward_min,
        body.reward_max,
        opt_trim(&body.link_url),
        body.enabled,
        body.sort,
    )
    .await
    .map_err(|e| ApiError::Internal(e.to_string()))?;
    Ok(Json(json!({ "id": id })))
}

/// PATCH /admin/reward-tasks/:id —— 更新奖励任务。
pub async fn update_reward_task(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(id): Path<i64>,
    Json(body): Json<RewardTaskBody>,
) -> Result<Json<Value>, ApiError> {
    admin_guard(&state, &headers)?;
    validate_reward_task(&body)?;
    storage::update_reward_task(
        &state.db,
        id,
        body.title.trim(),
        opt_trim(&body.description),
        &body.evidence_type,
        body.variable,
        body.reward_tokens,
        body.reward_min,
        body.reward_max,
        opt_trim(&body.link_url),
        body.enabled,
        body.sort,
    )
    .await
    .map_err(|e| ApiError::Internal(e.to_string()))?;
    Ok(Json(json!({ "ok": true })))
}

/// DELETE /admin/reward-tasks/:id —— 删除奖励任务(不影响历史申领记录)。
pub async fn delete_reward_task(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(id): Path<i64>,
) -> Result<Json<Value>, ApiError> {
    admin_guard(&state, &headers)?;
    storage::delete_reward_task(&state.db, id)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    Ok(Json(json!({ "ok": true })))
}

// ---- 奖励申领审核 ----

#[derive(Deserialize)]
pub struct RewardQuery {
    #[serde(default)]
    pub status: Option<String>, // pending | approved | rejected | all(默认全部)
    #[serde(default)]
    pub page: Option<u32>,
    #[serde(default)]
    pub page_size: Option<u32>,
}

/// GET /admin/rewards?status=&page=&page_size= —— 奖励申领列表(分页,保留 status 过滤)。
pub async fn list_rewards(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    axum::extract::Query(q): axum::extract::Query<RewardQuery>,
) -> Result<Json<Value>, ApiError> {
    admin_guard(&state, &headers)?;
    let status = match q.status.as_deref() {
        Some("pending") => Some(0),
        Some("approved") => Some(1),
        Some("rejected") => Some(2),
        _ => None,
    };
    let (page, page_size, offset) = normalize_page(q.page, q.page_size);
    let (data, total) = storage::list_reward_claims_page(&state.db, status, page_size as i64, offset as i64)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    Ok(Json(json!({
        "data": data,
        "page": page,
        "page_size": page_size,
        "total": total,
        "total_pages": total_pages(total, page_size),
    })))
}

#[derive(Deserialize)]
pub struct ReviewReward {
    pub approve: bool,
    #[serde(default)]
    pub note: Option<String>,
    /// 区间类任务通过时由管理员评定的入账额度(固定额度任务忽略此字段)。
    #[serde(default)]
    pub reward_tokens: Option<i64>,
}

/// POST /admin/rewards/:id/review —— 通过 / 驳回;通过则把奖励 token 入账(同步内存 + DB)。
pub async fn review_reward(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    Json(body): Json<ReviewReward>,
) -> Result<Json<Value>, ApiError> {
    admin_guard(&state, &headers)?;

    let claim = storage::get_reward_claim(&state.db, id)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?
        .ok_or_else(|| ApiError::BadRequest("申领不存在".into()))?;
    if claim.status != 0 {
        return Err(ApiError::BadRequest("该申领已审核,不能重复操作".into()));
    }

    // 计算通过时入账/记录的额度:区间类任务由管理员在 [min,max] 内评定;固定类用申领时记录的额度。
    let (credit, store_tokens) = if body.approve {
        if claim.variable {
            let (lo, hi) = (claim.reward_min, claim.reward_max);
            let amt = body.reward_tokens.ok_or_else(|| ApiError::BadRequest("请填写入账额度".into()))?;
            if amt < lo || amt > hi {
                return Err(ApiError::BadRequest(format!("入账额度需在 {lo} ~ {hi} 之间")));
            }
            (amt, Some(amt))
        } else {
            (claim.reward_tokens, None)
        }
    } else {
        (0, None)
    };

    let new_status = if body.approve { 1 } else { 2 };
    let note = body.note.as_deref().map(str::trim).filter(|s| !s.is_empty());
    let changed = storage::review_reward_claim(&state.db, id, new_status, note, store_tokens)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    // 并发下若已被他人审核,changed=0:不重复入账。
    if changed == 0 {
        return Err(ApiError::BadRequest("该申领已审核,不能重复操作".into()));
    }

    // 仅通过时入账。先写 DB,再同步内存余额。
    if body.approve {
        storage::add_tokens(&state.db, claim.user_id, credit)
            .await
            .map_err(|e| ApiError::Internal(e.to_string()))?;
        if let Some(u) = state.user(&claim.user_id) {
            u.token_balance.fetch_add(credit, Ordering::Relaxed);
        }
    }
    Ok(Json(json!({ "ok": true, "approved": body.approve, "reward_tokens": credit })))
}

/// GET /admin/config —— 供应商与路由(供管理端展示)。
// ---- 上游供应商(Provider) ----

/// GET /admin/providers —— 列出上游供应商及模型数量(分页)。
pub async fn list_providers(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    axum::extract::Query(q): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> Result<Json<Value>, ApiError> {
    admin_guard(&state, &headers)?;
    let page = q.get("page").and_then(|v| v.parse().ok());
    let page_size = q.get("page_size").and_then(|v| v.parse().ok());
    let (data, total) = storage::list_providers(&state.db, page, page_size)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    let ps = page_size.unwrap_or(20);
    let pg = page.unwrap_or(1);
    let total_pages = if total == 0 { 1 } else { ((total as f64) / (ps as f64)).ceil() as u64 };
    Ok(Json(json!({
        "data": data,
        "page": pg,
        "page_size": ps,
        "total": total,
        "total_pages": total_pages,
    })))
}

/// GET /admin/providers/:name/models —— 列出指定供应商下的所有模型。
pub async fn list_provider_models(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(name): Path<String>,
) -> Result<Json<Value>, ApiError> {
    admin_guard(&state, &headers)?;
    let data = storage::list_models_by_provider(&state.db, &name)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    Ok(Json(json!({ "data": data })))
}

/// DELETE /admin/providers/:name —— 级联删除供应商及其所有模型和路由。
pub async fn delete_provider(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(name): Path<String>,
) -> Result<Json<Value>, ApiError> {
    admin_guard(&state, &headers)?;
    storage::delete_provider_cascade(&state.db, &name)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    rebuild_routing(&state).await?;
    Ok(Json(json!({ "ok": true })))
}

/// PUT /admin/providers/:name —— 更新供应商 base_url 和 api_key。
#[derive(Deserialize)]
pub struct UpdateProvider {
    pub base_url: String,
    pub api_key: Option<String>,
}

pub async fn update_provider(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(name): Path<String>,
    Json(body): Json<UpdateProvider>,
) -> Result<Json<Value>, ApiError> {
    admin_guard(&state, &headers)?;
    if body.base_url.trim().is_empty() {
        return Err(ApiError::BadRequest("base_url required".into()));
    }
    storage::update_provider(&state.db, &name, body.base_url.trim(), body.api_key.as_deref())
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    rebuild_routing(&state).await?;
    Ok(Json(json!({ "ok": true })))
}

/// POST /admin/providers/exists —— 按 base_url + api_key 查询供应商是否已存在及其已有模型名。
#[derive(Deserialize)]
pub struct ProviderExists {
    pub base_url: String,
    pub api_key: Option<String>,
}

pub async fn provider_exists(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(body): Json<ProviderExists>,
) -> Result<Json<Value>, ApiError> {
    admin_guard(&state, &headers)?;
    let key = body.api_key.as_deref().filter(|k| !k.is_empty());
    let found = storage::find_provider_by_credentials(&state.db, body.base_url.trim(), key)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    match found {
        Some(name) => {
            let models = storage::list_model_names_by_provider(&state.db, &name)
                .await
                .map_err(|e| ApiError::Internal(e.to_string()))?;
            Ok(Json(json!({ "exists": true, "name": name, "models": models })))
        }
        None => Ok(Json(json!({ "exists": false }))),
    }
}

// ---- 模型(三方模型,自带上游连接)----

/// GET /admin/models
pub async fn list_models(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Json<Value>, ApiError> {
    admin_guard(&state, &headers)?;
    let data = storage::list_models(&state.db)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    Ok(Json(json!({ "data": data })))
}

#[derive(Deserialize)]
pub struct AddModel {
    pub label: Option<String>,  // 备注/显示名
    pub kind: String,           // openai | anthropic
    pub base_url: String,
    pub api_key: Option<String>,
    pub upstream_model: String, // 供应商上的真实模型名
    pub input_price: Option<f64>,  // 输入单价 $/1M tokens
    pub output_price: Option<f64>, // 输出单价 $/1M tokens
}

/// POST /admin/models —— 添加三方模型(自动建好它的上游连接)。
pub async fn add_model(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(body): Json<AddModel>,
) -> Result<Json<Value>, ApiError> {
    admin_guard(&state, &headers)?;
    if !matches!(body.kind.as_str(), "openai" | "anthropic") {
        return Err(ApiError::BadRequest("kind must be openai|anthropic".into()));
    }
    if body.base_url.trim().is_empty() || body.upstream_model.trim().is_empty() {
        return Err(ApiError::BadRequest("base_url and upstream_model required".into()));
    }
    if !price_ok(body.input_price) || !price_ok(body.output_price) {
        return Err(ApiError::BadRequest("prices must be finite and >= 0".into()));
    }
    // 按 base_url 复用已有 provider，不重复创建
    let (provider, _) = storage::find_or_create_provider(&state.db, &body.kind, &body.base_url, body.api_key.as_deref())
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    let id = storage::add_model(&state.db, &provider, &body.upstream_model, body.label.as_deref(), body.input_price, body.output_price)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    rebuild_routing(&state).await?;
    Ok(Json(json!({ "ok": true, "id": id })))
}

#[derive(Deserialize)]
pub struct AddModelsBatch {
    pub kind: String,
    pub base_url: String,
    pub api_key: Option<String>,
    pub models: Vec<AddModelBatchItem>,
}
#[derive(Deserialize)]
pub struct AddModelBatchItem {
    pub upstream_model: String,
    pub label: Option<String>,
    pub input_price: Option<f64>,  // 输入单价 $/1M tokens
    pub output_price: Option<f64>, // 输出单价 $/1M tokens
}

/// POST /admin/models/batch —— 批量添加同一供应商下的多个模型。
pub async fn add_models_batch(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(body): Json<AddModelsBatch>,
) -> Result<Json<Value>, ApiError> {
    admin_guard(&state, &headers)?;
    if !matches!(body.kind.as_str(), "openai" | "anthropic") {
        return Err(ApiError::BadRequest("kind must be openai|anthropic".into()));
    }
    if body.base_url.trim().is_empty() || body.models.is_empty() {
        return Err(ApiError::BadRequest("base_url and at least one model required".into()));
    }
    if body.models.iter().any(|m| !price_ok(m.input_price) || !price_ok(m.output_price)) {
        return Err(ApiError::BadRequest("prices must be finite and >= 0".into()));
    }
    let (provider, provider_created) = storage::find_or_create_provider(&state.db, &body.kind, &body.base_url, body.api_key.as_deref())
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    let mut added = 0usize;
    let mut skipped = 0usize;
    for m in &body.models {
        let (_, is_new) = storage::add_model_returning(&state.db, &provider, &m.upstream_model, m.label.as_deref(), m.input_price, m.output_price)
            .await
            .map_err(|e| ApiError::Internal(e.to_string()))?;
        if is_new {
            added += 1;
        } else {
            skipped += 1;
        }
    }
    rebuild_routing(&state).await?;
    Ok(Json(json!({ "ok": true, "provider": provider, "provider_created": provider_created, "added": added, "skipped": skipped })))
}

/// POST /admin/models/:id/test —— 向上游发最小请求校验连通性/密钥/模型名。
pub async fn test_model(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(id): Path<i64>,
) -> Result<Json<Value>, ApiError> {
    admin_guard(&state, &headers)?;
    let (kind, base_url, api_key, upstream) = {
        let routing = state.routing.load();
        let m = routing.models.get(&id).ok_or_else(|| ApiError::BadRequest("model not found".into()))?;
        let p = routing.providers.get(&m.provider).ok_or_else(|| ApiError::BadRequest("provider missing".into()))?;
        (p.kind, p.base_url.clone(), p.api_key.clone(), m.upstream_model.clone())
    };
    let key = api_key.filter(|k| !k.is_empty());
    let body = json!({ "model": upstream, "max_tokens": 1, "messages": [{ "role": "user", "content": "ping" }] });
    let started = std::time::Instant::now();
    let mut req = match kind {
        ProviderKind::Openai => {
            let mut rb = state.http.post(format!("{}/chat/completions", base_url.trim_end_matches('/')));
            if let Some(k) = &key { rb = rb.bearer_auth(k); }
            rb
        }
        ProviderKind::Anthropic => {
            let mut rb = state
                .http
                .post(format!("{}/messages", base_url.trim_end_matches('/')))
                .header("anthropic-version", "2023-06-01");
            if let Some(k) = &key { rb = rb.header("x-api-key", k); }
            rb
        }
    };
    req = req.json(&body);

    let resp = req
        .timeout(std::time::Duration::from_secs(20))
        .send()
        .await;
    let latency = started.elapsed().as_millis() as u64;

    match resp {
        Ok(r) if r.status().is_success() => Ok(Json(json!({ "ok": true, "latency_ms": latency }))),
        Ok(r) => {
            let code = r.status().as_u16();
            let body = r.text().await.unwrap_or_default();
            let snippet: String = body.chars().take(200).collect();
            Ok(Json(json!({ "ok": false, "error": format!("HTTP {code}: {snippet}") })))
        }
        Err(e) => Ok(Json(json!({ "ok": false, "error": e.to_string() }))),
    }
}

#[derive(Deserialize)]
pub struct FetchModelListReq {
    pub kind: String,
    pub base_url: String,
    pub api_key: Option<String>,
}

/// POST /admin/models/fetch-list —— 从上游拉取可用模型列表(OpenAI 兼容 /models 端点)。
pub async fn fetch_model_list(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(body): Json<FetchModelListReq>,
) -> Result<Json<Value>, ApiError> {
    admin_guard(&state, &headers)?;
    let base = body.base_url.trim().trim_end_matches('/');
    let url = format!("{}/models", base);
    let mut req = state.http.get(&url);
    if let Some(k) = body.api_key.as_deref().filter(|k| !k.is_empty()) {
        if body.kind == "anthropic" {
            req = req.header("x-api-key", k);
        } else {
            req = req.bearer_auth(k);
        }
    }
    let resp = req.timeout(std::time::Duration::from_secs(15)).send().await;
    match resp {
        Ok(r) if r.status().is_success() => {
            let val: Value = r.json().await.unwrap_or_default();
            // 兼容 OpenAI 格式: { "data": [{ "id": "gpt-4o", ... }] }
            let models = val.get("data").and_then(|d| d.as_array()).cloned().unwrap_or_default();
            let names: Vec<String> = models.iter().filter_map(|m| m.get("id").and_then(|v| v.as_str()).map(String::from)).collect();
            Ok(Json(json!({ "ok": true, "models": names })))
        }
        Ok(r) => {
            let code = r.status().as_u16();
            let text = r.text().await.unwrap_or_default();
            let snippet: String = text.chars().take(300).collect();
            Ok(Json(json!({ "ok": false, "error": format!("HTTP {code}: {snippet}") })))
        }
        Err(e) => Ok(Json(json!({ "ok": false, "error": e.to_string() }))),
    }
}

/// 价格字段三态:缺失=不改动 / null=清除定价 / 数字=设置单价($/1M tokens)。
fn deser_opt_f64<'de, D>(d: D) -> Result<Option<Option<f64>>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    Ok(Some(Option::<f64>::deserialize(d)?))
}

#[derive(Deserialize)]
pub struct UpdateModel {
    pub kind: String,
    pub base_url: String,
    pub api_key: Option<String>, // 留空=保持原密钥
    pub upstream_model: String,
    pub label: Option<String>,
    #[serde(default, deserialize_with = "deser_opt_f64")]
    pub input_price: Option<Option<f64>>,
    #[serde(default, deserialize_with = "deser_opt_f64")]
    pub output_price: Option<Option<f64>>,
}

/// PATCH /admin/models/:id —— 编辑模型。
pub async fn update_model(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(id): Path<i64>,
    Json(body): Json<UpdateModel>,
) -> Result<Json<Value>, ApiError> {
    admin_guard(&state, &headers)?;
    if !matches!(body.kind.as_str(), "openai" | "anthropic") {
        return Err(ApiError::BadRequest("kind must be openai|anthropic".into()));
    }
    if body.base_url.trim().is_empty() || body.upstream_model.trim().is_empty() {
        return Err(ApiError::BadRequest("base_url and upstream_model required".into()));
    }
    if !price_ok(body.input_price.flatten()) || !price_ok(body.output_price.flatten()) {
        return Err(ApiError::BadRequest("prices must be finite and >= 0".into()));
    }
    storage::update_model(&state.db, id, &body.kind, &body.base_url, body.api_key.as_deref(), &body.upstream_model, body.label.as_deref(), body.input_price, body.output_price)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    rebuild_routing(&state).await?;
    Ok(Json(json!({ "ok": true })))
}

/// DELETE /admin/models/:id
pub async fn delete_model(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(id): Path<i64>,
) -> Result<Json<Value>, ApiError> {
    admin_guard(&state, &headers)?;
    storage::delete_model_cascade(&state.db, id)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    rebuild_routing(&state).await?;
    Ok(Json(json!({ "ok": true })))
}

#[derive(Deserialize)]
pub struct BatchDeleteModels {
    pub ids: Vec<i64>,
}

/// POST /admin/models/batch-delete —— 批量删除模型(级联删除路由,被删空的供应商一并删除)。
pub async fn delete_models_batch(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(body): Json<BatchDeleteModels>,
) -> Result<Json<Value>, ApiError> {
    admin_guard(&state, &headers)?;
    if body.ids.is_empty() {
        return Err(ApiError::BadRequest("ids required".into()));
    }
    let mut deleted = 0usize;
    for id in &body.ids {
        if storage::delete_model_cascade(&state.db, *id)
            .await
            .map_err(|e| ApiError::Internal(e.to_string()))?
        {
            deleted += 1;
        }
    }
    rebuild_routing(&state).await?;
    Ok(Json(json!({ "ok": true, "deleted": deleted })))
}

// ---- 模型组 ----

/// GET /admin/groups
pub async fn list_groups(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Json<Value>, ApiError> {
    admin_guard(&state, &headers)?;
    let data = storage::list_groups(&state.db)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    Ok(Json(json!({ "data": data })))
}

#[derive(Deserialize)]
pub struct AddGroup {
    pub name: String,
}

/// POST /admin/groups
pub async fn add_group(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(body): Json<AddGroup>,
) -> Result<Json<Value>, ApiError> {
    admin_guard(&state, &headers)?;
    if body.name.trim().is_empty() {
        return Err(ApiError::BadRequest("name required".into()));
    }
    let id = storage::add_group(&state.db, body.name.trim())
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    rebuild_routing(&state).await?;
    Ok(Json(json!({ "ok": true, "id": id })))
}

/// POST /admin/groups/:id/activate —— 设为激活(默认)组,新用户注册默认绑定。
pub async fn activate_group(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(id): Path<i64>,
) -> Result<Json<Value>, ApiError> {
    admin_guard(&state, &headers)?;
    storage::set_active_group(&state.db, id)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    Ok(Json(json!({ "ok": true })))
}

#[derive(Deserialize)]
pub struct SetGroupStrategy {
    pub strategy: String,
}

/// POST /admin/groups/:id/strategy —— 设置该组的负载策略。
pub async fn set_group_strategy(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(id): Path<i64>,
    Json(body): Json<SetGroupStrategy>,
) -> Result<Json<Value>, ApiError> {
    admin_guard(&state, &headers)?;
    let strategy = crate::routing::strategy_from_str(&body.strategy);
    storage::set_group_strategy(&state.db, id, strategy.as_str())
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    rebuild_routing(&state).await?;
    Ok(Json(json!({ "ok": true, "strategy": strategy.as_str() })))
}

/// DELETE /admin/groups/:id
pub async fn delete_group(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(id): Path<i64>,
) -> Result<Json<Value>, ApiError> {
    admin_guard(&state, &headers)?;
    storage::delete_group(&state.db, id)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    rebuild_routing(&state).await?;
    Ok(Json(json!({ "ok": true })))
}

// ---- 高峰/低谷时段规则 ----

/// GET /admin/groups/:id/time-rules
pub async fn list_time_rules(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(id): Path<i64>,
) -> Result<Json<Value>, ApiError> {
    admin_guard(&state, &headers)?;
    let data = storage::list_time_rules(&state.db, id)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    Ok(Json(json!({ "data": data })))
}

#[derive(Deserialize)]
pub struct TimeRuleBody {
    pub name: String,
    #[serde(default = "time_rule_default_weekdays")]
    pub weekdays: String,
    pub start_time: String,
    pub end_time: String,
    #[serde(default = "time_rule_default_one")]
    pub multiplier: f64,
    #[serde(default)]
    pub weight_map: Option<String>,
    #[serde(default = "time_rule_default_true")]
    pub active: bool,
}

fn time_rule_default_weekdays() -> String { "0-6".to_string() }
fn time_rule_default_one() -> f64 { 1.0 }
fn time_rule_default_true() -> bool { true }

/// POST /admin/groups/:id/time-rules
pub async fn add_time_rule(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(id): Path<i64>,
    Json(body): Json<TimeRuleBody>,
) -> Result<Json<Value>, ApiError> {
    admin_guard(&state, &headers)?;
    let input = storage::TimeRuleInput {
        group_id: id,
        name: body.name.trim().to_string(),
        weekdays: body.weekdays,
        start_time: body.start_time,
        end_time: body.end_time,
        multiplier: body.multiplier,
        weight_map: body.weight_map,
        active: body.active,
    };
    if input.name.is_empty() {
        return Err(ApiError::BadRequest("name required".into()));
    }
    let nid = storage::add_time_rule(&state.db, &input)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    rebuild_routing(&state).await?;
    Ok(Json(json!({ "ok": true, "id": nid })))
}

/// PUT /admin/groups/:id/time-rules/:rule_id
pub async fn update_time_rule(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path((id, rule_id)): Path<(i64, i64)>,
    Json(body): Json<TimeRuleBody>,
) -> Result<Json<Value>, ApiError> {
    admin_guard(&state, &headers)?;
    let input = storage::TimeRuleInput {
        group_id: id,
        name: body.name.trim().to_string(),
        weekdays: body.weekdays,
        start_time: body.start_time,
        end_time: body.end_time,
        multiplier: body.multiplier,
        weight_map: body.weight_map,
        active: body.active,
    };
    if input.name.is_empty() {
        return Err(ApiError::BadRequest("name required".into()));
    }
    storage::update_time_rule(&state.db, rule_id, &input)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    rebuild_routing(&state).await?;
    Ok(Json(json!({ "ok": true })))
}

/// DELETE /admin/groups/:id/time-rules/:rule_id
pub async fn delete_time_rule(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path((_id, rule_id)): Path<(i64, i64)>,
) -> Result<Json<Value>, ApiError> {
    admin_guard(&state, &headers)?;
    storage::delete_time_rule(&state.db, rule_id)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    rebuild_routing(&state).await?;
    Ok(Json(json!({ "ok": true })))
}

// ---- 组内路由(对外模型名 -> 模型)----

/// GET /admin/groups/:id/routes
pub async fn list_routes(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(id): Path<i64>,
) -> Result<Json<Value>, ApiError> {
    admin_guard(&state, &headers)?;
    let data = storage::list_routes(&state.db, id)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    Ok(Json(json!({ "data": data })))
}

#[derive(Deserialize)]
pub struct AddRoute {
    pub public_name: String,
    pub model_id: i64,
    pub weight: Option<i64>,
    pub multiplier: Option<f64>,
}

/// POST /admin/groups/:id/routes —— 对外模型名 -> 指定模型。
pub async fn add_route(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(group_id): Path<i64>,
    Json(body): Json<AddRoute>,
) -> Result<Json<Value>, ApiError> {
    admin_guard(&state, &headers)?;
    if body.public_name.trim().is_empty() {
        return Err(ApiError::BadRequest("public_name required".into()));
    }
    let id = storage::add_route(
        &state.db,
        group_id,
        body.public_name.trim(),
        body.model_id,
        body.weight.unwrap_or(100),
        body.multiplier.unwrap_or(1.0),
    )
    .await
    .map_err(|e| ApiError::Internal(e.to_string()))?;
    rebuild_routing(&state).await?;
    Ok(Json(json!({ "ok": true, "id": id })))
}

#[derive(Deserialize)]
pub struct AddRoutesBatch {
    pub routes: Vec<AddRoute>,
}

/// POST /admin/groups/:id/routes/batch —— 批量添加路由。
pub async fn add_routes_batch(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(group_id): Path<i64>,
    Json(body): Json<AddRoutesBatch>,
) -> Result<Json<Value>, ApiError> {
    admin_guard(&state, &headers)?;
    let mut ids = Vec::new();
    for r in &body.routes {
        if r.public_name.trim().is_empty() { continue; }
        let id = storage::add_route(&state.db, group_id, r.public_name.trim(), r.model_id, r.weight.unwrap_or(100), r.multiplier.unwrap_or(1.0))
            .await
            .map_err(|e| ApiError::Internal(e.to_string()))?;
        ids.push(id);
    }
    rebuild_routing(&state).await?;
    Ok(Json(json!({ "ok": true, "ids": ids })))
}

/// PATCH /admin/routes/:id —— 编辑组内路由。
pub async fn update_route(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(id): Path<i64>,
    Json(body): Json<AddRoute>,
) -> Result<Json<Value>, ApiError> {
    admin_guard(&state, &headers)?;
    if body.public_name.trim().is_empty() {
        return Err(ApiError::BadRequest("public_name required".into()));
    }
    storage::update_route(
        &state.db,
        id,
        body.public_name.trim(),
        body.model_id,
        body.weight.unwrap_or(100),
        body.multiplier.unwrap_or(1.0),
    )
    .await
    .map_err(|e| ApiError::Internal(e.to_string()))?;
    rebuild_routing(&state).await?;
    Ok(Json(json!({ "ok": true })))
}

/// DELETE /admin/routes/:id
pub async fn delete_route(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(id): Path<i64>,
) -> Result<Json<Value>, ApiError> {
    admin_guard(&state, &headers)?;
    storage::delete_route(&state.db, id)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    rebuild_routing(&state).await?;
    Ok(Json(json!({ "ok": true })))
}

#[derive(Deserialize)]
pub struct BatchRoutesBody {
    pub ids: Vec<i64>,
    #[serde(default)]
    pub weight: Option<i64>,
    #[serde(default)]
    pub multiplier: Option<f64>,
}

/// POST /admin/routes/batch-delete —— 批量删除组内路由。
pub async fn batch_delete_routes(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(body): Json<BatchRoutesBody>,
) -> Result<Json<Value>, ApiError> {
    admin_guard(&state, &headers)?;
    for id in &body.ids {
        storage::delete_route(&state.db, *id)
            .await
            .map_err(|e| ApiError::Internal(e.to_string()))?;
    }
    rebuild_routing(&state).await?;
    Ok(Json(json!({ "ok": true, "deleted": body.ids.len() })))
}

/// POST /admin/routes/batch-update —— 批量改权重/倍率(至少给一个字段)。
pub async fn batch_update_routes(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(body): Json<BatchRoutesBody>,
) -> Result<Json<Value>, ApiError> {
    admin_guard(&state, &headers)?;
    if body.weight.is_none() && body.multiplier.is_none() {
        return Err(ApiError::BadRequest("weight or multiplier required".into()));
    }
    for id in &body.ids {
        storage::update_route_fields(&state.db, *id, body.weight, body.multiplier)
            .await
            .map_err(|e| ApiError::Internal(e.to_string()))?;
    }
    rebuild_routing(&state).await?;
    Ok(Json(json!({ "ok": true, "updated": body.ids.len() })))
}

// ---- 模型组导入/导出 ----

/// 组装单组导出 payload:组配置 + 路由(附 kind/base_url/upstream_model 三元组) + 时段规则,绝不含 api_key。
async fn group_export_payload(state: &AppState, group: &Value) -> Result<Value, ApiError> {
    let id = group.get("id").and_then(|v| v.as_i64()).unwrap_or(0);
    let routes = storage::list_routes_export(&state.db, id)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    let rules = storage::list_time_rules(&state.db, id)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    Ok(json!({
        "name": group.get("name").cloned().unwrap_or(json!("")),
        "strategy": group.get("strategy").cloned().unwrap_or(json!("weighted_random")),
        "routes": routes,
        "time_rules": rules.iter().map(|r| json!({
            "name": r.get("name"), "weekdays": r.get("weekdays"),
            "start_time": r.get("start_time"), "end_time": r.get("end_time"),
            "multiplier": r.get("multiplier"), "weight_map": r.get("weight_map"),
            "active": r.get("active"),
        })).collect::<Vec<_>>(),
    }))
}

/// GET /admin/groups/export —— 全量导出所有模型组。
pub async fn groups_export(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Json<Value>, ApiError> {
    admin_guard(&state, &headers)?;
    let groups = storage::list_groups(&state.db)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    let mut out = Vec::with_capacity(groups.len());
    for g in &groups {
        out.push(group_export_payload(&state, g).await?);
    }
    Ok(Json(json!({ "version": 1, "exported_at": storage::now_iso(), "groups": out })))
}

/// GET /admin/groups/:id/export —— 单组导出。
pub async fn group_export(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(id): Path<i64>,
) -> Result<Json<Value>, ApiError> {
    admin_guard(&state, &headers)?;
    let groups = storage::list_groups(&state.db)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    let g = groups
        .iter()
        .find(|g| g.get("id").and_then(|v| v.as_i64()) == Some(id))
        .ok_or_else(|| ApiError::BadRequest(format!("group {id} not found")))?;
    let payload = group_export_payload(&state, g).await?;
    Ok(Json(json!({ "version": 1, "exported_at": storage::now_iso(), "groups": [payload] })))
}

#[derive(Deserialize)]
pub struct ImportRoute {
    pub public_name: String,
    pub kind: String,
    pub base_url: String,
    pub upstream_model: String,
    #[serde(default = "import_default_weight")]
    pub weight: i64,
    #[serde(default = "import_default_multiplier")]
    pub multiplier: f64,
}

fn import_default_weight() -> i64 { 100 }
fn import_default_multiplier() -> f64 { 1.0 }

#[derive(Deserialize)]
pub struct ImportGroupBody {
    pub name: String,
    #[serde(default)]
    pub strategy: Option<String>,
    #[serde(default)]
    pub routes: Vec<ImportRoute>,
    #[serde(default)]
    pub time_rules: Vec<TimeRuleBody>,
}

#[derive(Deserialize)]
pub struct ImportBody {
    #[serde(default)]
    pub groups: Vec<ImportGroupBody>,
}

struct ResolvedGroup {
    name: String,
    strategy: String,
    routes: Vec<(String, i64, i64, f64)>,
    rules: Vec<storage::TimeRuleInput>,
    missing: Vec<Value>,
    action: &'static str,
    existing_routes: usize,
}

/// preview/import 共用:把 (kind, base_url, upstream_model) 解析为本库 model_id;
/// 找不到模型的路由跳过并记入 missing(不阻断整组导入),空组名直接报错。
async fn resolve_import(state: &AppState, body: &ImportBody) -> Result<Vec<ResolvedGroup>, ApiError> {
    let mut out = Vec::new();
    for g in &body.groups {
        let name = g.name.trim().to_string();
        if name.is_empty() {
            return Err(ApiError::BadRequest("group name required".into()));
        }
        let strategy = crate::routing::strategy_from_str(
            g.strategy.as_deref().unwrap_or("weighted_random"),
        )
        .as_str()
        .to_string();
        let mut routes = Vec::new();
        let mut missing = Vec::new();
        for r in &g.routes {
            let pn = r.public_name.trim().to_string();
            if pn.is_empty() { continue; }
            let model_id = storage::find_model_by_target(&state.db, &r.kind, &r.base_url, &r.upstream_model)
                .await
                .map_err(|e| ApiError::Internal(e.to_string()))?;
            match model_id {
                Some(mid) => routes.push((pn, mid, r.weight, r.multiplier)),
                None => missing.push(json!({
                    "public_name": pn, "kind": r.kind,
                    "base_url": r.base_url, "upstream_model": r.upstream_model,
                })),
            }
        }
        let rules = g.time_rules.iter().map(|t| storage::TimeRuleInput {
            group_id: 0,
            name: t.name.clone(),
            weekdays: t.weekdays.clone(),
            start_time: t.start_time.clone(),
            end_time: t.end_time.clone(),
            multiplier: t.multiplier,
            weight_map: t.weight_map.clone(),
            active: t.active,
        }).collect();
        let existing_id = storage::find_group_by_name(&state.db, &name)
            .await
            .map_err(|e| ApiError::Internal(e.to_string()))?;
        let (action, existing_routes) = match existing_id {
            Some(gid) => {
                let n = storage::list_routes(&state.db, gid)
                    .await
                    .map_err(|e| ApiError::Internal(e.to_string()))?
                    .len();
                ("overwrite", n)
            }
            None => ("create", 0),
        };
        out.push(ResolvedGroup {
            name, strategy, routes, rules, missing, action, existing_routes,
        });
    }
    Ok(out)
}

/// POST /admin/groups/import/preview —— 干跑:返回每组动作(新建/覆盖)、缺失模型与汇总,不落库。
pub async fn preview_import(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(body): Json<ImportBody>,
) -> Result<Json<Value>, ApiError> {
    admin_guard(&state, &headers)?;
    let resolved = resolve_import(&state, &body).await?;
    let (mut creates, mut overwrites, mut routes_n, mut rules_n, mut skipped) =
        (0usize, 0usize, 0usize, 0usize, 0usize);
    let mut data = Vec::with_capacity(resolved.len());
    for g in &resolved {
        creates += (g.action == "create") as usize;
        overwrites += (g.action == "overwrite") as usize;
        routes_n += g.routes.len();
        rules_n += g.rules.len();
        skipped += g.missing.len();
        data.push(json!({
            "name": g.name, "action": g.action, "existing_routes": g.existing_routes,
            "routes": g.routes.len(), "time_rules": g.rules.len(), "missing_models": g.missing,
        }));
    }
    Ok(Json(json!({
        "data": data,
        "summary": {
            "groups": resolved.len(), "create": creates, "overwrite": overwrites,
            "routes": routes_n, "time_rules": rules_n, "skipped_routes": skipped,
        },
    })))
}

/// POST /admin/groups/import —— 执行导入(按组名 upsert,同名整组覆盖)。
pub async fn groups_import(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(body): Json<ImportBody>,
) -> Result<Json<Value>, ApiError> {
    admin_guard(&state, &headers)?;
    let resolved = resolve_import(&state, &body).await?;
    let mut imported = Vec::with_capacity(resolved.len());
    for g in &resolved {
        let (id, overwritten) = storage::import_group(&state.db, &g.name, &g.strategy, &g.routes, &g.rules)
            .await
            .map_err(|e| ApiError::Internal(e.to_string()))?;
        imported.push(json!({
            "id": id, "name": g.name,
            "action": if overwritten { "overwrite" } else { "create" },
            "routes": g.routes.len(), "time_rules": g.rules.len(), "skipped_routes": g.missing.len(),
        }));
    }
    rebuild_routing(&state).await?;
    Ok(Json(json!({ "ok": true, "imported": imported })))
}

// ---- 系统配置:failover ----

/// GET /admin/settings/fallback -- 回显 failover 配置(DB 优先,否则配置文件值)。
pub async fn get_fallback_settings(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Json<Value>, ApiError> {
    admin_guard(&state, &headers)?;
    let kv = storage::load_settings(&state.db, crate::settings::FALLBACK_PREFIX)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    let cfg = state.config();
    let fallback_enabled = kv
        .get(crate::settings::K_FB_ENABLED)
        .and_then(|v| v.parse::<bool>().ok())
        .unwrap_or(cfg.defaults.fallback_enabled);
    let max_retries = kv
        .get(crate::settings::K_FB_MAX_RETRIES)
        .and_then(|v| v.parse::<u32>().ok())
        .unwrap_or(cfg.defaults.max_retries);
    Ok(Json(json!({
        "fallback_enabled": fallback_enabled,
        "max_retries": max_retries,
    })))
}

#[derive(Deserialize)]
pub struct FallbackSettingsBody {
    /// failover 总开关:关闭后仅尝试首个候选。
    pub fallback_enabled: bool,
    /// 最多尝试的候选数(含首个;0 = 不限)。
    #[serde(default)]
    pub max_retries: u32,
}

/// 校验 max_retries 合理范围(防误填超大值打挂上游)。
fn validate_fallback(b: &FallbackSettingsBody) -> Result<(), ApiError> {
    if b.max_retries > 100 {
        return Err(ApiError::BadRequest("max_retries 不能超过 100".into()));
    }
    Ok(())
}

/// POST /admin/settings/fallback -- 保存 failover 配置,刷新内存 Config。
pub async fn save_fallback_settings(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(body): Json<FallbackSettingsBody>,
) -> Result<Json<Value>, ApiError> {
    admin_guard(&state, &headers)?;
    validate_fallback(&body)?;

    let items: Vec<(String, String)> = vec![
        (crate::settings::K_FB_ENABLED.to_string(), body.fallback_enabled.to_string()),
        (crate::settings::K_FB_MAX_RETRIES.to_string(), body.max_retries.to_string()),
    ];
    storage::set_settings(&state.db, &items)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    // 重建内存 Config:从 DB 重读 fallback.* 覆盖当前快照,store() 刷新。
    let kv = storage::load_settings(&state.db, crate::settings::FALLBACK_PREFIX)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    let mut new_cfg = (**state.config()).clone();
    crate::settings::apply_fallback_settings(&mut new_cfg, &kv);
    state.config.store(Arc::new(new_cfg));
    Ok(Json(json!({ "ok": true })))
}

// ---- 系统配置:语义缓存 ----

/// GET /admin/settings/cache -- 回显语义缓存配置(DB 优先,否则配置文件值)。
pub async fn get_cache_settings(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Json<Value>, ApiError> {
    admin_guard(&state, &headers)?;
    let kv = storage::load_settings(&state.db, crate::settings::CACHE_PREFIX)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    let cfg = state.config();
    let cs = &cfg.cache_semantic;
    let enabled = kv
        .get(crate::settings::K_CACHE_ENABLED)
        .and_then(|v| v.parse::<bool>().ok())
        .unwrap_or(cs.enabled);
    let ttl_secs = kv
        .get(crate::settings::K_CACHE_TTL)
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(cs.ttl_secs);
    let similarity_threshold = kv
        .get(crate::settings::K_CACHE_THRESHOLD)
        .and_then(|v| v.parse::<f64>().ok())
        .unwrap_or(cs.similarity_threshold);
    let multi_turn_max = kv
        .get(crate::settings::K_CACHE_MULTI_TURN)
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(cs.multi_turn_max);
    Ok(Json(json!({
        "enabled": enabled,
        "ttl_secs": ttl_secs,
        "similarity_threshold": similarity_threshold,
        "multi_turn_max": multi_turn_max,
    })))
}

#[derive(Deserialize)]
pub struct CacheSettingsBody {
    /// 缓存总开关:关闭后不命中也不写入。
    pub enabled: bool,
    /// 条目 TTL(秒)。
    pub ttl_secs: u64,
    /// 语义相似度阈值(L2 embedding 预留,L1 仅存储与展示)。
    pub similarity_threshold: f64,
    /// 消息条数超过该值的多轮对话跳过缓存。
    pub multi_turn_max: usize,
}

/// 校验缓存配置合理范围。
fn validate_cache(b: &CacheSettingsBody) -> Result<(), ApiError> {
    if b.ttl_secs == 0 || b.ttl_secs > 604_800 {
        return Err(ApiError::BadRequest("TTL 需在 1~604800 秒(7 天)之间".into()));
    }
    if !(0.5..=0.95).contains(&b.similarity_threshold) {
        return Err(ApiError::BadRequest("相似度阈值需在 0.5~0.95 之间".into()));
    }
    if b.multi_turn_max == 0 || b.multi_turn_max > 100 {
        return Err(ApiError::BadRequest("多轮阈值需在 1~100 之间".into()));
    }
    Ok(())
}

/// POST /admin/settings/cache -- 保存语义缓存配置,刷新内存 Config。
pub async fn save_cache_settings(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(body): Json<CacheSettingsBody>,
) -> Result<Json<Value>, ApiError> {
    admin_guard(&state, &headers)?;
    validate_cache(&body)?;

    let items: Vec<(String, String)> = vec![
        (crate::settings::K_CACHE_ENABLED.to_string(), body.enabled.to_string()),
        (crate::settings::K_CACHE_TTL.to_string(), body.ttl_secs.to_string()),
        (crate::settings::K_CACHE_THRESHOLD.to_string(), body.similarity_threshold.to_string()),
        (crate::settings::K_CACHE_MULTI_TURN.to_string(), body.multi_turn_max.to_string()),
    ];
    storage::set_settings(&state.db, &items)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    // 重建内存 Config:从 DB 重读 cache.* 覆盖当前快照,store() 刷新。
    let kv = storage::load_settings(&state.db, crate::settings::CACHE_PREFIX)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    let mut new_cfg = (**state.config()).clone();
    crate::settings::apply_cache_settings(&mut new_cfg, &kv);
    state.config.store(Arc::new(new_cfg));
    Ok(Json(json!({ "ok": true })))
}

/// GET /admin/cache/stats -- 缓存统计快照(命中率/节省 tokens/条数/趋势)。
pub async fn cache_stats(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Json<Value>, ApiError> {
    admin_guard(&state, &headers)?;
    Ok(Json(state.semantic_cache.stats_snapshot()))
}

#[derive(Deserialize)]
pub struct CacheHitsQuery {
    pub limit: Option<u32>,
}

/// GET /admin/cache/hits?limit=50 -- 最近命中记录(新→旧)。
pub async fn cache_hits(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    axum::extract::Query(q): axum::extract::Query<CacheHitsQuery>,
) -> Result<Json<Value>, ApiError> {
    admin_guard(&state, &headers)?;
    let limit = q.limit.unwrap_or(50).clamp(1, 200) as usize;
    let hits = state.semantic_cache.recent_hits(limit);
    Ok(Json(json!({ "hits": hits })))
}

/// POST /admin/cache/clear -- 清空缓存与统计(含 L2 向量表)。
pub async fn cache_clear(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Json<Value>, ApiError> {
    admin_guard(&state, &headers)?;
    state.semantic_cache.clear();
    // L2 语义向量与缓存条目同生命周期,一并清理。
    if let Err(e) = crate::storage::clear_cache_vectors(&state.db).await {
        tracing::warn!("清理 L2 向量失败(不影响缓存清空): {e}");
    }
    Ok(Json(json!({ "ok": true })))
}

// ---- 系统配置:embedding(L2 语义缓存供应商) ----

/// GET /admin/settings/embedding -- 回显 embedding 配置(api_key 不回显,仅返回 has_key)。
pub async fn get_embedding_settings(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Json<Value>, ApiError> {
    admin_guard(&state, &headers)?;
    let kv = storage::load_settings(&state.db, crate::settings::EMB_PREFIX)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    let cfg = state.config();
    let ec = &cfg.embedding;
    // DB 有则取 DB,否则取配置文件值(回显当前生效配置)。
    let enabled = kv
        .get(crate::settings::K_EMB_ENABLED)
        .and_then(|v| v.parse::<bool>().ok())
        .unwrap_or(ec.enabled);
    let base_url = kv
        .get(crate::settings::K_EMB_BASE_URL)
        .cloned()
        .unwrap_or_else(|| ec.base_url.clone());
    let model = kv
        .get(crate::settings::K_EMB_MODEL)
        .cloned()
        .unwrap_or_else(|| ec.model.clone());
    let has_key = crate::settings::has_embedding_key(&kv) || !ec.api_key.is_empty();
    Ok(Json(json!({
        "enabled": enabled,
        "base_url": base_url,
        "model": model,
        "has_key": has_key,
    })))
}

#[derive(Deserialize)]
pub struct EmbeddingSettingsBody {
    /// L2 语义层总开关。
    pub enabled: bool,
    /// OpenAI 兼容接口基础地址,如 https://api.openai.com/v1。
    #[serde(default)]
    pub base_url: String,
    #[serde(default)]
    pub model: String,
    /// api_key;留空=保持不变。
    #[serde(default)]
    pub api_key: Option<String>,
}

/// 校验 embedding 配置:开启时 base_url 与 model 必填,base_url 需为 http(s)。
fn validate_embedding(b: &EmbeddingSettingsBody) -> Result<(), ApiError> {
    if b.enabled {
        if b.base_url.trim().is_empty() {
            return Err(ApiError::BadRequest("开启语义缓存需填写 base_url".into()));
        }
        if b.model.trim().is_empty() {
            return Err(ApiError::BadRequest("开启语义缓存需填写 embedding 模型名".into()));
        }
    }
    if !b.base_url.trim().is_empty()
        && !b.base_url.trim().starts_with("http://")
        && !b.base_url.trim().starts_with("https://")
    {
        return Err(ApiError::BadRequest("base_url 需以 http:// 或 https:// 开头".into()));
    }
    Ok(())
}

/// POST /admin/settings/embedding -- 保存 embedding 配置(api_key 加密入库),刷新内存 Config。
pub async fn save_embedding_settings(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(body): Json<EmbeddingSettingsBody>,
) -> Result<Json<Value>, ApiError> {
    admin_guard(&state, &headers)?;
    validate_embedding(&body)?;

    let secret = state.config().auth.jwt_secret.clone();
    let mut items: Vec<(String, String)> = vec![
        (crate::settings::K_EMB_ENABLED.to_string(), body.enabled.to_string()),
        (
            crate::settings::K_EMB_BASE_URL.to_string(),
            body.base_url.trim().trim_end_matches('/').to_string(),
        ),
        (crate::settings::K_EMB_MODEL.to_string(), body.model.trim().to_string()),
    ];
    // api_key 非空才写(加密);留空=保持原值。
    if let Some(k) = body.api_key.as_deref().filter(|s| !s.is_empty()) {
        let enc = crate::settings::encrypt(&secret, k)
            .map_err(|e| ApiError::Internal(format!("加密失败: {e}")))?;
        items.push((crate::settings::K_EMB_API_KEY_ENC.to_string(), enc));
    }

    storage::set_settings(&state.db, &items)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    // 重建内存 Config:从 DB 重读 embedding.* 覆盖当前快照,store() 刷新。
    let kv = storage::load_settings(&state.db, crate::settings::EMB_PREFIX)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    let mut new_cfg = (**state.config()).clone();
    crate::settings::apply_embedding_settings(&mut new_cfg, &kv, &secret);
    state.config.store(Arc::new(new_cfg));
    Ok(Json(json!({ "ok": true })))
}

#[derive(Deserialize)]
pub struct TestEmbeddingBody {
    #[serde(default)]
    pub base_url: String,
    #[serde(default)]
    pub model: String,
    /// api_key;留空时若库里有则用库里的(支持「不改 key 只测当前配置」)。
    #[serde(default)]
    pub api_key: Option<String>,
    /// 测试文本;默认用一句中文验证中文向量化。
    #[serde(default)]
    pub test_text: Option<String>,
}

/// POST /admin/settings/embedding/test -- 用表单配置调一次 embeddings 接口,不落库。
pub async fn test_embedding(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(body): Json<TestEmbeddingBody>,
) -> Result<Json<Value>, ApiError> {
    admin_guard(&state, &headers)?;
    if body.base_url.trim().is_empty() {
        return Err(ApiError::BadRequest("base_url 不能为空".into()));
    }
    if body.model.trim().is_empty() {
        return Err(ApiError::BadRequest("model 不能为空".into()));
    }

    // api_key 留空时回退库里已存的(解密),便于「不改 key 只测连通」。
    let api_key = match body.api_key.as_deref().filter(|s| !s.is_empty()) {
        Some(k) => k.to_string(),
        None => {
            let kv = storage::load_settings(&state.db, crate::settings::EMB_PREFIX)
                .await
                .map_err(|e| ApiError::Internal(e.to_string()))?;
            let secret = state.config().auth.jwt_secret.clone();
            kv.get(crate::settings::K_EMB_API_KEY_ENC)
                .and_then(|enc| crate::settings::decrypt(&secret, enc).ok())
                .unwrap_or_default()
        }
    };

    let text = body
        .test_text
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or("这是一条语义缓存连通性测试文本");
    let base_url = body.base_url.trim().trim_end_matches('/');
    match crate::embedding::embed(&state.http, base_url, &api_key, body.model.trim(), text).await {
        Ok(vec) => Ok(Json(json!({ "ok": true, "dim": vec.len() }))),
        Err(e) => Ok(Json(json!({ "ok": false, "error": e.to_string() }))),
    }
}

// ---- 系统配置:邮箱 ----

/// GET /admin/settings/email -- 回显邮箱配置(授权码不回显,仅返回 has_password)。
pub async fn get_email_settings(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Json<Value>, ApiError> {
    admin_guard(&state, &headers)?;
    let kv = storage::load_settings(&state.db, crate::settings::EMAIL_PREFIX)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    let cfg = state.config();
    // DB 有则取 DB,否则取配置文件值(回显当前生效配置)。
    let host = kv.get(crate::settings::K_HOST).cloned().unwrap_or(cfg.email.smtp_host.clone());
    let port = kv
        .get(crate::settings::K_PORT)
        .and_then(|v| v.parse::<u16>().ok())
        .unwrap_or(cfg.email.smtp_port);
    let username = kv.get(crate::settings::K_USER).cloned().unwrap_or(cfg.email.username.clone());
    let from = kv.get(crate::settings::K_FROM).cloned().unwrap_or(cfg.email.from.clone());
    let has_password = crate::settings::has_email_password(&kv);
    Ok(Json(json!({
        "smtp_host": host,
        "smtp_port": port,
        "username": username,
        "from": from,
        "has_password": has_password,
        "enabled": !host.is_empty(),
    })))
}

#[derive(Deserialize)]
pub struct EmailSettingsBody {
    pub smtp_host: String,
    pub smtp_port: u16,
    #[serde(default)]
    pub username: String,
    /// 授权码;留空=保持不变。
    #[serde(default)]
    pub password: Option<String>,
    #[serde(default)]
    pub from: String,
}

/// 校验端口合法。
fn validate_email(b: &EmailSettingsBody) -> Result<(), ApiError> {
    if b.smtp_port == 0 {
        return Err(ApiError::BadRequest("smtp_port 不能为 0".into()));
    }
    // host 允许为空(=关闭 SMTP,走 dev 模式);非空时不额外校验格式。
    Ok(())
}

/// POST /admin/settings/email -- 保存邮箱配置(授权码加密入库),刷新内存 Config。
pub async fn save_email_settings(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(body): Json<EmailSettingsBody>,
) -> Result<Json<Value>, ApiError> {
    admin_guard(&state, &headers)?;
    validate_email(&body)?;

    let secret = state.config().auth.jwt_secret.clone();
    let mut items: Vec<(String, String)> = vec![
        (crate::settings::K_HOST.to_string(), body.smtp_host.clone()),
        (crate::settings::K_PORT.to_string(), body.smtp_port.to_string()),
        (crate::settings::K_USER.to_string(), body.username.clone()),
        (crate::settings::K_FROM.to_string(), body.from.clone()),
    ];
    // 密码非空才写(加密);留空=保持原值。
    if let Some(pw) = body.password.as_deref().filter(|s| !s.is_empty()) {
        let enc = crate::settings::encrypt(&secret, pw)
            .map_err(|e| ApiError::Internal(format!("加密失败: {e}")))?;
        items.push((crate::settings::K_PASS_ENC.to_string(), enc));
    }

    storage::set_settings(&state.db, &items)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    // 重建内存 Config:从 DB 重读 email.* 覆盖当前快照,store() 刷新。
    let kv = storage::load_settings(&state.db, crate::settings::EMAIL_PREFIX)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    let mut new_cfg = (**state.config()).clone();
    crate::settings::apply_email_settings(&mut new_cfg, &kv, &secret);
    state.config.store(Arc::new(new_cfg));
    Ok(Json(json!({ "ok": true })))
}

#[derive(Deserialize)]
pub struct TestEmailBody {
    pub smtp_host: String,
    pub smtp_port: u16,
    #[serde(default)]
    pub username: String,
    /// 授权码;留空时若库里有则用库里的(支持「不改密码只测当前配置」)。
    #[serde(default)]
    pub password: Option<String>,
    #[serde(default)]
    pub from: String,
    pub test_to: String,
}

/// POST /admin/settings/email/test -- 用表单配置发一封测试邮件,不落库。
pub async fn test_email_settings(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(body): Json<TestEmailBody>,
) -> Result<Json<Value>, ApiError> {
    admin_guard(&state, &headers)?;
    if body.test_to.trim().is_empty() || !body.test_to.contains('@') {
        return Err(ApiError::BadRequest("test_to 邮箱格式不正确".into()));
    }
    if body.smtp_host.trim().is_empty() {
        return Err(ApiError::BadRequest("smtp_host 不能为空".into()));
    }

    let secret = state.config().auth.jwt_secret.clone();
    let kv = storage::load_settings(&state.db, crate::settings::EMAIL_PREFIX)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    let base = state.config().email.clone();
    let email_cfg = crate::settings::email_config_from_kv(
        &base,
        &kv,
        &secret,
        body.password.as_deref(),
    );
    // 表单字段覆盖(测试用请求体里的最新值,而非库里)。
    let email_cfg = EmailConfig {
        smtp_host: body.smtp_host.clone(),
        smtp_port: body.smtp_port,
        username: body.username.clone(),
        from: body.from.clone(),
        password: email_cfg.password,
    };

    let code: String = {
        use rand::Rng;
        format!("{:06}", rand::thread_rng().gen_range(0..1_000_000))
    };
    match crate::email::send_code(&email_cfg, body.test_to.trim(), &code).await {
        Ok(()) => Ok(Json(json!({ "ok": true }))),
        Err(e) => Ok(Json(json!({ "ok": false, "error": e.to_string() }))),
    }
}

// ============================================================
// 供应商健康总览(模型页徽标数据源)
// ============================================================

#[derive(Deserialize)]
pub struct HealthQuery {
    #[serde(default)]
    pub range_secs: Option<u64>,
}

/// GET /admin/providers/health?range_secs=3600 —— 供应商健康状态(内存指标+熔断快照,重启后从零累积)。
pub async fn providers_health(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    axum::extract::Query(q): axum::extract::Query<HealthQuery>,
) -> Result<Json<Value>, ApiError> {
    admin_guard(&state, &headers)?;
    Ok(Json(state.provider_health(q.range_secs).await))
}

// ============================================================
// 上游治理仪表盘(熔断器状态 + 并发槽占用)
// ============================================================

/// GET /admin/upstreams —— 上游运行时状态(熔断器 + 并发槽占用率)。
/// 熔断中的上游会排在最前,方便管理后台一眼看出故障点。
pub async fn list_upstreams(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Json<Value>, ApiError> {
    admin_guard(&state, &headers)?;
    Ok(Json(state.list_upstream_status().await))
}

/// POST /admin/upstreams/reset —— 手动清除 **所有** 上游的熔断器状态。
/// 场景:人工修复了上游故障后,不想等 30s 自动恢复。
pub async fn reset_all_breakers(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Json<Value>, ApiError> {
    admin_guard(&state, &headers)?;
    let mut cleared = 0u64;
    // 遍历所有已有熔断器,清零
    let keys: Vec<crate::state::UpstreamKey> = state
        .upstream_breakers
        .iter()
        .map(|e| e.key().clone())
        .collect();
    for k in &keys {
        if let Some(br) = state.upstream_breakers.get(k) {
            br.value().lock().await.record_success();
            cleared += 1;
        }
    }
    Ok(Json(json!({ "ok": true, "cleared": cleared })))
}

/// POST /admin/upstreams/reset/:id —— 手动清除 **单个** 上游的熔断器状态。
/// `:id` = 「kind|base_url|key_fingerprint」三段竖线分隔,其中 key_fingerprint 可以为空。
/// 之所以不直接用索引,是因为管理后台可以直接用前端拿到的 items[i] 拼这个 id。
pub async fn reset_one_breaker(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    admin_guard(&state, &headers)?;
    // id 格式: kind|base_url|fp  (fp 可以为空)
    let parts: Vec<&str> = id.splitn(3, '|').collect();
    let (Some(kind_s), Some(base_url), fp) = (parts.get(0), parts.get(1), parts.get(2)) else {
        return Err(ApiError::BadRequest("id 格式: kind|base_url|key_fingerprint".into()));
    };
    let kind = match kind_s.to_lowercase().as_str() {
        "openai" => ProviderKind::Openai,
        "anthropic" => ProviderKind::Anthropic,
        _ => return Err(ApiError::BadRequest(format!("未知 kind:{}", kind_s))),
    };
    let fp = fp.copied().unwrap_or("");
    let key = crate::state::UpstreamKey::from_fingerprint(kind, base_url.to_string(), fp);
    let matched = match state.upstream_breakers.get(&key) {
        Some(br) => {
            br.value().lock().await.record_success();
            true
        }
        None => false,
    };
    Ok(Json(json!({ "ok": true, "matched": matched })))
}

#[derive(Deserialize)]
pub struct AuditQuery {
    #[serde(default)]
    pub limit: Option<usize>,
}

/// GET /admin/api/audit/failures?limit=50 —— 最近的路由失败审计(ring buffer,倒序输出)。
/// 默认 50 条,最大 200 条(后端强制 clamp)。
pub async fn list_failure_audit(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    axum::extract::Query(q): axum::extract::Query<AuditQuery>,
) -> Result<Json<Value>, ApiError> {
    admin_guard(&state, &headers)?;
    Ok(Json(state.list_failures(q.limit).await))
}

#[derive(Deserialize)]
pub struct RequestLogsQuery {
    /// 检索关键字(request_id / 模型名)。空 = 最近全部。
    #[serde(default)]
    pub q: Option<String>,
    #[serde(default)]
    pub page: Option<u32>,
    /// 每页条数,默认 50,最大 200。
    #[serde(default)]
    pub page_size: Option<u32>,
}

/// GET /admin/api/request-logs?q=&page=&page_size= —— 最近请求链路日志(倒序,分页)。
/// 记录每次请求的候选顺序、权重、实际选中、failover 链与 tokens。
pub async fn list_request_logs(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    axum::extract::Query(q): axum::extract::Query<RequestLogsQuery>,
) -> Result<Json<Value>, ApiError> {
    admin_guard(&state, &headers)?;
    let keyword = q.q.as_deref().unwrap_or("");
    let page = q.page.unwrap_or(1).max(1);
    let page_size = q.page_size.unwrap_or(50).clamp(1, 200);
    let offset = (page - 1) * page_size;
    let logs = state
        .request_log
        .recent(keyword, page_size, offset)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    let total = state
        .request_log
        .count(keyword)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    Ok(Json(json!({
        "data": logs,
        "page": page,
        "page_size": page_size,
        "total": total,
        "total_pages": total_pages(total, page_size),
    })))
}

// ======================== 接口指标仪表盘 ========================

#[derive(Deserialize)]
pub struct MetricsQuery {
    /// 时间窗口秒数:0 = 全量(最多 48h 分钟桶)。默认 3600 = 最近 1 小时。
    /// 可选值:300 / 900 / 1800 / 3600 / 14400 / 43200 / 86400 / 0
    #[serde(default)]
    pub range_secs: Option<u64>,
    /// 上游排行榜 TopN,默认 20。
    #[serde(default)]
    pub top_n: Option<usize>,
}

/// GET /admin/api/metrics?range_secs=3600&top_n=20 —— 接口指标汇总(大盘 + TopN 上游 + 时序)。
/// 一次请求返回所有 dashboard 数据,减少前端往返。
pub async fn metrics_dashboard(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    axum::extract::Query(q): axum::extract::Query<MetricsQuery>,
) -> Result<Json<Value>, ApiError> {
    admin_guard(&state, &headers)?;
    Ok(Json(
        state
            .query_metrics(q.range_secs, q.top_n)
            .await,
    ))
}

/// GET /admin/api/metrics/overview?range_secs=3600 —— 只取汇总(KPI + top upstream 简要)。
pub async fn metrics_overview(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    axum::extract::Query(q): axum::extract::Query<MetricsQuery>,
) -> Result<Json<Value>, ApiError> {
    admin_guard(&state, &headers)?;
    let full = state.query_metrics(q.range_secs, q.top_n).await;
    // 去掉 series,前端首次刷盘时用
    let mut out = full;
    if let Some(obj) = out.as_object_mut() {
        obj.remove("series");
        obj.remove("upstream_series");
    }
    Ok(Json(out))
}

/// GET /metrics —— Prometheus exposition 文本导出(内存滚动 48h 窗口)。
/// 鉴权:管理端 JWT(与 /admin/api 一致),或 `Bearer <metrics.export_token>`(供 Prometheus 抓取器使用;未配置该令牌则仅 JWT 可访问)。
pub async fn metrics_export(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<axum::response::Response, ApiError> {
    if admin_guard(&state, &headers).is_err() {
        let token_ok = {
            let cfg = state.config();
            let want = cfg.metrics.export_token.clone();
            !want.is_empty()
                && headers
                    .get(axum::http::header::AUTHORIZATION)
                    .and_then(|v| v.to_str().ok())
                    .and_then(|s| s.strip_prefix("Bearer "))
                    .map(|t| t == want)
                    .unwrap_or(false)
        };
        if !token_ok {
            return Err(ApiError::Unauthorized);
        }
    }
    let snap = state.export_snapshot().await;
    let body = render_prometheus(&snap);
    Ok((
        [(axum::http::header::CONTENT_TYPE, "text/plain; version=0.0.4; charset=utf-8")],
        body,
    )
        .into_response())
}

/// Prometheus label 值转义(反斜杠 / 双引号 / 换行)。
fn escape_label(v: &str) -> String {
    v.replace('\\', "\\\\").replace('"', "\\\"").replace('\n', "\\n")
}

/// 数值输出:非有限数(如 NaN/缺失)统一输出 0,保证 exposition 格式合法。
fn prom_num(v: Option<f64>) -> String {
    match v {
        Some(x) if x.is_finite() => format!("{x}"),
        _ => "0".to_string(),
    }
}

/// 把 export_snapshot 渲染为 Prometheus exposition 文本(手写,零新依赖)。
fn render_prometheus(snap: &Value) -> String {
    let mut out = String::with_capacity(4096);
    let f64_at = |v: &Value, k: &str| -> Option<f64> { v.get(k).and_then(|x| x.as_f64()) };
    let window = snap.get("window_secs").and_then(|v| v.as_u64()).unwrap_or(0);
    let empty = json!({});
    let g = snap.get("global").unwrap_or(&empty);
    let providers = snap
        .get("providers")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();

    // ---- 窗口说明 ----
    out.push_str("# HELP relay_metrics_window_secs 指标聚合窗口长度(秒),内存滚动窗口,重启后从零累积。\n");
    out.push_str("# TYPE relay_metrics_window_secs gauge\n");
    out.push_str(&format!("relay_metrics_window_secs {window}\n"));

    // ---- 全局请求(按结果分类)----
    out.push_str("# HELP relay_upstream_requests_total 数据面请求数(按范围与结果分类;窗口滑动导致数值非单调)。\n");
    out.push_str("# TYPE relay_upstream_requests_total counter\n");
    for status in ["success", "fail_unavailable", "fail_other"] {
        out.push_str(&format!(
            "relay_upstream_requests_total{{scope=\"global\",status=\"{status}\"}} {}\n",
            prom_num(f64_at(g, status))
        ));
    }

    // ---- per-provider 请求 ----
    for p in &providers {
        let name = p.get("provider").and_then(|v| v.as_str()).unwrap_or("");
        if name.is_empty() {
            continue;
        }
        let lb = format!("provider=\"{}\"", escape_label(name));
        for status in ["success", "fail_unavailable", "fail_other"] {
            out.push_str(&format!(
                "relay_upstream_requests_total{{{lb},status=\"{status}\"}} {}\n",
                prom_num(f64_at(p, status))
            ));
        }
    }

    // ---- per-provider 延迟(summary:分位 + sum/count,sum 由 avg×count 反推)----
    out.push_str("# HELP relay_upstream_request_duration_ms 上游成功请求延迟(毫秒)。\n");
    out.push_str("# TYPE relay_upstream_request_duration_ms summary\n");
    for p in &providers {
        let name = p.get("provider").and_then(|v| v.as_str()).unwrap_or("");
        if name.is_empty() {
            continue;
        }
        let lb = format!("provider=\"{}\"", escape_label(name));
        for (q, key) in [("0.95", "p95_ms"), ("0.99", "p99_ms")] {
            out.push_str(&format!(
                "relay_upstream_request_duration_ms{{{lb},quantile=\"{q}\"}} {}\n",
                prom_num(f64_at(p, key))
            ));
        }
        let success = f64_at(p, "success").unwrap_or(0.0);
        let sum = f64_at(p, "avg_ms").unwrap_or(0.0) * success;
        out.push_str(&format!(
            "relay_upstream_request_duration_ms_sum{{{lb}}} {}\n",
            prom_num(Some(sum))
        ));
        out.push_str(&format!(
            "relay_upstream_request_duration_ms_count{{{lb}}} {}\n",
            prom_num(Some(success))
        ));
    }

    // ---- per-provider 成功率 / 熔断 ----
    out.push_str("# HELP relay_upstream_success_ratio 上游请求成功率(窗口内 success/requests)。\n");
    out.push_str("# TYPE relay_upstream_success_ratio gauge\n");
    for p in &providers {
        let name = p.get("provider").and_then(|v| v.as_str()).unwrap_or("");
        if name.is_empty() {
            continue;
        }
        let lb = format!("provider=\"{}\"", escape_label(name));
        out.push_str(&format!(
            "relay_upstream_success_ratio{{{lb}}} {}\n",
            prom_num(f64_at(p, "success_rate"))
        ));
    }
    out.push_str("# HELP relay_upstream_breaker_state 上游熔断器状态(1=熔断中,0=正常)。\n");
    out.push_str("# TYPE relay_upstream_breaker_state gauge\n");
    out.push_str("# HELP relay_upstream_breaker_fail_count 上游熔断器当前窗口失败计数。\n");
    out.push_str("# TYPE relay_upstream_breaker_fail_count gauge\n");
    for p in &providers {
        let name = p.get("provider").and_then(|v| v.as_str()).unwrap_or("");
        if name.is_empty() {
            continue;
        }
        let lb = format!("provider=\"{}\"", escape_label(name));
        let breaker = p.get("breaker").unwrap_or(&empty);
        let broken = breaker.get("is_broken").and_then(|v| v.as_bool()).unwrap_or(false);
        let fails = f64_at(breaker, "fail_count").unwrap_or(0.0);
        out.push_str(&format!(
            "relay_upstream_breaker_state{{{lb}}} {}\n",
            if broken { 1 } else { 0 }
        ));
        out.push_str(&format!(
            "relay_upstream_breaker_fail_count{{{lb}}} {}\n",
            prom_num(Some(fails))
        ));
    }

    // ---- 全局 tokens ----
    out.push_str("# HELP relay_tokens_input_total 输入 token 总量(窗口内,含失败请求已消耗部分)。\n");
    out.push_str("# TYPE relay_tokens_input_total counter\n");
    out.push_str(&format!("relay_tokens_input_total {}\n", prom_num(f64_at(g, "input_tokens"))));
    out.push_str("# HELP relay_tokens_output_total 输出 token 总量(窗口内)。\n");
    out.push_str("# TYPE relay_tokens_output_total counter\n");
    out.push_str(&format!("relay_tokens_output_total {}\n", prom_num(f64_at(g, "output_tokens"))));

    // ---- 语义缓存 ----
    out.push_str("# HELP relay_cache_requests_total 语义缓存请求数(命中/未命中)。\n");
    out.push_str("# TYPE relay_cache_requests_total counter\n");
    let cache = snap.get("cache").unwrap_or(&empty);
    let hits = f64_at(cache, "hits").unwrap_or(0.0);
    let misses = f64_at(cache, "misses").unwrap_or(0.0);
    out.push_str(&format!("relay_cache_requests_total{{result=\"hit\"}} {}\n", prom_num(Some(hits))));
    out.push_str(&format!("relay_cache_requests_total{{result=\"miss\"}} {}\n", prom_num(Some(misses))));

    // ---- 用户 ----
    out.push_str("# HELP relay_users_total 内存中活跃用户会话数(进程重启后重建)。\n");
    out.push_str("# TYPE relay_users_total gauge\n");
    let users = f64_at(snap, "users").unwrap_or(0.0);
    out.push_str(&format!("relay_users_total {}\n", prom_num(Some(users))));

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    // ---- normalize_page ----

    #[test]
    fn normalize_page_defaults() {
        let (page, size, offset) = normalize_page(None, None);
        assert_eq!(page, 1);
        assert_eq!(size, 20);
        assert_eq!(offset, 0);
    }

    #[test]
    fn normalize_page_clamps_low() {
        let (page, size, offset) = normalize_page(Some(0), Some(0));
        assert_eq!(page, 1);   // 0 → 1
        assert_eq!(size, 1);   // 0 → clamp(1,100) = 1
        assert_eq!(offset, 0);
    }

    #[test]
    fn normalize_page_clamps_high() {
        let (_, size, _) = normalize_page(None, Some(999));
        assert_eq!(size, 100); // 超上限 clamp 到 100
    }

    #[test]
    fn normalize_page_offset_calc() {
        let (_, _, offset) = normalize_page(Some(3), Some(10));
        assert_eq!(offset, 20); // (3-1)*10 = 20
    }

    // ---- total_pages ----

    #[test]
    fn total_pages_empty() {
        assert_eq!(total_pages(0, 20), 0);
        assert_eq!(total_pages(-1, 20), 0);
    }

    #[test]
    fn total_pages_exact_division() {
        assert_eq!(total_pages(20, 20), 1);
        assert_eq!(total_pages(40, 20), 2);
    }

    #[test]
    fn total_pages_remainder() {
        assert_eq!(total_pages(21, 20), 2);
        assert_eq!(total_pages(1, 20), 1);
        assert_eq!(total_pages(19, 20), 1);
    }

    #[test]
    fn total_pages_single_item() {
        assert_eq!(total_pages(1, 10), 1);
    }
}
