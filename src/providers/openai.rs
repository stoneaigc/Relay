use serde_json::{json, Value};

use crate::error::ApiError;
use crate::state::AppState;

/// 向 OpenAI 兼容上游发起 chat/completions 请求,返回原始流式/非流式响应。
/// 仅改写 `model` 为上游真名,并在流式时打开 `include_usage` 以便计费。
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

    let resp = rb
        .json(&body)
        .send()
        .await
        .map_err(|e| ApiError::Upstream(e.to_string()))?;

    if !resp.status().is_success() {
        let code = resp.status();
        let text = resp.text().await.unwrap_or_default();
        return Err(ApiError::Upstream(format!("{code}: {text}")));
    }

    Ok(resp)
}
