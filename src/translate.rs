//! OpenAI ⇄ Anthropic 协议互转:文本 + 工具调用(function calling)+ 多模态图片。

use serde_json::{json, Value};

/// OpenAI chat/completions 请求 → Anthropic messages 请求。
pub fn openai_to_anthropic(body: &Value, upstream_model: &str, stream: bool) -> Value {
    let mut out = serde_json::Map::new();
    out.insert("model".into(), json!(upstream_model));
    let max_tokens = body.get("max_tokens").and_then(|v| v.as_u64()).unwrap_or(4096);
    out.insert("max_tokens".into(), json!(max_tokens));

    let (system, messages) = convert_messages(body.get("messages").and_then(|m| m.as_array()));
    if !system.is_empty() {
        out.insert("system".into(), json!(system));
    }
    out.insert("messages".into(), json!(messages));

    for (src, dst) in [("temperature", "temperature"), ("top_p", "top_p")] {
        if let Some(v) = body.get(src) {
            out.insert(dst.into(), v.clone());
        }
    }
    match body.get("stop") {
        Some(Value::String(s)) => { out.insert("stop_sequences".into(), json!([s])); }
        Some(Value::Array(a)) => { out.insert("stop_sequences".into(), json!(a)); }
        _ => {}
    }
    if let Some(tools) = body.get("tools").and_then(|t| t.as_array()) {
        out.insert("tools".into(), json!(convert_tools(tools)));
    }
    if let Some(tc) = body.get("tool_choice") {
        if let Some(v) = convert_tool_choice(tc) {
            out.insert("tool_choice".into(), v);
        }
    }
    if stream {
        out.insert("stream".into(), json!(true));
    }
    Value::Object(out)
}

/// OpenAI messages → (system, Anthropic messages)。处理工具调用/结果与多模态。
fn convert_messages(msgs: Option<&Vec<Value>>) -> (String, Vec<Value>) {
    let mut system = String::new();
    let mut out: Vec<Value> = Vec::new();
    let Some(msgs) = msgs else { return (system, out) };

    let mut i = 0;
    while i < msgs.len() {
        let m = &msgs[i];
        let role = m.get("role").and_then(|r| r.as_str()).unwrap_or("user");
        match role {
            "system" => {
                let t = content_to_text(m.get("content"));
                if !t.is_empty() {
                    if !system.is_empty() { system.push_str("\n\n"); }
                    system.push_str(&t);
                }
                i += 1;
            }
            // 连续的 tool 结果合并到一个 user 回合(Anthropic 要求 tool_result 在 user 消息里)。
            "tool" => {
                let mut blocks = Vec::new();
                while i < msgs.len() && msgs[i].get("role").and_then(|r| r.as_str()) == Some("tool") {
                    let tm = &msgs[i];
                    let id = tm.get("tool_call_id").and_then(|v| v.as_str()).unwrap_or("");
                    blocks.push(json!({
                        "type": "tool_result",
                        "tool_use_id": id,
                        "content": content_to_text(tm.get("content")),
                    }));
                    i += 1;
                }
                out.push(json!({ "role": "user", "content": blocks }));
            }
            "assistant" => {
                let mut blocks = Vec::new();
                let text = content_to_text(m.get("content"));
                if !text.is_empty() {
                    blocks.push(json!({ "type": "text", "text": text }));
                }
                if let Some(tcs) = m.get("tool_calls").and_then(|v| v.as_array()) {
                    for tc in tcs {
                        let id = tc.get("id").and_then(|v| v.as_str()).unwrap_or("");
                        let name = tc.pointer("/function/name").and_then(|v| v.as_str()).unwrap_or("");
                        let args = tc.pointer("/function/arguments").and_then(|v| v.as_str()).unwrap_or("{}");
                        let input: Value = serde_json::from_str(args).unwrap_or_else(|_| json!({}));
                        blocks.push(json!({ "type": "tool_use", "id": id, "name": name, "input": input }));
                    }
                }
                let content = if blocks.is_empty() { json!("") } else { json!(blocks) };
                out.push(json!({ "role": "assistant", "content": content }));
                i += 1;
            }
            _ => {
                out.push(json!({ "role": "user", "content": convert_user_content(m.get("content")) }));
                i += 1;
            }
        }
    }
    (system, out)
}

