//! 管理操作审计:admin 面所有变更请求(POST/PUT/PATCH/DELETE)统一落库。
//! 结构 + 存储抽象(SQLite)+ 采集中间件,零侵入业务 handler。

use std::{
    future::Future,
    net::SocketAddr,
    pin::Pin,
    sync::Arc,
    time::Instant,
};

use axum::{
    extract::{ConnectInfo, State},
    http::Request,
    middleware::Next,
    response::Response,
};
use serde::Serialize;

use crate::state::AppState;

/// 一条管理操作审计记录。
#[derive(Debug, Clone, Serialize)]
pub struct AdminAuditLog {
    /// 操作者(admin 用户名);无 token 请求(登录)为 anonymous。
    pub actor: String,
    /// POST | PUT | PATCH | DELETE。
    pub method: String,
    /// 请求路径(如 /admin/api/users)。
    pub path: String,
    /// 响应状态码。
    pub status: i32,
    /// 客户端 IP。
    pub ip: String,
    /// 处理耗时。
    pub latency_ms: u32,
    /// unix 秒。
    pub ts: i64,
}

// ======================== 存储抽象(镜像 reqlog::RequestLogStore) ========================

pub trait AuditStore: Send + Sync {
    fn write(
        &self,
        log: &AdminAuditLog,
    ) -> Pin<Box<dyn Future<Output = anyhow::Result<()>> + Send + '_>>;

    /// 倒序分页读取。
    fn recent(
        &self,
        limit: u32,
        offset: u32,
    ) -> Pin<Box<dyn Future<Output = anyhow::Result<Vec<AdminAuditLog>>> + Send + '_>>;

    fn count(&self) -> Pin<Box<dyn Future<Output = anyhow::Result<i64>> + Send + '_>>;

    /// 删除超过 days 天的记录。
    fn prune(&self, days: u64) -> Pin<Box<dyn Future<Output = anyhow::Result<u64>> + Send + '_>>;
}

// ======================== SQLite 实现 ========================

const INSERT_SQL: &str = "INSERT INTO admin_audit_logs (actor, method, path, status, ip, latency_ms, created_at) VALUES (?,?,?,?,?,?,?)";

const SELECT_SQL: &str = "SELECT actor, method, path, status, ip, latency_ms, created_at FROM admin_audit_logs ORDER BY id DESC LIMIT ? OFFSET ?";

pub struct SqliteAuditStore {
    db: crate::storage::Db,
}

impl SqliteAuditStore {
    pub fn new(db: crate::storage::Db) -> Self {
        Self { db }
    }
}

impl AuditStore for SqliteAuditStore {
    fn write(
        &self,
        log: &AdminAuditLog,
    ) -> Pin<Box<dyn Future<Output = anyhow::Result<()>> + Send + '_>> {
        let created_at = format!("@{}", log.ts);
        let actor = log.actor.clone();
        let method = log.method.clone();
        let path = log.path.clone();
        let status = log.status;
        let ip = log.ip.clone();
        let latency_ms = log.latency_ms;
        let db = self.db.clone();
        Box::pin(async move {
            sqlx::query(INSERT_SQL)
                .bind(&actor)
                .bind(&method)
                .bind(&path)
                .bind(status as i64)
                .bind(&ip)
                .bind(latency_ms as i64)
                .bind(&created_at)
                .execute(&db)
                .await
                .map(|_| ())
                .map_err(|e| anyhow::anyhow!("write admin_audit_logs failed: {e}"))
        })
    }

    fn recent(
        &self,
        limit: u32,
        offset: u32,
    ) -> Pin<Box<dyn Future<Output = anyhow::Result<Vec<AdminAuditLog>>> + Send + '_>> {
        let limit = limit.clamp(1, 200) as i64;
        let offset = offset as i64;
        let db = self.db.clone();
        Box::pin(async move {
            use sqlx::Row;
            let rows = sqlx::query(SELECT_SQL)
                .bind(limit)
                .bind(offset)
                .fetch_all(&db)
                .await?;
            Ok(rows
                .iter()
                .map(|r| AdminAuditLog {
                    actor: r.try_get("actor").unwrap_or_default(),
                    method: r.try_get("method").unwrap_or_default(),
                    path: r.try_get("path").unwrap_or_default(),
                    status: r.try_get::<i64, _>("status").unwrap_or(0) as i32,
                    ip: r.try_get("ip").unwrap_or_default(),
                    latency_ms: r.try_get::<i64, _>("latency_ms").unwrap_or(0) as u32,
                    // created_at 以 "@unix秒" 定长文本存储,字典序=数值序。
                    ts: r
                        .try_get::<String, _>("created_at")
                        .unwrap_or_default()
                        .trim_start_matches('@')
                        .parse()
                        .unwrap_or(0),
                })
                .collect())
        })
    }

    fn count(&self) -> Pin<Box<dyn Future<Output = anyhow::Result<i64>> + Send + '_>> {
        let db = self.db.clone();
        Box::pin(async move {
            use sqlx::Row;
            let row = sqlx::query("SELECT COUNT(*) AS c FROM admin_audit_logs")
                .fetch_one(&db)
                .await?;
            Ok(row.get::<i64, _>("c"))
        })
    }

    /// created_at 字符串比较即时间比较;审计量小,单语句删除即可。
    fn prune(&self, days: u64) -> Pin<Box<dyn Future<Output = anyhow::Result<u64>> + Send + '_>> {
        let cutoff = format!(
            "@{}",
            crate::storage::now_secs().saturating_sub(days as i64 * 3600)
        );
        let db = self.db.clone();
        Box::pin(async move {
            let res = sqlx::query("DELETE FROM admin_audit_logs WHERE created_at < ?")
                .bind(&cutoff)
                .execute(&db)
                .await?;
            Ok(res.rows_affected() as u64)
        })
    }
}

