use std::future::Future;
use std::pin::Pin;
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::config::ProviderKind;
use crate::storage::{now_secs, Db};

/// 单个候选上游的一次尝试结果(failover 链的一环)。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RequestAttempt {
    /// 上游协议:openai / anthropic。
    pub kind: String,
    /// 命中的供应商名(展示用);旧日志无此字段,反序列化时回退空串。
    #[serde(default)]
    pub provider: String,
    pub base_url: String,
    /// 上游真实模型名。
    pub upstream_model: String,
    /// 该候选在路由中的权重。
    pub weight: u32,
    /// 200=成功; 503=不可用; -1=熔断跳过; 4xx/500=其它错误; 0=初始状态(候选列表)。
    pub status: i32,
    /// 本次尝试耗时(ms)。
    pub latency_ms: u32,
    /// 失败时的错误摘要。
    pub error: String,
}

/// 一次完整请求的链路日志(成功 + 失败均记录)。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RequestLog {
    pub request_id: String,
    pub user_id: String,
    /// chat | messages。
    pub path: String,
    /// 用户请求的对外模型名。
    pub requested_model: String,
    pub stream: bool,
    /// resolve_all 生成的候选顺序(加权命中 + failover 次序)。
    pub candidates: Vec<RequestAttempt>,
    /// 每个候选的实际尝试结果。
    pub attempts: Vec<RequestAttempt>,
    /// 最终命中的上游 kind(失败时为 null)。
    pub final_kind: Option<String>,
    /// 最终命中的上游模型名(失败时为 null)。
    pub final_upstream_model: Option<String>,
    /// 最终状态:200=成功;非200=失败。
    pub final_status: i32,
    /// 总耗时(ms)。
    pub latency_ms: u32,
    pub input_tokens: u32,
    pub output_tokens: u32,
    pub charged_tokens: i64,
    /// 落库时间(unix 秒)。
    pub ts: i64,
    /// 请求体预览(截断到 logging.body_preview_max_bytes;跟随存储后端:SQLite/PG 落 req_body 列,ES 落文档字段)。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub req_body: Option<String>,
    /// 响应体预览(跟随存储后端,同 req_body)。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resp_body: Option<String>,
}

fn kind_str(kind: ProviderKind) -> &'static str {
    match kind {
        ProviderKind::Openai => "openai",
        ProviderKind::Anthropic => "anthropic",
    }
}

/// 请求链路日志的存储后端抽象。
pub trait RequestLogStore: Send + Sync {
    fn write(
        &self,
        log: &RequestLog,
    ) -> Pin<Box<dyn Future<Output = anyhow::Result<()>> + Send + '_>>;

    /// 按关键字/失败状态/时间范围倒序查询。
    /// - `q`:匹配 request_id 或 requested_model;
    /// - `failed`:Some(true)=只看失败,Some(false)=只看成功,None=不过滤;
    /// - `hours`:只看最近 N 小时,None=不过滤。
    fn recent(
        &self,
        q: &str,
        failed: Option<bool>,
        hours: Option<u64>,
        limit: u32,
        offset: u32,
    ) -> Pin<Box<dyn Future<Output = anyhow::Result<Vec<RequestLog>>> + Send + '_>>;

    fn count(
        &self,
        q: &str,
        failed: Option<bool>,
        hours: Option<u64>,
    ) -> Pin<Box<dyn Future<Output = anyhow::Result<i64>> + Send + '_>>;

    /// 清理超过 days 天前的日志,返回删除条数。默认空实现(0 条);调用方仅在 days>0 时调用。
    fn prune(&self, _days: u64) -> Pin<Box<dyn Future<Output = anyhow::Result<u64>> + Send + '_>> {
        Box::pin(async move { Ok(0) })
    }

    /// 批量落库:默认逐条回退到 write;SQLite 实现覆写为单事务,高并发下大幅降低提交开销。
    fn write_batch(
        &self,
        logs: Vec<RequestLog>,
    ) -> Pin<Box<dyn Future<Output = anyhow::Result<()>> + Send + '_>> {
        Box::pin(async move {
            for log in &logs {
                self.write(log).await?;
            }
            Ok(())
        })
    }
}

