//! OpenAI Responses API(`/v1/responses`) ⇄ Chat Completions 协议适配层。
//!
//! 设计:入站 Responses 请求转换为等价 Chat Completions 请求后复用 `run_chat`
//! 主管道(路由/熔断/计费/链路/缓存全复用),出站再将 Chat 响应(或 SSE 字节流)
//! 回译为 Responses 格式。上游永远说 chat 方言,所有供应商通吃。
//!
//! 第一期边界(明确不支持并返回 400):`previous_response_id` 状态化会话、
//! 内置工具(web_search / file_search / computer_use)、本地 shell 等非 function 工具。
//! `store` 字段接受但忽略(网关无状态,不落会话存储)。

use axum::body::Body;
use bytes::Bytes;
use futures::StreamExt;
use serde_json::{json, Value};

use crate::error::ApiError;

/// 入站校验:必填项 / 显式拒绝不支持的能力(带清晰错误信息)。
pub fn validate_responses(req: &Value) -> Result<(), ApiError> {
    if req.get("model").and_then(|m| m.as_str()).map(|s| s.trim().is_empty()).unwrap_or(true) {
        return Err(ApiError::BadRequest("missing `model`".into()));
    }
    match req.get("input") {
        None | Some(Value::Null) => return Err(ApiError::BadRequest("missing `input`".into())),
        Some(Value::String(s)) if s.is_empty() => {
            return Err(ApiError::BadRequest("`input` 不能为空字符串".into()));
        }
        Some(Value::Array(a)) if a.is_empty() => {
            return Err(ApiError::BadRequest("`input` 不能为空数组".into()));
        }
        Some(Value::String(_)) | Some(Value::Array(_)) => {}
        Some(_) => return Err(ApiError::BadRequest("`input` 必须为字符串或数组".into())),
    }
    if let Some(s) = req.get("stream") {
        if !s.is_boolean() {
            return Err(ApiError::BadRequest("`stream` 必须为布尔值".into()));
        }
    }
    if req.get("previous_response_id").and_then(|v| v.as_str()).map(|s| !s.is_empty()).unwrap_or(false) {
        return Err(ApiError::BadRequest(
            "previous_response_id(状态化会话)暂不支持;请将历史 output 作为 input 数组回传".into(),
        ));
    }
    if let Some(tools) = req.get("tools").and_then(|t| t.as_array()) {
        for t in tools {
            let ty = t.get("type").and_then(|v| v.as_str()).unwrap_or("function");
            if ty != "function" {
                return Err(ApiError::BadRequest(format!(
                    "内置/非 function 工具(type={ty})暂不支持,仅支持 function 工具"
                )));
            }
        }
    }
    Ok(())
}

