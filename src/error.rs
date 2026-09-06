use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde_json::json;

/// 网关对外错误。`IntoResponse` 会按客户端协议(目前 OpenAI 风格)渲染错误体。
#[derive(Debug, thiserror::Error)]
pub enum ApiError {
    #[error("missing or malformed Authorization")]
    Unauthorized,
    /// 登录凭据错误。统一文案不区分用户名/密码,避免枚举信息泄露。
    #[error("用户名或密码错误")]
    InvalidCredentials,
    #[error("api key is invalid or revoked")]
    InvalidKey,
    #[error("account disabled")]
    AccountDisabled,
    #[error("insufficient token balance")]
    InsufficientBalance,
    #[error("model `{0}` not found in routing table")]
    ModelNotFound(String),
    #[error("model `{0}` not allowed for this key")]
    ModelNotAllowed(String),
    #[error("no healthy upstream target for `{0}`")]
    NoTarget(String),
    /// 路由有候选且已逐一尝试但全部失败,区别于 NoTarget(无候选):
    /// 携带末次真实错误(已脱敏)帮助调用方定位,502。
    #[error("all upstream candidates failed for `{model}`: last {last_status}: {last_error}")]
    AllFailed {
        model: String,
        last_error: String,
        last_status: i32,
    },
    #[error("concurrency limit exceeded")]
    TooManyRequests,
    /// 登录防爆破/防轰炸触发。中文消息直接透出给前端。
    #[error("{0}")]
    TooManyAttempts(String),
    #[error("bad request: {0}")]
    BadRequest(String),
    #[error("upstream error: {0}")]
    Upstream(String),
    /// 上游可重试错误(连接失败 / 超时 / 5xx / 429)。failover 时应切换下一目标。
    #[error("upstream unavailable: {0}")]
    Unavailable(String),
    #[error("internal error: {0}")]
    Internal(String),
    /// 周期预算耗尽(日/月窗口)。429 + insufficient_quota,对齐 OpenAI 语义。
    #[error("{0} token budget exhausted")]
    BudgetExhausted(&'static str),
}

impl ApiError {
    fn status(&self) -> StatusCode {
        match self {
            ApiError::Unauthorized | ApiError::InvalidCredentials | ApiError::InvalidKey => StatusCode::UNAUTHORIZED,
            ApiError::AccountDisabled => StatusCode::FORBIDDEN,
            ApiError::InsufficientBalance => StatusCode::PAYMENT_REQUIRED,
            ApiError::ModelNotFound(_) | ApiError::ModelNotAllowed(_) => StatusCode::NOT_FOUND,
            ApiError::NoTarget(_) | ApiError::AllFailed { .. } => StatusCode::BAD_GATEWAY,
            ApiError::Upstream(_) => StatusCode::BAD_GATEWAY,
            ApiError::Unavailable(_) => StatusCode::SERVICE_UNAVAILABLE,
            ApiError::TooManyRequests | ApiError::TooManyAttempts(_) | ApiError::BudgetExhausted(_) => StatusCode::TOO_MANY_REQUESTS,
            ApiError::BadRequest(_) => StatusCode::BAD_REQUEST,
            ApiError::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    fn error_type(&self) -> &'static str {
        match self {
            ApiError::Unauthorized | ApiError::InvalidCredentials | ApiError::InvalidKey => "authentication_error",
            ApiError::AccountDisabled => "permission_error",
            ApiError::InsufficientBalance | ApiError::BudgetExhausted(_) => "insufficient_quota",
            ApiError::ModelNotFound(_) | ApiError::ModelNotAllowed(_) => "not_found_error",
            ApiError::TooManyRequests | ApiError::TooManyAttempts(_) => "rate_limit_error",
            ApiError::BadRequest(_) => "invalid_request_error",
            ApiError::NoTarget(_) | ApiError::AllFailed { .. } | ApiError::Upstream(_) => "upstream_error",
            ApiError::Unavailable(_) => "upstream_error",
            ApiError::Internal(_) => "internal_error",
        }
    }
}

impl ApiError {
    /// 渲染成 Anthropic 规范错误体(供 /v1/messages 入口使用)。
    pub fn into_anthropic_response(self) -> Response {
        let body = json!({
            "type": "error",
            "error": {
                "type": self.anthropic_error_type(),
                "message": self.to_string(),
            }
        });
        (self.status(), Json(body)).into_response()
    }

