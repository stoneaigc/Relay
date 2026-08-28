use std::pin::Pin;
use std::future::Future;

use serde::{Deserialize, Serialize};

use crate::config::ProviderKind;
use crate::storage::Db;

/// 单个候选上游的一次尝试结果(failover 链的一环)。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RequestAttempt {
    /// 上游协议:openai / anthropic。
    pub kind: String,
    pub base_url: String,
    /// 上游真实模型名。
    pub upstream_model: String,
    /// 该候选在路由中的权重。
    pub weight: u32,
    /// 尝试结果状态:200=成功;503=上游不可用(UNAVAILABLE);-1=被熔断跳过(未实际调用);4xx/500=其它错误。
    pub status: i32,
    /// 本次尝试耗时(ms)。
    pub latency_ms: u32,
    /// 失败时的错误摘要。
    pub error: String,
}

/// 一次完整请求的链路日志(成功 + 失败均记录)。
#[derive(Debug, Clone, Serialize)]
pub struct RequestLog {
    /// 本次请求唯一 ID(UUID v4 字符串)。
    pub request_id: String,
    pub user_id: String,
    /// chat | messages。
    pub path: String,
    /// 用户请求的对外模型名。
    pub requested_model: String,
    pub stream: bool,
    /// resolve_all 生成的候选顺序(加权命中 + failover 次序)。
    pub candidates: Vec<RequestAttempt>,
    /// 每个候选的实际尝试结果(与 candidates 对应的 attempts;熔断跳过不入此列,记在 candidates 的 status 里)。
    pub attempts: Vec<RequestAttempt>,
    /// 最终命中的上游 kind(失败时为 null)。
    pub final_kind: Option<String>,
    /// 最终命中的上游模型名(失败时为 null)。
    pub final_upstream_model: Option<String>,
    /// 最终状态:200=成功;非 200=失败。
    pub final_status: i32,
    /// 总耗时(ms)。
    pub latency_ms: u32,
    pub input_tokens: u32,
    pub output_tokens: u32,
    pub charged_tokens: i64,
    /// 落库时间(unix 秒)。
    pub ts: i64,
}

fn kind_str(kind: ProviderKind) -> &'static str {
    match kind {
        ProviderKind::Openai => "openai",
        ProviderKind::Anthropic => "anthropic",
    }
}

/// 请求链路日志的存储后端抽象。当前实现:SQLite(默认);ES 预留。
/// 使用 `Pin<Box<dyn Future>>` 以保持 dyn-compatible(可供 Arc<dyn ...> 存储)。
pub trait RequestLogStore: Send + Sync {
    /// 写入一条请求链路日志。
    fn write(
        &self,
        log: &RequestLog,
    ) -> Pin<Box<dyn Future<Output = anyhow::Result<()>> + Send + '_>>;
    /// 最近 N 条(倒序)。`q` 可为空 = 全部;否则匹配 request_id / 模型。
    fn recent(
        &self,
        q: &str,
        limit: u32,
    ) -> Pin<Box<dyn Future<Output = anyhow::Result<Vec<RequestLog>>> + Send + '_>>;
}

/// 默认实现:写入 SQLite `request_logs` 表。
pub struct SqliteRequestLogStore {
    db: Db,
}

impl SqliteRequestLogStore {
    pub fn new(db: Db) -> Self {
        Self { db }
    }
}

const INSERT_SQL: &str = "INSERT INTO request_logs
  (request_id, user_id, path, requested_model, stream, candidates, attempts,
   final_kind, final_upstream_model, final_status, latency_ms,
   input_tokens, output_tokens, charged_tokens, created_at)
  VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)";

