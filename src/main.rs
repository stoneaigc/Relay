mod admin;
mod auth;
mod cache;
pub mod config;
mod email;
mod error;
mod handlers;
mod jwt;
mod portal;
mod providers;
mod reqlog;
pub mod routing;
mod settings;
mod state;
mod storage;
mod translate;

use std::collections::VecDeque;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Duration;

use arc_swap::ArcSwap;
use axum::{
    routing::{get, post},
    Router,
};
use dashmap::DashMap;
use tokio::sync::{mpsc, Mutex as AsyncMutex};

use crate::config::Config;
use crate::state::{AppState, AUDIT_FAILURE_CAP, UsageEvent};

use chrono::{Datelike, Timelike};

/// 解析高峰/低谷时区:优先按 IANA 名解析为秒偏移(Asia/Shanghai → 28800);
/// 解析失败时回退到 tz_offset_hours * 3600 秒偏移。
fn parse_timezone_offset_secs(name: &str, fallback_offset_h: i64) -> i32 {
    match name.trim().parse::<chrono_tz::Tz>() {
        Ok(tz) => {
            // 取该时区当前 UTC 偏移秒数(DST 下取当前时刻的偏移)。
            // 通过比较「UTC 总天数+分钟」与「本地总天数+分钟」的差来计算,正确处理跨天(如 UTC 23:xx + 8h → 次日 07:xx)。
            let utc_now = chrono::Utc::now();
            let local_now = utc_now.with_timezone(&tz);
            let utc_days = utc_now.num_days_from_ce();
            let local_days = local_now.num_days_from_ce();
            let utc_min = utc_now.hour() as i32 * 60 + utc_now.minute() as i32;
            let local_min = local_now.hour() as i32 * 60 + local_now.minute() as i32;
            let offset_secs = ((local_days - utc_days) as i32 * 86400 + (local_min - utc_min) * 60) as i32;
            tracing::info!("time-of-day policy timezone: {} (UTC{:+}h)", tz.name(), offset_secs / 3600);
            offset_secs
        }
        Err(e) => {
            let offset_secs = (fallback_offset_h as i32) * 3600;
            tracing::warn!("timezone '{}' 无法解析(UTC{:+}h): {e},回退到固定偏移", name.trim(), fallback_offset_h);
            offset_secs
        }
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,relay=debug".into()),
        )
        .init();

    let mut cfg = Config::load()?;

    let db = storage::init_pool(&cfg.database).await?;
    storage::init_schema(&db).await?;

    // 从 DB 构建内存路由图(供应商/模型/模型组/路由)。
    let routing = storage::load_routing(&db).await?;
    tracing::info!(
        "routing: {} providers, {} models, {} groups",
        routing.providers.len(),
        routing.models.len(),
        routing.groups.len()
    );

    // 首次启动播种默认奖励任务(star / issue / 提建议)。
    storage::seed_reward_tasks_if_empty(&db).await?;

    // 从 DB 加载系统配置(目前仅 email.*),覆盖到内存 Config(DB 优先于配置文件)。
    {
        let kv = storage::load_settings(&db, settings::EMAIL_PREFIX)
            .await
            .map_err(|e| anyhow::anyhow!("load settings: {e}"))?;
        if !kv.is_empty() {
            let secret = cfg.auth.jwt_secret.clone();
            settings::apply_email_settings(&mut cfg, &kv, &secret);
            tracing::info!("loaded {} email settings from DB", kv.len());
        }
    }

    // 冷启动:全量加载用户与 Key 到内存。
    let keys = DashMap::new();
    let users = DashMap::new();
    storage::load_into_memory(&db, &cfg, &users, &keys).await?;
    tracing::info!("loaded {} users, {} keys into memory", users.len(), keys.len());

    let http = reqwest::Client::builder()
        .timeout(Duration::from_secs(120))
        .build()?;

    let (usage_tx, usage_rx) = mpsc::channel::<UsageEvent>(4096);
    // 请求链路日志:按配置选择存储后端(默认 SQLite;ES 预留)。
    let (request_log_tx, request_log_rx) = mpsc::channel::<reqlog::RequestLog>(4096);
    let log_store: Arc<dyn reqlog::RequestLogStore> = {
        let store_kind = cfg.logging.store.clone();
        let es_url = cfg.logging.elasticsearch_url.clone();
        match store_kind.as_str() {
            "elasticsearch" => Arc::new(reqlog::EsRequestLogStore::new(es_url)),
            _ => Arc::new(reqlog::SqliteRequestLogStore::new(db.clone())),
        }
    };
    tracing::info!("request-log store backend: {}", cfg.logging.store);

    let cache = cache::Cache::init(&cfg.cache).await?;
    tracing::info!("cache backend: {}", cfg.cache.kind);

    let bind = cfg.server.bind.clone();
    // 高峰/低谷时区:按 IANA 名解析为偏移秒数(DST 感知),失败回退固定偏移。
    let tz_offset_secs = parse_timezone_offset_secs(&cfg.defaults.timezone, cfg.defaults.tz_offset_hours);
    let state = Arc::new(AppState {
        config: ArcSwap::from_pointee(cfg),
        routing: ArcSwap::from_pointee(routing),
        keys,
        users,
        http,
        db,
        usage_tx,
        cache,
        upstream_slots: DashMap::new(),
        upstream_breakers: DashMap::new(),
        round_robin: DashMap::new(),
        tz_offset_secs,
        audit_failures: AsyncMutex::new(VecDeque::with_capacity(AUDIT_FAILURE_CAP)),
        metrics: crate::state::MetricsStore::default(),
        request_log: log_store,
        request_log_tx,
    });

    // 后台:用量落盘 + 请求链路落库 + 周期性余额回写。
    tokio::spawn(background_task(Arc::clone(&state), usage_rx, request_log_rx));

    // ---- 门户 API(挂到 /portal/api)----
    let portal_api = Router::new()
        .route("/auth/login", post(portal::password_login))
        .route("/auth/email/send_code", post(portal::send_email_code))
        .route("/auth/register", post(portal::register))
        .route("/auth/reset_password", post(portal::reset_password))
        .route("/me", get(portal::me))
        .route("/balance", get(portal::balance))
        .route("/summary", get(portal::summary))
        .route("/keys", get(portal::list_keys).post(portal::create_key))
        .route("/keys/rotate", post(portal::rotate_key))
        .route("/models", get(portal::models))
        .route("/chat", post(portal::chat))
        .route("/usage", get(portal::usage))
        .route("/series", get(portal::series))
        .route(
            "/rewards",
            get(portal::rewards)
                .post(portal::claim_reward)
                // star 截图以 base64 提交,放宽请求体上限(默认 2MB)。
                .layer(axum::extract::DefaultBodyLimit::max(8 * 1024 * 1024)),
        );

    // ---- 管理 API(挂到 /admin/api)----
    let admin_api = Router::new()
        .route("/auth/login", post(admin::login))
        .route("/overview", get(admin::overview))
        .route("/overview/series", get(admin::overview_series))
        .route("/users", get(admin::list_users).post(admin::create_user))
        .route(
            "/users/:id",
            get(admin::get_user).patch(admin::patch_user).delete(admin::delete_user),
        )
        .route("/users/:id/usage", get(admin::user_usage))
        .route("/users/:id/series", get(admin::user_series))
        .route("/usage", get(admin::global_usage))
        .route("/rewards", get(admin::list_rewards))
        .route("/rewards/:id/review", post(admin::review_reward))
        .route("/reward-tasks", get(admin::list_reward_tasks).post(admin::create_reward_task))
        .route(
            "/reward-tasks/:id",
            axum::routing::delete(admin::delete_reward_task).patch(admin::update_reward_task),
        )
        .route("/models", get(admin::list_models).post(admin::add_model))
        .route("/models/fetch-list", post(admin::fetch_model_list))
        .route("/models/batch", post(admin::add_models_batch))
        .route("/models/:id", axum::routing::delete(admin::delete_model).patch(admin::update_model))
        .route("/models/:id/test", post(admin::test_model))
        // ---- 上游供应商(Provider) ----
        .route("/providers", get(admin::list_providers))
        .route("/providers/exists", post(admin::provider_exists))
        .route("/providers/:name/models", get(admin::list_provider_models))
        .route("/providers/:name", axum::routing::delete(admin::delete_provider))
        .route("/groups", get(admin::list_groups).post(admin::add_group))
        .route("/groups/:id", axum::routing::delete(admin::delete_group))
        .route("/groups/:id/activate", post(admin::activate_group))
        .route("/groups/:id/strategy", post(admin::set_group_strategy))
        .route("/groups/:id/routes", get(admin::list_routes).post(admin::add_route))
        .route("/groups/:id/routes/batch", post(admin::add_routes_batch))
        .route("/routes/:id", axum::routing::delete(admin::delete_route).patch(admin::update_route))
        .route("/groups/:id/time-rules", get(admin::list_time_rules).post(admin::add_time_rule))
        .route("/groups/:id/time-rules/:rule_id", axum::routing::delete(admin::delete_time_rule).put(admin::update_time_rule))
        .route("/settings/email", get(admin::get_email_settings).post(admin::save_email_settings))
        .route("/settings/email/test", post(admin::test_email_settings))
        // ---- 上游治理仪表盘 ----
        .route("/upstreams", get(admin::list_upstreams))
        .route("/upstreams/reset", post(admin::reset_all_breakers))
        .route("/upstreams/reset/:id", post(admin::reset_one_breaker))
        .route("/audit/failures", get(admin::list_failure_audit))
        // 接口指标仪表盘
        .route("/metrics", get(admin::metrics_dashboard))
        .route("/metrics/overview", get(admin::metrics_overview))
        // 请求链路追踪
        .route("/request-logs", get(admin::list_request_logs));

    // SPA 静态资源(未命中的子路径回退到 index.html,交给前端路由)。
    let spa = |dir: &str| {
        tower_http::services::ServeDir::new(format!("frontend/{dir}/dist"))
            .not_found_service(tower_http::services::ServeFile::new(format!("frontend/{dir}/dist/index.html")))
    };

    let app = Router::new()
        .route("/healthz", get(handlers::health))
        // ---- 数据面(给 SDK 用)----
        .route("/v1/models", get(handlers::list_models))
        .route("/v1/chat/completions", post(handlers::chat_completions))
        .route("/v1/messages", post(handlers::messages))
        .nest("/portal/api", portal_api)
        .nest("/admin/api", admin_api)
        .with_state(Arc::clone(&state))
        // ---- 静态前端 ----
        .nest_service("/portal", spa("portal"))
        .nest_service("/admin", spa("admin"))
        .route("/", get(|| async { axum::response::Redirect::to("/portal/") }));

    let listener = tokio::net::TcpListener::bind(&bind).await?;
    tracing::info!("Relay listening on {}", bind);

    let shutdown_state = Arc::clone(&state);
    axum::serve(listener, app)
        .with_graceful_shutdown(async move {
            let _ = tokio::signal::ctrl_c().await;
            tracing::info!("shutting down, flushing balances...");
            flush_dirty(&shutdown_state).await;
        })
        .await?;

    Ok(())
}

