//! 短信验证码发送:通用 HTTP 网关(与 email 模块同模式)。
//!
//! settings 键(管理端「设置」页可配):
//! - `sms.gateway_url`    网关地址;**空 = 开发模式**(验证码随响应回显并打日志,便于本地联调;
//!   生产必须配置,否则手机号注册只能用开发模式返回的码)。
//! - `sms.gateway_method` GET | POST(默认 POST)
//! - `sms.headers_enc`    请求头 JSON(如 {"Authorization":"APPCODE xxx"}),AES-256-GCM 加密存储
//! - `sms.gateway_body`   请求体/查询模板,支持 `{phone}` `{code}` 占位符
//!   (POST 为请求体,GET 追加为 query);示例:
//!   `{"phone":"{phone}","code":"{code}"}` 或 `phone={phone}&code={code}`
//!
//! 不绑定任何特定短信服务商:任何能用简单 HTTP 调用下发短信的网关(云厂商函数、
//! 自建转发服务、聚合网关)均可对接;成功判定 = HTTP 2xx。

use serde_json::{json, Value};

use crate::storage::{self, Db};

pub struct SmsGateway {
    pub url: String,
    pub method: String,
    /// 解密后的请求头 JSON 字符串(原样透传);空 = 无自定义头。
    pub headers_json: String,
    pub body_template: String,
}

pub async fn load_gateway(pool: &Db) -> SmsGateway {
    let kv = storage::load_settings(pool, crate::settings::SMS_PREFIX)
        .await
        .unwrap_or_default();
    let headers_enc = kv.get(crate::settings::K_SMS_HEADERS_ENC).cloned().unwrap_or_default();
    let mut headers_json = if headers_enc.is_empty() {
        String::new()
    } else {
        // 解密失败按无头处理(记 warn 不阻断;与 email.password 同策略)。
        crate::settings::decrypt(&storage::field_secret(), &headers_enc).unwrap_or_else(|e| {
            tracing::warn!("解密 sms.gateway_headers 失败,按未配置处理: {e}");
            String::new()
        })
    };
    // "{}"(空对象)= 用户显式清除自定义头。
    if headers_json.trim() == "{}" {
        headers_json.clear();
    }
    SmsGateway {
        url: kv.get(crate::settings::K_SMS_GATEWAY_URL).cloned().unwrap_or_default(),
        method: kv.get(crate::settings::K_SMS_GATEWAY_METHOD).cloned().unwrap_or_else(|| "POST".into()),
        headers_json,
        body_template: kv.get(crate::settings::K_SMS_BODY_TEMPLATE).cloned().unwrap_or_default(),
    }
}

impl SmsGateway {
    /// 未配置网关 = 开发模式。
    pub fn is_dev(&self) -> bool {
        self.url.trim().is_empty()
    }
}

/// 经网关下发验证码。仅 HTTP 2xx 视为成功;非 2xx 返回错误摘要(截断)。
pub async fn send(http: &reqwest::Client, g: &SmsGateway, phone: &str, code: &str) -> Result<(), String> {
    if g.is_dev() {
        // 开发模式不该走到这里(portal 层短路回显),兜底打日志。
        tracing::warn!("[DEV] 短信验证码 {phone}: {code}(未配置 sms.gateway_url)");
        return Ok(());
    }
    let method = g.method.trim().to_uppercase();
    let subst = |s: &str| s.replace("{phone}", phone).replace("{code}", code);
    let url = subst(g.url.trim());
    let mut req = match method.as_str() {
        "GET" => {
            // GET:模板作为 query 追加(已含占位符替换;若模板本身是 k=v&k=v 形式)。
            let sep = if url.contains('?') { "&" } else { "?" };
            let tmpl = subst(g.body_template.trim());
            let full = if tmpl.is_empty() { url.clone() } else { format!("{url}{sep}{tmpl}") };
            http.get(&full)
        }
        _ => {
            http.post(&url)
                .header("Content-Type", "application/x-www-form-urlencoded; charset=utf-8")
        }
        // body 在下方按 JSON 与表单两种形态分别挂
    };
    // 自定义请求头(JSON 对象)。
    if !g.headers_json.trim().is_empty() {
        match serde_json::from_str::<Value>(g.headers_json.trim()) {
            Ok(Value::Object(map)) => {
                for (k, v) in map {
                    if let Some(vs) = v.as_str() {
                        req = req.header(k, vs);
                    }
                }
            }
            _ => return Err("短信网关请求头配置不是合法 JSON 对象".into()),
        }
    }
    // 请求体:POST 时,若模板是合法 JSON 对象则按 JSON 发(并修正 Content-Type),否则按表单发。
    if method != "GET" {
        let body = subst(g.body_template.trim());
        match serde_json::from_str::<Value>(&body) {
            Ok(v @ Value::Object(_)) => {
                req = req.header("Content-Type", "application/json; charset=utf-8").json(&v);
            }
            _ => {
                req = req.body(body);
            }
        }
    }
    let resp = req.send().await.map_err(|e| format!("短信网关请求失败: {e}"))?;
    let status = resp.status();
    if !status.is_success() {
        let t = resp.text().await.unwrap_or_default();
        let summary: String = t.chars().take(200).collect();
        return Err(format!("短信网关返回 {status}: {summary}"));
    }
    Ok(())
}

/// 短信网关连通性测试(管理端「发送测试」按钮):直接发一条真实验证码短信。
pub async fn test_gateway(http: &reqwest::Client, g: &SmsGateway, phone: &str) -> Result<Value, String> {
    if g.is_dev() {
        return Ok(json!({ "dev": true, "message": "未配置网关,当前为开发模式(验证码回显响应)" }));
    }
    let code: String = {
        use rand::Rng;
        format!("{:06}", rand::thread_rng().gen_range(0..1_000_000))
    };
    send(http, g, phone, &code).await?;
    Ok(json!({ "dev": false, "message": format!("测试短信已提交网关(验证码 {code},请查收)") }))
}