/// 请求/响应体预览采集:UTF-8 边界安全截断。
/// 空 body 或 `max=0` 返回 None;超长时按 `max` 字节截断并丢弃残缺的多字节序列尾部。
pub fn body_preview(bytes: &[u8], max: usize) -> Option<String> {
    if bytes.is_empty() || max == 0 {
        return None;
    }
    let slice: &[u8] = if bytes.len() <= max { bytes } else { &bytes[..max] };
    let s = match std::str::from_utf8(slice) {
        Ok(s) => s,
        Err(e) => std::str::from_utf8(&slice[..e.valid_up_to()]).unwrap_or(""),
    };
    let s = s.trim();
    if s.is_empty() {
        None
    } else {
        Some(s.to_string())
    }
}

// ======================== SQLite 实现 ========================

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
   input_tokens, output_tokens, charged_tokens, created_at, req_body, resp_body)
  VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)";

/// 分页计数封顶:COUNT 在子查询 LIMIT 处截断,超大表下避免 O(全表) 扫描。
const COUNT_CAP: i64 = 20000;

/// 根据筛选条件拼接动态 WHERE(q 关键字 / failed 状态 / hours 时间窗)。
fn sqlite_where(q: &str, failed: Option<bool>, hours: Option<u64>) -> (String, Option<String>) {
    let qt = q.trim();
    let has_q = !qt.is_empty();
    let cutoff = hours
        .map(|h| format!("@{}", now_secs().saturating_sub(h as i64 * 3600)));

    let mut conds: Vec<&str> = Vec::new();
    if has_q {
        conds.push("(request_id LIKE ? OR requested_model LIKE ?)");
    }
    match failed {
        Some(true) => conds.push("final_status != 200"),
        Some(false) => conds.push("final_status = 200"),
        None => {}
    }
    if cutoff.is_some() {
        // created_at 以 "@unix秒" 存储,定长字符串,字典序=数值序。
        conds.push("created_at >= ?");
    }
    let where_sql = if conds.is_empty() {
        String::new()
    } else {
        format!(" WHERE {}", conds.join(" AND "))
    };
    (where_sql, cutoff)
}

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
        let req_body = log.req_body.clone();
        let resp_body = log.resp_body.clone();
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
                .bind(&req_body)
                .bind(&resp_body)
                .execute(&db)
                .await
                .map(|_| ())
                .map_err(|e| anyhow::anyhow!("write request_logs failed: {e}"))
        })
    }

    fn write_batch(
        &self,
        logs: Vec<RequestLog>,
    ) -> Pin<Box<dyn Future<Output = anyhow::Result<()>> + Send + '_>> {
        Box::pin(async move {
            if logs.is_empty() {
                return Ok(());
            }
            let mut tx = self
                .db
                .begin()
                .await
                .map_err(|e| anyhow::anyhow!("begin request_logs batch: {e}"))?;
            for log in &logs {
                let candidates_json = match serde_json::to_string(&log.candidates) {
                    Ok(s) => s,
                    Err(e) => {
                        tracing::warn!("write_batch: skip {}: serde: {e}", log.request_id);
                        continue;
                    }
                };
                let attempts_json = match serde_json::to_string(&log.attempts) {
                    Ok(s) => s,
                    Err(e) => {
                        tracing::warn!("write_batch: skip {}: serde: {e}", log.request_id);
                        continue;
                    }
                };
                if let Err(e) = sqlx::query(INSERT_SQL)
                    .bind(&log.request_id)
                    .bind(&log.user_id)
                    .bind(&log.path)
                    .bind(&log.requested_model)
                    .bind(log.stream as i64)
                    .bind(&candidates_json)
                    .bind(&attempts_json)
                    .bind(&log.final_kind)
                    .bind(&log.final_upstream_model)
                    .bind(log.final_status as i64)
                    .bind(log.latency_ms as i64)
                    .bind(log.input_tokens as i64)
                    .bind(log.output_tokens as i64)
                    .bind(log.charged_tokens)
                    .bind(format!("@{}", log.ts))
                    .bind(&log.req_body)
                    .bind(&log.resp_body)
                    .execute(&mut *tx)
                    .await
                {
                    tracing::error!("write request_logs batch item failed: {e}");
                }
            }
            tx.commit()
                .await
                .map_err(|e| anyhow::anyhow!("commit request_logs batch: {e}"))?;
            Ok(())
        })
    }

    fn recent(
        &self,
        q: &str,
        failed: Option<bool>,
        hours: Option<u64>,
        limit: u32,
        offset: u32,
    ) -> Pin<Box<dyn Future<Output = anyhow::Result<Vec<RequestLog>>> + Send + '_>> {
        let (where_sql, cutoff) = sqlite_where(q, failed, hours);
        let limit = limit.clamp(1, 200);
        let offset = offset as i64;
        let db = self.db.clone();
        let has_q = !q.trim().is_empty();
        let pat = format!("%{}%", q.trim());
        let sql = format!(
            "SELECT request_id, user_id, path, requested_model, stream, candidates, attempts,
                    final_kind, final_upstream_model, final_status, latency_ms,
                    input_tokens, output_tokens, charged_tokens, created_at,
                    req_body, resp_body
             FROM request_logs{where_sql}
             ORDER BY id DESC LIMIT ? OFFSET ?"
        );
        Box::pin(async move {
            let mut query = sqlx::query(&sql);
            if has_q {
                query = query.bind(&pat).bind(&pat);
            }
            if let Some(c) = &cutoff {
                query = query.bind(c);
            }
            query = query.bind(limit as i64).bind(offset);
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
                    req_body: r.try_get("req_body").ok(),
                    resp_body: r.try_get("resp_body").ok(),
                });
            }
            Ok(out)
        })
    }

    fn count(
        &self,
        q: &str,
        failed: Option<bool>,
        hours: Option<u64>,
    ) -> Pin<Box<dyn Future<Output = anyhow::Result<i64>> + Send + '_>> {
        let (where_sql, cutoff) = sqlite_where(q, failed, hours);
        let db = self.db.clone();
        let has_q = !q.trim().is_empty();
        let pat = format!("%{}%", q.trim());
        // 大表保护:COUNT 在子查询 LIMIT 处截断(封顶 COUNT_CAP),分页无需精确总数。
        let sql = format!("SELECT COUNT(*) AS c FROM (SELECT 1 FROM request_logs{where_sql} LIMIT ?)");
        Box::pin(async move {
            use sqlx::Row;
            let mut query = sqlx::query(&sql);
            if has_q {
                query = query.bind(&pat).bind(&pat);
            }
            if let Some(c) = &cutoff {
                query = query.bind(c);
            }
            query = query.bind(COUNT_CAP + 1);
            let row = query.fetch_one(&db).await?;
            Ok(row.get::<i64, _>("c").min(COUNT_CAP))
        })
    }

    /// 分批删除超期日志:created_at 以 "@unix秒" 定长文本存储,字典序=数值序,字符串比较即时间比较。
    /// 每批 2000 条循环删除,避免单次大 DELETE 长时间持锁;SQLite/PG 通用。
    fn prune(&self, days: u64) -> Pin<Box<dyn Future<Output = anyhow::Result<u64>> + Send + '_>> {
        let db = self.db.clone();
        let cutoff = format!("@{}", now_secs().saturating_sub(days as i64 * 3600));
        Box::pin(async move {
            let mut total: u64 = 0;
            loop {
                let res = sqlx::query(
                    "DELETE FROM request_logs WHERE id IN (SELECT id FROM request_logs WHERE created_at < ? LIMIT 2000)",
                )
                .bind(&cutoff)
                .execute(&db)
                .await?;
                let n = res.rows_affected();
                total += n as u64;
                if n == 0 {
                    break;
                }
            }
            Ok(total)
        })
    }
}

