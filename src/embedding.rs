//! L2 语义缓存:OpenAI 兼容 embedding 客户端 + 余弦相似度。
//!
//! 调用约定:embedding 属于旁路增强,任何失败(网络/解析/超时)都不得影响
//! 主请求路径 —— 调用方应在错误时静默降级为 L1 miss。
//! 向量以 f32 little-endian 字节存于 SQLite cache_vectors 表,暴力余弦扫描。

use serde_json::Value;

/// 单条 embedding 文本上限(字符):请求摘要超长时截断,控制上游耗时与费用。
pub const MAX_EMBED_CHARS: usize = 4000;
/// 余弦扫描单次最多拉取向量行数,防止表无限增长拖慢查询。
pub const SCAN_LIMIT: i64 = 4096;

/// 从 chat 请求体抽取 embedding 输入文本:逐条拼接 role+content。
/// 与 L1 的 make_key 使用同一份请求语义(排除 model/stream 后的 messages);
/// 空 messages 或无可抽取内容时返回 None(调用方跳过 L2)。
pub fn embed_text(req: &Value) -> Option<String> {
    let msgs = req.get("messages")?.as_array()?;
    let mut parts: Vec<String> = Vec::with_capacity(msgs.len());
    for m in msgs {
        let role = m.get("role").and_then(|v| v.as_str()).unwrap_or("");
        let content = match m.get("content") {
            Some(Value::String(s)) => s.clone(),
            Some(Value::Array(arr)) => arr
                .iter()
                .filter_map(|p| p.get("text").and_then(|t| t.as_str()))
                .collect::<Vec<_>>()
                .join(" "),
            _ => String::new(),
        };
        if content.is_empty() && role.is_empty() {
            continue;
        }
        parts.push(format!("{role}: {content}"));
    }
    let joined = parts.join("\n");
    if joined.trim().is_empty() {
        return None;
    }
    let mut s: String = joined.chars().take(MAX_EMBED_CHARS).collect();
    s.shrink_to_fit();
    Some(s)
}

/// 一次 embedding 批量调用的结果:向量(顺序与输入一致)+ 上游用量 tokens。
pub struct EmbedBatch {
    pub vectors: Vec<Vec<f32>>,
    pub prompt_tokens: u32,
}

/// 调用 OpenAI 兼容 /embeddings,一次请求多向量;超时由调用方按场景设定
/// (语义缓存旁路 3s / 对外端点 30s)。返回向量顺序与输入一一对应,数量不符视为失败。
pub async fn embed_batch(
    http: &reqwest::Client,
    base_url: &str,
    api_key: &str,
    model: &str,
    texts: &[String],
    timeout_secs: u64,
) -> anyhow::Result<EmbedBatch> {
    let url = format!("{}/embeddings", base_url.trim_end_matches('/'));
    let mut rb = http
        .post(&url)
        .timeout(std::time::Duration::from_secs(timeout_secs))
        .json(&serde_json::json!({ "model": model, "input": texts }));
    if !api_key.is_empty() {
        rb = rb.bearer_auth(api_key);
    }
    let resp = rb.send().await?;
    let status = resp.status();
    if !status.is_success() {
        anyhow::bail!("embeddings 上游返回 {status}");
    }
    let v: Value = resp.json().await?;
    let data = v
        .get("data")
        .and_then(|d| d.as_array())
        .ok_or_else(|| anyhow::anyhow!("embeddings 响应缺少 data 数组"))?;
    if data.len() != texts.len() {
        anyhow::bail!(
            "embeddings 返回 {} 条向量,与输入 {} 条不符",
            data.len(),
            texts.len()
        );
    }
    let mut vectors = Vec::with_capacity(data.len());
    for item in data {
        let arr = item
            .get("embedding")
            .and_then(|e| e.as_array())
            .ok_or_else(|| anyhow::anyhow!("embeddings 响应缺少 embedding 字段"))?;
        let mut vec = Vec::with_capacity(arr.len());
        for x in arr {
            let f = x.as_f64().ok_or_else(|| anyhow::anyhow!("embedding 含非数值"))?;
            vec.push(f as f32);
        }
        if vec.is_empty() {
            anyhow::bail!("embedding 为空");
        }
        vectors.push(vec);
    }
    let prompt_tokens = v
        .pointer("/usage/prompt_tokens")
        .and_then(|t| t.as_u64())
        .unwrap_or(0) as u32;
    Ok(EmbedBatch { vectors, prompt_tokens })
}

/// 单文本快捷调用(语义缓存旁路专用):固定 3s 超时,失败即 Err,不得影响主请求路径。
pub async fn embed(
    http: &reqwest::Client,
    base_url: &str,
    api_key: &str,
    model: &str,
    text: &str,
) -> anyhow::Result<Vec<f32>> {
    let texts = [text.to_string()];
    let mut batch = embed_batch(http, base_url, api_key, model, &texts, 3).await?;
    Ok(batch.vectors.remove(0))
}

