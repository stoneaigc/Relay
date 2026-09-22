use std::sync::Arc;

use axum::{
    body::{Body, Bytes},
    extract::State,
    http::HeaderMap,
    response::{IntoResponse, Response},
    Json,
};
use futures::StreamExt;
use serde_json::{json, Value};
use tokio::sync::OwnedSemaphorePermit;

use crate::auth::authenticate;
use crate::config::ProviderKind;
use crate::error::ApiError;
use crate::state::{AppState, UpstreamKey, UsageEvent};
use crate::providers;
use crate::reqlog::{self, RequestAttempt};

/// 一次请求的链路追踪上下文;贯穿 run_chat/run_messages 及各响应函数,
/// 用于在拿到 tokens 后统一写 request_logs。
pub struct RunTrace {
    pub request_id: String,
    pub user_id: String,
    pub path: &'static str,
    pub requested_model: String,
    pub stream: bool,
    /// resolve_all 的候选顺序(含权重)。
    pub candidates: Vec<RequestAttempt>,
    /// 每个候选的实际尝试结果(含 failover 链)。
    pub attempts: Vec<RequestAttempt>,
    pub final_kind: Option<String>,
    pub final_upstream_model: Option<String>,
    pub final_status: i32,
    pub latency_ms: u32,
    /// 请求体预览(截断到 body_preview_max_bytes)。
    pub req_preview: Option<String>,
    /// 响应体预览(仅非流式响应捕获;流式为 None)。
    pub resp_preview: Option<String>,
}

impl RunTrace {
    /// 成功结算:补 tokens 后写日志。
    pub async fn emit_success(
        &self,
        state: &AppState,
        input: u32,
        output: u32,
        charged: i64,
    ) {
        let log = reqlog::RequestLog {
            request_id: self.request_id.clone(),
            user_id: self.user_id.clone(),
            path: self.path.to_string(),
            requested_model: self.requested_model.clone(),
            stream: self.stream,
            candidates: self.candidates.clone(),
            attempts: self.attempts.clone(),
            final_kind: self.final_kind.clone(),
            final_upstream_model: self.final_upstream_model.clone(),
            final_status: self.final_status,
            latency_ms: self.latency_ms,
            input_tokens: input,
            output_tokens: output,
            charged_tokens: charged,
            ts: crate::storage::now_secs(),
            req_body: self.req_preview.clone(),
            resp_body: self.resp_preview.clone(),
        };
        let _ = state.request_log_tx.send(log).await;
    }

    /// 失败结算(无 tokens)。
    pub async fn emit_error(&self, state: &AppState, final_status: i32) {
        let log = reqlog::RequestLog {
            request_id: self.request_id.clone(),
            user_id: self.user_id.clone(),
            path: self.path.to_string(),
            requested_model: self.requested_model.clone(),
            stream: self.stream,
            candidates: self.candidates.clone(),
            attempts: self.attempts.clone(),
            final_kind: self.final_kind.clone(),
            final_upstream_model: self.final_upstream_model.clone(),
            final_status,
            latency_ms: self.latency_ms,
            input_tokens: 0,
            output_tokens: 0,
            charged_tokens: 0,
            ts: crate::storage::now_secs(),
            req_body: self.req_preview.clone(),
            resp_body: self.resp_preview.clone(),
        };
        let _ = state.request_log_tx.send(log).await;
    }
}

/// 响应体预览:尊重 body_preview_max_bytes(0=关闭),UTF-8 边界安全截断。
fn resp_preview_of(state: &AppState, v: &Value) -> Option<String> {
    let max = state.config.load().logging.body_preview_max_bytes;
    if max == 0 {
        return None;
    }
    reqlog::body_preview(&serde_json::to_vec(v).unwrap_or_default(), max)
}

/// 构造候选初始列表(weight 填充,status 未定)。
fn init_candidates(
    candidates: &[crate::routing::Resolved],
) -> Vec<RequestAttempt> {
    candidates
        .iter()
        .map(|r| reqlog::candidate_attempt(r.kind, &r.provider, &r.base_url, &r.upstream_model, r.weight))
        .collect()
}