impl RequestLogStore for SqliteRequestLogStore {
    fn write(
        &self,
        log: &RequestLog,
    ) -> Pin<Box<dyn Future<Output = anyhow::Result<()>> + Send + '_>> {
        let candidates_json = match serde_json::to_string(&log.candidates) {
            Ok(s) => s,
            Err(e) => return Box::pin(async move { Err(anyhow::anyhow!("serde: {e}")) }),
        };
        let attempts_json = match serde_json::to_string(&log.attempts) {
            Ok(s) => s,
            Err(e) => return Box::pin(async move { Err(anyhow::anyhow!("serde: {e}")) }),
        };
        let created_at = format!("@{}", log.ts);
        let request_id = log.request_id.clone();
        let user_id = log.user_id.clone();
        let path = log.path.clone();
        let requested_model = log.requested_model.clone();
        let final_kind = log.final_kind.clone();
        let final_upstream_model = log.final_upstream_model.clone();
        let stream = log.stream;
        let final_status = log.final_status;
        let latency_ms = log.latency_ms;
        let input_tokens = log.input_tokens;
        let output_tokens = log.output_tokens;
        let charged_tokens = log.charged_tokens;
        let db = self.db.clone();
        Box::pin(async move {
            sqlx::query(INSERT_SQL)
                .bind(&request_id)
                .bind(&user_id)
                .bind(&path)
                .bind(&requested_model)
                .bind(stream as i64)
                .bind(&candidates_json)
                .bind(&attempts_json)
                .bind(&final_kind)
                .bind(&final_upstream_model)
                .bind(final_status as i64)
                .bind(latency_ms as i64)
                .bind(input_tokens as i64)
                .bind(output_tokens as i64)
                .bind(charged_tokens)
                .bind(&created_at)
                .execute(&db)
                .await
                .map(|_| ())
                .map_err(|e| anyhow::anyhow!("write request_logs failed: {e}"))
        })
    }

    fn recent(
        &self,
        q: &str,
        limit: u32,
    ) -> Pin<Box<dyn Future<Output = anyhow::Result<Vec<RequestLog>>> + Send + '_>> {
        let q = q.to_string();
        let limit = limit.clamp(1, 200);
        let db = self.db.clone();
        let has_q = !q.trim().is_empty();
        let sql: &'static str = if has_q {
            "SELECT request_id, user_id, path, requested_model, stream, candidates, attempts,
                    final_kind, final_upstream_model, final_status, latency_ms,
                    input_tokens, output_tokens, charged_tokens, created_at
             FROM request_logs
             WHERE request_id LIKE ? OR requested_model LIKE ?
             ORDER BY id DESC LIMIT ?"
        } else {
            "SELECT request_id, user_id, path, requested_model, stream, candidates, attempts,
                    final_kind, final_upstream_model, final_status, latency_ms,
                    input_tokens, output_tokens, charged_tokens, created_at
             FROM request_logs ORDER BY id DESC LIMIT ?"
        };
        let pat = format!("%{}%", q.trim());
        Box::pin(async move {
            let mut query = sqlx::query(sql);
            if has_q {
                query = query.bind(&pat).bind(&pat).bind(limit as i64);
            } else {
                query = query.bind(limit as i64);
            }
            let rows = query.fetch_all(&db).await?;
            let mut out = Vec::with_capacity(rows.len());
            for r in rows {
                use sqlx::Row;
                let candidates: Vec<RequestAttempt> = match r.try_get::<String, _>("candidates") {
                    Ok(s) => serde_json::from_str(&s).unwrap_or_default(),
                    Err(_) => Vec::new(),
                };
                let attempts: Vec<RequestAttempt> = match r.try_get::<String, _>("attempts") {
                    Ok(s) => serde_json::from_str(&s).unwrap_or_default(),
                    Err(_) => Vec::new(),
                };
                out.push(RequestLog {
                    request_id: r.try_get("request_id").unwrap_or_default(),
                    user_id: r.try_get("user_id").unwrap_or_default(),
                    path: r.try_get("path").unwrap_or_default(),
                    requested_model: r.try_get("requested_model").unwrap_or_default(),
                    stream: r.try_get::<i64, _>("stream").unwrap_or(0) != 0,
                    candidates,
                    attempts,
                    final_kind: r.try_get("final_kind").ok(),
                    final_upstream_model: r.try_get("final_upstream_model").ok(),
                    final_status: r.try_get::<i64, _>("final_status").unwrap_or(0) as i32,
                    latency_ms: r.try_get::<i64, _>("latency_ms").unwrap_or(0) as u32,
                    input_tokens: r.try_get::<i64, _>("input_tokens").unwrap_or(0) as u32,
                    output_tokens: r.try_get::<i64, _>("output_tokens").unwrap_or(0) as u32,
                    charged_tokens: r.try_get::<i64, _>("charged_tokens").unwrap_or(0),
                    ts: parse_ts(&r.try_get::<String, _>("created_at").unwrap_or_default()),
                });
            }
            Ok(out)
        })
    }
}

fn parse_ts(created_at: &str) -> i64 {
    created_at
        .trim_start_matches('@')
        .parse::<i64>()
        .unwrap_or(0)
}

/// 预留实现:接到 Elasticsearch,暂未启用。
pub struct EsRequestLogStore {
    _url: String,
}

impl EsRequestLogStore {
    pub fn new(url: String) -> Self {
        Self { _url: url }
    }
}

impl RequestLogStore for EsRequestLogStore {
    fn write(
        &self,
        _log: &RequestLog,
    ) -> Pin<Box<dyn Future<Output = anyhow::Result<()>> + Send + '_>> {
        Box::pin(async move {
            anyhow::bail!("elasticsearch request-log store is not implemented yet; use sqlite")
        })
    }

    fn recent(
        &self,
        _q: &str,
        _limit: u32,
    ) -> Pin<Box<dyn Future<Output = anyhow::Result<Vec<RequestLog>>> + Send + '_>> {
        Box::pin(async move {
            anyhow::bail!("elasticsearch request-log store is not implemented yet; use sqlite")
        })
    }
}

/// 按 `Resolved`(候选)构造一个待尝试的 attempt 条目。
pub fn candidate_attempt(
    kind: ProviderKind,
    base_url: &str,
    upstream_model: &str,
    weight: u32,
) -> RequestAttempt {
    RequestAttempt {
        kind: kind_str(kind).to_string(),
        base_url: base_url.to_string(),
        upstream_model: upstream_model.to_string(),
        weight,
        status: 0,
        latency_ms: 0,
        error: String::new(),
    }
}
