use std::time::Duration;

use serde_json::{json, Value};

use crate::error::ApiError;
use crate::state::AppState;

/// 向 OpenAI 兼容上游发起 chat/completions 请求,返回原始流式/非流式响应。
/// 仅改写 `model` 为上游真名,并在流式时打开 `include_usage` 以便计费。
/// 非流式请求按 [proxy].upstream_timeout_secs 设总超时;流式请求不设总超时。
/// 传输失败(连接/超时)、429 与 5xx 记作可重试的 `Unavailable`;其余错误为非重试 `Upstream`。
pub async fn chat_completions(
    state: &AppState,
    base_url: &str,
    api_key: Option<&str>,
    upstream_model: &str,
    mut body: Value,
    stream: bool,
) -> Result<reqwest::Response, ApiError> {
    body["model"] = json!(upstream_model);
    if stream {
        body["stream"] = json!(true);
        body["stream_options"] = json!({ "include_usage": true });
    }

    let url = format!("{}/chat/completions", base_url.trim_end_matches('/'));
    // 密钥可选:有则加 Authorization: Bearer,无则不加(适合本地无鉴权上游)。
    let mut rb = state.http.post(&url);
    if let Some(k) = api_key.filter(|k| !k.is_empty()) {
        rb = rb.bearer_auth(k);
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
