//! 语义缓存 L1:精确哈希响应缓存(进程内 RwLock<HashMap> + TTL,零外部依赖)。
//!
//! - 缓存键 = sha256(path | model | provider | upstream_model | stream | 归一化请求体),
//!   按 model/provider/upstream_model 隔离;stream 进入键,SSE 与 JSON 各自精确回放。
//! - 命中在熔断检查之前:上游熔断开启时缓存仍可服务。
//! - 存储由 handlers 的 tee 包装(cache_wrap)完成:旁路留存返回给客户端的字节,
//!   流完整结束才落缓存(客户端中断/上游出错不缓存)。
//! - L2(embedding 相似度)预留:similarity 字段与 threshold 配置先行落位。
//! - 锁纪律:请求热路径只有单条 get/insert(短临界区,无嵌套锁),全表容量操作
//!   (清过期/逐出)只在后台 sweep 执行;统计锁一律 try_lock 降级,拿不到即丢弃,
//!   绝不阻塞请求 —— 避免 shard 锁等待在 async 上下文中占满 worker 线程。

use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, RwLock};

use axum::body::{Body, Bytes};
use axum::response::Response;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use crate::storage as storage_crate;

/// 当前 Unix 秒(u64;storage::now_secs 返回 i64,纪元秒恒为正,收窄安全)。
pub fn now_secs() -> u64 {
    storage_crate::now_secs() as u64
}

/// 最近命中记录上限(ring)。
const RECENT_CAP: usize = 200;
/// 命中/未命中事件上限(用于 12×5min 趋势桶,余量充足)。
const EVENTS_CAP: usize = 6000;
/// 缓存条目上限(超出由后台 sweep 逐出,热路径不检查)。
const MAX_ENTRIES: usize = 2048;
/// 单条缓存响应体上限(超大响应不缓存,防内存放大;cache_wrap 累积过程同步检查)。
pub(crate) const MAX_BODY: usize = 4 * 1024 * 1024;

/// 一条缓存的响应。
#[derive(Debug, Clone)]
pub struct Entry {
    /// 回放时的 Content-Type(text/event-stream | application/json)。
    pub content_type: String,
    /// 完整响应体(SSE 为原始字节流,JSON 为序列化结果)。
    pub body: Bytes,
    pub input_tokens: u32,
    pub output_tokens: u32,
    pub expires_at: u64,
}

/// 一次缓存命中记录(供「最近命中」表格)。
#[derive(Debug, Clone)]
pub struct HitRecord {
    pub ts: u64,
    /// "exact"(L1);L2 将为 "semantic"。
    pub hit_type: String,
    /// 语义相似度(L1 精确命中为 None,前端显示 —)。
    pub similarity: Option<f64>,
    pub model: String,
    pub provider: String,
    /// 命中节省的 tokens(input + output)。
    pub tokens_saved: u64,
}

/// 语义缓存存储 + 统计。
pub struct SemanticCache {
    map: RwLock<HashMap<String, Entry>>,
    hits: AtomicU64,
    misses: AtomicU64,
    tokens_saved: AtomicU64,
    recent: Mutex<VecDeque<HitRecord>>,
    events: Mutex<VecDeque<(u64, bool)>>,
}

impl Default for SemanticCache {
    fn default() -> Self {
        Self::new()
    }
}

impl SemanticCache {
    pub fn new() -> Self {
        Self {
            map: RwLock::new(HashMap::new()),
            hits: AtomicU64::new(0),
            misses: AtomicU64::new(0),
            tokens_saved: AtomicU64::new(0),
            recent: Mutex::new(VecDeque::new()),
            events: Mutex::new(VecDeque::new()),
        }
    }

    /// 取缓存(惰性过期:过期即移除并视为未命中)。
    pub fn get(&self, key: &str) -> Option<Entry> {
        let now = now_secs();
        {
            let m = self.map.read().ok()?;
            if let Some(e) = m.get(key) {
                if e.expires_at > now {
                    return Some(e.clone());
                }
            }
        }
        // 过期清理用 try_write:拿不到就交给后台 sweep,热路径绝不久等。
        if let Ok(mut m) = self.map.try_write() {
            m.remove(key);
        }
        None
    }