fn parse_ts(created_at: &str) -> i64 {
    created_at
        .trim_start_matches('@')
        .parse::<i64>()
        .unwrap_or(0)
}

// ======================== Elasticsearch 实现 ========================

const ES_TIMEOUT: Duration = Duration::from_secs(30);
const DEFAULT_ES_INDEX_PREFIX: &str = "relay-logs";

pub struct EsRequestLogStore {
    client: reqwest::Client,
    base_url: String,
    index_prefix: String,
    username: String,
    password: String,
}

impl EsRequestLogStore {
    pub fn new(base_url: String, index_prefix: String, username: String, password: String) -> Self {
        let client = reqwest::Client::builder()
            .timeout(ES_TIMEOUT)
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());
        let base_url = base_url.trim_end_matches('/').to_string();
        let index_prefix = if index_prefix.trim().is_empty() {
            DEFAULT_ES_INDEX_PREFIX.to_string()
        } else {
            index_prefix.trim().to_string()
        };
        Self {
            client,
            base_url,
            index_prefix,
            username,
            password,
        }
    }

    /// 按落库时间(unix 秒)计算按日索引名: <prefix>-YYYY.MM.DD (UTC)。
    fn index_for(&self, ts: i64) -> String {
        let dt = es_dt(ts).unwrap_or_else(chrono::Utc::now);
        format!("{}-{}", self.index_prefix, dt.format("%Y.%m.%d"))
    }

    fn authed(&self, req: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        if self.username.is_empty() {
            req
        } else {
            req.basic_auth(&self.username, Some(&self.password))
        }
    }

    /// ES 文档 = RequestLog 序列化 + @timestamp(ISO8601,Kibana 友好)。
    fn doc_value(&self, log: &RequestLog) -> anyhow::Result<serde_json::Value> {
        let mut v = serde_json::to_value(log).map_err(|e| anyhow::anyhow!("serde: {e}"))?;
        if let Some(obj) = v.as_object_mut() {
            obj.insert(
                "@timestamp".into(),
                serde_json::Value::String(es_timestamp(log.ts)),
            );
        }
        Ok(v)
    }
}