/// Responses 请求 → Chat Completions 请求。
pub fn responses_to_chat(req: &Value) -> Result<Value, ApiError> {
    let mut messages: Vec<Value> = Vec::new();

    // instructions → 前置 system 消息。
    if let Some(inst) = req.get("instructions").and_then(|v| v.as_str()) {
        if !inst.is_empty() {
            messages.push(json!({"role": "system", "content": inst}));
        }
    }

    // input → messages。
    match req.get("input") {
        Some(Value::String(s)) => messages.push(json!({"role": "user", "content": s})),
        Some(Value::Array(items)) => {
            for item in items {
                let ty = item.get("type").and_then(|v| v.as_str());
                match ty {
                    // 无 type 的 {role, content} 简写也接受。
                    Some("message") | None => {
                        let role = item.get("role").and_then(|v| v.as_str()).unwrap_or("user");
                        let content = item.get("content");
                        messages.push(json!({"role": role, "content": convert_content(role, content)}));
                    }
                    Some("function_call") => {
                        let call_id = item.get("call_id").and_then(|v| v.as_str()).unwrap_or("");
                        let name = item.get("name").and_then(|v| v.as_str()).unwrap_or("");
                        let args = item.get("arguments").and_then(|v| v.as_str()).unwrap_or("{}");
                        messages.push(json!({
                            "role": "assistant",
                            "content": null,
                            "tool_calls": [{"id": call_id, "type": "function",
                                            "function": {"name": name, "arguments": args}}]
                        }));
                    }
                    Some("function_call_output") => {
                        let call_id = item.get("call_id").and_then(|v| v.as_str()).unwrap_or("");
                        let out = item.get("output").cloned().unwrap_or(Value::String(String::new()));
                        let content = match out {
                            Value::String(s) => Value::String(s),
                            other => Value::String(other.to_string()),
                        };
                        messages.push(json!({"role": "tool", "tool_call_id": call_id, "content": content}));
                    }
                    // reasoning 历史条目:内部推理不入 chat 消息,跳过。
                    Some("reasoning") => {}
                    Some(other) => {
                        return Err(ApiError::BadRequest(format!("不支持的 input item 类型: {other}")));
                    }
                }
            }
        }
        _ => return Err(ApiError::BadRequest("missing `input`".into())),
    }
    if messages.is_empty() {
        return Err(ApiError::BadRequest("`input` 不能为空".into()));
    }

    let mut chat = json!({
        "model": req.get("model").and_then(|m| m.as_str()).unwrap_or_default(),
        "messages": messages,
    });

    // 直传参数(字段名一致)。
    for key in ["stream", "temperature", "top_p", "parallel_tool_calls", "user", "metadata"] {
        if let Some(v) = req.get(key) {
            if !v.is_null() {
                chat[key] = v.clone();
            }
        }
    }
    // max_output_tokens → max_tokens。
    if let Some(m) = req.get("max_output_tokens").and_then(|v| v.as_u64()) {
        chat["max_tokens"] = json!(m);
    }
    // reasoning.effort → reasoning_effort(OpenAI chat 同名参数)。
    if let Some(effort) = req.pointer("/reasoning/effort").and_then(|v| v.as_str()) {
        chat["reasoning_effort"] = json!(effort);
    }
    // tools:Responses 扁平 function → chat 嵌套 function。
    if let Some(tools) = req.get("tools").and_then(|t| t.as_array()) {
        let converted: Vec<Value> = tools
            .iter()
            .filter_map(|t| {
                if t.get("type").and_then(|v| v.as_str()) != Some("function") {
                    return None;
                }
                let mut f = json!({"type": "function"});
                let mut inner = json!({});
                for k in ["name", "description", "parameters", "strict"] {
                    if let Some(v) = t.get(k) {
                        inner[k] = v.clone();
                    }
                }
                f["function"] = inner;
                Some(f)
            })
            .collect();
        if !converted.is_empty() {
            chat["tools"] = Value::Array(converted);
        }
    }
    // tool_choice:{type:"function",name} → {type:"function",function:{name}}。
    if let Some(tc) = req.get("tool_choice") {
        if tc.is_string() {
            chat["tool_choice"] = tc.clone();
        } else if tc.get("type").and_then(|v| v.as_str()) == Some("function") {
            if let Some(name) = tc.get("name").and_then(|v| v.as_str()) {
                chat["tool_choice"] = json!({"type": "function", "function": {"name": name}});
            }
        }
    }
    // text.format → response_format。
    if let Some(fmt) = req.pointer("/text/format") {
        match fmt.get("type").and_then(|v| v.as_str()) {
            Some("json_object") => chat["response_format"] = json!({"type": "json_object"}),
            Some("json_schema") => {
                let mut inner = json!({});
                for k in ["name", "schema", "strict"] {
                    if let Some(v) = fmt.get(k) {
                        inner[k] = v.clone();
                    }
                }
                chat["response_format"] = json!({"type": "json_schema", "json_schema": inner});
            }
            _ => {}
        }
    }
    Ok(chat)
}

