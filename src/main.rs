mod admin;
mod audit;
mod auth;
mod cache;
pub mod config;
mod email;
mod embedding;
mod error;
mod guard;
mod handlers;
mod jwt;
mod portal;
mod pricing;
mod providers;
mod reqlog;
mod responses;
pub mod routing;
mod semantic_cache;
mod settings;
mod state;
mod storage;
mod translate;

use std::collections::VecDeque;
use std::net::SocketAddr;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Duration;

use arc_swap::ArcSwap;
use axum::{
    extract::Request,
    middleware::Next,
    response::{IntoResponse, Redirect, Response},
    routing::{get, post},
    Router,
};
use dashmap::DashMap;
use tokio::sync::{mpsc, Mutex as AsyncMutex};
use tower::Layer as _;

use crate::config::Config;
use crate::state::{AppState, AUDIT_FAILURE_CAP, UsageEvent};

use chrono::{Datelike, Timelike};

/// 子路径反代兼容:nginx 若不剥离前缀(如 location /relay/ 直接转发),
/// 请求会以 /relay/admin/... 到达网关;此中间件把 admin/portal 之前的前缀剥掉,
/// 使网关在"域名+端口+任意路径"代理下均可工作。已挂在根路径(/admin、/portal)的请求原样通过。
/// 无结尾斜杠的入口(如 /relay/admin)301 重定向到带斜杠的原始路径,保证相对资源引用正确解析。
async fn strip_web_prefix(req: Request, next: Next) -> Response {
    let path = req.uri().path().to_string();
    let stripped = ["admin", "portal"].iter().find_map(|app| {
        let pat = format!("/{app}");
        let idx = path.find(&pat)?;
        // idx==0 说明请求本就挂在根路径(如 /admin/...),无前缀可剥;
        // 模式自带前导斜杠,".../xadmin/..." 不会被误匹配。
        if idx == 0 {
            return None;
        }
        let rest = &path[idx + pat.len()..];
        if !rest.is_empty() && !rest.starts_with('/') {
            return None;
        }
        Some(format!("{pat}{rest}"))
    });
    match stripped {
        // 入口无结尾斜杠:301 到原始路径+斜杠(保留反代前缀),相对资源才能落在正确目录。
        Some(p) if p == "/admin" || p == "/portal" => {
            Redirect::permanent(&format!("{path}/")).into_response()
        }
        Some(new_path) => {
            let (mut parts, body) = req.into_parts();
            let pq = match parts.uri.query() {
                Some(q) => format!("{new_path}?{q}"),
                None => new_path,
            };
            if let Ok(uri) = pq.parse() {
                parts.uri = uri;
            }
            next.run(Request::from_parts(parts, body)).await
        }
        // 无前缀(根路径部署,或 nginx proxy_pass 尾斜杠已剥掉前缀):
        // 精确 /admin、/portal 无尾斜杠时,用「相对 Location」301 —— 浏览器按当前地址解析,
        // 自动保留任何反代前缀(如 /relay/admin → admin/ → /relay/admin/),相对资源才不会错位。
        None => {
            if path == "/admin" || path == "/portal" {
                let app = &path[1..];
                return Redirect::permanent(&format!("{app}/")).into_response();
            }
            next.run(req).await
        }
    }
}

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

    // 从 DB 加载系统配置(目前 email.* 与 fallback.*),覆盖到内存 Config(DB 优先于配置文件)。
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
    {
        let kv = storage::load_settings(&db, settings::FALLBACK_PREFIX)
            .await
            .map_err(|e| anyhow::anyhow!("load settings: {e}"))?;
        if !kv.is_empty() {
            settings::apply_fallback_settings(&mut cfg, &kv);
            tracing::info!("loaded {} fallback settings from DB", kv.len());
        }
    }
    {
        let kv = storage::load_settings(&db, settings::CACHE_PREFIX)
            .await
            .map_err(|e| anyhow::anyhow!("load settings: {e}"))?;
        if !kv.is_empty() {
            settings::apply_cache_settings(&mut cfg, &kv);
            tracing::info!("loaded {} cache settings from DB", kv.len());
        }
    }
    {
        let kv = storage::load_settings(&db, settings::EMB_PREFIX)
            .await
            .map_err(|e| anyhow::anyhow!("load settings: {e}"))?;
        if !kv.is_empty() {
            let secret = cfg.auth.jwt_secret.clone();
            settings::apply_embedding_settings(&mut cfg, &kv, &secret);
            tracing::info!("loaded {} embedding settings from DB", kv.len());
        }
    }
    {
        let kv = storage::load_settings(&db, settings::LOG_PREFIX)
            .await
            .map_err(|e| anyhow::anyhow!("load settings: {e}"))?;
        if !kv.is_empty() {
            settings::apply_logging_settings(&mut cfg, &kv);
            tracing::info!("loaded {} logging settings from DB", kv.len());
        }
    }

    // 存量回填:users.token_used_total 死列修复(幂等,只执行一次)。
    storage::backfill_used_total(&db).await?;

    // 冷启动:全量加载用户与 Key 到内存。
    let keys = DashMap::new();
    let users = DashMap::new();
    storage::load_into_memory(&db, &cfg, &users, &keys).await?;
    tracing::info!("loaded {} users, {} keys into memory", users.len(), keys.len());

    // 数据面 HTTP 客户端:仅设全局连接超时([proxy].connect_timeout_secs);请求总超时按流式/非流式在转发层区分设置。
    let http = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(cfg.proxy.connect_timeout_secs))
        .build()?;

    let (usage_tx, usage_rx) = mpsc::channel::<UsageEvent>(4096);
    // 请求链路日志:按配置选择存储后端(sqlite=默认写 request_logs 表;elasticsearch=按日索引写入 ES)。
    let (request_log_tx, request_log_rx) = mpsc::channel::<reqlog::RequestLog>(4096);
    // 管理操作审计:admin 面变更请求经中间件采集,后台任务落库(量小,通道收窄)。
    let (audit_tx, audit_rx) = mpsc::channel::<audit::AdminAuditLog>(1024);
    let log_store: Arc<dyn reqlog::RequestLogStore> = {
        let store_kind = cfg.logging.store.clone();
        match store_kind.as_str() {
            "elasticsearch" => Arc::new(reqlog::EsRequestLogStore::new(
                cfg.logging.elasticsearch_url.clone(),
                cfg.logging.elasticsearch_index_prefix.clone(),
                cfg.logging.elasticsearch_username.clone(),
                cfg.logging.elasticsearch_password.clone(),
            )),
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
        db: db.clone(),
        usage_tx,
        cache,
        semantic_cache: crate::semantic_cache::SemanticCache::new(),
        upstream_slots: DashMap::new(),
        upstream_breakers: DashMap::new(),
        round_robin: DashMap::new(),
        latency_p50: DashMap::new(),
        tz_offset_secs,
        audit_failures: AsyncMutex::new(VecDeque::with_capacity(AUDIT_FAILURE_CAP)),
        metrics: crate::state::MetricsStore::default(),
        request_log: log_store,
        request_log_tx,
        audit_log: Arc::new(audit::SqliteAuditStore::new(db.clone())),
        audit_tx,
        login_guard: Arc::new(guard::LoginGuard::default()),
    });

    // 后台:用量落盘 + 请求链路落库 + 周期性余额回写。
    tokio::spawn(background_task(
        Arc::clone(&state),
        usage_rx,
        request_log_rx,
        audit_rx,
    ));

    // 后台:延迟优先策略的 P50 快照周期刷新(独立于落盘任务,15s 一次,只读内存桶)。
    tokio::spawn({
        let state = Arc::clone(&state);
        async move {
            let mut tick = tokio::time::interval(Duration::from_secs(15));
            tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            loop {
                tick.tick().await;
                state.refresh_latency_p50().await;
            }
        }
    });

    // ---- 门户 API(挂到 /portal/api)----
    let portal_api = Router::new()
        .route("/auth/login", post(portal::password_login))
        .route("/auth/email/send_code", post(portal::send_email_code))
        .route("/auth/register", post(portal::register))
        .route("/auth/reset_password", post(portal::reset_password))
        .route("/me", get(portal::me))
        .route("/config", get(portal::config))
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
        .route("/usage/breakdown", get(admin::usage_breakdown))
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
        .route("/models/batch-delete", post(admin::delete_models_batch))
        .route("/models/:id", axum::routing::delete(admin::delete_model).patch(admin::update_model))
        .route("/models/:id/test", post(admin::test_model))
        // ---- 上游供应商(Provider) ----
        .route("/providers", get(admin::list_providers))
        .route("/providers/health", get(admin::providers_health))
        .route("/providers/exists", post(admin::provider_exists))
        .route("/providers/:name/models", get(admin::list_provider_models))
        .route("/providers/:name/models/batch", post(admin::add_models_to_provider_batch))
        .route("/providers/:name", axum::routing::put(admin::update_provider).delete(admin::delete_provider))
        .route("/groups", get(admin::list_groups).post(admin::add_group))
        .route("/groups/:id", axum::routing::delete(admin::delete_group).patch(admin::rename_group))
        .route("/groups/:id/activate", post(admin::activate_group))
        .route("/groups/:id/strategy", post(admin::set_group_strategy))
        .route("/groups/:id/routes", get(admin::list_routes).post(admin::add_route))
        .route("/groups/:id/routes/batch", post(admin::add_routes_batch))
        .route("/routes/:id", axum::routing::delete(admin::delete_route).patch(admin::update_route))
        .route("/routes/batch-delete", post(admin::batch_delete_routes))
        .route("/routes/batch-update", post(admin::batch_update_routes))
        .route("/groups/:id/time-rules", get(admin::list_time_rules).post(admin::add_time_rule))
        .route("/groups/:id/time-rules/:rule_id", axum::routing::delete(admin::delete_time_rule).put(admin::update_time_rule))
        .route("/groups/export", get(admin::groups_export))
        .route("/groups/import/preview", post(admin::preview_import))
        .route("/groups/import", post(admin::groups_import))
        .route("/groups/:id/export", get(admin::group_export))
        .route("/settings/email", get(admin::get_email_settings).post(admin::save_email_settings))
        .route("/settings/email/test", post(admin::test_email_settings))
        .route("/settings/fallback", get(admin::get_fallback_settings).post(admin::save_fallback_settings))
        // ---- 语义缓存 ----
        .route("/settings/cache", get(admin::get_cache_settings).post(admin::save_cache_settings))
        .route("/cache/stats", get(admin::cache_stats))
        .route("/cache/hits", get(admin::cache_hits))
        .route("/cache/clear", post(admin::cache_clear))
        .route("/settings/embedding", get(admin::get_embedding_settings).post(admin::save_embedding_settings))
        .route("/settings/embedding/test", post(admin::test_embedding))
        .route("/settings/logging", get(admin::get_logging_settings).post(admin::save_logging_settings))
        .route("/settings/portal", get(admin::get_portal_settings).post(admin::save_portal_settings))
        // 检查更新(比对 GitHub 最新 Release)
        .route("/version/check", get(admin::version_check))
        // ---- 上游治理仪表盘 ----
        .route("/upstreams", get(admin::list_upstreams))
        .route("/upstreams/reset", post(admin::reset_all_breakers))
        .route("/upstreams/reset/:id", post(admin::reset_one_breaker))
        .route("/audit/failures", get(admin::list_failure_audit))
        // 接口指标仪表盘
        .route("/metrics", get(admin::metrics_dashboard))
        .route("/metrics/overview", get(admin::metrics_overview))
        // 请求链路追踪
        .route("/request-logs", get(admin::list_request_logs))
        // 管理操作审计
        .route("/audit-logs", get(admin::list_audit_logs))
        // 审计中间件:包裹全部 admin 路由,仅变更方法落库。
        .layer(axum::middleware::from_fn_with_state(
            Arc::clone(&state),
            audit::audit_middleware,
        ));

    // SPA 静态资源(未命中的子路径回退到 index.html,交给前端路由)。
    let spa = |dir: &str| {
        tower_http::services::ServeDir::new(format!("frontend/{dir}/dist"))
            .not_found_service(tower_http::services::ServeFile::new(format!("frontend/{dir}/dist/index.html")))
    };

    let app = Router::new()
        .route("/healthz", get(handlers::health))
        // Prometheus 指标导出(管理端 JWT 或 metrics.export_token 鉴权)。
        .route("/metrics", get(admin::metrics_export))
        // ---- 数据面(给 SDK 用)----
        .route("/v1/models", get(handlers::list_models))
        .route("/v1/chat/completions", post(handlers::chat_completions))
        .route("/v1/responses", post(handlers::responses))
        .route("/v1/messages", post(handlers::messages))
        .route("/v1/embeddings", post(handlers::embeddings))
        .nest("/portal/api", portal_api)
        .nest("/admin/api", admin_api)
        .with_state(Arc::clone(&state))
        // ---- 静态前端 ----
        .nest_service("/portal", spa("portal"))
        .nest_service("/admin", spa("admin"))
        .route("/", get(|| async { axum::response::Redirect::to("portal/") }));
    // 子路径反代兼容:URI 改写必须在路由匹配之前生效,故包在 Router 外层
    // (Router::layer 的中间件在匹配后才执行,改 URI 无效)。
    let app = axum::middleware::from_fn(strip_web_prefix).layer(app);

    let listener = tokio::net::TcpListener::bind(&bind).await?;
    tracing::info!("Relay v{} listening on {}", env!("CARGO_PKG_VERSION"), bind);

    let shutdown_state = Arc::clone(&state);
    // with_connect_info:中间件经 ConnectInfo 提取客户端 IP。
    // app 已被 strip_web_prefix 包裹,不再是 Router,改用 ServiceExt 的扩展方法。
    use axum::ServiceExt as _;
    axum::serve(listener, app.into_make_service_with_connect_info::<SocketAddr>())
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
    mut audit_rx: mpsc::Receiver<audit::AdminAuditLog>,
) {
    let mut tick = tokio::time::interval(Duration::from_secs(5));
    // 日志保留清理周期:每 10 分钟检查一次,按 logging.retention_days 删除超期链路日志(0=不清理)。
    let mut retention_tick = tokio::time::interval(Duration::from_secs(600));
    let mut calibrate_counter = 0u32;
    loop {
        tokio::select! {
            maybe = rx.recv() => {
                match maybe {
                    Some(ev) => {
                        // 批量 drain:首条已到手,再非阻塞收取一批,单事务落库,高并发下不再逐条积压。
                        let mut batch = vec![ev];
                        while batch.len() < 256 {
                            match rx.try_recv() {
                                Ok(e) => batch.push(e),
                                Err(_) => break,
                            }
                        }
                        // 费用折算(消费端单点):按 (provider, upstream_model) 查模型价格,
                        // 估基础成本后用 charged/(input+output) 反推总倍率,与计费口径完全一致;未定价记 0。
                        let routing = state.routing.load();
                        for ev in batch.iter_mut() {
                            let total = ev.input_tokens + ev.output_tokens;
                            if total > 0 {
                                let prices = routing.models.values()
                                    .find(|m| m.provider == ev.provider && m.upstream_model == ev.upstream_model)
                                    .map(|m| (m.input_price, m.output_price));
                                if let Some((pi, po)) = prices {
                                    let base = crate::pricing::estimate_cost(&ev.upstream_model, ev.input_tokens as u64, ev.output_tokens as u64, pi, po);
                                    ev.cost_usd = crate::pricing::billed_cost(base, ev.input_tokens as u64, ev.output_tokens as u64, ev.charged_tokens);
                                }
                            }
                        }
                        drop(routing);
                        if let Err(e) = storage::insert_usage_batch(&state.db, &batch).await {
                            tracing::error!("insert usage batch failed: {e}");
                        }
                        // 周期预算内存累计:所有 UsageEvent 都经过此处,与库内口径一致。
                        let tz_offset = state.tz_offset_secs as i64 / 3600;
                        for ev in batch.iter() {
                            if let Some(u) = state.users.get(&ev.user_id) {
                                u.record_budget(ev.charged_tokens, tz_offset);
                            }
                        }
                    }
                    None => break,
                }
            }
            log = req_rx.recv() => {
                match log {
                    Some(l) => {
                        // 批量 drain + 单事务落库,与 usage 消费同思路。
                        let mut batch = vec![l];
                        while batch.len() < 256 {
                            match req_rx.try_recv() {
                                Ok(x) => batch.push(x),
                                Err(_) => break,
                            }
                        }
                        if let Err(e) = state.request_log.write_batch(batch).await {
                            tracing::error!("write request_log batch failed: {e}");
                        }
                    }
                    None => break,
                }
            }
            a = audit_rx.recv() => {
                // 审计量小(管理操作低频):逐条落库即可。
                match a {
                    Some(a) => {
                        if let Err(e) = state.audit_log.write(&a).await {
                            tracing::error!("write audit log failed: {e}");
                        }
                    }
                    None => break,
                }
            }
            _ = tick.tick() => {
                flush_dirty(&state).await;
                // calibrate_budgets 对 usage_logs 做聚合扫描,大表下代价高:降频到约每分钟一次。
                // 请求路径的预算扣减走内存累计,此处只是周期性对齐库内值。
                calibrate_counter += 1;
                if calibrate_counter >= 12 {
                    calibrate_counter = 0;
                    calibrate_budgets(&state).await;
                }
                state.semantic_cache.sweep();
            }
            _ = retention_tick.tick() => {
                // 超期链路日志清理:retention_days=0 时跳过(永久保留)。
                let days = state.config.load().logging.retention_days as u64;
                if days > 0 {
                    match state.request_log.prune(days).await {
                        Ok(n) if n > 0 => tracing::info!("pruned {} request logs older than {days} days", n),
                        Ok(_) => {}
                        Err(e) => tracing::error!("prune request logs failed: {e}"),
                    }
                    match state.audit_log.prune(days).await {
                        Ok(n) if n > 0 => tracing::info!("pruned {} audit logs older than {days} days", n),
                        Ok(_) => {}
                        Err(e) => tracing::error!("prune audit logs failed: {e}"),
                    }
                }
            }
        }
    }
}