/// 构造 X-Relay-* 透传头:命中供应商 + 请求 ID,便于客户端排障与命中验证。
fn relay_headers(provider: &str, request_id: &str) -> [(&'static str, String); 2] {
    [("x-relay-upstream", sanitize_header_value(provider)), ("x-relay-request-id", request_id.to_string())]
}

/// HeaderValue 只接受可见 ASCII;供应商名含中文等字符时回退 unknown(明细仍在请求日志)。
fn sanitize_header_value(v: &str) -> String {
    if axum::http::HeaderValue::from_str(v).is_ok() { v.to_string() } else { "unknown".to_string() }
}

fn apply_relay_headers(resp: &mut Response, hdrs: &[(&'static str, String)]) {
    for (name, val) in hdrs {
        if let Ok(hv) = axum::http::HeaderValue::from_str(val) {
            resp.headers_mut().insert(axum::http::header::HeaderName::from_static(name), hv);
        }
    }
}

pub async fn health() -> &'static str {
    "ok"
}

/// GET /v1/models —— 返回逻辑模型清单(OpenAI 格式)。
pub async fn list_models(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Json<Value>, ApiError> {
    let auth = authenticate(&state, &headers)?;
    let names = state.routing.load().group_model_names(auth.user.group());
    let data: Vec<Value> = names
        .iter()
        .map(|n| json!({ "id": n, "object": "model", "owned_by": "relay" }))
        .collect();
    Ok(Json(json!({ "object": "list", "data": data })))
}

/// POST /v1/embeddings 对外单请求 input 条数上限:防止单请求放大上游成本。
const MAX_EMBEDDING_INPUTS: usize = 64;
/// 对外 embeddings 转发超时(秒);语义缓存旁路仍走 embed() 的 3s 短超时。
const EMBEDDING_TIMEOUT_SECS: u64 = 30;

/// POST /v1/embeddings —— OpenAI 兼容 embedding 转发(API Key 鉴权)。
/// 实际模型固定为网关配置的 embedding 服务;不计费(价格表无 embedding 项),照常记请求日志与指标。
pub async fn embeddings(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    match run_embeddings(&state, &headers, &body).await {
        Ok(r) => r,
        Err(e) => e.into_response(),
    }
}

/// 解析 OpenAI embeddings 入参:input 支持 string | string[];空 / 超量 / 超长 / 非字符串均拒绝。
fn embed_inputs(v: &Value) -> Result<Vec<String>, ApiError> {
    let texts: Vec<String> = match v.get("input") {
        Some(Value::String(s)) => vec![s.clone()],
        Some(Value::Array(arr)) => arr
            .iter()
            .map(|x| x.as_str().map(str::to_string))
            .collect::<Option<Vec<_>>>()
            .ok_or_else(|| ApiError::BadRequest("`input` 数组元素必须为字符串".into()))?,
        _ => return Err(ApiError::BadRequest("missing or invalid `input`".into())),
    };
    if texts.is_empty() || texts.iter().any(|t| t.trim().is_empty()) {
        return Err(ApiError::BadRequest("`input` 不能为空".into()));
    }
    if texts.len() > MAX_EMBEDDING_INPUTS {
        return Err(ApiError::BadRequest(format!("`input` 最多 {MAX_EMBEDDING_INPUTS} 条")));
    }
    for t in &texts {
        if t.chars().count() > crate::embedding::MAX_EMBED_CHARS {
            return Err(ApiError::BadRequest(format!(
                "单条 input 超过 {} 字符上限",
                crate::embedding::MAX_EMBED_CHARS
            )));
        }
    }
    Ok(texts)
}

async fn run_embeddings(
    state: &Arc<AppState>,
    headers: &HeaderMap,
    body: &Bytes,
) -> Result<Response, ApiError> {
    let req_start = std::time::Instant::now();
    let result = embeddings_inner(state, headers, body).await;
    match &result {
        Ok(_) => {
            state
                .record_metrics_request(None, crate::state::METRIC_STATUS_OK, req_start.elapsed())
                .await;
        }
        Err(_) => {
            state.record_metrics_request(None, 400, req_start.elapsed()).await;
        }
    }
    result
}

async fn embeddings_inner(
    state: &Arc<AppState>,
    headers: &HeaderMap,
    body: &Bytes,
) -> Result<Response, ApiError> {
    let auth = authenticate(state, headers)?;
    let user = auth.user;
    if user.balance() <= 0 {
        return Err(ApiError::InsufficientBalance);
    }
    if !user.try_rpm() {
        return Err(ApiError::TooManyRequests);
    }
    let ec = state.config.load().embedding.clone();
    if !ec.service_ready() {
        return Err(ApiError::Unavailable("embedding 服务未配置".into()));
    }

    let v: Value = serde_json::from_slice(body)
        .map_err(|e| ApiError::BadRequest(format!("invalid JSON: {e}")))?;
    // model 字段按 OpenAI 规范必填;网关固定转发到配置的 embedding 模型,忽略其取值。
    if v.get("model").and_then(|m| m.as_str()).is_none() {
        return Err(ApiError::BadRequest("missing `model`".into()));
    }
    let texts = embed_inputs(&v)?;
    let request_id = format!("req_{}", &uuid::Uuid::new_v4().to_string()[..24]);
    let req_preview = reqlog::body_preview(body, state.config.load().logging.body_preview_max_bytes);

    let mut trace = RunTrace {
        request_id: request_id.clone(),
        user_id: user.id.to_string(),
        path: "embeddings",
        requested_model: ec.model.clone(),
        stream: false,
        candidates: Vec::new(),
        attempts: Vec::new(),
        final_kind: Some("embedding".into()),
        final_upstream_model: Some(ec.model.clone()),
        final_status: 200,
        latency_ms: 0,
        req_preview,
        resp_preview: None,
    };
    let started = std::time::Instant::now();
    match crate::embedding::embed_batch(
        &state.http,
        &ec.base_url,
        &ec.api_key,
        &ec.model,
        &texts,
        EMBEDDING_TIMEOUT_SECS,
    )
    .await
    {
        Ok(batch) => {
            trace.latency_ms = started.elapsed().as_millis() as u32;
            trace.emit_success(state, batch.prompt_tokens, 0, 0).await;
            let data: Vec<Value> = batch
                .vectors
                .iter()
                .enumerate()
                .map(|(i, e)| json!({ "object": "embedding", "index": i, "embedding": e }))
                .collect();
            Ok(Json(json!({
                "object": "list",
                "data": data,
                "model": ec.model,
                "usage": { "prompt_tokens": batch.prompt_tokens, "total_tokens": batch.prompt_tokens }
            }))
            .into_response())
        }
        Err(e) => {
            trace.latency_ms = started.elapsed().as_millis() as u32;
            trace.final_status = 503;
            trace.emit_error(state, 503).await;
            Err(ApiError::Unavailable(format!(
                "embedding 上游失败: {}",
                crate::error::sanitize_upstream_error(&e.to_string())
            )))
        }
    }
}

/// POST /v1/chat/completions —— OpenAI 兼容入口(API Key 鉴权)。
pub async fn chat_completions(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let result = run_openai(state, headers, body).await;
    match result {
        Ok(r) => r,
        Err(e) => e.into_response(),
    }
}

/// L4 逃生口:请求头 `x-relay-cache-control: no-cache` 时跳过语义缓存读/写(不区分大小写)。
fn no_cache_requested(headers: &HeaderMap) -> bool {
    headers
        .get("x-relay-cache-control")
        .and_then(|v| v.to_str().ok())
        .map_or(false, |v| v.trim().eq_ignore_ascii_case("no-cache"))
}

async fn run_openai(
    state: Arc<AppState>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, ApiError> {
    let auth = authenticate(&state, &headers)?;
    let req: Value = serde_json::from_slice(&body)
        .map_err(|e| ApiError::BadRequest(format!("invalid json: {e}")))?;
    let no_cache = no_cache_requested(&headers);
    run_chat(state, auth.user, Some(auth.key_id), req, no_cache).await
}

/// 入站校验:messages 必须为非空数组,stream/max_tokens 类型必须正确(双协议同规则)。
/// 无效请求直接 400,不再转发上游(避免白耗 failover 与上游 400 语义混淆)。
fn validate_chat_payload(req: &Value) -> Result<(), ApiError> {
    match req.get("messages") {
        None | Some(Value::Null) => {
            return Err(ApiError::BadRequest("missing `messages`".into()));
        }
        Some(Value::Array(a)) if a.is_empty() => {
            return Err(ApiError::BadRequest("`messages` 不能为空数组".into()));
        }
        Some(Value::Array(_)) => {}
        Some(_) => return Err(ApiError::BadRequest("`messages` 必须为数组".into())),
    }
    if let Some(s) = req.get("stream") {
        if !s.is_boolean() {
            return Err(ApiError::BadRequest("`stream` 必须为布尔值".into()));
        }
    }
    if let Some(mt) = req.get("max_tokens") {
        if !mt.is_u64() {
            return Err(ApiError::BadRequest("`max_tokens` 必须为正整数".into()));
        }
    }
    Ok(())
}

/// 数据面核心:对已鉴权的用户执行一次 chat 调用(路由 + 上游 + 计费 + 并发)。
/// 同时供 `/v1/chat/completions`(Key)与 `/portal/chat`(门户 JWT)复用;门户无密钥传 None。
pub async fn run_chat(
    state: Arc<AppState>,
    user: Arc<crate::state::UserState>,
    key_id: Option<uuid::Uuid>,
    req: Value,
    no_cache: bool,
) -> Result<Response, ApiError> {
    // 全局计时器:用于 catch-all 失败路径写指标(成功率 / RPS / 延迟)
    let req_start = std::time::Instant::now();
    // 请求体预览:跟随存储后端落库(SQLite/PG req_body 列 / ES 文档字段);body_preview_max_bytes=0 时不采集。
    let req_preview = reqlog::body_preview(&serde_json::to_vec(&req).unwrap_or_default(), state.config.load().logging.body_preview_max_bytes);

    macro_rules! fail_global {
        () => {
            state
                .record_metrics_request(
                    None,
                    400,
                    req_start.elapsed(),
                )
                .await;
        };
    }

    if user.balance() <= 0 {
        fail_global!();
        return Err(ApiError::InsufficientBalance);
    }
    if !user.try_rpm() || !user.tpm_ok() {
        fail_global!();
        return Err(ApiError::TooManyRequests);
    }
    match user.budget_status(state.tz_offset_secs as i64 / 3600) {
        crate::state::BudgetStatus::Ok => {}
        crate::state::BudgetStatus::DailyExhausted => {
            fail_global!();
            return Err(ApiError::BudgetExhausted("daily"));
        }
        crate::state::BudgetStatus::MonthlyExhausted => {
            fail_global!();
            return Err(ApiError::BudgetExhausted("monthly"));
        }
    }

    validate_chat_payload(&req)?;
    let model = req
        .get("model")
        .and_then(|m| m.as_str())
        .ok_or_else(|| ApiError::BadRequest("missing `model`".into()))?
        .to_string();
    let stream = req.get("stream").and_then(|s| s.as_bool()).unwrap_or(false);
    let request_id = format!("req_{}", &uuid::Uuid::new_v4().to_string()[..24]);

    // 占用并发名额(RAII,流结束/出错自动释放)。
    let guard = match user.try_acquire() {
        Some(g) => g,
        None => {
            fail_global!();
            return Err(ApiError::TooManyRequests);
        }
    };

    // 路由解析:按用户所在模型组,把对外模型名解析到全部候选上游。
    // failover:依次尝试每个候选;仅可重试错误(Unavailable)才切换到下一目标。
    let candidates = match state.routing.load().resolve_all(user.group(), &model, &state.round_robin, &state.latency_p50, state.tz_offset_secs, None) {
        Ok(c) => c,
        Err(e) => {
            fail_global!();
            return Err(e);
        }
    };
    let cand_init = init_candidates(&candidates);
    // failover 策略:fallback 关闭只试首个候选;max_retries 限制尝试候选数(0=不限)。
    let attempt_cap = state.config.load().defaults.attempt_cap(candidates.len());
    // 语义缓存:全局关/带 no-cache 逃生口/消息超阈值/非确定性请求(temperature>0 且无 seed)时整体跳过(不记 miss)。
    let cache_ok = {
        let cs = state.config.load().cache_semantic.clone();
        cs.enabled
            && !no_cache
            && crate::semantic_cache::eligible(&req, cs.multi_turn_max)
            && crate::semantic_cache::deterministic(&req)
    };
    let mut cache_missed = false;
    // L2 语义查找只做一次(首个未命中 candidate);输入文本循环外抽取一次复用。
    let mut l2_tried = false;
    let cache_embed_text = if cache_ok {
        crate::embedding::embed_text(&req)
    } else {
        None
    };
    let mut attempts: Vec<RequestAttempt> = Vec::new();
    let run_start = std::time::Instant::now();
    let run_path: &'static str = "chat";
    let user_id_str = user.id.to_string();
    let mut final_kind: Option<String> = None;
    let mut final_upstream: Option<String> = None;
    let final_status: i32 = 200;
    for r in candidates.iter().take(attempt_cap) {
        let kind = r.kind;
        let (provider_name, base_url, api_key, upstream_model, multiplier) =
            (&r.provider, &r.base_url, r.api_key.as_deref(), &r.upstream_model, r.multiplier);

        // ---- 语义缓存查找:先于熔断/并发检查,命中即回放;L1 还需路由级 opt-in(cache_enabled) ----
        let cache_key = if cache_ok && r.cache_enabled {
            Some(crate::semantic_cache::make_key(
                run_path, &model, provider_name, upstream_model, stream, &req,
            ))
        } else {
            None
        };
        let embed_meta = cache_key.as_ref().map(|_| CacheEmbedMeta {
            model: model.clone(),
            provider: provider_name.clone(),
            embed_text: cache_embed_text.clone(),
        });
        if let Some(key) = cache_key.as_ref() {
            if let Some(entry) = state.semantic_cache.get(key) {
                state.semantic_cache.record_hit(crate::semantic_cache::HitRecord {
                    ts: crate::semantic_cache::now_secs(),
                    hit_type: "exact".into(),
                    similarity: None,
                    model: model.clone(),
                    provider: provider_name.clone(),
                    tokens_saved: (entry.input_tokens + entry.output_tokens) as u64,
                });
                state
                    .record_metrics_request(None, crate::state::METRIC_STATUS_OK, req_start.elapsed())
                    .await;
                // 命中扣费(L3):tokens 照实入日志,charged 按 路由倍率×用户倍率×折扣率 取整。
                let input = entry.input_tokens;
                let output = entry.output_tokens;
                let charged = crate::semantic_cache::cache_charged(
                    input, output, multiplier, user.bill_multiplier(),
                    state.config.load().cache_semantic.billing_ratio,
                );
                user.deduct(charged);
                user.record_tokens(input.saturating_add(output));
                let _ = state.usage_tx.send(UsageEvent {
                    user_id: user.id, key_id, cost_usd: 0.0, model: model.clone(),
                    provider: provider_name.clone(), upstream_model: upstream_model.clone(),
                    input_tokens: input, output_tokens: output, charged_tokens: charged, status: 200,
                    request_id: Some(request_id.clone()),
                }).await;
                let trace = RunTrace {
                    request_id: request_id.clone(),
                    user_id: user_id_str.clone(),
                    path: run_path,
                    requested_model: model.clone(),
                    stream,
                    candidates: cand_init.clone(),
                    attempts: Vec::new(),
                    final_kind: Some("cache".into()),
                    final_upstream_model: Some(upstream_model.clone()),
                    final_status: 200,
                    req_preview: req_preview.clone(),
                    resp_preview: None,
                    latency_ms: run_start.elapsed().as_millis() as u32,
                };
                trace.emit_success(&state, input, output, charged).await;
                return Ok(crate::semantic_cache::replay_response(entry, provider_name, &request_id, "exact"));
            }
            // L2 语义查找:相似度达阈值即免费回放(embedding 失败自动降级 miss)。
            if !l2_tried {
                l2_tried = true;
                if let Some(text) = cache_embed_text.as_ref() {
                    if let Some((entry, sim)) =
                        semantic_lookup(&state, &model, provider_name, text).await
                    {
                        state.semantic_cache.record_hit(crate::semantic_cache::HitRecord {
                            ts: crate::semantic_cache::now_secs(),
                            hit_type: "semantic".into(),
                            similarity: Some(sim),
                            model: model.clone(),
                            provider: provider_name.clone(),
                            tokens_saved: (entry.input_tokens + entry.output_tokens) as u64,
                        });
                        state
                            .record_metrics_request(None, crate::state::METRIC_STATUS_OK, req_start.elapsed())
                            .await;
                        // 命中扣费(L3):tokens 照实入日志,charged 按 路由倍率×用户倍率×折扣率 取整。
                        let input = entry.input_tokens;
                        let output = entry.output_tokens;
                        let charged = crate::semantic_cache::cache_charged(
                            input, output, multiplier, user.bill_multiplier(),
                            state.config.load().cache_semantic.billing_ratio,
                        );
                        user.deduct(charged);
                        user.record_tokens(input.saturating_add(output));
                        let _ = state.usage_tx.send(UsageEvent {
                            user_id: user.id, key_id, cost_usd: 0.0, model: model.clone(),
                            provider: provider_name.clone(), upstream_model: upstream_model.clone(),
                            input_tokens: input, output_tokens: output, charged_tokens: charged, status: 200,
                            request_id: Some(request_id.clone()),
                        }).await;
                        let trace = RunTrace {
                            request_id: request_id.clone(),
                            user_id: user_id_str.clone(),
                            path: run_path,
                            requested_model: model.clone(),
                            stream,
                            candidates: cand_init.clone(),
                            attempts: Vec::new(),
                            final_kind: Some("semantic".into()),
                            final_upstream_model: Some(upstream_model.clone()),
                            final_status: 200,
                            req_preview: req_preview.clone(),
                            resp_preview: None,
                            latency_ms: run_start.elapsed().as_millis() as u32,
                        };
                        trace.emit_success(&state, input, output, charged).await;
                        return Ok(crate::semantic_cache::replay_response(entry, provider_name, &request_id, "semantic"));
                    }
                }
            }
            if !cache_missed {
                cache_missed = true;
                state.semantic_cache.record_miss();
            }
        }

        // ---- P2:熔断检查 ----
        let ukey = UpstreamKey::new(kind, base_url, api_key);
        if state.breaker_should_skip(&ukey).await {
            // 跳过的候选也留痕:kind="skipped" + 原因在 error(status=-1 前端渲染为跳过)。
            attempts.push(RequestAttempt {
                kind: "skipped".to_string(),
                provider: provider_name.clone(),
                base_url: base_url.clone(),
                upstream_model: upstream_model.clone(),
                weight: r.weight,
                status: -1,
                latency_ms: 0,
                error: "breaker open, skip".into(),
            });
            continue;
        }
        // ---- P1:占用上游并发槽(排队直到拿到;permit 持有到请求结束自动释放) ----
        let slot = state.acquire_upstream(ukey.clone(), provider_name).await;
        // ---- 指标计时器:用于请求成功率 / RPS / 延迟 ----
        let up_start = std::time::Instant::now();

        match kind {
            crate::config::ProviderKind::Openai => {
                let resp = providers::openai::chat_completions(
                    &state, base_url, api_key, upstream_model, req.clone(), stream,
                )
                .await;
                match resp {
                    Ok(resp) if stream => {
                        state.breaker_success(&ukey).await;
                        state.record_metrics_request(Some(&ukey), crate::state::METRIC_STATUS_OK, up_start.elapsed()).await;
                        final_kind = Some(match kind { ProviderKind::Openai => "openai".into(), ProviderKind::Anthropic => "anthropic".into() });
                        final_upstream = Some(upstream_model.clone());
                        attempts.push(RequestAttempt {
                            kind: "openai".into(),
                            provider: provider_name.clone(),
                            base_url: base_url.clone(),
                            upstream_model: upstream_model.clone(),
                            weight: r.weight,
                            status: 200,
                            latency_ms: up_start.elapsed().as_millis() as u32,
                            error: String::new(),
                        });
                        let trace = RunTrace {
                            request_id: request_id.clone(),
                            user_id: user_id_str.clone(),
                            path: run_path,
                            requested_model: model.clone(),
                            stream,
                            candidates: cand_init.clone(),
                            attempts: attempts.clone(),
                            final_kind: final_kind.clone(),
                            final_upstream_model: final_upstream.clone(),
                            final_status,
                            req_preview: req_preview.clone(),
                            resp_preview: None,
                            latency_ms: run_start.elapsed().as_millis() as u32,
                        };
                        return Ok(cache_wrap(state.clone(), stream_response(state.clone(), resp, guard, slot, model.clone(), provider_name.clone(), upstream_model.clone(), multiplier, user.clone(), key_id, base_url.to_string(), api_key.map(|s| s.to_string()), trace), cache_key, stream, embed_meta));
                    }
                    Ok(resp) => {
                        state.breaker_success(&ukey).await;
                        state.record_metrics_request(Some(&ukey), crate::state::METRIC_STATUS_OK, up_start.elapsed()).await;
                        final_kind = Some("openai".into());
                        final_upstream = Some(upstream_model.clone());
                        attempts.push(RequestAttempt {
                            kind: "openai".into(),
                            provider: provider_name.clone(),
                            base_url: base_url.clone(),
                            upstream_model: upstream_model.clone(),
                            weight: r.weight,
                            status: 200,
                            latency_ms: up_start.elapsed().as_millis() as u32,
                            error: String::new(),
                        });
                        let trace = RunTrace {
                            request_id: request_id.clone(),
                            user_id: user_id_str.clone(),
                            path: run_path,
                            requested_model: model.clone(),
                            stream,
                            candidates: cand_init.clone(),
                            attempts: attempts.clone(),
                            final_kind: final_kind.clone(),
                            final_upstream_model: final_upstream.clone(),
                            final_status,
                            req_preview: req_preview.clone(),
                            resp_preview: None,
                            latency_ms: run_start.elapsed().as_millis() as u32,
                        };
                        return non_stream_response(&state, resp, model.clone(), provider_name.clone(), upstream_model.clone(), multiplier, user.clone(), key_id, slot, kind, base_url, api_key, trace).await.map(|r| cache_wrap(state.clone(), r, cache_key, stream, embed_meta));
                    }
                    Err(ApiError::Unavailable(e)) => {
                        let fail_cnt = state.breaker_unavailable(&ukey).await;
                        state.record_metrics_request(Some(&ukey), crate::state::METRIC_STATUS_UNAVAILABLE, up_start.elapsed()).await;
                        state
                            .record_failure(&ukey, &model, "chat", &e.to_string(), fail_cnt)
                            .await;
                        attempts.push(RequestAttempt {
                            kind: "openai".into(),
                            provider: provider_name.clone(),
                            base_url: base_url.clone(),
                            upstream_model: upstream_model.clone(),
                            weight: r.weight,
                            status: crate::state::METRIC_STATUS_UNAVAILABLE as i32,
                            latency_ms: up_start.elapsed().as_millis() as u32,
                            error: e,
                        });
                        continue;
                    }
                    Err(e) => {
                        // 非重试错误(4xx / 解析错 / 权限不足 ...):记 per-upstream + 全局 fail_other 后直接返回
                        state.record_metrics_request(Some(&ukey), 400, up_start.elapsed()).await;
                        attempts.push(RequestAttempt {
                            kind: "openai".into(),
                            provider: provider_name.clone(),
                            base_url: base_url.clone(),
                            upstream_model: upstream_model.clone(),
                            weight: r.weight,
                            status: 400,
                            latency_ms: up_start.elapsed().as_millis() as u32,
                            error: e.to_string(),
                        });
                        let trace = RunTrace {
                            request_id, user_id: user_id_str, path: run_path,
                            requested_model: model, stream,
                            candidates: cand_init, attempts,
                            final_kind, final_upstream_model: final_upstream,
                            final_status: 400, req_preview: req_preview.clone(), resp_preview: None,
                            latency_ms: run_start.elapsed().as_millis() as u32,
                        };
                        trace.emit_error(&state, 400).await;
                        return Err(e);
                    }
                }
            }
            crate::config::ProviderKind::Anthropic => {
                // OpenAI 入站 → Anthropic 上游:翻译请求 + 翻译响应。
                let abody = crate::translate::openai_to_anthropic(&req, upstream_model, stream);
                let resp = providers::anthropic::messages(&state, base_url, api_key, abody).await;
                match resp {
                    Ok(resp) if stream => {
                        state.breaker_success(&ukey).await;
                        state.record_metrics_request(Some(&ukey), crate::state::METRIC_STATUS_OK, up_start.elapsed()).await;
                        final_kind = Some("anthropic".into());
                        final_upstream = Some(upstream_model.clone());
                        attempts.push(RequestAttempt {
                            kind: "anthropic".into(),
                            provider: provider_name.clone(),
                            base_url: base_url.clone(),
                            upstream_model: upstream_model.clone(),
                            weight: r.weight,
                            status: 200,
                            latency_ms: up_start.elapsed().as_millis() as u32,
                            error: String::new(),
                        });
                        let trace = RunTrace {
                            request_id: request_id.clone(),
                            user_id: user_id_str.clone(),
                            path: run_path,
                            requested_model: model.clone(),
                            stream,
                            candidates: cand_init.clone(),
                            attempts: attempts.clone(),
                            final_kind: final_kind.clone(),
                            final_upstream_model: final_upstream.clone(),
                            final_status,
                            req_preview: req_preview.clone(),
                            resp_preview: None,
                            latency_ms: run_start.elapsed().as_millis() as u32,
                        };
                        return Ok(cache_wrap(state.clone(), anthropic_stream_response(state.clone(), resp, guard, slot, model.clone(), provider_name.clone(), upstream_model.clone(), multiplier, user.clone(), key_id, base_url.to_string(), api_key.map(|s| s.to_string()), trace), cache_key, stream, embed_meta));
                    }
                    Ok(resp) => {
                        state.breaker_success(&ukey).await;
                        state.record_metrics_request(Some(&ukey), crate::state::METRIC_STATUS_OK, up_start.elapsed()).await;
                        final_kind = Some("anthropic".into());
                        final_upstream = Some(upstream_model.clone());
                        attempts.push(RequestAttempt {
                            kind: "anthropic".into(),
                            provider: provider_name.clone(),
                            base_url: base_url.clone(),
                            upstream_model: upstream_model.clone(),
                            weight: r.weight,
                            status: 200,
                            latency_ms: up_start.elapsed().as_millis() as u32,
                            error: String::new(),
                        });
                        let trace = RunTrace {
                            request_id: request_id.clone(),
                            user_id: user_id_str.clone(),
                            path: run_path,
                            requested_model: model.clone(),
                            stream,
                            candidates: cand_init.clone(),
                            attempts: attempts.clone(),
                            final_kind: final_kind.clone(),
                            final_upstream_model: final_upstream.clone(),
                            final_status,
                            req_preview: req_preview.clone(),
                            resp_preview: None,
                            latency_ms: run_start.elapsed().as_millis() as u32,
                        };
                        return anthropic_nonstream_response(&state, resp, model.clone(), provider_name.clone(), upstream_model.clone(), multiplier, user.clone(), key_id, slot, base_url, api_key, trace).await.map(|r| cache_wrap(state.clone(), r, cache_key, stream, embed_meta));
                    }
                    Err(ApiError::Unavailable(e)) => {
                        let fail_cnt = state.breaker_unavailable(&ukey).await;
                        state.record_metrics_request(Some(&ukey), crate::state::METRIC_STATUS_UNAVAILABLE, up_start.elapsed()).await;
                        state
                            .record_failure(&ukey, &model, "chat", &e.to_string(), fail_cnt)
                            .await;
                        attempts.push(RequestAttempt {
                            kind: "anthropic".into(),
                            provider: provider_name.clone(),
                            base_url: base_url.clone(),
                            upstream_model: upstream_model.clone(),
                            weight: r.weight,
                            status: crate::state::METRIC_STATUS_UNAVAILABLE as i32,
                            latency_ms: up_start.elapsed().as_millis() as u32,
                            error: e,
                        });
                        continue;
                    }
                    Err(e) => {
                        state.record_metrics_request(Some(&ukey), 400, up_start.elapsed()).await;
                        attempts.push(RequestAttempt {
                            kind: "anthropic".into(),
                            provider: provider_name.clone(),
                            base_url: base_url.clone(),
                            upstream_model: upstream_model.clone(),
                            weight: r.weight,
                            status: 400,
                            latency_ms: up_start.elapsed().as_millis() as u32,
                            error: e.to_string(),
                        });
                        let trace = RunTrace {
                            request_id, user_id: user_id_str, path: run_path,
                            requested_model: model, stream,
                            candidates: cand_init, attempts,
                            final_kind, final_upstream_model: final_upstream,
                            final_status: 400, req_preview: req_preview.clone(), resp_preview: None,
                            latency_ms: run_start.elapsed().as_millis() as u32,
                        };
                        trace.emit_error(&state, 400).await;
                        return Err(e);
                    }
                }
            }
        }
    }
    // 走完全部候选都没命中(空数组 / 全被熔断 / 全部 Unavailable 转移完 / 没写路由模型):
    fail_global!();
    let err = exhausted_error(&model, &attempts);
    let trace = RunTrace {
        request_id, user_id: user_id_str, path: run_path,
        requested_model: model.clone(), stream,
        candidates: cand_init, attempts,
        final_kind, final_upstream_model: final_upstream,
        final_status: crate::state::METRIC_STATUS_BAD_GATEWAY as i32,
        req_preview: req_preview.clone(),
        resp_preview: None,
        latency_ms: run_start.elapsed().as_millis() as u32,
    };
    trace.emit_error(&state, crate::state::METRIC_STATUS_BAD_GATEWAY as i32).await;
    Err(err)
}

/// 走完全部候选后的对外错误:有过真实尝试 → AllFailed(带脱敏末次错误);
/// 从未真正发起请求(无候选/全被熔断跳过,status 全为 -1) → NoTarget。
fn exhausted_error(model: &str, attempts: &[RequestAttempt]) -> ApiError {
    if attempts.is_empty() || attempts.iter().all(|a| a.status == -1) {
        return ApiError::NoTarget(model.to_string());
    }
    let last = attempts
        .iter()
        .rev()
        .find(|a| a.status != -1)
        .unwrap_or_else(|| attempts.last().expect("attempts 非空"));
    ApiError::AllFailed {
        model: model.to_string(),
        last_error: crate::error::sanitize_upstream_error(&last.error),
        last_status: last.status,
    }
}

#[allow(clippy::too_many_arguments)]
async fn anthropic_nonstream_response(
    state: &AppState,
    resp: reqwest::Response,
    model: String,
    provider_name: String,
    upstream_model: String,
    multiplier: f64,
    user: Arc<crate::state::UserState>,
    key_id: Option<uuid::Uuid>,
    _slot: Option<OwnedSemaphorePermit>,
    base_url: &str,
    api_key: Option<&str>,
    trace: RunTrace,
) -> Result<Response, ApiError> {
    let hdrs = relay_headers(&provider_name, &trace.request_id);
    let aresp: Value = resp.json().await.map_err(|e| ApiError::Upstream(e.to_string()))?;
    let input = aresp.pointer("/usage/input_tokens").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
    let output = aresp.pointer("/usage/output_tokens").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
    let charged = ((input as f64 + output as f64) * multiplier * user.bill_multiplier()).round() as i64;
    user.deduct(charged);
    user.record_tokens(input.saturating_add(output));

    let payload = crate::translate::anthropic_to_openai(&aresp, &model);
    let mut trace = trace;
    trace.resp_preview = resp_preview_of(state, &payload);
    let _ = state.usage_tx.send(UsageEvent {
        user_id: user.id, key_id, cost_usd: 0.0, model: model.clone(), provider: provider_name, upstream_model,
        input_tokens: input, output_tokens: output, charged_tokens: charged, status: 200,
        request_id: Some(trace.request_id.clone()),
    }).await;
    // 补记指标 tokens(Anthropic → OpenAI)
    let k = UpstreamKey::new(ProviderKind::Anthropic, base_url, api_key);
    state.record_metrics_tokens(Some(&k), input as u64, output as u64).await;
    trace.emit_success(state, input, output, charged).await;
    let mut out = Json(payload).into_response();
    apply_relay_headers(&mut out, &hdrs);
    Ok(out)
}

#[allow(clippy::too_many_arguments)]
fn anthropic_stream_response(
    state: Arc<AppState>,
    resp: reqwest::Response,
    guard: crate::state::ConcurrencyGuard,
    slot: Option<OwnedSemaphorePermit>,
    model: String,
    provider_name: String,
    upstream_model: String,
    multiplier: f64,
    user: Arc<crate::state::UserState>,
    key_id: Option<uuid::Uuid>,
    base_url: String,
    api_key: Option<String>,
    trace: RunTrace,
) -> Response {
    let user_id = user.id;
    let hdrs = relay_headers(&provider_name, &trace.request_id);
    let s = async_stream::stream! {
        let _guard = guard;
        let _slot = slot; // 持有上游并发槽直到流结束
        let mut upstream = resp.bytes_stream();
        let mut buf: Vec<u8> = Vec::new();
        let mut input = 0u32;
        let mut output = 0u32;
        let mut finish = "stop";
        let id = format!("chatcmpl-{}", &uuid::Uuid::new_v4().to_string()[..8]);
        // Anthropic content block index -> OpenAI tool_call index。
        let mut block_to_tool: std::collections::HashMap<i64, usize> = std::collections::HashMap::new();
        let mut next_tool = 0usize;

        // 起始 chunk:role。
        yield Ok::<Bytes, std::io::Error>(Bytes::from(oai_chunk(&id, &model, json!({"role":"assistant"}), None)));

        while let Some(item) = upstream.next().await {
            match item {
                Ok(bytes) => {
                    buf.extend_from_slice(&bytes);
                    while let Some(pos) = buf.iter().position(|&b| b == b'\n') {
                        let line: Vec<u8> = buf.drain(..=pos).collect();
                        let line = String::from_utf8_lossy(&line);
                        let line = line.trim();
                        let Some(payload) = line.strip_prefix("data:") else { continue };
                        let payload = payload.trim();
                        if payload.is_empty() { continue; }
                        let Ok(ev) = serde_json::from_str::<Value>(payload) else { continue };
                        match ev.get("type").and_then(|t| t.as_str()) {
                            Some("message_start") => {
                                input = ev.pointer("/message/usage/input_tokens").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
                            }
                            Some("content_block_start") => {
                                if ev.pointer("/content_block/type").and_then(|t| t.as_str()) == Some("tool_use") {
                                    let bidx = ev.get("index").and_then(|v| v.as_i64()).unwrap_or(0);
                                    let tidx = next_tool; next_tool += 1;
                                    block_to_tool.insert(bidx, tidx);
                                    let tid = ev.pointer("/content_block/id").and_then(|v| v.as_str()).unwrap_or("");
                                    let name = ev.pointer("/content_block/name").and_then(|v| v.as_str()).unwrap_or("");
                                    let delta = json!({"tool_calls":[{"index":tidx,"id":tid,"type":"function","function":{"name":name,"arguments":""}}]});
                                    yield Ok(Bytes::from(oai_chunk(&id, &model, delta, None)));
                                }
                            }
                            Some("content_block_delta") => {
                                match ev.pointer("/delta/type").and_then(|t| t.as_str()) {
                                    Some("text_delta") => {
                                        if let Some(text) = ev.pointer("/delta/thinking").and_then(|v| v.as_str()) {
                                            yield Ok(Bytes::from(oai_chunk(&id, &model, json!({"reasoning_content": text}), None)));
                                        }
                                        if let Some(text) = ev.pointer("/delta/text").and_then(|v| v.as_str()) {
                                            yield Ok(Bytes::from(oai_chunk(&id, &model, json!({"content": text}), None)));
                                        }
                                    }
                                    // Anthropic 扩展思考 → OpenAI reasoning_content。
                                    Some("thinking_delta") => {
                                        if let Some(t) = ev.pointer("/delta/thinking").and_then(|v| v.as_str()) {
                                            yield Ok(Bytes::from(oai_chunk(&id, &model, json!({"reasoning_content": t}), None)));
                                        }
                                    }
                                    Some("input_json_delta") => {
                                        let bidx = ev.get("index").and_then(|v| v.as_i64()).unwrap_or(0);
                                        let tidx = block_to_tool.get(&bidx).copied().unwrap_or(0);
                                        let pj = ev.pointer("/delta/partial_json").and_then(|v| v.as_str()).unwrap_or("");
                                        let delta = json!({"tool_calls":[{"index":tidx,"function":{"arguments":pj}}]});
                                        yield Ok(Bytes::from(oai_chunk(&id, &model, delta, None)));
                                    }
                                    _ => {}
                                }
                            }
                            Some("message_delta") => {
                                if let Some(sr) = ev.pointer("/delta/stop_reason").and_then(|v| v.as_str()) {
                                    finish = crate::translate::map_stop_reason(Some(sr));
                                }
                                if let Some(o) = ev.pointer("/usage/output_tokens").and_then(|v| v.as_u64()) {
                                    output = o as u32;
                                }
                            }
                            Some("message_stop") => {
                                yield Ok(Bytes::from(oai_chunk(&id, &model, json!({}), Some(finish))));
                                yield Ok(Bytes::from(Bytes::from_static(b"data: [DONE]\n\n")));
                            }
                            _ => {}
                        }
                    }
                }
                Err(e) => {
                    yield Err(std::io::Error::new(std::io::ErrorKind::Other, e.to_string()));
                    break;
                }
            }
        }

        let charged = ((input as f64 + output as f64) * multiplier * user.bill_multiplier()).round() as i64;
        user.deduct(charged);
        user.record_tokens(input.saturating_add(output));
        let _ = state.usage_tx.send(UsageEvent {
            user_id, key_id, cost_usd: 0.0, model, provider: provider_name, upstream_model,
            input_tokens: input, output_tokens: output, charged_tokens: charged, status: 200,
            request_id: Some(trace.request_id.clone()),
        }).await;
        // 补记指标 tokens(Anthropic stream → OpenAI SSE)
        let k = UpstreamKey::new(ProviderKind::Anthropic, &base_url, api_key.as_deref());
        state.record_metrics_tokens(Some(&k), input as u64, output as u64).await;
        trace.emit_success(&state, input, output, charged).await;
    };

    let mut resp = sse_response(s);
    apply_relay_headers(&mut resp, &hdrs);
    resp
}

// ================== Anthropic 入站 /v1/messages ==================

/// POST /v1/messages —— Anthropic 兼容入口(API Key 鉴权)。
pub async fn messages(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let result = run_anthropic(state, headers, body).await;
    match result {
        Ok(r) => r,
        Err(e) => e.into_anthropic_response(),
    }
}

async fn run_anthropic(
    state: Arc<AppState>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, ApiError> {
    let auth = authenticate(&state, &headers)?;
    let req: Value = serde_json::from_slice(&body)
        .map_err(|e| ApiError::BadRequest(format!("invalid json: {e}")))?;
    let no_cache = no_cache_requested(&headers);
    run_messages(state, auth.user, Some(auth.key_id), req, no_cache).await
}

/// Anthropic 入站核心:路由 + 上游(Anthropic 直通 / OpenAI 翻译)+ 计费。
pub async fn run_messages(
    state: Arc<AppState>,
    user: Arc<crate::state::UserState>,
    key_id: Option<uuid::Uuid>,
    req: Value,
    no_cache: bool,
) -> Result<Response, ApiError> {
    let req_start = std::time::Instant::now();
    // 请求体预览:跟随存储后端落库(SQLite/PG req_body 列 / ES 文档字段);body_preview_max_bytes=0 时不采集。
    let req_preview = reqlog::body_preview(&serde_json::to_vec(&req).unwrap_or_default(), state.config.load().logging.body_preview_max_bytes);
    macro_rules! fail_global {
        () => {
            state
                .record_metrics_request(
                    None,
                    400,
                    req_start.elapsed(),
                )
                .await;
        };
    }

    if user.balance() <= 0 {
        fail_global!();
        return Err(ApiError::InsufficientBalance);
    }
    if !user.try_rpm() || !user.tpm_ok() {
        fail_global!();
        return Err(ApiError::TooManyRequests);
    }
    match user.budget_status(state.tz_offset_secs as i64 / 3600) {
        crate::state::BudgetStatus::Ok => {}
        crate::state::BudgetStatus::DailyExhausted => {
            fail_global!();
            return Err(ApiError::BudgetExhausted("daily"));
        }
        crate::state::BudgetStatus::MonthlyExhausted => {
            fail_global!();
            return Err(ApiError::BudgetExhausted("monthly"));
        }
    }
    validate_chat_payload(&req)?;
    let model = req
        .get("model")
        .and_then(|m| m.as_str())
        .ok_or_else(|| ApiError::BadRequest("missing `model`".into()))?
        .to_string();
    let stream = req.get("stream").and_then(|s| s.as_bool()).unwrap_or(false);
    let guard = match user.try_acquire() {
        Some(g) => g,
        None => { fail_global!(); return Err(ApiError::TooManyRequests); }
    };

    let candidates = match state.routing.load().resolve_all(user.group(), &model, &state.round_robin, &state.latency_p50, state.tz_offset_secs, None) {
        Ok(c) => c,
        Err(e) => { fail_global!(); return Err(e); }
    };
    let request_id = format!("req_{}", &uuid::Uuid::new_v4().to_string()[..24]);
    let cand_init = init_candidates(&candidates);
    // failover 策略:fallback 关闭只试首个候选;max_retries 限制尝试候选数(0=不限)。
    let attempt_cap = state.config.load().defaults.attempt_cap(candidates.len());
    // 语义缓存:全局关/带 no-cache 逃生口/消息超阈值/非确定性请求(temperature>0 且无 seed)时整体跳过(不记 miss)。
    let cache_ok = {
        let cs = state.config.load().cache_semantic.clone();
        cs.enabled
            && !no_cache
            && crate::semantic_cache::eligible(&req, cs.multi_turn_max)
            && crate::semantic_cache::deterministic(&req)
    };
    let mut cache_missed = false;
    // L2 语义查找只做一次(首个未命中 candidate);输入文本循环外抽取一次复用。
    let mut l2_tried = false;
    let cache_embed_text = if cache_ok {
        crate::embedding::embed_text(&req)
    } else {
        None
    };
    let mut attempts: Vec<RequestAttempt> = Vec::new();
    let run_start = std::time::Instant::now();
    let run_path: &'static str = "messages";
    let user_id_str = user.id.to_string();
    let mut final_kind: Option<String> = None;
    let mut final_upstream: Option<String> = None;
    let final_status: i32 = 200;
    for r in candidates.iter().take(attempt_cap) {
        let kind = r.kind;
        let (provider_name, base_url, api_key, upstream_model, multiplier) =
            (&r.provider, &r.base_url, r.api_key.as_deref(), &r.upstream_model, r.multiplier);

        // ---- 语义缓存查找:先于熔断/并发检查,命中即回放;L1 还需路由级 opt-in(cache_enabled) ----
        let cache_key = if cache_ok && r.cache_enabled {
            Some(crate::semantic_cache::make_key(
                run_path, &model, provider_name, upstream_model, stream, &req,
            ))
        } else {
            None
        };
        let embed_meta = cache_key.as_ref().map(|_| CacheEmbedMeta {
            model: model.clone(),
            provider: provider_name.clone(),
            embed_text: cache_embed_text.clone(),
        });
        if let Some(key) = cache_key.as_ref() {
            if let Some(entry) = state.semantic_cache.get(key) {
                state.semantic_cache.record_hit(crate::semantic_cache::HitRecord {
                    ts: crate::semantic_cache::now_secs(),
                    hit_type: "exact".into(),
                    similarity: None,
                    model: model.clone(),
                    provider: provider_name.clone(),
                    tokens_saved: (entry.input_tokens + entry.output_tokens) as u64,
                });
                state
                    .record_metrics_request(None, crate::state::METRIC_STATUS_OK, req_start.elapsed())
                    .await;
                // 命中扣费(L3):tokens 照实入日志,charged 按 路由倍率×用户倍率×折扣率 取整。
                let input = entry.input_tokens;
                let output = entry.output_tokens;
                let charged = crate::semantic_cache::cache_charged(
                    input, output, multiplier, user.bill_multiplier(),
                    state.config.load().cache_semantic.billing_ratio,
                );
                user.deduct(charged);
                user.record_tokens(input.saturating_add(output));
                let _ = state.usage_tx.send(UsageEvent {
                    user_id: user.id, key_id, cost_usd: 0.0, model: model.clone(),
                    provider: provider_name.clone(), upstream_model: upstream_model.clone(),
                    input_tokens: input, output_tokens: output, charged_tokens: charged, status: 200,
                    request_id: Some(request_id.clone()),
                }).await;
                let trace = RunTrace {
                    request_id: request_id.clone(),
                    user_id: user_id_str.clone(),
                    path: run_path,
                    requested_model: model.clone(),
                    stream,
                    candidates: cand_init.clone(),
                    attempts: Vec::new(),
                    final_kind: Some("cache".into()),
                    final_upstream_model: Some(upstream_model.clone()),
                    final_status: 200,
                    req_preview: req_preview.clone(),
                    resp_preview: None,
                    latency_ms: run_start.elapsed().as_millis() as u32,
                };
                trace.emit_success(&state, input, output, charged).await;
                return Ok(crate::semantic_cache::replay_response(entry, provider_name, &request_id, "exact"));
            }
            // L2 语义查找:相似度达阈值即免费回放(embedding 失败自动降级 miss)。
            if !l2_tried {
                l2_tried = true;
                if let Some(text) = cache_embed_text.as_ref() {
                    if let Some((entry, sim)) =
                        semantic_lookup(&state, &model, provider_name, text).await
                    {
                        state.semantic_cache.record_hit(crate::semantic_cache::HitRecord {
                            ts: crate::semantic_cache::now_secs(),
                            hit_type: "semantic".into(),
                            similarity: Some(sim),
                            model: model.clone(),
                            provider: provider_name.clone(),
                            tokens_saved: (entry.input_tokens + entry.output_tokens) as u64,
                        });
                        state
                            .record_metrics_request(None, crate::state::METRIC_STATUS_OK, req_start.elapsed())
                            .await;
                        // 命中扣费(L3):tokens 照实入日志,charged 按 路由倍率×用户倍率×折扣率 取整。
                        let input = entry.input_tokens;
                        let output = entry.output_tokens;
                        let charged = crate::semantic_cache::cache_charged(
                            input, output, multiplier, user.bill_multiplier(),
                            state.config.load().cache_semantic.billing_ratio,
                        );
                        user.deduct(charged);
                        user.record_tokens(input.saturating_add(output));
                        let _ = state.usage_tx.send(UsageEvent {
                            user_id: user.id, key_id, cost_usd: 0.0, model: model.clone(),
                            provider: provider_name.clone(), upstream_model: upstream_model.clone(),
                            input_tokens: input, output_tokens: output, charged_tokens: charged, status: 200,
                            request_id: Some(request_id.clone()),
                        }).await;
                        let trace = RunTrace {
                            request_id: request_id.clone(),
                            user_id: user_id_str.clone(),
                            path: run_path,
                            requested_model: model.clone(),
                            stream,
                            candidates: cand_init.clone(),
                            attempts: Vec::new(),
                            final_kind: Some("semantic".into()),
                            final_upstream_model: Some(upstream_model.clone()),
                            final_status: 200,
                            req_preview: req_preview.clone(),
                            resp_preview: None,
                            latency_ms: run_start.elapsed().as_millis() as u32,
                        };
                        trace.emit_success(&state, input, output, charged).await;
                        return Ok(crate::semantic_cache::replay_response(entry, provider_name, &request_id, "semantic"));
                    }
                }
            }
            if !cache_missed {
                cache_missed = true;
                state.semantic_cache.record_miss();
            }
        }

        // ---- P2:熔断检查 ----
        let ukey = UpstreamKey::new(kind, base_url, api_key);
        if state.breaker_should_skip(&ukey).await {
            // 跳过的候选也留痕:kind="skipped" + 原因在 error(status=-1 前端渲染为跳过)。
            attempts.push(RequestAttempt {
                kind: "skipped".to_string(),
                provider: provider_name.clone(),
                base_url: base_url.clone(),
                upstream_model: upstream_model.clone(),
                weight: r.weight,
                status: -1,
                latency_ms: 0,
                error: "breaker open, skip".into(),
            });
            continue;
        }
        // ---- P1:上游并发槽(RAII,请求/流结束自动归还) ----
        let slot = state.acquire_upstream(ukey.clone(), provider_name).await;
        // ---- 指标计时器 ----
        let up_start = std::time::Instant::now();

        match kind {
            crate::config::ProviderKind::Anthropic => {
                let mut areq = req.clone();
                areq["model"] = json!(upstream_model);
                let resp = providers::anthropic::messages(&state, base_url, api_key, areq).await;
                match resp {
                    Ok(resp) if stream => {
                        state.breaker_success(&ukey).await;
                        state.record_metrics_request(Some(&ukey), crate::state::METRIC_STATUS_OK, up_start.elapsed()).await;
                        final_kind = Some("anthropic".into());
                        final_upstream = Some(upstream_model.clone());
                        attempts.push(RequestAttempt {
                            kind: "anthropic".into(),
                            provider: provider_name.clone(),
                            base_url: base_url.clone(),
                            upstream_model: upstream_model.clone(),
                            weight: r.weight,
                            status: 200,
                            latency_ms: up_start.elapsed().as_millis() as u32,
                            error: String::new(),
                        });
                        let trace = RunTrace {
                            request_id: request_id.clone(), user_id: user_id_str.clone(), path: run_path,
                            requested_model: model.clone(), stream,
                            candidates: cand_init.clone(), attempts: attempts.clone(),
                            final_kind: final_kind.clone(), final_upstream_model: final_upstream.clone(),
                            final_status, req_preview: req_preview.clone(), resp_preview: None,
                            latency_ms: run_start.elapsed().as_millis() as u32,
                        };
                        return Ok(cache_wrap(state.clone(), anthropic_passthrough_stream(state.clone(), resp, guard, slot, model.clone(), provider_name.clone(), upstream_model.clone(), multiplier, user.clone(), key_id, base_url.to_string(), api_key.map(|s| s.to_string()), trace), cache_key, stream, embed_meta));
                    }
                    Ok(resp) => {
                        state.breaker_success(&ukey).await;
                        state.record_metrics_request(Some(&ukey), crate::state::METRIC_STATUS_OK, up_start.elapsed()).await;
                        final_kind = Some("anthropic".into());
                        final_upstream = Some(upstream_model.clone());
                        attempts.push(RequestAttempt {
                            kind: "anthropic".into(),
                            provider: provider_name.clone(),
                            base_url: base_url.clone(),
                            upstream_model: upstream_model.clone(),
                            weight: r.weight,
                            status: 200,
                            latency_ms: up_start.elapsed().as_millis() as u32,
                            error: String::new(),
                        });
                        let trace = RunTrace {
                            request_id: request_id.clone(), user_id: user_id_str.clone(), path: run_path,
                            requested_model: model.clone(), stream,
                            candidates: cand_init.clone(), attempts: attempts.clone(),
                            final_kind: final_kind.clone(), final_upstream_model: final_upstream.clone(),
                            final_status, req_preview: req_preview.clone(), resp_preview: None,
                            latency_ms: run_start.elapsed().as_millis() as u32,
                        };
                        return anthropic_passthrough_nonstream(&state, resp, model.clone(), provider_name.clone(), upstream_model.clone(), multiplier, user.clone(), key_id, slot, base_url, api_key, trace).await.map(|r| cache_wrap(state.clone(), r, cache_key, stream, embed_meta));
                    }
                    Err(ApiError::Unavailable(e)) => {
                        let fail_cnt = state.breaker_unavailable(&ukey).await;
                        state.record_metrics_request(Some(&ukey), crate::state::METRIC_STATUS_UNAVAILABLE, up_start.elapsed()).await;
                        state
                            .record_failure(&ukey, &model, "messages", &e.to_string(), fail_cnt)
                            .await;
                        attempts.push(RequestAttempt {
                            kind: "anthropic".into(),
                            provider: provider_name.clone(),
                            base_url: base_url.clone(),
                            upstream_model: upstream_model.clone(),
                            weight: r.weight,
                            status: crate::state::METRIC_STATUS_UNAVAILABLE as i32,
                            latency_ms: up_start.elapsed().as_millis() as u32,
                            error: e,
                        });
                        continue;
                    }
                    Err(e) => {
                        state.record_metrics_request(Some(&ukey), 400, up_start.elapsed()).await;
                        attempts.push(RequestAttempt {
                            kind: "anthropic".into(),
                            provider: provider_name.clone(),
                            base_url: base_url.clone(),
                            upstream_model: upstream_model.clone(),
                            weight: r.weight,
                            status: 400,
                            latency_ms: up_start.elapsed().as_millis() as u32,
                            error: e.to_string(),
                        });
                        let trace = RunTrace {
                            request_id, user_id: user_id_str, path: run_path,
                            requested_model: model, stream,
                            candidates: cand_init, attempts,
                            final_kind, final_upstream_model: final_upstream,
                            final_status: 400, req_preview: req_preview.clone(), resp_preview: None,
                            latency_ms: run_start.elapsed().as_millis() as u32,
                        };
                        trace.emit_error(&state, 400).await;
                        return Err(e);
                    }
                }
            }
            crate::config::ProviderKind::Openai => {
                let obody = crate::translate::anthropic_to_openai_request(&req, upstream_model, stream);
                let resp = providers::openai::chat_completions(&state, base_url, api_key, upstream_model, obody, stream).await;
                match resp {
                    Ok(resp) if stream => {
                        state.breaker_success(&ukey).await;
                        state.record_metrics_request(Some(&ukey), crate::state::METRIC_STATUS_OK, up_start.elapsed()).await;
                        final_kind = Some("openai".into());
                        final_upstream = Some(upstream_model.clone());
                        attempts.push(RequestAttempt {
                            kind: "openai".into(),
                            provider: provider_name.clone(),
                            base_url: base_url.clone(),
                            upstream_model: upstream_model.clone(),
                            weight: r.weight,
                            status: 200,
                            latency_ms: up_start.elapsed().as_millis() as u32,
                            error: String::new(),
                        });
                        let trace = RunTrace {
                            request_id: request_id.clone(), user_id: user_id_str.clone(), path: run_path,
                            requested_model: model.clone(), stream,
                            candidates: cand_init.clone(), attempts: attempts.clone(),
                            final_kind: final_kind.clone(), final_upstream_model: final_upstream.clone(),
                            final_status, req_preview: req_preview.clone(), resp_preview: None,
                            latency_ms: run_start.elapsed().as_millis() as u32,
                        };
                        return Ok(cache_wrap(state.clone(), messages_openai_stream(state.clone(), resp, guard, slot, model.clone(), provider_name.clone(), upstream_model.clone(), multiplier, user.clone(), key_id, kind, base_url.to_string(), api_key.map(|s| s.to_string()), trace), cache_key, stream, embed_meta));
                    }
                    Ok(resp) => {
                        state.breaker_success(&ukey).await;
                        state.record_metrics_request(Some(&ukey), crate::state::METRIC_STATUS_OK, up_start.elapsed()).await;
                        final_kind = Some("openai".into());
                        final_upstream = Some(upstream_model.clone());
                        attempts.push(RequestAttempt {
                            kind: "openai".into(),
                            provider: provider_name.clone(),
                            base_url: base_url.clone(),
                            upstream_model: upstream_model.clone(),
                            weight: r.weight,
                            status: 200,
                            latency_ms: up_start.elapsed().as_millis() as u32,
                            error: String::new(),
                        });
                        let trace = RunTrace {
                            request_id: request_id.clone(), user_id: user_id_str.clone(), path: run_path,
                            requested_model: model.clone(), stream,
                            candidates: cand_init.clone(), attempts: attempts.clone(),
                            final_kind: final_kind.clone(), final_upstream_model: final_upstream.clone(),
                            final_status, req_preview: req_preview.clone(), resp_preview: None,
                            latency_ms: run_start.elapsed().as_millis() as u32,
                        };
                        return messages_openai_nonstream(&state, resp, model.clone(), provider_name.clone(), upstream_model.clone(), multiplier, user.clone(), key_id, slot, kind, base_url, api_key, trace).await.map(|r| cache_wrap(state.clone(), r, cache_key, stream, embed_meta));
                    }
                    Err(ApiError::Unavailable(e)) => {
                        let fail_cnt = state.breaker_unavailable(&ukey).await;
                        state.record_metrics_request(Some(&ukey), crate::state::METRIC_STATUS_UNAVAILABLE, up_start.elapsed()).await;
                        state
                            .record_failure(&ukey, &model, "messages", &e.to_string(), fail_cnt)
                            .await;
                        attempts.push(RequestAttempt {
                            kind: "openai".into(),
                            provider: provider_name.clone(),
                            base_url: base_url.clone(),
                            upstream_model: upstream_model.clone(),
                            weight: r.weight,
                            status: crate::state::METRIC_STATUS_UNAVAILABLE as i32,
                            latency_ms: up_start.elapsed().as_millis() as u32,
                            error: e,
                        });
                        continue;
                    }
                    Err(e) => {
                        state.record_metrics_request(Some(&ukey), 400, up_start.elapsed()).await;
                        attempts.push(RequestAttempt {
                            kind: "openai".into(),
                            provider: provider_name.clone(),
                            base_url: base_url.clone(),
                            upstream_model: upstream_model.clone(),
                            weight: r.weight,
                            status: 400,
                            latency_ms: up_start.elapsed().as_millis() as u32,
                            error: e.to_string(),
                        });
                        let trace = RunTrace {
                            request_id, user_id: user_id_str, path: run_path,
                            requested_model: model, stream,
                            candidates: cand_init, attempts,
                            final_kind, final_upstream_model: final_upstream,
                            final_status: 400, req_preview: req_preview.clone(), resp_preview: None,
                            latency_ms: run_start.elapsed().as_millis() as u32,
                        };
                        trace.emit_error(&state, 400).await;
                        return Err(e);
                    }
                }
            }
        }
    }
    fail_global!();
    let err = exhausted_error(&model, &attempts);
    let trace = RunTrace {
        request_id, user_id: user_id_str, path: run_path,
        requested_model: model.clone(), stream,
        candidates: cand_init, attempts,
        final_kind, final_upstream_model: final_upstream,
        final_status: crate::state::METRIC_STATUS_BAD_GATEWAY as i32,
        req_preview: req_preview.clone(),
        resp_preview: None,
        latency_ms: run_start.elapsed().as_millis() as u32,
    };
    trace.emit_error(&state, crate::state::METRIC_STATUS_BAD_GATEWAY as i32).await;
    Err(err)
}

/// 计费 + 用量事件 + 补记指标 tokens(不增加 req 计数 / 不影响成功率与延迟)。
#[allow(clippy::too_many_arguments)]
async fn anth_charge(
    state: &AppState,
    user: &Arc<crate::state::UserState>,
    key_id: Option<uuid::Uuid>,
    input: u32,
    output: u32,
    multiplier: f64,
    model: String,
    provider: String,
    upstream: String,
    kind: ProviderKind,
    base_url: &str,
    api_key: Option<&str>,
    request_id: Option<String>,
    trace: RunTrace,
) {
    let charged = ((input as f64 + output as f64) * multiplier * user.bill_multiplier()).round() as i64;
    user.deduct(charged);
    user.record_tokens(input.saturating_add(output));
    let tx = state.usage_tx.clone();
    let ev = UsageEvent { user_id: user.id, key_id, cost_usd: 0.0, model, provider, upstream_model: upstream, input_tokens: input, output_tokens: output, charged_tokens: charged, status: 200, request_id };
    tokio::spawn(async move { let _ = tx.send(ev).await; });
    // 补记 token 指标:上游分桶 + 全局
    let k = UpstreamKey::new(kind, base_url, api_key);
    state.record_metrics_tokens(Some(&k), input as u64, output as u64).await;
    trace.emit_success(state, input, output, charged).await;
}

#[allow(clippy::too_many_arguments)]
async fn anthropic_passthrough_nonstream(
    state: &AppState, resp: reqwest::Response, model: String, provider_name: String,
    upstream_model: String, multiplier: f64, user: Arc<crate::state::UserState>,
    key_id: Option<uuid::Uuid>,
    _slot: Option<OwnedSemaphorePermit>,
    base_url: &str,
    api_key: Option<&str>,
    trace: RunTrace,
) -> Result<Response, ApiError> {
    let hdrs = relay_headers(&provider_name, &trace.request_id);
    let mut payload: Value = resp.json().await.map_err(|e| ApiError::Upstream(e.to_string()))?;
    let input = payload.pointer("/usage/input_tokens").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
    let output = payload.pointer("/usage/output_tokens").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
    payload["model"] = json!(model);
    let mut trace = trace;
    trace.resp_preview = resp_preview_of(state, &payload);
    anth_charge(state, &user, key_id, input, output, multiplier, model, provider_name, upstream_model, ProviderKind::Anthropic, base_url, api_key, Some(trace.request_id.clone()), trace).await;
    let mut out = Json(payload).into_response();
    apply_relay_headers(&mut out, &hdrs);
    Ok(out)
}

#[allow(clippy::too_many_arguments)]
fn anthropic_passthrough_stream(
    state: Arc<AppState>, resp: reqwest::Response, guard: crate::state::ConcurrencyGuard,
    slot: Option<OwnedSemaphorePermit>,
    model: String, provider_name: String, upstream_model: String, multiplier: f64, user: Arc<crate::state::UserState>,
    key_id: Option<uuid::Uuid>,
    base_url: String,
    api_key: Option<String>,
    trace: RunTrace,
) -> Response {
    let hdrs = relay_headers(&provider_name, &trace.request_id);
    let s = async_stream::stream! {
        let _guard = guard;
        let _slot = slot;
        let mut upstream = resp.bytes_stream();
        let mut buf: Vec<u8> = Vec::new();
        let mut input = 0u32;
        let mut output = 0u32;
        while let Some(item) = upstream.next().await {
            match item {
                Ok(bytes) => {
                    // 透传 Anthropic 事件,仅把 model 名改回对外名,同时扫 usage。
                    buf.extend_from_slice(&bytes);
                    while let Some(pos) = buf.iter().position(|&b| b == b'\n') {
                        let line: Vec<u8> = buf.drain(..=pos).collect();
                        let s = String::from_utf8_lossy(&line);
                        if let Some(p) = s.trim().strip_prefix("data:") {
                            if let Ok(v) = serde_json::from_str::<Value>(p.trim()) {
                                if let Some(i) = v.pointer("/message/usage/input_tokens").and_then(|x| x.as_u64()) { input = i as u32; }
                                if let Some(o) = v.pointer("/usage/output_tokens").and_then(|x| x.as_u64()) { output = o as u32; }
                            }
                        }
                    }
                    yield Ok::<Bytes, std::io::Error>(rewrite_model(&bytes, &upstream_model, &model));
                }
                Err(e) => { yield Err(std::io::Error::new(std::io::ErrorKind::Other, e.to_string())); break; }
            }
        }
        anth_charge(&state, &user, key_id, input, output, multiplier, model, provider_name, upstream_model, ProviderKind::Anthropic, &base_url, api_key.as_deref(), Some(trace.request_id.clone()), trace).await;
    };
    let mut resp = sse_response(s);
    apply_relay_headers(&mut resp, &hdrs);
    resp
}

#[allow(clippy::too_many_arguments)]
async fn messages_openai_nonstream(
    state: &AppState, resp: reqwest::Response, model: String, provider_name: String,
    upstream_model: String, multiplier: f64, user: Arc<crate::state::UserState>,
    key_id: Option<uuid::Uuid>,
    _slot: Option<OwnedSemaphorePermit>,
    kind: ProviderKind,
    base_url: &str,
    api_key: Option<&str>,
    trace: RunTrace,
) -> Result<Response, ApiError> {
    let hdrs = relay_headers(&provider_name, &trace.request_id);
    let oai: Value = resp.json().await.map_err(|e| ApiError::Upstream(e.to_string()))?;
    let input = oai.pointer("/usage/prompt_tokens").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
    let output = oai.pointer("/usage/completion_tokens").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
    let payload = crate::translate::openai_to_anthropic_response(&oai, &model);
    let mut trace = trace;
    trace.resp_preview = resp_preview_of(state, &payload);
    anth_charge(state, &user, key_id, input, output, multiplier, model, provider_name, upstream_model, kind, base_url, api_key, Some(trace.request_id.clone()), trace).await;
    let mut out = Json(payload).into_response();
    apply_relay_headers(&mut out, &hdrs);
    Ok(out)
}

/// OpenAI 上游 SSE → Anthropic 事件流(状态机)。
#[allow(clippy::too_many_arguments)]
fn messages_openai_stream(
    state: Arc<AppState>, resp: reqwest::Response, guard: crate::state::ConcurrencyGuard,
    slot: Option<OwnedSemaphorePermit>,
    model: String, provider_name: String, upstream_model: String, multiplier: f64, user: Arc<crate::state::UserState>,
    key_id: Option<uuid::Uuid>,
    kind: ProviderKind,
    base_url: String,
    api_key: Option<String>,
    trace: RunTrace,
) -> Response {
    let hdrs = relay_headers(&provider_name, &trace.request_id);
    let s = async_stream::stream! {
        let _guard = guard;
        let _slot = slot;
        let mut upstream = resp.bytes_stream();
        let mut buf: Vec<u8> = Vec::new();
        let mut started = false;
        let mut input = 0u32;
        let mut output = 0u32;
        let mut finish: Option<String> = None;
        let mut cur: Option<(i64, bool)> = None; // (block index, is_tool)
        let mut next_index = 0i64;
        let mut tool_map: std::collections::HashMap<u64, i64> = std::collections::HashMap::new();
        let msg_id = format!("msg_{}", &uuid::Uuid::new_v4().to_string()[..12]);

        while let Some(item) = upstream.next().await {
            let bytes = match item { Ok(b) => b, Err(e) => { yield Err(std::io::Error::new(std::io::ErrorKind::Other, e.to_string())); break; } };
            buf.extend_from_slice(&bytes);
            while let Some(pos) = buf.iter().position(|&b| b == b'\n') {
                let line: Vec<u8> = buf.drain(..=pos).collect();
                let line = String::from_utf8_lossy(&line);
                let Some(p) = line.trim().strip_prefix("data:") else { continue };
                let p = p.trim();
                if p.is_empty() || p == "[DONE]" { continue; }
                let Ok(v) = serde_json::from_str::<Value>(p) else { continue };

                if let Some(u) = v.get("usage").filter(|u| u.is_object()) {
                    if let Some(i) = u.get("prompt_tokens").and_then(|x| x.as_u64()) { input = i as u32; }
                    if let Some(o) = u.get("completion_tokens").and_then(|x| x.as_u64()) { output = o as u32; }
                }
                if !started {
                    started = true;
                    yield Ok::<Bytes, std::io::Error>(a_event("message_start", json!({
                        "type":"message_start",
                        "message":{"id":msg_id,"type":"message","role":"assistant","model":model,"content":[],"stop_reason":Value::Null,"stop_sequence":Value::Null,"usage":{"input_tokens":input,"output_tokens":0}}
                    })));
                }
                let delta = v.pointer("/choices/0/delta");
                if let Some(text) = delta.and_then(|d| d.get("content")).and_then(|c| c.as_str()) {
                    if !text.is_empty() {
                        if !matches!(cur, Some((_, false))) {
                            if let Some((idx, _)) = cur { yield Ok(a_event("content_block_stop", json!({"type":"content_block_stop","index":idx}))); }
                            yield Ok(a_event("content_block_start", json!({"type":"content_block_start","index":next_index,"content_block":{"type":"text","text":""}})));
                            cur = Some((next_index, false)); next_index += 1;
                        }
                        let idx = cur.unwrap().0;
                        yield Ok(a_event("content_block_delta", json!({"type":"content_block_delta","index":idx,"delta":{"type":"text_delta","text":text}})));
                    }
                }
                if let Some(tcs) = delta.and_then(|d| d.get("tool_calls")).and_then(|t| t.as_array()) {
                    for tc in tcs {
                        let oi = tc.get("index").and_then(|v| v.as_u64()).unwrap_or(0);
                        if !tool_map.contains_key(&oi) {
                            if let Some((idx, _)) = cur { yield Ok(a_event("content_block_stop", json!({"type":"content_block_stop","index":idx}))); }
                            let bidx = next_index; next_index += 1; tool_map.insert(oi, bidx);
                            let tid = tc.get("id").and_then(|v| v.as_str()).unwrap_or("");
                            let name = tc.pointer("/function/name").and_then(|v| v.as_str()).unwrap_or("");
                            yield Ok(a_event("content_block_start", json!({"type":"content_block_start","index":bidx,"content_block":{"type":"tool_use","id":tid,"name":name,"input":{}}})));
                            cur = Some((bidx, true));
                        }
                        if let Some(args) = tc.pointer("/function/arguments").and_then(|v| v.as_str()) {
                            if !args.is_empty() {
                                let bidx = *tool_map.get(&oi).unwrap();
                                yield Ok(a_event("content_block_delta", json!({"type":"content_block_delta","index":bidx,"delta":{"type":"input_json_delta","partial_json":args}})));
                            }
                        }
                    }
                }
                if let Some(f) = v.pointer("/choices/0/finish_reason").and_then(|v| v.as_str()) {
                    finish = Some(f.to_string());
                }
            }
        }

        if let Some((idx, _)) = cur { yield Ok(a_event("content_block_stop", json!({"type":"content_block_stop","index":idx}))); }
        if started {
            let stop = crate::translate::map_finish_to_stop(finish.as_deref());
            yield Ok(a_event("message_delta", json!({"type":"message_delta","delta":{"stop_reason":stop,"stop_sequence":Value::Null},"usage":{"output_tokens":output}})));
            yield Ok(a_event("message_stop", json!({"type":"message_stop"})));
        }
        anth_charge(&state, &user, key_id, input, output, multiplier, model, provider_name, upstream_model, kind, &base_url, api_key.as_deref(), Some(trace.request_id.clone()), trace).await;
    };
    let mut resp = sse_response(s);
    apply_relay_headers(&mut resp, &hdrs);
    resp
}

fn a_event(event: &str, data: Value) -> Bytes {
    Bytes::from(format!("event: {event}\ndata: {data}\n\n"))
}

fn sse_response<S>(s: S) -> Response
where
    S: futures::Stream<Item = Result<Bytes, std::io::Error>> + Send + 'static,
{
    Response::builder()
        .header("content-type", "text/event-stream")
        .header("cache-control", "no-cache")
        .body(Body::from_stream(s))
        .unwrap()
}

/// cache_wrap 写回元数据:model/provider 为向量隔离维度,embed_text 用于 L2 向量生成。
#[derive(Clone)]
struct CacheEmbedMeta {
    model: String,
    provider: String,
    embed_text: Option<String>,
}

/// L2 语义查找:L1 未命中时 embed 请求文本,与 cache_vectors 暴力余弦,
/// 相似度达阈值取最优者回查 L1 拿响应体。任何失败返回 None(静默降级为 miss),
/// embedding 供应商未配置时直接 None —— 语义缓存自动只跑 L1。
async fn semantic_lookup(
    state: &Arc<AppState>,
    model: &str,
    provider: &str,
    req_text: &str,
) -> Option<(crate::semantic_cache::Entry, f64)> {
    let ec = state.config.load().embedding.clone();
    if !ec.usable() {
        return None;
    }
    let query = crate::embedding::embed(&state.http, &ec.base_url, &ec.api_key, &ec.model, req_text)
        .await
        .ok()?;
    let rows = crate::storage::load_scope_vectors(&state.db, model, provider)
        .await
        .ok()?;
    let thr = state.config.load().cache_semantic.similarity_threshold;
    let mut best: Option<(String, f64)> = None;
    for (key, vec) in rows {
        let sim = crate::embedding::cosine(&query, &vec) as f64;
        if sim >= thr && best.as_ref().map_or(true, |(_, bs)| sim > *bs) {
            best = Some((key, sim));
        }
    }
    let (key, sim) = best?;
    // 回查 L1:对应条目已被 TTL/容量淘汰则视为 miss(向量随龄清理,短暂不一致可接受)。
    let entry = state.semantic_cache.get(&key)?;
    Some((entry, sim))
}

/// 语义缓存写回包装:透传原响应,同时旁路累积响应体;
/// 流完整结束(客户端未中断)才提取 usage 并写入缓存;命中回放走 replay_response。
/// 写回后若 embedding 已配置且带 embed_text,则顺路生成向量入库(L2)。
fn cache_wrap(
    state: Arc<AppState>,
    resp: Response,
    cache_key: Option<String>,
    is_stream: bool,
    meta: Option<CacheEmbedMeta>,
) -> Response {
    let Some(key) = cache_key else {
        return resp;
    };
    let cfg_snap = (**state.config.load()).clone();
    let ttl = cfg_snap.cache_semantic.ttl_secs;
    let (parts, body) = resp.into_parts();
    let content_type = parts
        .headers
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or(if is_stream { "text/event-stream" } else { "application/json" })
        .to_string();
    let upstream = body.into_data_stream();
    let s = async_stream::stream! {
        let mut upstream = upstream;
        let mut buf: Vec<u8> = Vec::new();
        let mut complete = true;
        // 超过单条上限即提前放弃缓存:清空累积并继续透传(逃生口兜底,不让大响应卡住链路)。
        let max_body = crate::semantic_cache::MAX_BODY;
        let mut caching = true;
        while let Some(chunk) = upstream.next().await {
            match chunk {
                Ok(b) => {
                    if caching && buf.len() + b.len() > max_body {
                        caching = false;
                        buf.clear();
                    }
                    if caching {
                        buf.extend_from_slice(&b);
                    }
                    yield Ok::<Bytes, axum::Error>(b);
                }
                Err(_) => {
                    complete = false;
                    break;
                }
            }
        }
        if caching && complete && !buf.is_empty() {
            let body_bytes = Bytes::from(buf);
            let (input_tokens, output_tokens) =
                crate::semantic_cache::extract_usage(&body_bytes, is_stream);
            let now = crate::semantic_cache::now_secs();
            state.semantic_cache.store(key.clone(), crate::semantic_cache::Entry {
                content_type,
                body: body_bytes,
                input_tokens,
                output_tokens,
                expires_at: now + ttl,
            });
            // L2 向量写回:embedding 配置就绪才生成;任何失败静默跳过(纯旁路增强)。
            if let Some(m) = meta {
                if let Some(text) = m.embed_text {
                    if cfg_snap.embedding.usable() {
                        if let Ok(vec) = crate::embedding::embed(
                            &state.http,
                            &cfg_snap.embedding.base_url,
                            &cfg_snap.embedding.api_key,
                            &cfg_snap.embedding.model,
                            &text,
                        )
                        .await
                        {
                            let emb = crate::embedding::encode_vec_b64(&vec);
                            let _ = crate::storage::save_cache_vector(
                                &state.db, &key, &m.model, &m.provider, &emb, now, ttl,
                            )
                            .await;
                        }
                    }
                }
            }
        }
    };
    let mut resp = Response::from_parts(parts, Body::from_stream(s));
    if let Ok(v) = axum::http::HeaderValue::from_str("MISS") {
        resp.headers_mut().insert("x-relay-cache", v);
    }
    resp
}

/// 构造一个 OpenAI chat.completion.chunk 的 SSE 行。
fn oai_chunk(id: &str, model: &str, delta: Value, finish: Option<&str>) -> String {
    let v = json!({
        "id": id,
        "object": "chat.completion.chunk",
        "model": model,
        "choices": [{ "index": 0, "delta": delta, "finish_reason": finish }],
    });
    format!("data: {v}\n\n")
}

#[allow(clippy::too_many_arguments)]
fn stream_response(
    state: Arc<AppState>,
    resp: reqwest::Response,
    guard: crate::state::ConcurrencyGuard,
    slot: Option<OwnedSemaphorePermit>,
    model: String,
    provider_name: String,
    upstream_model: String,
    multiplier: f64,
    user: Arc<crate::state::UserState>,
    key_id: Option<uuid::Uuid>,
    base_url: String,
    api_key: Option<String>,
    trace: RunTrace,
) -> Response {
    let user_id = user.id;
    let hdrs = relay_headers(&provider_name, &trace.request_id);
    let s = async_stream::stream! {
        // guard 在 stream 生命周期内持有,流结束(或客户端断开导致 drop)才释放并发名额。
        let _guard = guard;
        let _slot = slot;
        let mut upstream = resp.bytes_stream();
        let mut buf: Vec<u8> = Vec::new();
        let mut input = 0u32;
        let mut output = 0u32;

        while let Some(item) = upstream.next().await {
            match item {
                Ok(bytes) => {
                    scan_usage(&mut buf, &bytes, &mut input, &mut output);
                    // 把响应里的上游模型名透明改回用户请求的对外名。
                    let out = rewrite_model(&bytes, &upstream_model, &model);
                    yield Ok::<Bytes, std::io::Error>(out);
                }
                Err(e) => {
                    yield Err(std::io::Error::new(std::io::ErrorKind::Other, e.to_string()));
                    break;
                }
            }
        }

        let charged =
        ((input as f64 + output as f64) * multiplier * user.bill_multiplier()).round() as i64;
        user.deduct(charged);
        user.record_tokens(input.saturating_add(output));
        let _ = state
            .usage_tx
            .send(UsageEvent {
                user_id,
                key_id,
                cost_usd: 0.0,
                model,
                provider: provider_name,
                upstream_model,
                input_tokens: input,
                output_tokens: output,
                charged_tokens: charged,
                status: 200,
                request_id: Some(trace.request_id.clone()),
            })
            .await;
        // 补记指标 tokens(OpenAI stream)
        let k = UpstreamKey::new(ProviderKind::Openai, &base_url, api_key.as_deref());
        state.record_metrics_tokens(Some(&k), input as u64, output as u64).await;
        trace.emit_success(&state, input, output, charged).await;
    };

    let mut resp = sse_response(s);
    apply_relay_headers(&mut resp, &hdrs);
    resp
}

#[allow(clippy::too_many_arguments)]
async fn non_stream_response(
    state: &AppState,
    resp: reqwest::Response,
    model: String,
    provider_name: String,
    upstream_model: String,
    multiplier: f64,
    user: Arc<crate::state::UserState>,
    key_id: Option<uuid::Uuid>,
    _slot: Option<OwnedSemaphorePermit>,
    kind: ProviderKind,
    base_url: &str,
    api_key: Option<&str>,
    trace: RunTrace,
) -> Result<Response, ApiError> {
    let hdrs = relay_headers(&provider_name, &trace.request_id);
    let mut payload: Value = resp
        .json()
        .await
        .map_err(|e| ApiError::Upstream(e.to_string()))?;

    let input = payload
        .pointer("/usage/prompt_tokens")
        .and_then(|v| v.as_u64())
        .unwrap_or(0) as u32;
    let output = payload
        .pointer("/usage/completion_tokens")
        .and_then(|v| v.as_u64())
        .unwrap_or(0) as u32;

    let charged =
        ((input as f64 + output as f64) * multiplier * user.bill_multiplier()).round() as i64;
    user.deduct(charged);
    user.record_tokens(input.saturating_add(output));

    // 把上游真名换回客户端请求的逻辑名。
    payload["model"] = json!(model);

    let _ = state
        .usage_tx
        .send(UsageEvent {
            user_id: user.id,
            key_id,
            cost_usd: 0.0,
            model,
            provider: provider_name,
            upstream_model,
            input_tokens: input,
            output_tokens: output,
            charged_tokens: charged,
            status: 200,
            request_id: Some(trace.request_id.clone()),
        })
        .await;
    // 补记指标 tokens(OpenAI nonstream)
    let k = UpstreamKey::new(kind, base_url, api_key);
    state.record_metrics_tokens(Some(&k), input as u64, output as u64).await;
    let mut trace = trace;
    trace.resp_preview = resp_preview_of(state, &payload);
    trace.emit_success(state, input, output, charged).await;

    let mut out = Json(payload).into_response();
    apply_relay_headers(&mut out, &hdrs);
    Ok(out)
}

/// 字节级把 `"model":"<upstream>"` 改写成对外名(ASCII 模式,避免 UTF-8 破坏)。
fn rewrite_model(bytes: &Bytes, upstream: &str, public: &str) -> Bytes {
    if upstream == public {
        return bytes.clone();
    }
    let mut data = bytes.to_vec();
    for sep in ["\":\"", "\": \""] {
        let from = format!("\"model{sep}{upstream}\"").into_bytes();
        let to = format!("\"model{sep}{public}\"").into_bytes();
        data = replace_all_bytes(&data, &from, &to);
    }
    Bytes::from(data)
}

fn replace_all_bytes(haystack: &[u8], from: &[u8], to: &[u8]) -> Vec<u8> {
    if from.is_empty() || haystack.len() < from.len() {
        return haystack.to_vec();
    }
    let mut out = Vec::with_capacity(haystack.len());
    let mut i = 0;
    while i < haystack.len() {
        if i + from.len() <= haystack.len() && &haystack[i..i + from.len()] == from {
            out.extend_from_slice(to);
            i += from.len();
        } else {
            out.push(haystack[i]);
            i += 1;
        }
    }
    out
}

/// 从 SSE 字节流里增量提取 OpenAI 的 usage(出现在 include_usage 的末尾 chunk)。
fn scan_usage(buf: &mut Vec<u8>, new: &[u8], input: &mut u32, output: &mut u32) {
    buf.extend_from_slice(new);
    while let Some(pos) = buf.iter().position(|&b| b == b'\n') {
        let line: Vec<u8> = buf.drain(..=pos).collect();
        let line = String::from_utf8_lossy(&line);
        let line = line.trim();
        let Some(payload) = line.strip_prefix("data:") else {
            continue;
        };
        let payload = payload.trim();
        if payload.is_empty() || payload == "[DONE]" {
            continue;
        }
        if let Ok(v) = serde_json::from_str::<Value>(payload) {
            if let Some(u) = v.get("usage") {
                if u.is_object() {
                    if let Some(p) = u.get("prompt_tokens").and_then(|x| x.as_u64()) {
                        *input = p as u32;
                    }
                    if let Some(c) = u.get("completion_tokens").and_then(|x| x.as_u64()) {
                        *output = c as u32;
                    }
                }
            }
        }
    }
}

/// POST /v1/responses —— OpenAI Responses 协议入站。
/// 请求转换为等价 chat 请求后复用 run_chat 主管道(路由/熔断/计费/链路/缓存全复用),
/// 出站把 chat 响应(非流式 JSON / 流式 SSE 字节流)回译为 Responses 格式。
pub async fn responses(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, ApiError> {
    let auth = authenticate(&state, &headers)?;
    let req: Value = serde_json::from_slice(&body)
        .map_err(|e| ApiError::BadRequest(format!("invalid json: {e}")))?;
    crate::responses::validate_responses(&req)?;
    let no_cache = no_cache_requested(&headers);
    let stream = req.get("stream").and_then(|s| s.as_bool()).unwrap_or(false);
    let chat_req = crate::responses::responses_to_chat(&req)?;
    let model = chat_req.get("model").and_then(|m| m.as_str()).unwrap_or_default().to_string();

    let resp = run_chat(state, auth.user, Some(auth.key_id), chat_req, no_cache).await?;
    let (parts, body) = resp.into_parts();
    let new_body = if stream {
        // 流式:chat SSE 字节流 → Responses 事件流。
        crate::responses::chat_sse_to_responses_body(body, model)
    } else {
        // 非流式:chat JSON → Responses JSON(保留 run_chat 的响应头,如 x-relay-upstream)。
        let bytes = axum::body::to_bytes(body, 32 * 1024 * 1024)
            .await
            .map_err(|e| ApiError::Internal(e.to_string()))?;
        let chat: Value = serde_json::from_slice(&bytes)
            .map_err(|e| ApiError::Internal(format!("上游响应解析失败: {e}")))?;
        let out = serde_json::to_vec(&crate::responses::chat_to_responses(&chat))
            .unwrap_or_default();
        Body::from(out)
    };
    Ok(Response::from_parts(parts, new_body))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn skipped_attempt() -> RequestAttempt {
        RequestAttempt {
            kind: "skipped".into(),
            provider: "p1".into(),
            base_url: "https://u1".into(),
            upstream_model: "m".into(),
            weight: 100,
            status: -1,
            latency_ms: 0,
            error: "breaker open, skip".into(),
        }
    }

    #[test]
    fn exhausted_error_empty_attempts_maps_to_no_target() {
        assert!(matches!(exhausted_error("gpt-x", &[]), ApiError::NoTarget(_)));
    }

    #[test]
    fn exhausted_error_all_breaker_skipped_maps_to_no_target() {
        // 全被熔断跳过:从未真正发起请求,对外应为 NoTarget 而非带 last_status=-1 的 AllFailed。
        let attempts = vec![skipped_attempt()];
        assert!(matches!(exhausted_error("gpt-x", &attempts), ApiError::NoTarget(_)));
    }

    #[test]
    fn exhausted_error_with_real_attempts_picks_last_real() {
        let attempts = vec![
            RequestAttempt { kind: "openai".into(), provider: "p1".into(), base_url: "https://u1".into(), upstream_model: "m".into(), weight: 100, status: 503, latency_ms: 10, error: "upstream down".into() },
            RequestAttempt { kind: "skipped".into(), provider: "p2".into(), base_url: "https://u2".into(), upstream_model: "m".into(), weight: 100, status: -1, latency_ms: 0, error: "breaker open, skip".into() },
        ];
        let e = exhausted_error("gpt-x", &attempts);
        // 末尾的 skipped 条目应被过滤,取末次真实尝试(503)。
        assert!(matches!(e, ApiError::AllFailed { last_status: 503, ref last_error, .. } if last_error == "upstream down"));
    }

    #[test]
    fn no_cache_requested_matches_only_exact_header() {
        let mut h = HeaderMap::new();
        assert!(!no_cache_requested(&h), "无头时不应触发逃生口");
        h.insert("x-relay-cache-control", axum::http::HeaderValue::from_static("no-cache"));
        assert!(no_cache_requested(&h));
        h.insert("x-relay-cache-control", axum::http::HeaderValue::from_static("No-Cache"));
        assert!(no_cache_requested(&h), "头部名不区分大小写");
        h.insert("x-relay-cache-control", axum::http::HeaderValue::from_static("max-age=0"));
        assert!(!no_cache_requested(&h), "非 no-cache 值不触发");
    }

    #[test]
    fn validate_chat_payload_rejects_missing_messages() {
        let req = serde_json::json!({"model": "m", "max_tokens": 10});
        assert!(matches!(
            validate_chat_payload(&req),
            Err(ApiError::BadRequest(ref m)) if m.contains("missing `messages`")
        ));
    }

    #[test]
    fn validate_chat_payload_rejects_empty_messages() {
        let req = serde_json::json!({"model": "m", "max_tokens": 10, "messages": []});
        assert!(matches!(
            validate_chat_payload(&req),
            Err(ApiError::BadRequest(ref m)) if m.contains("不能为空数组")
        ));
    }

    #[test]
    fn validate_chat_payload_rejects_non_array_messages() {
        let req = serde_json::json!({"model": "m", "max_tokens": 10, "messages": "hi"});
        assert!(matches!(
            validate_chat_payload(&req),
            Err(ApiError::BadRequest(ref m)) if m.contains("必须为数组")
        ));
    }

    #[test]
    fn validate_chat_payload_rejects_stream_and_max_tokens_type_errors() {
        let bad_stream = serde_json::json!({"model": "m", "max_tokens": 10, "messages": [{"role":"user","content":"hi"}], "stream": "yes"});
        assert!(matches!(
            validate_chat_payload(&bad_stream),
            Err(ApiError::BadRequest(ref m)) if m.contains("`stream` 必须为布尔值")
        ));
        let bad_maxtok = serde_json::json!({"model": "m", "max_tokens": "100", "messages": [{"role":"user","content":"hi"}]});
        assert!(matches!(
            validate_chat_payload(&bad_maxtok),
            Err(ApiError::BadRequest(ref m)) if m.contains("`max_tokens` 必须为正整数")
        ));
    }

    #[test]
    fn validate_chat_payload_accepts_valid_payload() {
        let req = serde_json::json!({"model": "m", "max_tokens": 10, "messages": [{"role":"user","content":"hi"}], "stream": true});
        assert!(validate_chat_payload(&req).is_ok());
    }
}