/// 暴力余弦相似度;长度不一致或零向量时返回 0(视为不相似,不命中)。
pub fn cosine(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }
    let (mut dot, mut na, mut nb) = (0f32, 0f32, 0f32);
    for i in 0..a.len() {
        dot += a[i] * b[i];
        na += a[i] * a[i];
        nb += b[i] * b[i];
    }
    if na == 0.0 || nb == 0.0 {
        return 0.0;
    }
    dot / (na.sqrt() * nb.sqrt())
}

/// f32 向量 → little-endian 字节(SQLite BLOB)。
pub fn vec_to_bytes(v: &[f32]) -> Vec<u8> {
    let mut out = Vec::with_capacity(v.len() * 4);
    for f in v {
        out.extend_from_slice(&f.to_le_bytes());
    }
    out
}

/// SQLite BLOB → f32 向量;长度非 4 倍数时返回 None(脏数据,跳过)。
pub fn bytes_to_vec(b: &[u8]) -> Option<Vec<f32>> {
    if b.is_empty() || b.len() % 4 != 0 {
        return None;
    }
    Some(
        b.chunks_exact(4)
            .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
            .collect(),
    )
}

/// 向量 → base64(cache_vectors.embedding 列的存储格式;跨 SQLite/Postgres 均为 TEXT)。
pub fn encode_vec_b64(v: &[f32]) -> String {
    use base64::{engine::general_purpose::STANDARD as B64, Engine};
    B64.encode(vec_to_bytes(v))
}

/// base64 → 向量;格式非法时返回 None(脏数据,跳过)。
pub fn decode_vec_b64(s: &str) -> Option<Vec<f32>> {
    use base64::{engine::general_purpose::STANDARD as B64, Engine};
    bytes_to_vec(&B64.decode(s).ok()?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn embed_text_joins_roles_and_contents() {
        let req = json!({
            "model": "m",
            "messages": [
                {"role": "system", "content": "你是助手"},
                {"role": "user", "content": "介绍上海"}
            ]
        });
        let t = embed_text(&req).unwrap();
        assert_eq!(t, "system: 你是助手\nuser: 介绍上海");
    }

    #[test]
    fn embed_text_handles_array_content_and_empty() {
        let req = json!({
            "messages": [
                {"role": "user", "content": [
                    {"type": "text", "text": "部分一"},
                    {"type": "text", "text": "部分二"}
                ]}
            ]
        });
        assert_eq!(embed_text(&req).unwrap(), "user: 部分一 部分二");
        // 空 messages → None
        assert!(embed_text(&json!({"messages": []})).is_none());
        // 无 messages → None
        assert!(embed_text(&json!({"model": "m"})).is_none());
    }

    #[test]
    fn embed_text_truncates_to_limit() {
        let long = "字".repeat(MAX_EMBED_CHARS + 500);
        let req = json!({"messages": [{"role": "user", "content": long}]});
        let t = embed_text(&req).unwrap();
        assert_eq!(t.chars().count(), MAX_EMBED_CHARS);
    }

    #[test]
    fn cosine_basics() {
        let a = [1.0f32, 0.0, 0.0];
        let b = [1.0f32, 0.0, 0.0];
        assert!((cosine(&a, &b) - 1.0).abs() < 1e-6);
        let c = [0.0f32, 1.0, 0.0];
        assert!(cosine(&a, &c).abs() < 1e-6);
        let d = [-1.0f32, 0.0, 0.0];
        assert!((cosine(&a, &d) + 1.0).abs() < 1e-6);
        // 长度不一致 / 零向量 → 0
        assert_eq!(cosine(&a, &[1.0, 0.0]), 0.0);
        assert_eq!(cosine(&a, &[0.0, 0.0, 0.0]), 0.0);
    }

    #[test]
    fn vec_bytes_roundtrip() {
        let v = vec![0.25f32, -1.5, 3.75, 0.0, 1e-8];
        let back = bytes_to_vec(&vec_to_bytes(&v)).unwrap();
        assert_eq!(v, back);
        // 脏数据
        assert!(bytes_to_vec(&[1, 2, 3]).is_none());
        assert!(bytes_to_vec(&[]).is_none());
    }

    #[test]
    fn vec_b64_roundtrip() {
        let v = vec![0.1f32, -2.5, 7.25];
        assert_eq!(decode_vec_b64(&encode_vec_b64(&v)).unwrap(), v);
        assert!(decode_vec_b64("not-base64!!").is_none());
        assert!(decode_vec_b64("").is_none());
    }
}