    /// 存缓存(热路径只做单条插入;容量上限由后台 sweep 兜底;超大响应体拒绝)。
    pub fn store(&self, key: String, entry: Entry) {
        if entry.body.len() > MAX_BODY {
            return;
        }
        if let Ok(mut m) = self.map.write() {
            m.insert(key, entry);
        }
    }

    /// 清理全部过期条目 + 容量兜底逐出(后台 5s tick 调用,全表操作不进请求热路径)。
    pub fn sweep(&self) {
        let now = now_secs();
        let Ok(mut m) = self.map.write() else { return };
        m.retain(|_, e| e.expires_at > now);
        // 软上限留 25% 缓冲:超过 cap 时按 expires_at 从旧到新逐出至 3/4,
        // 避免请求热路径在满容时抖动。
        let cap = MAX_ENTRIES + MAX_ENTRIES / 4;
        if m.len() > cap {
            let floor = MAX_ENTRIES - MAX_ENTRIES / 4;
            let mut olds: Vec<(String, u64)> =
                m.iter().map(|(k, e)| (k.clone(), e.expires_at)).collect();
            olds.sort_by_key(|(_, ex)| *ex);
            for (k, _) in olds.into_iter().take(m.len() - floor) {
                m.remove(&k);
            }
        }
    }

    /// 清空缓存与统计(管理端「清空缓存」)。
    pub fn clear(&self) {
        if let Ok(mut m) = self.map.write() {
            m.clear();
        }
        self.hits.store(0, Ordering::Relaxed);
        self.misses.store(0, Ordering::Relaxed);
        self.tokens_saved.store(0, Ordering::Relaxed);
        if let Ok(mut r) = self.recent.lock() {
            r.clear();
        }
        if let Ok(mut e) = self.events.lock() {
            e.clear();
        }
    }

    /// 条目数(仅测试断言用)。
    #[cfg(test)]
    pub fn len(&self) -> usize {
        self.map.read().map(|m| m.len()).unwrap_or(0)
    }

    /// 是否为空(仅测试断言用)。
    #[cfg(test)]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// 记录一次命中(统计 + 最近记录 + 事件)。
    pub fn record_hit(&self, rec: HitRecord) {
        self.hits.fetch_add(1, Ordering::Relaxed);
        self.tokens_saved
            .fetch_add(rec.tokens_saved, Ordering::Relaxed);
        // try_lock 降级:最近记录非关键路径,锁竞争时丢弃,绝不阻塞请求。
        if let Ok(mut r) = self.recent.try_lock() {
            r.push_back(rec);
            while r.len() > RECENT_CAP {
                r.pop_front();
            }
        }
        self.push_event(true);
    }

    /// 记录一次未命中(每请求至多一次,由调用方去重)。
    pub fn record_miss(&self) {
        self.misses.fetch_add(1, Ordering::Relaxed);
        self.push_event(false);
    }

    fn push_event(&self, is_hit: bool) {
        // try_lock 降级:统计事件非关键路径,锁竞争时丢弃,绝不阻塞请求。
        if let Ok(mut e) = self.events.try_lock() {
            e.push_back((now_secs(), is_hit));
            while e.len() > EVENTS_CAP {
                e.pop_front();
            }
        }
    }