/// Responses content(text / parts)→ chat content(string 或 parts 数组)。
fn convert_content(role: &str, content: Option<&Value>) -> Value {
    match content {
        None | Some(Value::Null) => Value::Null,
        Some(Value::String(s)) => Value::String(s.clone()),
        Some(Value::Array(parts)) => {
            let converted: Vec<Value> = parts
                .iter()
                .filter_map(|p| {
                    let ty = p.get("type").and_then(|v| v.as_str()).unwrap_or("input_text");
                    match ty {
                        "input_text" | "output_text" => {
                            p.get("text").map(|t| json!({"type": "text", "text": t}))
                        }
                        "refusal" => p.get("refusal").map(|t| json!({"type": "text", "text": t})),
                        "input_image" => p.get("image_url").map(|u| {
                            json!({"type": "image_url", "image_url": {"url": u}})
                        }),
                        _ => None,
                    }
                })
                .collect();
            if converted.is_empty() {
                // 空数组降级:assistant 空 content 合法,其余给空串防上游报错。
                if role == "assistant" { Value::Null } else { Value::String(String::new()) }
            } else if converted.len() == 1 && converted[0].get("type").and_then(|v| v.as_str()) == Some("text") {
                // 纯文本单段:直接给字符串(chat 的 assistant content 不支持 parts 数组,统一降级)。
                converted[0]["text"].clone()
            } else {
                Value::Array(converted)
            }
        }
        Some(other) => other.clone(),
    }
}

/// Chat Completions 非流式响应 → Responses 响应。
pub fn chat_to_responses(chat: &Value) -> Value {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let model = chat.get("model").and_then(|m| m.as_str()).unwrap_or_default().to_string();

    let mut output: Vec<Value> = Vec::new();
    let msg = chat.pointer("/choices/0/message");
    // reasoning_content(上游扩展)→ reasoning item 摘要占位。
    if let Some(rc) = msg.and_then(|m| m.get("reasoning_content")).and_then(|v| v.as_str()) {
        if !rc.is_empty() {
            output.push(json!({
                "type": "reasoning", "id": format!("rs_{}", short_id()),
                "summary": [{"type": "summary_text", "text": rc}]
            }));
        }
    }
    if let Some(text) = msg.and_then(|m| m.get("content")).and_then(|v| v.as_str()) {
        if !text.is_empty() {
            output.push(json!({
                "type": "message", "id": format!("msg_{}", short_id()), "status": "completed",
                "role": "assistant",
                "content": [{"type": "output_text", "text": text, "annotations": []}]
            }));
        }
    }
    if let Some(calls) = msg.and_then(|m| m.get("tool_calls")).and_then(|v| v.as_array()) {
        for c in calls {
            let f = c.pointer("/function");
            output.push(json!({
                "type": "function_call", "id": format!("fc_{}", short_id()),
                "call_id": c.get("id").and_then(|v| v.as_str()).unwrap_or(""),
                "name": f.and_then(|f| f.get("name")).and_then(|v| v.as_str()).unwrap_or(""),
                "arguments": f.and_then(|f| f.get("arguments")).and_then(|v| v.as_str()).unwrap_or("{}"),
                "status": "completed"
            }));
        }
    }

    let usage_in = chat.pointer("/usage/prompt_tokens").and_then(|v| v.as_u64()).unwrap_or(0);
    let usage_out = chat.pointer("/usage/completion_tokens").and_then(|v| v.as_u64()).unwrap_or(0);
    let usage_total = chat.pointer("/usage/total_tokens").and_then(|v| v.as_u64()).unwrap_or(usage_in + usage_out);

    let finish = chat.pointer("/choices/0/finish_reason").and_then(|v| v.as_str());
    let status = if finish == Some("length") { "incomplete" } else { "completed" };

    json!({
        "id": format!("resp_{}", short_id()),
        "object": "response",
        "created_at": now,
        "status": status,
        "model": model,
        "output": output,
        "usage": {
            "input_tokens": usage_in,
            "input_tokens_details": {"cached_tokens": 0},
            "output_tokens": usage_out,
            "output_tokens_details": {"reasoning_tokens": 0},
            "total_tokens": usage_total
        },
        "error": Value::Null,
        "incomplete_details": if status == "incomplete" { json!({"reason": "max_output_tokens"}) } else { Value::Null },
        "instructions": Value::Null,
        "metadata": {},
        "parallel_tool_calls": true,
        "previous_response_id": Value::Null,
        "store": false,
        "temperature": Value::Null,
        "top_p": Value::Null,
        "tool_choice": "auto",
        "tools": []
    })
}

