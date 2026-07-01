mod admin;
mod auth;
mod cache;
mod config;
mod email;
mod error;
mod handlers;
mod jwt;
mod portal;
mod providers;
mod routing;
mod state;
mod storage;
mod translate;

use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Duration;

use arc_swap::ArcSwap;
use axum::{
    routing::{get, post},
    Router,
};
use dashmap::DashMap;
use tokio::sync::mpsc;

use crate::config::Config;
use crate::state::{AppState, UsageEvent};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,runapi=debug".into()),
        )
        .init();

    let cfg = Config::load()?;

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

    if let Some(key) = storage::seed_if_empty(&db, &cfg).await? {
        tracing::warn!("seeded demo user. Test API key (shown once): {}", key);
    }
    // 首次启动播种默认奖励任务(star / issue / 提建议)。
    storage::seed_reward_tasks_if_empty(&db).await?;

    // 冷启动:全量加载用户与 Key 到内存。
    let keys = DashMap::new();
    let users = DashMap::new();
    storage::load_into_memory(&db, &cfg, &users, &keys).await?;
    tracing::info!("loaded {} users, {} keys into memory", users.len(), keys.len());

    let http = reqwest::Client::builder()
        .timeout(Duration::from_secs(120))
        .build()?;

    let (usage_tx, usage_rx) = mpsc::channel::<UsageEvent>(4096);

    let cache = cache::Cache::init(&cfg.cache).await?;
    tracing::info!("cache backend: {}", cfg.cache.kind);

    let bind = cfg.server.bind.clone();
    let state = Arc::new(AppState {
        config: ArcSwap::from_pointee(cfg),
        routing: ArcSwap::from_pointee(routing),
        keys,
        users,
        http,
        db,
        usage_tx,
        cache,
    });

    // 后台:用量落盘 + 周期性余额回写。
    tokio::spawn(background_task(Arc::clone(&state), usage_rx));

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
        .route("/models/:id", axum::routing::delete(admin::delete_model).patch(admin::update_model))
        .route("/models/:id/test", post(admin::test_model))
        .route("/groups", get(admin::list_groups).post(admin::add_group))
        .route("/groups/:id", axum::routing::delete(admin::delete_group))
        .route("/groups/:id/activate", post(admin::activate_group))
        .route("/groups/:id/routes", get(admin::list_routes).post(admin::add_route))
        .route("/routes/:id", axum::routing::delete(admin::delete_route).patch(admin::update_route));

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
    tracing::info!("RunAPI listening on {}", bind);

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

async fn background_task(state: Arc<AppState>, mut rx: mpsc::Receiver<UsageEvent>) {
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