// ======================== 采集中间件 ========================

/// 登录成功时 handler 经此响应头回传真实用户名,中间件读取后覆盖 anonymous。
pub const AUDIT_ACTOR_HEADER: &str = "x-relay-actor";

/// admin_api 统一审计:仅变更方法(POST/PUT/PATCH/DELETE)落库。
/// GET 天然不记录 → 审计查询端点无递归;不记录请求体(避免密码等敏感信息入库)。
/// actor 能解出 admin JWT 记用户名,否则 anonymous(登录端点);写入走通道,审计不影响主流程。
pub async fn audit_middleware(
    State(state): State<Arc<AppState>>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    req: Request<axum::body::Body>,
    next: Next,
) -> Response {
    // 仅变更方法落库;GET/HEAD 等查询请求直接放行(审计查询端点亦无递归)。
    if !matches!(
        *req.method(),
        axum::http::Method::POST
            | axum::http::Method::PUT
            | axum::http::Method::PATCH
            | axum::http::Method::DELETE
    ) {
        return next.run(req).await;
    }

    let method = req.method().to_string();
    let path = req.uri().path().to_string();
    let actor = crate::jwt::from_headers(&state.config().auth.jwt_secret, req.headers(), "admin")
        .map(|c| c.sub)
        .unwrap_or_else(|_| "anonymous".to_string());

    let start = Instant::now();
    let resp = next.run(req).await;

    // 登录端点经响应头回传真实用户名,覆盖 anonymous(响应头客户端不可伪造)。
    let actor = resp
        .headers()
        .get(AUDIT_ACTOR_HEADER)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_owned())
        .unwrap_or(actor);

    let log = AdminAuditLog {
        actor,
        method,
        path,
        status: resp.status().as_u16() as i32,
        ip: addr.ip().to_string(),
        latency_ms: start.elapsed().as_millis().min(u32::MAX as u128) as u32,
        ts: crate::storage::now_secs(),
    };
    let _ = state.audit_tx.send(log).await;
    resp
}

// ======================== 单元测试 ========================

#[cfg(test)]
mod tests {
    use super::*;

    async fn audit_test_db() -> crate::storage::Db {
        sqlx::any::install_default_drivers();
        let pool = sqlx::pool::PoolOptions::<sqlx::Any>::new()
            .max_connections(1)
            .connect("sqlite::memory:?mode=rwc")
            .await
            .expect("connect in-memory sqlite");
        crate::storage::init_schema(&pool).await.expect("init_schema");
        pool
    }

    fn log(actor: &str, status: i32, ts: i64) -> AdminAuditLog {
        AdminAuditLog {
            actor: actor.into(),
            method: "POST".into(),
            path: "/admin/api/users".into(),
            status,
            ip: "127.0.0.1".into(),
            latency_ms: 12,
            ts,
        }
    }

    #[tokio::test]
    async fn sqlite_write_recent_count_roundtrip() {
        let store = SqliteAuditStore::new(audit_test_db().await);
        store.write(&log("admin", 200, 1726000000)).await.unwrap();
        store.write(&log("ops", 403, 1726000060)).await.unwrap();
        let got = store.recent(10, 0).await.unwrap();
        assert_eq!(got.len(), 2);
        // id DESC:后写的在前。
        assert_eq!(got[0].actor, "ops");
        assert_eq!(got[0].status, 403);
        assert_eq!(got[0].ts, 1726000060);
        assert_eq!(got[1].actor, "admin");
        assert_eq!(store.count().await.unwrap(), 2);
    }

    #[tokio::test]
    async fn sqlite_recent_pagination() {
        let store = SqliteAuditStore::new(audit_test_db().await);
        for i in 0..5 {
            store.write(&log("admin", 200, 1726000000 + i)).await.unwrap();
        }
        let page2 = store.recent(2, 2).await.unwrap();
        assert_eq!(page2.len(), 2);
        // id DESC:第 2 页页首为倒数第 3 条。
        assert_eq!(page2[0].ts, 1726000002);
        assert_eq!(page2[1].ts, 1726000001);
    }

    #[tokio::test]
    async fn sqlite_prune_deletes_expired() {
        let store = SqliteAuditStore::new(audit_test_db().await);
        store.write(&log("admin", 200, 1)).await.unwrap();
        let fresh = crate::storage::now_secs() + 3600;
        store.write(&log("admin", 200, fresh)).await.unwrap();
        assert_eq!(store.prune(30).await.unwrap(), 1);
        let got = store.recent(10, 0).await.unwrap();
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].ts, fresh);
    }
}