    /// 统计快照:命中率大数字 + 节省 tokens + 条数 + 12×5min 趋势(旧→新)。
    pub fn stats_snapshot(&self) -> Value {
        let hits = self.hits.load(Ordering::Relaxed);
        let misses = self.misses.load(Ordering::Relaxed);
        let total = hits + misses;
        let rate = if total > 0 {
            hits as f64 / total as f64
        } else {
            0.0
        };
        let bucket = 300u64;
        let now = now_secs();
        let cur_start = now - (now % bucket);
        let oldest = cur_start - 11 * bucket;
        let mut trend: Vec<Value> = (0..12)
            .map(|i| {
                json!({ "ts": oldest + i * bucket, "hits": 0, "misses": 0 })
            })
            .collect();
        if let Ok(e) = self.events.try_lock() {
            for &(ts, is_hit) in e.iter() {
                if ts < oldest {
                    continue;
                }
                let idx = ((ts - oldest) / bucket) as usize;
                if idx < 12 {
                    if let Some(obj) = trend[idx].as_object_mut() {
                        let k = if is_hit { "hits" } else { "misses" };
                        let cur = obj.get(k).and_then(|v| v.as_u64()).unwrap_or(0);
                        obj.insert(k.to_string(), Value::from(cur + 1));
                    }
                }
            }
        }
        json!({
            "hits": hits,
            "misses": misses,
            "hit_rate": (rate * 1000.0).round() / 1000.0,
            "tokens_saved": self.tokens_saved.load(Ordering::Relaxed),
            "entries": self.map.read().map(|m| m.len()).unwrap_or(0),
            "trend": trend,
        })
    }

    /// 最近命中记录(新→旧)。
    pub fn recent_hits(&self, limit: usize) -> Vec<Value> {
        match self.recent.lock() {
            Ok(r) => r
                .iter()
                .rev()
                .take(limit)
                .map(|h| {
                    json!({
                        "ts": h.ts,
                        "hit_type": h.hit_type,
                        "similarity": h.similarity,
                        "model": h.model,
                        "provider": h.provider,
                        "tokens_saved": h.tokens_saved,
                    })
                })
                .collect(),
            Err(_) => Vec::new(),
        }
    }
}

/// 构造缓存键:sha256(path|model|provider|upstream_model|stream|归一化请求)。
/// 请求体中的 model/stream 字段移除(已单独进键);按 model/provider 隔离。
pub fn make_key(
    path: &str,
    model: &str,
    provider: &str,
    upstream_model: &str,
    stream: bool,
    req: &Value,
) -> String {
    let mut q = req.clone();
    if let Some(obj) = q.as_object_mut() {
        obj.remove("model");
        obj.remove("stream");
    }
    let payload = json!({
        "path": path,
        "m": model,
        "p": provider,
        "u": upstream_model,
        "s": stream,
        "q": q,
    });
    let bytes = serde_json::to_vec(&payload).unwrap_or_default();
    let digest = Sha256::digest(&bytes);
    hex::encode(digest)
}

/// 缓存资格:messages 数组非空且条数 ≤ multi_turn_max(多轮超过阈值跳过)。
pub fn eligible(req: &Value, multi_turn_max: usize) -> bool {
    match req.get("messages").and_then(|m| m.as_array()) {
        Some(arr) => !arr.is_empty() && arr.len() <= multi_turn_max,
        None => false,
    }
}

/// 从响应体提取 usage(兼容 OpenAI 与 Anthropic 两种键位;SSE 逐行扫)。
pub fn extract_usage(body: &[u8], sse: bool) -> (u32, u32) {
    if sse {
        let mut input = 0u32;
        let mut output = 0u32;
        let text = String::from_utf8_lossy(body);
        for line in text.lines() {
            let Some(p) = line.trim().strip_prefix("data:") else {
                continue;
            };
            let p = p.trim();
            if p.is_empty() || p == "[DONE]" {
                continue;
            }
            if let Ok(v) = serde_json::from_str::<Value>(p) {
                let (i, o) = usage_from(&v);
                if i > 0 {
                    input = i;
                }
                if o > 0 {
                    output = o;
                }
            }
        }
        (input, output)
    } else {
        serde_json::from_slice::<Value>(body)
            .map(|v| usage_from(&v))
            .unwrap_or((0, 0))
    }
}