fn es_dt(ts: i64) -> Option<chrono::DateTime<chrono::Utc>> {
    use chrono::TimeZone;
    chrono::Utc.timestamp_opt(ts, 0).single()
}

/// unix 秒 → ISO8601 毫秒精度 UTC(如 1970-01-01T00:00:00.000Z)。
fn es_timestamp(ts: i64) -> String {
    es_dt(ts)
        .map(|dt| dt.to_rfc3339_opts(chrono::SecondsFormat::Millis, true))
        .unwrap_or_else(|| ts.to_string())
}

/// 构造 ES query 子句(bool 组合):关键字 / 失败筛选 / 时间窗。
fn es_query(q: &str, failed: Option<bool>, hours: Option<u64>) -> serde_json::Value {
    let mut must: Vec<serde_json::Value> = Vec::new();
    let mut filter: Vec<serde_json::Value> = Vec::new();
    let mut must_not: Vec<serde_json::Value> = Vec::new();

    let qt = q.trim();
    if !qt.is_empty() {
        must.push(serde_json::json!({
            "bool": {
                "should": [
                    { "wildcard": { "request_id.keyword": { "value": format!("*{qt}*") } } },
                    { "match_phrase_prefix": { "requested_model": qt } }
                ],
                "minimum_should_match": 1
            }
        }));
    }
    match failed {
        Some(true) => must_not.push(serde_json::json!({ "term": { "final_status": 200 } })),
        Some(false) => filter.push(serde_json::json!({ "term": { "final_status": 200 } })),
        None => {}
    }
    if let Some(h) = hours {
        let cutoff = now_secs().saturating_sub(h as i64 * 3600);
        filter.push(serde_json::json!({ "range": { "ts": { "gte": cutoff } } }));
    }

    let mut boolq = serde_json::Map::new();
    if !must.is_empty() {
        boolq.insert("must".into(), serde_json::Value::Array(must));
    }
    if !filter.is_empty() {
        boolq.insert("filter".into(), serde_json::Value::Array(filter));
    }
    if !must_not.is_empty() {
        boolq.insert("must_not".into(), serde_json::Value::Array(must_not));
    }
    serde_json::json!({ "bool": boolq })
}