async fn background_task(
    state: Arc<AppState>,
    mut rx: mpsc::Receiver<UsageEvent>,
    mut req_rx: mpsc::Receiver<reqlog::RequestLog>,
) {
    let mut tick = tokio::time::interval(Duration::from_secs(5));
    loop {
        tokio::select! {
            maybe = rx.recv() => {
                match maybe {
                    Some(ev) => {
                        if let Err(e) = storage::insert_usage(&state.db, &ev).await {
                            tracing::error!("insert usage failed: {e}");
                        }
                    }
                    None => break,
                }
            }
            log = req_rx.recv() => {
                match log {
                    Some(l) => {
                        if let Err(e) = state.request_log.write(&l).await {
                            tracing::error!("write request_log failed: {e}");
                        }
                    }
                    None => break,
                }
            }
            _ = tick.tick() => {
                flush_dirty(&state).await;
            }
        }
    }
}

/// 把内存中 dirty 的用户余额批量回写 SQLite。
async fn flush_dirty(state: &AppState) {
    for entry in state.users.iter() {
        let u = entry.value();
        if u.dirty.swap(false, Ordering::AcqRel) {
            let bal = u.token_balance.load(Ordering::Relaxed);
            if let Err(e) = storage::flush_balance(&state.db, u.id, bal).await {
                tracing::error!("flush balance failed for {}: {e}", u.id);
                u.dirty.store(true, Ordering::Relaxed); // 回写失败,保留 dirty 下次重试
            }
        }
    }
}