fn short_id() -> String {
    uuid::Uuid::new_v4().to_string()[..20].replace('-', "")
}

// ---------------------------------------------------------------------------
// 流式:chat SSE 字节流 → Responses 事件流(response.created / output_text.delta / ...)
// ---------------------------------------------------------------------------

/// 将 `run_chat` 返回的 chat 格式 SSE 流包装为 Responses 事件流。
/// 事件均带递增 sequence_number;usage 在 response.completed 合入(上游尾片缺省时为 0)。
pub fn chat_sse_to_responses_body(body: Body, model: String) -> Body {
    let s = async_stream::stream! {
        let resp_id = format!("resp_{}", short_id());
        let msg_id = format!("msg_{}", short_id());
        let created = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let mut seq: u64 = 0;
        let mut buf: Vec<u8> = Vec::new();
        let mut input: u64 = 0;
        let mut output: u64 = 0;
        let mut full_text = String::new();
        let mut started = false;   // 是否已发 output_item.added / content_part.added
        // function_call 聚合:index → (call_id, name, args)
        let mut calls: std::collections::HashMap<u64, (String, String, String)> = std::collections::HashMap::new();
        let mut call_order: Vec<u64> = Vec::new();
        let mut status = "in_progress";

        let skeleton = |rid: &str, mdl: &str, st: &str, out: &Vec<Value>, input: u64, output: u64| json!({
            "id": rid, "object": "response", "created_at": created, "status": st,
            "model": mdl, "output": out,
            "error": Value::Null, "incomplete_details": Value::Null,
            "instructions": Value::Null, "metadata": {},
            "parallel_tool_calls": true, "previous_response_id": Value::Null,
            "store": false, "temperature": Value::Null, "top_p": Value::Null,
            "tool_choice": "auto", "tools": [],
            "usage": {"input_tokens": input, "input_tokens_details": {"cached_tokens": 0},
                       "output_tokens": output, "output_tokens_details": {"reasoning_tokens": 0},
                       "total_tokens": input + output}
        });

        // 起始事件:response.created + response.in_progress。
        let empty_out: Vec<Value> = Vec::new();
        seq += 1;
        yield Ok::<Bytes, std::io::Error>(sse_event("response.created", &json!({"sequence_number": seq, "response": skeleton(&resp_id, &model, "in_progress", &empty_out, input, output)})));
        seq += 1;
        yield Ok(sse_event("response.in_progress", &json!({"sequence_number": seq, "response": skeleton(&resp_id, &model, "in_progress", &empty_out, input, output)})));

        let mut upstream = body.into_data_stream();
        while let Some(item) = upstream.next().await {
            let bytes = match item {
                Ok(b) => b,
                Err(_) => break,
            };
            buf.extend_from_slice(&bytes);
            while let Some(pos) = buf.iter().position(|&b| b == b'\n') {
                let line: Vec<u8> = buf.drain(..=pos).collect();
                let line = String::from_utf8_lossy(&line);
                let line = line.trim().to_string();
                let Some(payload) = line.strip_prefix("data:") else { continue };
                let payload = payload.trim().to_string();
                if payload.is_empty() { continue; }
                if payload == "[DONE]" { continue; }
                let Ok(ev) = serde_json::from_str::<Value>(&payload) else { continue };

                if let Some(u) = ev.get("usage").filter(|u| !u.is_null()) {
                    input = u.get("prompt_tokens").and_then(|v| v.as_u64()).unwrap_or(input);
                    output = u.get("completion_tokens").and_then(|v| v.as_u64()).unwrap_or(output);
                }
                let Some(delta) = ev.pointer("/choices/0/delta") else { continue };

                // ---- 文本增量 ----
                if let Some(text) = delta.get("content").and_then(|v| v.as_str()) {
                    if !text.is_empty() {
                        if !started {
                            started = true;
                            seq += 1;
                            yield Ok(sse_event("response.output_item.added", &json!({
                                "sequence_number": seq, "output_index": 0,
                                "item": {"type": "message", "id": msg_id, "status": "in_progress", "role": "assistant", "content": []}
                            })));
                            seq += 1;
                            yield Ok(sse_event("response.content_part.added", &json!({
                                "sequence_number": seq, "item_id": msg_id, "output_index": 0, "content_index": 0,
                                "part": {"type": "output_text", "text": "", "annotations": []}
                            })));
                        }
                        full_text.push_str(text);
                        seq += 1;
                        yield Ok(sse_event("response.output_text.delta", &json!({
                            "sequence_number": seq, "item_id": msg_id, "output_index": 0, "content_index": 0, "delta": text
                        })));
                    }
                }
                // ---- 推理增量:透传为 reasoning 摘要 delta(非标准字段,尽力兼容) ----
                if let Some(rt) = delta.get("reasoning_content").and_then(|v| v.as_str()) {
                    if !rt.is_empty() {
                        seq += 1;
                        yield Ok(sse_event("response.reasoning_summary_text.delta", &json!({
                            "sequence_number": seq, "item_id": msg_id, "output_index": 0, "summary_index": 0, "delta": rt
                        })));
                    }
                }
                // ---- function_call 增量 ----
                if let Some(tcs) = delta.get("tool_calls").and_then(|v| v.as_array()) {
                    for tc in tcs {
                        let idx = tc.get("index").and_then(|v| v.as_u64()).unwrap_or(0);
                        let is_new = !calls.contains_key(&idx);
                        let entry = calls.entry(idx).or_insert_with(|| {
                            call_order.push(idx);
                            let cid = tc.get("id").and_then(|v| v.as_str()).unwrap_or("").to_string();
                            let nm = tc.pointer("/function/name").and_then(|v| v.as_str()).unwrap_or("").to_string();
                            (cid, nm, String::new())
                        });
                        if is_new {
                            // 新 function_call:发 output_item.added。
                            seq += 1;
                            let pos = call_order.iter().position(|&i| i == idx).unwrap_or(0);
                            yield Ok(sse_event("response.output_item.added", &json!({
                                "sequence_number": seq, "output_index": pos,
                                "item": {"type": "function_call", "id": format!("fc_{}", short_id()),
                                          "call_id": entry.0, "name": entry.1, "arguments": "", "status": "in_progress"}
                            })));
                        }
                        if let Some(pj) = tc.pointer("/function/arguments").and_then(|v| v.as_str()) {
                            if !pj.is_empty() {
                                entry.2.push_str(pj);
                                seq += 1;
                                let pos = call_order.iter().position(|&i| i == idx).unwrap_or(0);
                                yield Ok(sse_event("response.function_call_arguments.delta", &json!({
                                    "sequence_number": seq, "item_id": format!("fc_{}", short_id()),
                                    "output_index": pos, "delta": pj
                                })));
                            }
                        }
                    }
                }
                // ---- finish_reason:文本收尾 ----
                if let Some(finish) = ev.pointer("/choices/0/finish_reason").and_then(|v| v.as_str()) {
                    if finish == "length" { status = "incomplete"; }
                }
            }
        }

        // ---- 收尾:done 系列 + response.completed ----
        let mut final_output: Vec<Value> = Vec::new();
        if started {
            seq += 1;
            yield Ok(sse_event("response.output_text.done", &json!({
                "sequence_number": seq, "item_id": msg_id, "output_index": 0, "content_index": 0, "text": full_text
            })));
            seq += 1;
            yield Ok(sse_event("response.content_part.done", &json!({
                "sequence_number": seq, "item_id": msg_id, "output_index": 0, "content_index": 0,
                "part": {"type": "output_text", "text": full_text, "annotations": []}
            })));
            seq += 1;
            yield Ok(sse_event("response.output_item.done", &json!({
                "sequence_number": seq, "output_index": 0,
                "item": {"type": "message", "id": msg_id, "status": "completed", "role": "assistant",
                          "content": [{"type": "output_text", "text": full_text, "annotations": []}]}
            })));
            final_output.push(json!({"type": "message", "id": msg_id, "status": "completed", "role": "assistant",
                                      "content": [{"type": "output_text", "text": full_text, "annotations": []}]}));
        }
        for (pos, idx) in call_order.iter().enumerate() {
            if let Some((cid, name, args)) = calls.get(idx) {
                let fid = format!("fc_{}", short_id());
                seq += 1;
                yield Ok(sse_event("response.function_call_arguments.done", &json!({
                    "sequence_number": seq, "item_id": fid, "output_index": pos + 1, "arguments": args
                })));
                seq += 1;
                yield Ok(sse_event("response.output_item.done", &json!({
                    "sequence_number": seq, "output_index": pos + 1,
                    "item": {"type": "function_call", "id": fid, "call_id": cid, "name": name,
                              "arguments": args, "status": "completed"}
                })));
                final_output.push(json!({"type": "function_call", "id": fid, "call_id": cid, "name": name,
                                          "arguments": args, "status": "completed"}));
            }
        }
        status = if status == "incomplete" { "incomplete" } else { "completed" };
        seq += 1;
        yield Ok(sse_event("response.completed", &json!({
            "sequence_number": seq, "response": skeleton(&resp_id, &model, status, &final_output, input, output)
        })));
    };
    Body::from_stream(s)
}