/// usage 键位兼容:OpenAI(prompt_tokens/completion_tokens)与
/// Anthropic(input_tokens/output_tokens,含 message_start 的嵌套路径)。
fn usage_from(v: &Value) -> (u32, u32) {
    let get = |keys: &[&str]| -> u32 {
        for k in keys {
            if let Some(x) = v.pointer(k).and_then(|x| x.as_u64()) {
                return x as u32;
            }
        }
        0
    };
    let input = get(&[
        "/usage/prompt_tokens",
        "/usage/input_tokens",
        "/message/usage/input_tokens",
    ]);
    let output = get(&[
        "/usage/completion_tokens",
        "/usage/output_tokens",
        "/message/usage/output_tokens",
    ]);
    (input, output)
}

/// 判定请求是否确定性:temperature <= 0.001 或显式带 seed(缺省都不算,宁可不缓存)。
pub fn deterministic(req: &serde_json::Value) -> bool {
    if let Some(t) = req.get("temperature").and_then(|v| v.as_f64()) {
        if t <= 0.001 {
            return true;
        }
    }
    matches!(req.get("seed"), Some(v) if !v.is_null())
}

/// 命中扣费:tokens 照实入日志,charged 按路由倍率 × 用户倍率 × 折扣率取整。
pub fn cache_charged(input: u32, output: u32, multiplier: f64, user_multiplier: f64, billing_ratio: f64) -> i64 {
    ((input as f64 + output as f64) * multiplier * user_multiplier * billing_ratio).round() as i64
}