/// user content → Anthropic content(字符串或含图片的 block 数组)。
fn convert_user_content(content: Option<&Value>) -> Value {
    match content {
        Some(Value::String(s)) => json!(s),
        Some(Value::Array(parts)) => {
            let blocks: Vec<Value> = parts
                .iter()
                .filter_map(|p| match p.get("type").and_then(|t| t.as_str()) {
                    Some("image_url") => p.pointer("/image_url/url").and_then(|v| v.as_str()).map(image_block),
                    _ => p.get("text").and_then(|t| t.as_str()).map(|s| json!({ "type": "text", "text": s })),
                })
                .collect();
            if blocks.is_empty() { json!("") } else { json!(blocks) }
        }
        _ => json!(""),
    }
}

/// OpenAI image_url(data URI 或 http)→ Anthropic image block。
fn image_block(url: &str) -> Value {
    if let Some(rest) = url.strip_prefix("data:") {
        if let Some((meta, data)) = rest.split_once(',') {
            let media_type = meta.split(';').next().unwrap_or("image/png");
            return json!({ "type": "image", "source": { "type": "base64", "media_type": media_type, "data": data } });
        }
    }
    json!({ "type": "image", "source": { "type": "url", "url": url } })
}

fn convert_tools(tools: &[Value]) -> Vec<Value> {
    tools
        .iter()
        .filter_map(|t| {
            let f = t.get("function")?;
            Some(json!({
                "name": f.get("name").cloned().unwrap_or(json!("")),
                "description": f.get("description").cloned().unwrap_or(json!("")),
                "input_schema": f.get("parameters").cloned().unwrap_or(json!({ "type": "object" })),
            }))
        })
        .collect()
}

fn convert_tool_choice(tc: &Value) -> Option<Value> {
    match tc {
        Value::String(s) => match s.as_str() {
            "required" => Some(json!({ "type": "any" })),
            "none" => None, // Anthropic 无显式 none;省略即可
            _ => Some(json!({ "type": "auto" })),
        },
        Value::Object(_) => {
            let name = tc.pointer("/function/name").and_then(|v| v.as_str())?;
            Some(json!({ "type": "tool", "name": name }))
        }
        _ => Some(json!({ "type": "auto" })),
    }
}

/// content(字符串或多段)→ 纯文本(用于 system / tool_result)。
fn content_to_text(content: Option<&Value>) -> String {
    match content {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Array(parts)) => parts
            .iter()
            .filter_map(|p| p.get("text").and_then(|t| t.as_str()).map(|s| s.to_string()))
            .collect::<Vec<_>>()
            .join(""),
        _ => String::new(),
    }
}

/// Anthropic messages 响应(非流式)→ OpenAI chat.completion(含 tool_calls)。
pub fn anthropic_to_openai(aresp: &Value, public_model: &str) -> Value {
    let mut text = String::new();
    let mut tool_calls: Vec<Value> = Vec::new();
    if let Some(blocks) = aresp.get("content").and_then(|c| c.as_array()) {
        for b in blocks {
            match b.get("type").and_then(|t| t.as_str()) {
                Some("text") => {
                    if let Some(t) = b.get("text").and_then(|t| t.as_str()) {
                        text.push_str(t);
                    }
                }
                Some("tool_use") => {
                    let args = serde_json::to_string(b.get("input").unwrap_or(&json!({}))).unwrap_or_else(|_| "{}".into());
                    tool_calls.push(json!({
                        "id": b.get("id").cloned().unwrap_or(json!("")),
                        "type": "function",
                        "function": { "name": b.get("name").cloned().unwrap_or(json!("")), "arguments": args },
                    }));
                }
                _ => {}
            }
        }
    }

    let finish = map_stop_reason(aresp.get("stop_reason").and_then(|s| s.as_str()));
    let input = aresp.pointer("/usage/input_tokens").and_then(|v| v.as_u64()).unwrap_or(0);
    let output = aresp.pointer("/usage/output_tokens").and_then(|v| v.as_u64()).unwrap_or(0);
    let id = aresp.get("id").and_then(|v| v.as_str()).unwrap_or("chatcmpl-relay");

    let mut message = json!({ "role": "assistant" });
    message["content"] = if text.is_empty() && !tool_calls.is_empty() { Value::Null } else { json!(text) };
    if !tool_calls.is_empty() {
        message["tool_calls"] = json!(tool_calls);
    }

    json!({
        "id": id,
        "object": "chat.completion",
        "model": public_model,
        "choices": [{ "index": 0, "message": message, "finish_reason": finish }],
        "usage": { "prompt_tokens": input, "completion_tokens": output, "total_tokens": input + output }
    })
}