fn sse_event(event: &str, data: &Value) -> Bytes {
    Bytes::from(format!("event: {event}\ndata: {data}\n\n"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn string_input_becomes_user_message() {
        let req = json!({"model": "m", "input": "你好"});
        let chat = responses_to_chat(&req).unwrap();
        assert_eq!(chat["model"], "m");
        assert_eq!(chat["messages"][0]["role"], "user");
        assert_eq!(chat["messages"][0]["content"], "你好");
        assert!(chat.get("max_tokens").is_none());
    }

    #[test]
    fn instructions_and_items_are_converted() {
        let req = json!({
            "model": "m",
            "instructions": "你是助手",
            "max_output_tokens": 128,
            "reasoning": {"effort": "high"},
            "input": [
                {"type": "message", "role": "user", "content": "北京天气?"},
                {"type": "message", "role": "assistant", "content": [{"type": "output_text", "text": "晴"}]},
                {"type": "function_call", "call_id": "call_1", "name": "get_weather", "arguments": "{\"city\":\"北京\"}"},
                {"type": "function_call_output", "call_id": "call_1", "output": "25 度"},
                {"type": "message", "role": "user", "content": [{"type": "input_text", "text": "总结"}]}
            ]
        });
        let chat = responses_to_chat(&req).unwrap();
        let msgs = chat["messages"].as_array().unwrap();
        assert_eq!(msgs[0]["role"], "system");
        assert_eq!(msgs[0]["content"], "你是助手");
        assert_eq!(msgs[1]["content"], "北京天气?");
        assert_eq!(msgs[2]["content"], "晴");
        assert_eq!(msgs[3]["tool_calls"][0]["id"], "call_1");
        assert_eq!(msgs[4]["role"], "tool");
        assert_eq!(msgs[4]["tool_call_id"], "call_1");
        assert_eq!(msgs[5]["content"], "总结");
        assert_eq!(chat["max_tokens"], 128);
        assert_eq!(chat["reasoning_effort"], "high");
    }

    #[test]
    fn tools_flat_to_nested_and_choice_mapping() {
        let req = json!({
            "model": "m", "input": "hi",
            "tools": [{"type": "function", "name": "f1", "description": "d", "parameters": {"type": "object"}}],
            "tool_choice": {"type": "function", "name": "f1"}
        });
        let chat = responses_to_chat(&req).unwrap();
        assert_eq!(chat["tools"][0]["function"]["name"], "f1");
        assert_eq!(chat["tool_choice"]["function"]["name"], "f1");
    }

    #[test]
    fn text_format_maps_to_response_format() {
        let req = json!({"model": "m", "input": "hi",
                          "text": {"format": {"type": "json_schema", "name": "o", "schema": {}}}});
        let chat = responses_to_chat(&req).unwrap();
        assert_eq!(chat["response_format"]["type"], "json_schema");
        assert!(chat["response_format"]["json_schema"]["name"].is_string());
    }

    #[test]
    fn rejects_previous_response_id_and_builtin_tools() {
        let r1 = json!({"model": "m", "input": "hi", "previous_response_id": "resp_x"});
        // 拒绝职责在 validate(handler 先校验再转换)。
        assert!(validate_responses(&r1).is_err());
        let r2 = json!({"model": "m", "input": "hi", "tools": [{"type": "web_search"}]});
        assert!(validate_responses(&r2).is_err());
    }

    #[test]
    fn rejects_missing_or_empty_input() {
        assert!(validate_responses(&json!({"model": "m"})).is_err());
        assert!(validate_responses(&json!({"model": "m", "input": ""})).is_err());
        assert!(validate_responses(&json!({"model": "m", "input": []})).is_err());
        assert!(validate_responses(&json!({"input": "hi"})).is_err());
    }

    #[test]
    fn chat_response_maps_to_responses_shape() {
        let chat = json!({
            "id": "chatcmpl-abc", "model": "m",
            "choices": [{"finish_reason": "stop", "message": {"role": "assistant", "content": "你好!"}}],
            "usage": {"prompt_tokens": 10, "completion_tokens": 5, "total_tokens": 15}
        });
        let r = chat_to_responses(&chat);
        assert!(r["id"].as_str().unwrap().starts_with("resp_"));
        assert_eq!(r["status"], "completed");
        let out = r["output"].as_array().unwrap();
        assert_eq!(out[0]["type"], "message");
        assert_eq!(out[0]["content"][0]["type"], "output_text");
        assert_eq!(out[0]["content"][0]["text"], "你好!");
        assert_eq!(r["usage"]["input_tokens"], 10);
        assert_eq!(r["usage"]["output_tokens"], 5);
        assert_eq!(r["usage"]["total_tokens"], 15);
    }

    #[test]
    fn tool_calls_map_to_function_call_items() {
        let chat = json!({
            "id": "x", "model": "m",
            "choices": [{"finish_reason": "tool_calls", "message": {
                "role": "assistant", "content": null,
                "tool_calls": [{"id": "call_9", "type": "function",
                                 "function": {"name": "f", "arguments": "{\"a\":1}"}}]}}],
            "usage": {"prompt_tokens": 1, "completion_tokens": 1, "total_tokens": 2}
        });
        let r = chat_to_responses(&chat);
        let out = r["output"].as_array().unwrap();
        assert_eq!(out[0]["type"], "function_call");
        assert_eq!(out[0]["call_id"], "call_9");
        assert_eq!(out[0]["name"], "f");
    }

    #[test]
    fn finish_length_marks_incomplete() {
        let chat = json!({
            "id": "x", "model": "m",
            "choices": [{"finish_reason": "length", "message": {"role": "assistant", "content": "半"}}],
            "usage": {"prompt_tokens": 1, "completion_tokens": 1, "total_tokens": 2}
        });
        let r = chat_to_responses(&chat);
        assert_eq!(r["status"], "incomplete");
        assert_eq!(r["incomplete_details"]["reason"], "max_output_tokens");
    }
}
