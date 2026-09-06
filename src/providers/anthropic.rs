use std::time::Duration;

use serde_json::Value;

use crate::error::ApiError;
use crate::state::AppState;

/// 向 Anthropic Messages API 发起请求,返回原始流式/非流式响应。
/// 非流式请求按 [proxy].upstream_timeout_secs 设总超时;流式请求不设总超时。
/// 传输失败(连接/超时)、429 与 5xx 记作可重试的 `Unavailable`;其余错误为非重试 `Upstream`。
pub async fn messages(
    state: &AppState,
    base_url: &str,
    api_key: Option<&str>,
    body: Value,
) -> Result<reqwest::Response, ApiError> {
    let url = format!("{}/messages", base_url.trim_end_matches('/'));
    // 流式标记从请求体读取(anthropic 转发无独立 stream 参数)。
    let stream = body
        .get("stream")
        .and_then(|s| s.as_bool())
        .unwrap_or(false);
    let mut rb = state
        .http
        .post(&url)
        .header("anthropic-version", "2023-06-01");
    if let Some(k) = api_key.filter(|k| !k.is_empty()) {
        rb = rb.header("x-api-key", k);
    }
    // 非流式请求设总超时;流式请求不设,连接建立后靠上游自然结束/对端断开兜底。
    if !stream {
        rb = rb.timeout(Duration::from_secs(
            state.config.load().proxy.upstream_timeout_secs,
        ));
    }

    let resp = rb
        .json(&body)
        .send()
        .await
        .map_err(|e| ApiError::Unavailable(format!("transport: {e}")))?;

    if !resp.status().is_success() {
        let code = resp.status().as_u16();
        let text = resp.text().await.unwrap_or_default();
        if code == 401 || code == 403 || code == 429 || code >= 500 {
            return Err(ApiError::Unavailable(format!("{code}: {text}")));
        }
        return Err(ApiError::Upstream(format!("{code}: {text}")));
    }
    Ok(resp)
}