pub fn map_stop_reason(reason: Option<&str>) -> &'static str {
    match reason {
        Some("max_tokens") => "length",
        Some("tool_use") => "tool_calls",
        _ => "stop",
    }
}

// ================== 反向:Anthropic 入站 → OpenAI 上游 ==================

/// Anthropic Messages 请求 → OpenAI chat/completions 请求。
pub fn anthropic_to_openai_request(areq: &Value, upstream_model: &str, stream: bool) -> Value {
    let mut out = serde_json::Map::new();
    out.insert("model".into(), json!(upstream_model));
    if let Some(mt) = areq.get("max_tokens") {
        out.insert("max_tokens".into(), mt.clone());
    }

    let mut messages: Vec<Value> = Vec::new();
    if let Some(sys) = areq.get("system") {
        let t = anth_text(sys);
        if !t.is_empty() {
            messages.push(json!({ "role": "system", "content": t }));
        }
    }
    if let Some(arr) = areq.get("messages").and_then(|m| m.as_array()) {
        for m in arr {
            let role = m.get("role").and_then(|r| r.as_str()).unwrap_or("user");
            let content = m.get("content");
            if role == "assistant" {
                messages.push(anth_assistant_to_openai(content));
            } else {
                // user:可能含 text/image/tool_result
                anth_user_to_openai(content, &mut messages);
            }
        }
    }
    out.insert("messages".into(), json!(messages));

    for k in ["temperature", "top_p"] {
        if let Some(v) = areq.get(k) {
            out.insert(k.into(), v.clone());
        }
    }
    if let Some(s) = areq.get("stop_sequences") {
        out.insert("stop".into(), s.clone());
    }
    if let Some(tools) = areq.get("tools").and_then(|t| t.as_array()) {
        let mapped: Vec<Value> = tools
            .iter()
            .map(|t| json!({
                "type": "function",
                "function": {
                    "name": t.get("name").cloned().unwrap_or(json!("")),
                    "description": t.get("description").cloned().unwrap_or(json!("")),
                    "parameters": t.get("input_schema").cloned().unwrap_or(json!({"type":"object"})),
                }
            }))
            .collect();
        out.insert("tools".into(), json!(mapped));
    }
    if let Some(tc) = areq.get("tool_choice") {
        let mapped = match tc.get("type").and_then(|t| t.as_str()) {
            Some("any") => json!("required"),
            Some("tool") => json!({ "type": "function", "function": { "name": tc.get("name").cloned().unwrap_or(json!("")) } }),
            _ => json!("auto"),
        };
        out.insert("tool_choice".into(), mapped);
    }
    if stream {
        out.insert("stream".into(), json!(true));
    }
    Value::Object(out)
}

fn anth_assistant_to_openai(content: Option<&Value>) -> Value {
    match content {
        Some(Value::String(s)) => json!({ "role": "assistant", "content": s }),
        Some(Value::Array(blocks)) => {
            let mut text = String::new();
            let mut tool_calls = Vec::new();
            for b in blocks {
                match b.get("type").and_then(|t| t.as_str()) {
                    Some("text") => {
                        if let Some(t) = b.get("text").and_then(|t| t.as_str()) { text.push_str(t); }
                    }
                    Some("tool_use") => {
                        let args = serde_json::to_string(b.get("input").unwrap_or(&json!({}))).unwrap_or_else(|_| "{}".into());
                        tool_calls.push(json!({
                            "id": b.get("id").cloned().unwrap_or(json!("")),
                            "type": "function",
                            "function": { "name": b.get("name").cloned().unwrap_or(json!("")), "arguments": args },
                        }));
                    }
                    _ => {}
                }
            }
            let mut msg = json!({ "role": "assistant" });
            msg["content"] = if text.is_empty() && !tool_calls.is_empty() { Value::Null } else { json!(text) };
            if !tool_calls.is_empty() { msg["tool_calls"] = json!(tool_calls); }
            msg
        }
        _ => json!({ "role": "assistant", "content": "" }),
    }
}