impl RequestLogStore for EsRequestLogStore {
    fn write(
        &self,
        log: &RequestLog,
    ) -> Pin<Box<dyn Future<Output = anyhow::Result<()>> + Send + '_>> {
        let index = self.index_for(log.ts);
        let doc = match self.doc_value(log) {
            Ok(v) => v,
            Err(e) => return Box::pin(async move { Err(e) }),
        };
        // 以 request_id 为 _id,天然幂等(重放不产生重复文档)。
        let url = format!("{}/{}/_doc/{}", self.base_url, index, log.request_id);
        let authed = self.authed(self.client.post(&url).json(&doc));
        Box::pin(async move {
            let resp = authed
                .send()
                .await
                .map_err(|e| anyhow::anyhow!("es write: {e}"))?;
            let status = resp.status();
            // 409 = 同 _id 已存在,视为成功。
            if !status.is_success() && status.as_u16() != 409 {
                let body = resp.text().await.unwrap_or_default();
                anyhow::bail!("es write {index}: {} {body}", status);
            }
            Ok(())
        })
    }

    fn write_batch(
        &self,
        logs: Vec<RequestLog>,
    ) -> Pin<Box<dyn Future<Output = anyhow::Result<()>> + Send + '_>> {
        Box::pin(async move {
            if logs.is_empty() {
                return Ok(());
            }
            let mut ndjson = String::new();
            for log in &logs {
                let doc = match self.doc_value(log) {
                    Ok(v) => v,
                    Err(e) => {
                        tracing::warn!("write_batch: skip {}: {e}", log.request_id);
                        continue;
                    }
                };
                let action = serde_json::json!({
                    "index": { "_index": self.index_for(log.ts), "_id": log.request_id }
                });
                ndjson.push_str(&action.to_string());
                ndjson.push('\n');
                ndjson.push_str(&doc.to_string());
                ndjson.push('\n');
            }
            if ndjson.is_empty() {
                return Ok(());
            }
            let url = format!("{}/_bulk", self.base_url);
            let authed = self.authed(
                self.client
                    .post(&url)
                    .header(reqwest::header::CONTENT_TYPE, "application/x-ndjson")
                    .body(ndjson),
            );
            let resp = authed
                .send()
                .await
                .map_err(|e| anyhow::anyhow!("es bulk: {e}"))?;
            let status = resp.status();
            if !status.is_success() {
                let body = resp.text().await.unwrap_or_default();
                anyhow::bail!("es bulk: {} {body}", status);
            }
            // _bulk 的 HTTP 200 不代表全部成功,需检查 body.errors。
            let v: serde_json::Value = resp.json().await.unwrap_or_default();
            if v["errors"] == serde_json::Value::Bool(true) {
                let failed = v["items"]
                    .as_array()
                    .map(|items| {
                        items
                            .iter()
                            .filter(|it| it["index"]["error"].is_object())
                            .count()
                    })
                    .unwrap_or(0);
                tracing::warn!("es bulk: {failed} item(s) failed");
            }
            Ok(())
        })
    }

    fn recent(
        &self,
        q: &str,
        failed: Option<bool>,
        hours: Option<u64>,
        limit: u32,
        offset: u32,
    ) -> Pin<Box<dyn Future<Output = anyhow::Result<Vec<RequestLog>>> + Send + '_>> {
        let url = format!("{}/{}-*/_search", self.base_url, self.index_prefix);
        let limit = limit.clamp(1, 200);
        let body = serde_json::json!({
            "from": offset,
            "size": limit,
            "sort": [{ "ts": { "order": "desc", "unmapped_type": "long" } }],
            "query": es_query(q, failed, hours),
        });
        let authed = self.authed(self.client.post(&url).json(&body));
        Box::pin(async move {
            let resp = authed
                .send()
                .await
                .map_err(|e| anyhow::anyhow!("es search: {e}"))?;
            let status = resp.status();
            if !status.is_success() {
                let text = resp.text().await.unwrap_or_default();
                anyhow::bail!("es search: {} {text}", status);
            }
            let v: serde_json::Value = resp
                .json()
                .await
                .map_err(|e| anyhow::anyhow!("es search json: {e}"))?;
            let hits = v["hits"]["hits"].as_array().cloned().unwrap_or_default();
            let mut out = Vec::with_capacity(hits.len());
            for h in hits {
                match serde_json::from_value::<RequestLog>(h["_source"].clone()) {
                    Ok(log) => out.push(log),
                    Err(e) => tracing::warn!("es search: skip unmappable doc: {e}"),
                }
            }
            Ok(out)
        })
    }

    fn count(
        &self,
        q: &str,
        failed: Option<bool>,
        hours: Option<u64>,
    ) -> Pin<Box<dyn Future<Output = anyhow::Result<i64>> + Send + '_>> {
        let url = format!("{}/{}-*/_count", self.base_url, self.index_prefix);
        let body = serde_json::json!({ "query": es_query(q, failed, hours) });
        let authed = self.authed(self.client.post(&url).json(&body));
        Box::pin(async move {
            let resp = authed
                .send()
                .await
                .map_err(|e| anyhow::anyhow!("es count: {e}"))?;
            let status = resp.status();
            if !status.is_success() {
                let text = resp.text().await.unwrap_or_default();
                anyhow::bail!("es count: {} {text}", status);
            }
            let v: serde_json::Value = resp.json().await.unwrap_or_default();
            Ok(v["count"].as_i64().unwrap_or(0))
        })
    }

    /// 删除所有按日索引中超过 days 天的文档:delete_by_query 按 ts 范围过滤;404=尚无索引,视为 0 条。
    fn prune(&self, days: u64) -> Pin<Box<dyn Future<Output = anyhow::Result<u64>> + Send + '_>> {
        let url = format!("{}/{}-*/_delete_by_query", self.base_url, self.index_prefix);
        let cutoff = now_secs().saturating_sub(days as i64 * 3600);
        let body = serde_json::json!({
            "query": { "range": { "ts": { "lt": cutoff } } }
        });
        let authed = self.authed(self.client.post(&url).json(&body));
        Box::pin(async move {
            let resp = authed
                .send()
                .await
                .map_err(|e| anyhow::anyhow!("es prune: {e}"))?;
            let status = resp.status();
            if status.as_u16() == 404 {
                return Ok(0);
            }
            if !status.is_success() {
                let text = resp.text().await.unwrap_or_default();
                anyhow::bail!("es prune: {} {text}", status);
            }
            let v: serde_json::Value = resp.json().await.unwrap_or_default();
            Ok(v["deleted"].as_u64().unwrap_or(0))
        })
    }
}

