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
    #[error("concurrency limit exceeded")]
    TooManyRequests,
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
            ApiError::Unauthorized | ApiError::InvalidKey => StatusCode::UNAUTHORIZED,
            ApiError::AccountDisabled => StatusCode::FORBIDDEN,
            ApiError::InsufficientBalance => StatusCode::PAYMENT_REQUIRED,
            ApiError::ModelNotFound(_) | ApiError::ModelNotAllowed(_) => StatusCode::NOT_FOUND,
            ApiError::NoTarget(_) => StatusCode::BAD_GATEWAY,
            ApiError::Upstream(_) => StatusCode::BAD_GATEWAY,
            ApiError::Unavailable(_) => StatusCode::SERVICE_UNAVAILABLE,
            ApiError::TooManyRequests | ApiError::BudgetExhausted(_) => StatusCode::TOO_MANY_REQUESTS,
            ApiError::BadRequest(_) => StatusCode::BAD_REQUEST,
            ApiError::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    fn error_type(&self) -> &'static str {
        match self {
            ApiError::Unauthorized | ApiError::InvalidKey => "authentication_error",
            ApiError::AccountDisabled => "permission_error",
            ApiError::InsufficientBalance | ApiError::BudgetExhausted(_) => "insufficient_quota",
            ApiError::ModelNotFound(_) | ApiError::ModelNotAllowed(_) => "not_found_error",
            ApiError::TooManyRequests => "rate_limit_error",
            ApiError::BadRequest(_) => "invalid_request_error",
            ApiError::NoTarget(_) | ApiError::Upstream(_) => "upstream_error",
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
            ApiError::Unauthorized | ApiError::InvalidKey => "authentication_error",
            ApiError::AccountDisabled => "permission_error",
            ApiError::InsufficientBalance => "api_error",
            ApiError::ModelNotFound(_) | ApiError::ModelNotAllowed(_) => "not_found_error",
            ApiError::TooManyRequests | ApiError::BudgetExhausted(_) => "rate_limit_error",
            ApiError::BadRequest(_) => "invalid_request_error",
            ApiError::NoTarget(_) | ApiError::Upstream(_) | ApiError::Unavailable(_) => "api_error",
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