/// Anthropic user 消息 → OpenAI 消息(tool_result 拆成 role:tool;text/image 合成 user)。
fn anth_user_to_openai(content: Option<&Value>, out: &mut Vec<Value>) {
    match content {
        Some(Value::String(s)) => out.push(json!({ "role": "user", "content": s })),
        Some(Value::Array(blocks)) => {
            let mut parts: Vec<Value> = Vec::new();
            for b in blocks {
                match b.get("type").and_then(|t| t.as_str()) {
                    Some("tool_result") => {
                        out.push(json!({
                            "role": "tool",
                            "tool_call_id": b.get("tool_use_id").cloned().unwrap_or(json!("")),
                            "content": anth_text(b.get("content").unwrap_or(&json!(""))),
                        }));
                    }
                    Some("image") => {
                        if let Some(p) = anth_image_to_openai(b) { parts.push(p); }
                    }
                    _ => {
                        if let Some(t) = b.get("text").and_then(|t| t.as_str()) {
                            parts.push(json!({ "type": "text", "text": t }));
                        }
                    }
                }
            }
            if !parts.is_empty() {
                out.push(json!({ "role": "user", "content": parts }));
            }
        }
        _ => {}
    }
}

/// Anthropic image block → OpenAI image_url part。
fn anth_image_to_openai(b: &Value) -> Option<Value> {
    let src = b.get("source")?;
    let url = match src.get("type").and_then(|t| t.as_str()) {
        Some("base64") => {
            let mt = src.get("media_type").and_then(|v| v.as_str()).unwrap_or("image/png");
            let data = src.get("data").and_then(|v| v.as_str()).unwrap_or("");
            format!("data:{mt};base64,{data}")
        }
        _ => src.get("url").and_then(|v| v.as_str()).unwrap_or("").to_string(),
    };
    Some(json!({ "type": "image_url", "image_url": { "url": url } }))
}

/// Anthropic 文本(string 或 text-block 数组)→ 纯文本。
fn anth_text(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Array(arr) => arr
            .iter()
            .filter_map(|b| b.get("text").and_then(|t| t.as_str()).map(|s| s.to_string()))
            .collect::<Vec<_>>()
            .join(""),
        _ => String::new(),
    }
}

/// OpenAI chat.completion(非流式)→ Anthropic Messages 响应。
pub fn openai_to_anthropic_response(oai: &Value, public_model: &str) -> Value {
    let msg = oai.pointer("/choices/0/message").cloned().unwrap_or(json!({}));
    let mut content: Vec<Value> = Vec::new();
    if let Some(text) = msg.get("content").and_then(|c| c.as_str()) {
        if !text.is_empty() {
            content.push(json!({ "type": "text", "text": text }));
        }
    }
    if let Some(tcs) = msg.get("tool_calls").and_then(|v| v.as_array()) {
        for tc in tcs {
            let args = tc.pointer("/function/arguments").and_then(|v| v.as_str()).unwrap_or("{}");
            let input: Value = serde_json::from_str(args).unwrap_or_else(|_| json!({}));
            content.push(json!({
                "type": "tool_use",
                "id": tc.get("id").cloned().unwrap_or(json!("")),
                "name": tc.pointer("/function/name").cloned().unwrap_or(json!("")),
                "input": input,
            }));
        }
    }
    let finish = oai.pointer("/choices/0/finish_reason").and_then(|v| v.as_str());
    let input = oai.pointer("/usage/prompt_tokens").and_then(|v| v.as_u64()).unwrap_or(0);
    let output = oai.pointer("/usage/completion_tokens").and_then(|v| v.as_u64()).unwrap_or(0);

    json!({
        "id": oai.get("id").cloned().unwrap_or(json!("msg_relay")),
        "type": "message",
        "role": "assistant",
        "model": public_model,
        "content": content,
        "stop_reason": map_finish_to_stop(finish),
        "stop_sequence": Value::Null,
        "usage": { "input_tokens": input, "output_tokens": output }
    })
}

pub fn map_finish_to_stop(finish: Option<&str>) -> &'static str {
    match finish {
        Some("length") => "max_tokens",
        Some("tool_calls") => "tool_use",
        _ => "end_turn",
    }
}
