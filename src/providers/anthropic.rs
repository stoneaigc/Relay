use serde_json::Value;

use crate::error::ApiError;
use crate::state::AppState;

/// 向 Anthropic Messages API 发起请求,返回原始流式/非流式响应。
/// 传输失败(连接/超时)、429 与 5xx 记作可重试的 `Unavailable`;其余错误为非重试 `Upstream`。
pub async fn messages(
    state: &AppState,
    base_url: &str,
    api_key: Option<&str>,
    body: Value,
) -> Result<reqwest::Response, ApiError> {
    let url = format!("{}/messages", base_url.trim_end_matches('/'));
    let mut rb = state
        .http
        .post(&url)
        .header("anthropic-version", "2023-06-01");
    if let Some(k) = api_key.filter(|k| !k.is_empty()) {
        rb = rb.header("x-api-key", k);
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