    fn anthropic_error_type(&self) -> &'static str {
        match self {
            ApiError::Unauthorized | ApiError::InvalidCredentials | ApiError::InvalidKey => "authentication_error",
            ApiError::AccountDisabled => "permission_error",
            ApiError::InsufficientBalance => "api_error",
            ApiError::ModelNotFound(_) | ApiError::ModelNotAllowed(_) => "not_found_error",
            ApiError::TooManyRequests | ApiError::TooManyAttempts(_) | ApiError::BudgetExhausted(_) => "rate_limit_error",
            ApiError::BadRequest(_) => "invalid_request_error",
            ApiError::NoTarget(_) | ApiError::AllFailed { .. } | ApiError::Upstream(_) | ApiError::Unavailable(_) => "api_error",
            ApiError::Internal(_) => "internal_error",
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let body = json!({
            "error": {
                "message": self.to_string(),
                "type": self.error_type(),
            }
        });
        (self.status(), Json(body)).into_response()
    }
}

/// 对外可见的末次上游错误摘要:抹掉 URL(含 base_url/内网地址)、Bearer 凭据、
/// sk- 形态密钥,再截断到 200 字符,避免把上游配置泄露给调用方。
pub fn sanitize_upstream_error(raw: &str) -> String {
    let s = redact_urls(raw);
    let s = redact_bearer(&s);
    let s = redact_sk_keys(&s);
    let head: String = s.chars().take(200).collect();
    if head.chars().count() < s.chars().count() {
        format!("{head}…")
    } else {
        head
    }
}

/// 把 `scheme://host/path` 形态的 URL 替换为 `[url]`(覆盖 reqwest 错误串里的
/// 内网地址,如 `error sending request for url (http://10.0.0.1/v1/chat/completions)`)。
fn redact_urls(s: &str) -> String {
    let bytes = s.as_bytes();
    // 两遍扫描:先收集所有 URL 的字节区间,再整体拼接,保证 scheme 不残留。
    let mut segments: Vec<(usize, usize)> = Vec::new();
    let mut i = 0;
    while i + 3 <= s.len() {
        if bytes[i..].starts_with(b"://") {
            // 回溯 scheme(字母数字与 .-+),确定 URL 起点。
            let mut start = i;
            while start > 0 {
                let b = bytes[start - 1];
                if b.is_ascii_alphanumeric() || b == b'.' || b == b'-' || b == b'+' {
                    start -= 1;
                } else {
                    break;
                }
            }
            if start == i {
                i += 1;
                continue;
            }
            // 终点:空白、引号、括号、逗号、反引号、尖括号。
            let mut end = i + 3;
            while end < s.len() {
                let b = bytes[end];
                if b.is_ascii_whitespace()
                    || matches!(b, b'"' | b'\'' | b'(' | b')' | b'[' | b']' | b'<' | b'>' | b',' | b'`')
                {
                    break;
                }
                end += 1;
            }
            segments.push((start, end));
            i = end;
        } else {
            i += 1;
        }
    }
    let mut out = String::with_capacity(s.len());
    let mut pos = 0;
    for (start, end) in segments {
        if start < pos {
            continue;
        }
        out.push_str(&s[pos..start]);
        out.push_str("[url]");
        pos = end;
    }
    out.push_str(&s[pos..]);
    out
}

/// 抹掉 `Bearer <token>`(大小写不敏感)形态的凭据,token 换成 `***`。
fn redact_bearer(s: &str) -> String {
    let lower = s.to_ascii_lowercase();
    const NEEDLE: &str = "bearer";
    let mut out = String::with_capacity(s.len());
    let mut pos = 0;
    // 每轮 pos 至少前进 NEEDLE.len(),必然终止。
    while let Some(rel) = lower[pos..].find(NEEDLE) {
        let hit = pos + rel;
        out.push_str(&s[pos..hit]);
        out.push_str(&s[hit..hit + NEEDLE.len()]);
        let after = &s[hit + NEEDLE.len()..];
        let ws = after.len() - after.trim_start().len();
        out.push_str(&after[..ws]);
        let rest = &after[ws..];
        let token_end = rest.find(char::is_whitespace).unwrap_or(rest.len());
        if token_end > 0 {
            out.push_str("***");
        }
        pos = hit + NEEDLE.len() + ws + token_end;
    }
    out.push_str(&s[pos..]);
    out
}

/// 抹掉 `sk-` 开头、长度足够的密钥形态字符串(OpenAI 风格 key)。
/// 要求前邻字符不是字母/数字/下划线,且密钥体 ≥8 字符,避免误伤普通词。
fn redact_sk_keys(s: &str) -> String {
    const NEEDLE: &str = "sk-";
    let bytes = s.as_bytes();
    let mut out = String::with_capacity(s.len());
    let mut i = 0;
    while i < s.len() {
        if bytes[i..].starts_with(NEEDLE.as_bytes()) {
            let prev_ok = i == 0 || {
                let prev = bytes[i - 1];
                !(prev.is_ascii_alphanumeric() || prev == b'_')
            };
            if prev_ok {
                let mut end = i + NEEDLE.len();
                while end < s.len() {
                    let b = bytes[end];
                    if b.is_ascii_whitespace()
                        || matches!(b, b'"' | b'\'' | b'(' | b')' | b'[' | b']' | b',' | b'`')
                    {
                        break;
                    }
                    end += 1;
                }
                if end - i - NEEDLE.len() >= 8 {
                    out.push_str("sk-***");
                    i = end;
                    continue;
                }
            }
            // 不构成密钥:原样带走 "sk-" 并保证推进(else 分支绝不原地踏步)。
            out.push_str(NEEDLE);
            i += NEEDLE.len();
        } else {
            let ch = s[i..].chars().next().expect("i 落在字符边界且未到结尾");
            out.push(ch);
            i += ch.len_utf8();
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_redacts_url_with_scheme() {
        let out = sanitize_upstream_error(
            "transport: error sending request for url (https://10.0.0.1:8000/v1/chat/completions)",
        );
        assert_eq!(out, "transport: error sending request for url ([url])");
    }

    #[test]
    fn sanitize_redacts_bearer_token() {
        let out = sanitize_upstream_error("Authorization: Bearer sk-abcdef1234567890 denied");
        assert!(!out.contains("abcdef"), "token 泄露: {out}");
        assert!(out.contains("***"), "{out}");
    }

    #[test]
    fn sanitize_redacts_long_sk_key_only() {
        let out = sanitize_upstream_error("key sk-abcdefgh12345678 invalid; task-skills ok");
        assert!(out.contains("sk-***"), "{out}");
        assert!(out.contains("task-skills ok"), "误伤普通词: {out}");
    }

    #[test]
    fn sanitize_truncates_to_200_chars() {
        let long = "x".repeat(500);
        let out = sanitize_upstream_error(&long);
        assert_eq!(out.chars().count(), 201);
        assert!(out.ends_with('…'));
    }

    #[test]
    fn sanitize_keeps_plain_text() {
        assert_eq!(sanitize_upstream_error("connection refused"), "connection refused");
    }
}