/// 周期校准预算窗口:以库内日/月用量为下界修正内存计数(fetch_max)。
/// 内存领先(未落库)不丢失;库内更大(重启/漂移)则纠正,方向安全。
async fn calibrate_budgets(state: &AppState) {
    let any = state.users.iter().any(|e| {
        let u = e.value();
        u.budget_daily.load(Ordering::Relaxed) > 0 || u.budget_monthly.load(Ordering::Relaxed) > 0
    });
    if !any {
        return;
    }
    let tz = state.tz_offset_secs as i64 / 3600;
    let day_map = storage::usage_by_user(&state.db, Some(storage::today_start(tz)))
        .await
        .unwrap_or_default();
    let month_map = storage::usage_by_user(&state.db, Some(storage::month_start(tz)))
        .await
        .unwrap_or_default();
    for entry in state.users.iter() {
        let u = entry.value();
        if u.budget_daily.load(Ordering::Relaxed) == 0 && u.budget_monthly.load(Ordering::Relaxed) == 0 {
            continue;
        }
        u.record_budget(0, tz); // 先推进窗口翻转,再对齐库内值
        let uid = u.id.to_string();
        u.used_daily
            .fetch_max(day_map.get(&uid).copied().unwrap_or(0), Ordering::Relaxed);
        u.used_monthly
            .fetch_max(month_map.get(&uid).copied().unwrap_or(0), Ordering::Relaxed);
    }
}

/// 把内存中 dirty 的用户余额批量回写 SQLite。
/// 先同步收集快照并立即释放 DashMap 读锁,再逐条 await 落库:
/// 读锁绝不能跨 .await 持有,否则并发 deduct 的写锁会 park worker 线程,
/// I/O driver 饿死后本函数的 SQLite await 永不完成,形成全运行时永久死锁。
async fn flush_dirty(state: &AppState) {
    let snapshot: Vec<_> = state
        .users
        .iter()
        .filter_map(|entry| {
            let u = entry.value();
            if u.dirty.swap(false, Ordering::AcqRel) {
                Some((
                    u.id,
                    u.token_balance.load(Ordering::Relaxed),
                    u.token_used_total.load(Ordering::Relaxed),
                ))
            } else {
                None
            }
        })
        .collect();
    for (id, bal, used) in snapshot {
        if let Err(e) = storage::flush_balance(&state.db, id, bal, used).await {
            tracing::error!("flush balance failed for {}: {e}", id);
            if let Some(u) = state.users.get(&id) {
                u.dirty.store(true, Ordering::Relaxed); // 回写失败,保留 dirty 下次重试
            }
        }
    }
}