// ======================== 工具函数 ========================

/// 按 Resolved 构造一个待尝试的 attempt 条目。
pub fn candidate_attempt(
    kind: ProviderKind,
    provider: &str,
    base_url: &str,
    upstream_model: &str,
    weight: u32,
) -> RequestAttempt {
    RequestAttempt {
        kind: kind_str(kind).to_string(),
        provider: provider.to_string(),
        base_url: base_url.to_string(),
        upstream_model: upstream_model.to_string(),
        weight,
        status: 0,
        latency_ms: 0,
        error: String::new(),
    }
}

// ======================== 单元测试 ========================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn candidate_attempt_fields() {
        let a = candidate_attempt(ProviderKind::Openai, "openai-official", "https://api.openai.com", "gpt-4", 200);
        assert_eq!(a.kind, "openai");
        assert_eq!(a.provider, "openai-official");
        assert_eq!(a.base_url, "https://api.openai.com");
        assert_eq!(a.upstream_model, "gpt-4");
        assert_eq!(a.weight, 200);
        assert_eq!(a.status, 0);
        assert_eq!(a.latency_ms, 0);
        assert!(a.error.is_empty());
    }

    #[test]
    fn request_attempt_json_roundtrip() {
        let a = RequestAttempt {
            kind: "anthropic".into(),
            provider: "anthropic-official".into(),
            base_url: "https://api.anthropic.com".into(),
            upstream_model: "claude-3".into(),
            weight: 100,
            status: 200,
            latency_ms: 42,
            error: String::new(),
        };
        let json = serde_json::to_string(&a).unwrap();
        let de: RequestAttempt = serde_json::from_str(&json).unwrap();
        assert_eq!(de.kind, "anthropic");
        assert_eq!(de.status, 200);
        assert_eq!(de.latency_ms, 42);
    }

    #[test]
    fn request_attempt_old_json_without_provider() {
        let old = r#"{"kind":"openai","base_url":"http://x","upstream_model":"gpt-4","weight":1,"status":200,"latency_ms":5,"error":""}"#;
        let de: RequestAttempt = serde_json::from_str(old).unwrap();
        assert_eq!(de.provider, "");
        assert_eq!(de.status, 200);
    }

    #[test]
    fn request_log_json_roundtrip() {
        let log = RequestLog {
            request_id: "req_test123".into(),
            user_id: "user-uuid".into(),
            path: "chat".into(),
            requested_model: "model-a".into(),
            stream: false,
            candidates: vec![RequestAttempt {
                kind: "openai".into(),
                provider: "openrouter".into(),
                base_url: "http://localhost".into(),
                upstream_model: "gpt-4".into(),
                weight: 100,
                status: 0,
                latency_ms: 0,
                error: String::new(),
            }],
            attempts: vec![RequestAttempt {
                kind: "openai".into(),
                provider: "openrouter".into(),
                base_url: "http://localhost".into(),
                upstream_model: "gpt-4".into(),
                weight: 100,
                status: 200,
                latency_ms: 42,
                error: String::new(),
            }],
            final_kind: Some("openai".into()),
            final_upstream_model: Some("gpt-4".into()),
            final_status: 200,
            latency_ms: 42,
            input_tokens: 12,
            output_tokens: 8,
            charged_tokens: 20,
            ts: 1726000000,
            req_body: Some("{\"model\":\"model-a\"}".into()),
            resp_body: None,
        };
        let json = serde_json::to_string(&log).unwrap();
        // req_body 有值应序列化,resp_body 为 None 应被跳过。
        assert!(json.contains("req_body"));
        assert!(!json.contains("resp_body"));
        let de: RequestLog = serde_json::from_str(&json).unwrap();
        assert_eq!(de.request_id, "req_test123");
        assert_eq!(de.candidates.len(), 1);
        assert_eq!(de.attempts[0].status, 200);
        assert_eq!(de.charged_tokens, 20);
        assert_eq!(de.req_body.as_deref(), Some("{\"model\":\"model-a\"}"));
        assert_eq!(de.resp_body, None);
    }

    #[test]
    fn request_log_old_json_without_bodies() {
        let old = r#"{"request_id":"r1","user_id":"u","path":"chat","requested_model":"m","stream":false,"candidates":[],"attempts":[],"final_kind":null,"final_upstream_model":null,"final_status":200,"latency_ms":1,"input_tokens":0,"output_tokens":0,"charged_tokens":0,"ts":1}"#;
        let de: RequestLog = serde_json::from_str(old).unwrap();
        assert_eq!(de.req_body, None);
        assert_eq!(de.resp_body, None);
    }

    #[test]
    fn body_preview_truncates_safely() {
        assert_eq!(body_preview(b"hello world", 5).as_deref(), Some("hello"));
        assert_eq!(body_preview(b"", 100), None);
        assert_eq!(body_preview(b"   ", 100), None);
        assert_eq!(body_preview(b"abc", 0), None);
        assert_eq!(body_preview(b"abc", 100).as_deref(), Some("abc"));
        // "你好" 共 6 字节,截 4 字节会切碎第二个汉字 → 回退到有效前缀 "你"。
        let hi = "你好".as_bytes();
        assert_eq!(body_preview(hi, 4).as_deref(), Some("你"));
        assert_eq!(body_preview(hi, 6).as_deref(), Some("你好"));
    }

    #[test]
    fn es_timestamp_format() {
        assert_eq!(es_timestamp(0), "1970-01-01T00:00:00.000Z");
        assert!(es_timestamp(1726000000).ends_with('Z'));
        assert_eq!(es_timestamp(1726000000).len(), 24);
    }

    #[test]
    fn es_query_filters() {
        let q = es_query("", None, None);
        assert_eq!(q["bool"].as_object().map(|m| m.len()), Some(0));

        let q = es_query("req_1", Some(true), Some(24));
        let must_not = q["bool"]["must_not"].as_array().unwrap();
        assert_eq!(must_not[0]["term"]["final_status"], 200);
        assert!(q["bool"]["filter"].as_array().unwrap()[0]["range"]["ts"]["gte"].is_i64());
        let should = q["bool"]["must"][0]["bool"]["should"].as_array().unwrap();
        assert!(should[0]["wildcard"]["request_id.keyword"]["value"]
            .as_str()
            .unwrap()
            .contains("req_1"));

        let q = es_query("m1", Some(false), None);
        assert_eq!(q["bool"]["filter"][0]["term"]["final_status"], 200);
        assert!(q["bool"]["must_not"].is_null());
    }

    #[test]
    fn kind_str_mapping() {
        assert_eq!(kind_str(ProviderKind::Openai), "openai");
        assert_eq!(kind_str(ProviderKind::Anthropic), "anthropic");
    }

    // ---- SQLite 落库往返(防 bind 缺 body 回归) ----

    async fn reqlog_test_db() -> Db {
        sqlx::any::install_default_drivers();
        let pool = sqlx::pool::PoolOptions::<sqlx::Any>::new()
            .max_connections(1)
            .connect("sqlite::memory:?mode=rwc")
            .await
            .expect("connect in-memory sqlite");
        crate::storage::init_schema(&pool).await.expect("init_schema");
        pool
    }

    fn body_log(id: &str, rb: Option<&str>, sb: Option<&str>) -> RequestLog {
        RequestLog {
            request_id: id.into(),
            user_id: "u1".into(),
            path: "chat".into(),
            requested_model: "m1".into(),
            stream: false,
            candidates: vec![],
            attempts: vec![],
            final_kind: Some("openai".into()),
            final_upstream_model: Some("m1".into()),
            final_status: 200,
            latency_ms: 5,
            input_tokens: 1,
            output_tokens: 2,
            charged_tokens: 3,
            ts: 1726000000,
            req_body: rb.map(|s| s.into()),
            resp_body: sb.map(|s| s.into()),
        }
    }

    #[tokio::test]
    async fn sqlite_write_recent_body_roundtrip() {
        let store = SqliteRequestLogStore::new(reqlog_test_db().await);
        store
            .write(&body_log("req_w1", Some("req-payload"), Some("resp-payload")))
            .await
            .unwrap();
        let got = store.recent("", None, None, 10, 0).await.unwrap();
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].request_id, "req_w1");
        assert_eq!(got[0].req_body.as_deref(), Some("req-payload"));
        assert_eq!(got[0].resp_body.as_deref(), Some("resp-payload"));
    }

    #[tokio::test]
    async fn sqlite_write_batch_body_roundtrip() {
        let store = SqliteRequestLogStore::new(reqlog_test_db().await);
        store
            .write_batch(vec![
                body_log("req_b1", Some("b1-req"), None),
                body_log("req_b2", None, Some("b2-resp")),
            ])
            .await
            .unwrap();
        // recent 按 id DESC:后写入的 b2 在前。
        let got = store.recent("", None, None, 10, 0).await.unwrap();
        assert_eq!(got.len(), 2);
        assert_eq!(got[0].request_id, "req_b2");
        assert_eq!(got[0].req_body, None);
        assert_eq!(got[0].resp_body.as_deref(), Some("b2-resp"));
        assert_eq!(got[1].request_id, "req_b1");
        assert_eq!(got[1].req_body.as_deref(), Some("b1-req"));
        assert_eq!(got[1].resp_body, None);
    }
}