/// 回放缓存命中:原字节 + 原 Content-Type + x-relay-cache: HIT (类型) + 透传头。
pub fn replay_response(entry: Entry, provider: &str, request_id: &str, hit_type: &str) -> Response {
    let mut resp = Response::builder()
        .header("content-type", entry.content_type)
        .header("x-relay-cache", format!("HIT ({hit_type})"))
        .body(Body::from(entry.body))
        .expect("static replay response parts");
    if let Ok(hv) = axum::http::HeaderValue::from_str(provider) {
        resp.headers_mut().insert("x-relay-upstream", hv);
    }
    if let Ok(hv) = axum::http::HeaderValue::from_str(request_id) {
        resp.headers_mut().insert("x-relay-request-id", hv);
    }
    resp
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(body: &str, ttl: u64) -> Entry {
        Entry {
            content_type: "application/json".into(),
            body: Bytes::from(body.to_string()),
            input_tokens: 10,
            output_tokens: 20,
            expires_at: now_secs() + ttl,
        }
    }

    #[test]
    fn make_key_is_stable_and_isolated() {
        let req = json!({"model":"gpt-x","messages":[{"role":"user","content":"hi"}],"stream":true});
        let a = make_key("chat", "gpt-x", "prov", "up-x", true, &req);
        let b = make_key("chat", "gpt-x", "prov", "up-x", true, &req);
        assert_eq!(a, b);
        assert_eq!(a.len(), 64);
        // provider 隔离
        let c = make_key("chat", "gpt-x", "prov2", "up-x", true, &req);
        assert_ne!(a, c);
        // stream 隔离
        let d = make_key("chat", "gpt-x", "prov", "up-x", false, &req);
        assert_ne!(a, d);
        // path 隔离
        let e = make_key("messages", "gpt-x", "prov", "up-x", true, &req);
        assert_ne!(a, e);
        // 请求体扰动改变键
        let mut req2 = req.clone();
        req2["temperature"] = json!(0.7);
        let f = make_key("chat", "gpt-x", "prov", "up-x", true, &req2);
        assert_ne!(a, f);
    }

    #[test]
    fn eligible_checks_messages_len() {
        let one = json!({"messages":[{"role":"user","content":"hi"}]});
        assert!(eligible(&one, 3));
        let three = json!({"messages":[{"role":"user"},{"role":"assistant"},{"role":"user"}]});
        assert!(eligible(&three, 3));
        assert!(!eligible(&three, 2));
        let empty = json!({"messages":[]});
        assert!(!eligible(&empty, 3));
        let none = json!({"prompt":"x"});
        assert!(!eligible(&none, 3));
    }

    #[test]
    fn deterministic_four_branches() {
        let missing = json!({"messages":[{"role":"user","content":"hi"}]});
        assert!(!deterministic(&missing));
        let zero = json!({"temperature": 0});
        assert!(deterministic(&zero));
        let tiny = json!({"temperature": 0.0005});
        assert!(deterministic(&tiny));
        let hot = json!({"temperature": 0.7});
        assert!(!deterministic(&hot));
        let seeded = json!({"temperature": 0.9, "seed": 42});
        assert!(deterministic(&seeded));
        let null_seed = json!({"seed": null});
        assert!(!deterministic(&null_seed));
    }

    #[test]
    fn cache_charged_scales_with_ratio() {
        // (100+200) × 1.5 × 2.0 × 0.5 = 450
        assert_eq!(cache_charged(100, 200, 1.5, 2.0, 0.5), 450);
        assert_eq!(cache_charged(100, 200, 1.5, 2.0, 0.0), 0);
        assert_eq!(cache_charged(10, 20, 1.0, 1.0, 1.0), 30);
    }

    #[test]
    fn replay_header_marks_hit_type() {
        let resp = replay_response(entry("{\"ok\":1}", 3600), "prov", "req-1", "exact");
        assert_eq!(resp.headers().get("x-relay-cache").unwrap().to_str().unwrap(), "HIT (exact)");
        let resp2 = replay_response(entry("{\"ok\":2}", 3600), "prov", "req-1", "semantic");
        assert_eq!(resp2.headers().get("x-relay-cache").unwrap().to_str().unwrap(), "HIT (semantic)");
    }

    #[test]
    fn store_get_expire_and_clear() {
        let c = SemanticCache::new();
        c.store("k1".into(), entry("{\"ok\":1}", 3600));
        assert_eq!(c.len(), 1);
        let e = c.get("k1").unwrap();
        assert_eq!(e.body.as_ref(), b"{\"ok\":1}");
        assert_eq!(e.input_tokens, 10);
        // 过期 → None 且移除
        c.store("k2".into(), entry("{\"ok\":2}", 0));
        assert!(c.get("k2").is_none());
        assert_eq!(c.len(), 1);
        c.clear();
        assert!(c.is_empty());
        assert!(c.get("k1").is_none());
    }

    #[test]
    fn stats_and_recent() {
        let c = SemanticCache::new();
        c.record_miss();
        c.record_hit(HitRecord {
            ts: now_secs(),
            hit_type: "exact".into(),
            similarity: None,
            model: "gpt-x".into(),
            provider: "prov".into(),
            tokens_saved: 30,
        });
        let s = c.stats_snapshot();
        assert_eq!(s["hits"], 1);
        assert_eq!(s["misses"], 1);
        assert_eq!(s["tokens_saved"], 30);
        let rate = s["hit_rate"].as_f64().unwrap();
        assert!((rate - 0.5).abs() < 1e-9);
        assert_eq!(s["trend"].as_array().unwrap().len(), 12);
        let recent = c.recent_hits(10);
        assert_eq!(recent.len(), 1);
        assert_eq!(recent[0]["hit_type"], "exact");
        assert!(recent[0]["similarity"].is_null());
    }

    #[test]
    fn extract_usage_openai_and_anthropic() {
        let oai = br#"{"id":"1","usage":{"prompt_tokens":11,"completion_tokens":22}}"#;
        assert_eq!(extract_usage(oai, false), (11, 22));
        let anth = br#"{"id":"msg_1","usage":{"input_tokens":33,"output_tokens":44}}"#;
        assert_eq!(extract_usage(anth, false), (33, 44));
        let sse = b"data: {\"type\":\"message_start\",\"message\":{\"usage\":{\"input_tokens\":7,\"output_tokens\":0}}}\n\ndata: [DONE]\n\n";
        assert_eq!(extract_usage(sse, true), (7, 0));
        let sse2 = b"data: {\"usage\":{\"prompt_tokens\":5,\"completion_tokens\":9}}\n\ndata: [DONE]\n\n";
        assert_eq!(extract_usage(sse2, true), (5, 9));
    }
}
