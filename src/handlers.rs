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

use crate::auth::authenticate;
use crate::error::ApiError;
use crate::state::{AppState, UsageEvent};
use crate::providers;

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
        .map(|n| json!({ "id": n, "object": "model", "owned_by": "runapi" }))
        .collect();
    Ok(Json(json!({ "object": "list", "data": data })))
}

/// POST /v1/chat/completions —— OpenAI 兼容入口(API Key 鉴权)。
pub async fn chat_completions(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, ApiError> {
    let auth = authenticate(&state, &headers)?;
    let req: Value = serde_json::from_slice(&body)
        .map_err(|e| ApiError::BadRequest(format!("invalid json: {e}")))?;
    run_chat(state, auth.user, req).await
}

/// 数据面核心:对已鉴权的用户执行一次 chat 调用(路由 + 上游 + 计费 + 并发)。
/// 同时供 `/v1/chat/completions`(Key)与 `/portal/chat`(门户 JWT)复用。
pub async fn run_chat(
    state: Arc<AppState>,
    user: Arc<crate::state::UserState>,
    req: Value,
) -> Result<Response, ApiError> {
    if user.balance() <= 0 {
        return Err(ApiError::InsufficientBalance);
    }

    let model = req
        .get("model")
        .and_then(|m| m.as_str())
        .ok_or_else(|| ApiError::BadRequest("missing `model`".into()))?
        .to_string();
    let stream = req.get("stream").and_then(|s| s.as_bool()).unwrap_or(false);

    // 占用并发名额(RAII,流结束/出错自动释放)。
    let guard = user.try_acquire().ok_or(ApiError::TooManyRequests)?;

    // 路由解析:按用户所在模型组,把对外模型名解析到具体上游。
    let r = state.routing.load().resolve(user.group(), &model)?;
    let kind = r.kind;
    let (provider_name, base_url, api_key, upstream_model, multiplier) =
        (r.provider, r.base_url, r.api_key, r.upstream_model, r.multiplier);

    match kind {
        crate::config::ProviderKind::Openai => {
            let resp = providers::openai::chat_completions(
                &state, &base_url, api_key.as_deref(), &upstream_model, req, stream,
            )
            .await?;
            if stream {
                Ok(stream_response(state, resp, guard, model, provider_name, upstream_model, multiplier, user))
            } else {
                non_stream_response(&state, resp, model, provider_name, upstream_model, multiplier, user).await
            }
        }
        crate::config::ProviderKind::Anthropic => {
            // OpenAI 入站 → Anthropic 上游:翻译请求 + 翻译响应。
            let abody = crate::translate::openai_to_anthropic(&req, &upstream_model, stream);
            let resp = providers::anthropic::messages(&state, &base_url, api_key.as_deref(), abody).await?;
            if stream {
                Ok(anthropic_stream_response(state, resp, guard, model, provider_name, upstream_model, multiplier, user))
            } else {
                anthropic_nonstream_response(&state, resp, model, provider_name, upstream_model, multiplier, user).await
            }
        }
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
) -> Result<Response, ApiError> {
    let aresp: Value = resp.json().await.map_err(|e| ApiError::Upstream(e.to_string()))?;
    let input = aresp.pointer("/usage/input_tokens").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
    let output = aresp.pointer("/usage/output_tokens").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
    let charged = ((input as f64 + output as f64) * multiplier * user.bill_multiplier()).round() as i64;
    user.deduct(charged);

    let payload = crate::translate::anthropic_to_openai(&aresp, &model);
    let _ = state.usage_tx.send(UsageEvent {
        user_id: user.id, model, provider: provider_name, upstream_model,
        input_tokens: input, output_tokens: output, charged_tokens: charged, status: 200,
    }).await;
    Ok(Json(payload).into_response())
}

#[allow(clippy::too_many_arguments)]
fn anthropic_stream_response(
    state: Arc<AppState>,
    resp: reqwest::Response,
    guard: crate::state::ConcurrencyGuard,
    model: String,
    provider_name: String,
    upstream_model: String,
    multiplier: f64,
    user: Arc<crate::state::UserState>,
) -> Response {
    let user_id = user.id;
    let s = async_stream::stream! {
        let _guard = guard;
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
                                        if let Some(text) = ev.pointer("/delta/text").and_then(|v| v.as_str()) {
                                            yield Ok(Bytes::from(oai_chunk(&id, &model, json!({"content": text}), None)));
                                        }
                                    }
                                    // Anthropic 扩展思考 → OpenAI reasoning_content(前端按主流字段渲染)。
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
        let _ = state.usage_tx.send(UsageEvent {
            user_id, model, provider: provider_name, upstream_model,
            input_tokens: input, output_tokens: output, charged_tokens: charged, status: 200,
        }).await;
    };

    Response::builder()
        .header("content-type", "text/event-stream")
        .header("cache-control", "no-cache")
        .body(Body::from_stream(s))
        .unwrap()
}

// ================== Anthropic 入站 /v1/messages ==================

/// POST /v1/messages —— Anthropic 兼容入口(API Key 鉴权)。
pub async fn messages(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, ApiError> {
    let auth = authenticate(&state, &headers)?;
    let req: Value = serde_json::from_slice(&body)
        .map_err(|e| ApiError::BadRequest(format!("invalid json: {e}")))?;
    run_messages(state, auth.user, req).await
}

/// Anthropic 入站核心:路由 + 上游(Anthropic 直通 / OpenAI 翻译)+ 计费。
pub async fn run_messages(
    state: Arc<AppState>,
    user: Arc<crate::state::UserState>,
    req: Value,
) -> Result<Response, ApiError> {
    if user.balance() <= 0 {
        return Err(ApiError::InsufficientBalance);
    }
    let model = req
        .get("model")
        .and_then(|m| m.as_str())
        .ok_or_else(|| ApiError::BadRequest("missing `model`".into()))?
        .to_string();
    let stream = req.get("stream").and_then(|s| s.as_bool()).unwrap_or(false);
    let guard = user.try_acquire().ok_or(ApiError::TooManyRequests)?;

    let r = state.routing.load().resolve(user.group(), &model)?;
    let kind = r.kind;
    let (provider_name, base_url, api_key, upstream_model, multiplier) =
        (r.provider, r.base_url, r.api_key, r.upstream_model, r.multiplier);

    match kind {
        crate::config::ProviderKind::Anthropic => {
            let mut areq = req;
            areq["model"] = json!(upstream_model);
            let resp = providers::anthropic::messages(&state, &base_url, api_key.as_deref(), areq).await?;
            if stream {
                Ok(anthropic_passthrough_stream(state, resp, guard, model, provider_name, upstream_model, multiplier, user))
            } else {
                anthropic_passthrough_nonstream(&state, resp, model, provider_name, upstream_model, multiplier, user).await
            }
        }
        crate::config::ProviderKind::Openai => {
            let obody = crate::translate::anthropic_to_openai_request(&req, &upstream_model, stream);
            let resp = providers::openai::chat_completions(&state, &base_url, api_key.as_deref(), &upstream_model, obody, stream).await?;
            if stream {
                Ok(messages_openai_stream(state, resp, guard, model, provider_name, upstream_model, multiplier, user))
            } else {
                messages_openai_nonstream(&state, resp, model, provider_name, upstream_model, multiplier, user).await
            }
        }
    }
}

fn anth_charge(state: &AppState, user: &Arc<crate::state::UserState>, input: u32, output: u32, multiplier: f64, model: String, provider: String, upstream: String) {
    let charged = ((input as f64 + output as f64) * multiplier * user.bill_multiplier()).round() as i64;
    user.deduct(charged);
    let tx = state.usage_tx.clone();
    let ev = UsageEvent { user_id: user.id, model, provider, upstream_model: upstream, input_tokens: input, output_tokens: output, charged_tokens: charged, status: 200 };
    tokio::spawn(async move { let _ = tx.send(ev).await; });
}

#[allow(clippy::too_many_arguments)]
async fn anthropic_passthrough_nonstream(
    state: &AppState, resp: reqwest::Response, model: String, provider_name: String,
    upstream_model: String, multiplier: f64, user: Arc<crate::state::UserState>,
) -> Result<Response, ApiError> {
    let mut payload: Value = resp.json().await.map_err(|e| ApiError::Upstream(e.to_string()))?;
    let input = payload.pointer("/usage/input_tokens").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
    let output = payload.pointer("/usage/output_tokens").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
    payload["model"] = json!(model);
    anth_charge(state, &user, input, output, multiplier, model, provider_name, upstream_model);
    Ok(Json(payload).into_response())
}

#[allow(clippy::too_many_arguments)]
fn anthropic_passthrough_stream(
    state: Arc<AppState>, resp: reqwest::Response, guard: crate::state::ConcurrencyGuard,
    model: String, provider_name: String, upstream_model: String, multiplier: f64, user: Arc<crate::state::UserState>,
) -> Response {
    let s = async_stream::stream! {
        let _guard = guard;
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
        anth_charge(&state, &user, input, output, multiplier, model, provider_name, upstream_model);
    };
    sse_response(s)
}

#[allow(clippy::too_many_arguments)]
async fn messages_openai_nonstream(
    state: &AppState, resp: reqwest::Response, model: String, provider_name: String,
    upstream_model: String, multiplier: f64, user: Arc<crate::state::UserState>,
) -> Result<Response, ApiError> {
    let oai: Value = resp.json().await.map_err(|e| ApiError::Upstream(e.to_string()))?;
    let input = oai.pointer("/usage/prompt_tokens").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
    let output = oai.pointer("/usage/completion_tokens").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
    let payload = crate::translate::openai_to_anthropic_response(&oai, &model);
    anth_charge(state, &user, input, output, multiplier, model, provider_name, upstream_model);
    Ok(Json(payload).into_response())
}

/// OpenAI 上游 SSE → Anthropic 事件流(状态机)。
#[allow(clippy::too_many_arguments)]
fn messages_openai_stream(
    state: Arc<AppState>, resp: reqwest::Response, guard: crate::state::ConcurrencyGuard,
    model: String, provider_name: String, upstream_model: String, multiplier: f64, user: Arc<crate::state::UserState>,
) -> Response {
    let s = async_stream::stream! {
        let _guard = guard;
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
        anth_charge(&state, &user, input, output, multiplier, model, provider_name, upstream_model);
    };
    sse_response(s)
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
    model: String,
    provider_name: String,
    upstream_model: String,
    multiplier: f64,
    user: Arc<crate::state::UserState>,
) -> Response {
    let user_id = user.id;
    let s = async_stream::stream! {
        // guard 在 stream 生命周期内持有,流结束(或客户端断开导致 drop)才释放并发名额。
        let _guard = guard;
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
        let _ = state
            .usage_tx
            .send(UsageEvent {
                user_id,
                model,
                provider: provider_name,
                upstream_model,
                input_tokens: input,
                output_tokens: output,
                charged_tokens: charged,
                status: 200,
            })
            .await;
    };

    Response::builder()
        .header("content-type", "text/event-stream")
        .header("cache-control", "no-cache")
        .body(Body::from_stream(s))
        .unwrap()
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
) -> Result<Response, ApiError> {
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

    // 把上游真名换回客户端请求的逻辑名。
    payload["model"] = json!(model);

    let _ = state
        .usage_tx
        .send(UsageEvent {
            user_id: user.id,
            model,
            provider: provider_name,
            upstream_model,
            input_tokens: input,
            output_tokens: output,
            charged_tokens: charged,
            status: 200,
        })
        .await;

    Ok(Json(payload).into_response())
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
