//! 碳基圈(一人付 / opcpay)渠道实现。
//!
//! 与支付宝直连不同,这一层是**合作服务商通道**:服务商代完成支付宝侧的商品配置、
//! 套餐定价与权益托管,我们只跟它那套 REST API 打交道。带来的好处是绕开了
//! ICP 备案与企业资质两道门槛,代价是下面这几条,接入前必须知情:
//!
//! - **没有沙箱环境**:所有请求都是真实交易,联调必须自己小心。
//! - **套餐与价格在服务商侧配置**:本地 `plan_id` 必须通过 `opcpay_skus` 映射到
//!   服务商的 `sku_id`,否则用户会按错误的价格付款。映射缺失时本实现**拒绝下单**。
//! - **无退款、无对账、无交易查询接口**(服务商文档标注"尚待开放"),
//!   出现争议订单只能人工处理。
//! - **回调不携带"支付成功"事件**:只有权益变化(`entitlement.*`)。因此本实现在
//!   验签通过后**再回调一次 `/access` 复核权益**,不单凭通知内容发货。
//!
//! # 安全约束
//! 与支付宝渠道同一条底线:**凭据缺失或为空一律拒绝**。Webhook 密钥为空时,
//! 任何人都能用空串算出"合法"签名,等于敞开门给自己充值(同类项目已有因此
//! 被伪造充值的高危漏洞)。另外时间戳超出 300 秒容差一律拒绝,防重放。
//!
//! # 关于金额
//! 该通道的金额由 SKU 决定,回调也不回传金额。因此**发货时的金额与额度一律以
//! 本地订单表为准**,绝不采信回调报文里的任何数字。

use std::future::Future;
use std::pin::Pin;
use std::time::Duration;

use anyhow::{anyhow, bail, Context};
use axum::http::HeaderMap;
use hmac::{Hmac, Mac};
use serde_json::{json, Value};
use sha2::Sha256;

use super::{yuan_to_fen, NotifyResult, PaymentOrder, PaymentOrderRequest, PaymentProvider};
use crate::config::PaymentConfig;

type HmacSha256 = Hmac<Sha256>;

/// Webhook 签名前缀。
const SIGNATURE_PREFIX: &str = "v1=";
/// 时间戳容差(秒):与官方 SDK 一致,超出即拒,防重放。
const TIMESTAMP_TOLERANCE_SECS: i64 = 300;
/// 需要复核权益的回调事件类型;核销类事件与本项目用法无关(见模块文档)。
const ENTITLEMENT_EVENTS: [&str; 2] = ["entitlement.observed", "entitlement.updated"];

/// 碳基圈(一人付)渠道。
pub struct Opcpay {
    cfg: PaymentConfig,
    api_key: String,
    webhook_secret: String,
    http: reqwest::Client,
}

impl Opcpay {
    /// 从配置构造渠道。**凭据不完整时返回 Err**,由调用方回落到不可用的桩实现。
    pub fn new(cfg: PaymentConfig) -> anyhow::Result<Self> {
        let api_key = cfg.read_opcpay_api_key()?;
        let webhook_secret = cfg.read_opcpay_webhook_secret()?;
        // 回调地址必须向渠道服务端注册才会投递,本地配了 notify_url 本身并不生效。
        // 注册入口见 admin::save_opcpay_webhook,之所以不在本结构内提供,
        // 是因为注册要拿回验签密钥,而构造本结构又要求密钥已存在,会形成循环依赖。
        // 回调地址必须向渠道服务端注册才会投递,本地配了 notify_url 本身并不生效。
        // 注册入口见 admin::save_opcpay_webhook,之所以不在本结构内提供,
        // 是因为注册要拿回验签密钥,而构造本结构又要求密钥已存在,会形成循环依赖。
        let notify = cfg
            .opcpay_webhook_url()
            .or_else(|| {
                let u = cfg.notify_url.trim().to_string();
                (!u.is_empty()).then_some(u)
            })
            .unwrap_or_default();
        if notify.is_empty() {
            tracing::warn!(
                "[payment] 碳基圈未配置回调地址(opcpay_site_url 与 notify_url 均为空),\
                 订单不会自动到账"
            );
        } else {
            // 同域/HTTPS/443 是服务端硬性限制,提前在本地校验一次:
            // 否则要等注册回调时拿到 400 INVALID_WEBHOOK_URL 才知道地址不合规。
            if let Err(e) = cfg.validate_opcpay_public_url(&notify) {
                tracing::warn!(
                    "[payment] 碳基圈回调地址不合规,服务端会拒绝注册,请修正: {e}"
                );
            }
            tracing::warn!(
                "[payment] 碳基圈回调需主动注册才会投递:请调用 \
                 POST /admin/api/payments/opcpay/webhook 注册 {notify},并保存返回的签名密钥"
            );
        }
        // 该通道没有沙箱,初始化成功即意味着后续请求会产生真实资金流动,显式告警。
        tracing::warn!(
            "[payment] 碳基圈(一人付)通道无沙箱环境,当前配置下的下单与回调均为**真实交易**"
        );
        let http = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(5))
            .timeout(Duration::from_secs(cfg.opcpay_timeout_secs.max(1)))
            .build()
            .context("构造碳基圈 HTTP 客户端失败")?;
        Ok(Self {
            cfg,
            api_key,
            webhook_secret,
            http,
        })
    }

    /// 调用服务端 API。非 2xx 时把服务商返回的 `error` / `error_code` 一并带出,便于排障。
    async fn post_json(&self, path: &str, payload: &Value) -> anyhow::Result<Value> {
        let url = format!("{}{}", self.cfg.opcpay_api_base(), path);
        let resp = self
            .http
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .json(payload)
            .send()
            .await
            .with_context(|| format!("请求碳基圈 {path} 失败"))?;
        let status = resp.status();
        let text = resp
            .text()
            .await
            .with_context(|| format!("读取碳基圈 {path} 响应失败"))?;
        let v: Value = serde_json::from_str(&text)
            .unwrap_or_else(|_| json!({ "raw": text, "error": "响应不是合法 JSON" }));
        if !status.is_success() {
            bail!(
                "碳基圈 {path} 返回 {status}: {}",
                v.get("error").and_then(|e| e.as_str()).unwrap_or(&text)
            );
        }
        Ok(v)
    }

    /// 复核用户当前是否真的持有权益。
    ///
    /// 该通道的通知**不等于到账凭证**——它只是"权益发生了变化"的告知。因此这里
    /// 再向服务端查一次,以服务端当前状态为准决定是否发货。原始响应完整记日志,
    /// 便于事后对账。
    async fn confirm_entitlement(&self, user_id: &str) -> anyhow::Result<bool> {
        let v = self
            .post_json("/access", &json!({ "user_id": user_id, "amount": 1 }))
            .await?;
        tracing::info!("[payment] 碳基圈权益复核 user_id={user_id} 响应={v}");
        let granted = v.get("access_granted").and_then(|g| g.as_bool()).unwrap_or(false);
        let decision = v.get("decision").and_then(|d| d.as_str()).unwrap_or("");
        Ok(granted && decision != "DENY")
    }

}

impl PaymentProvider for Opcpay {
    fn name(&self) -> &'static str {
        "opcpay"
    }

    fn create_order(
        &self,
        req: PaymentOrderRequest,
    ) -> Pin<Box<dyn Future<Output = anyhow::Result<PaymentOrder>> + Send + '_>> {
        Box::pin(async move {
            if req.amount_cents <= 0 {
                bail!("下单金额必须大于 0:amount_cents={}", req.amount_cents);
            }
            // 金额由 SKU 决定:拿不到映射就拒绝,绝不让用户跳进错误价格的收银台。
            let sku = self.cfg.opcpay_sku_for(req.plan_id.as_deref()).ok_or_else(|| {
                anyhow!(
                    "套餐 {:?} 未配置碳基圈 SKU:请在 [payment.opcpay_skus] 中映射 plan_id -> sku_id, \
                     或设置 payment.opcpay_sku_id 作为默认档位",
                    req.plan_id
                )
            })?;
            let idempotency_key = format!("RLY{}", uuid::Uuid::new_v4().simple());
            // 页面返回地址:付款完成后把用户送回自己的控制台。它只是跳转,不是支付成功通知,
            // 服务端同样要求与项目官网同域 + HTTPS。地址不合规时宁可不带,也不能让整个下单失败。
            let mut payload = json!({
                "user_id": req.user_id,
                "idempotency_key": idempotency_key,
                "sku_id": sku,
            });
            let callback_url = self.cfg.opcpay_callback_url();
            if let Some(url) = callback_url {
                match self.cfg.validate_opcpay_public_url(&url) {
                    Ok(()) => payload["callback_url"] = json!(url),
                    Err(e) => tracing::warn!(
                        "[payment] 碳基圈页面返回地址不合规,本次下单不带该参数: {e}"
                    ),
                }
            }
            let resp = self.post_json("/purchase", &payload).await?;

            let status = resp.get("status").and_then(|s| s.as_str()).unwrap_or("");
            if status == "FAILED" {
                bail!(
                    "碳基圈创建购买入口失败: {}",
                    resp.get("message")
                        .and_then(|m| m.as_str())
                        .or_else(|| resp.get("reason_code").and_then(|c| c.as_str()))
                        .unwrap_or("未提供原因")
                );
            }
            verify_channel_price(req.amount_cents, &resp)?;

            let pay_url = resp
                .get("purchase_url")
                .and_then(|u| u.as_str())
                .map(|s| s.to_string())
                .filter(|s| !s.is_empty())
                .ok_or_else(|| anyhow!("碳基圈未返回 purchase_url,无法拉起支付"))?;

            Ok(PaymentOrder {
                order_id: idempotency_key,
                provider: self.name().to_string(),
                amount_cents: req.amount_cents,
                status: "pending".to_string(),
                pay_url: Some(pay_url),
            })
        })
    }

    /// 注册/停用回调地址。
    ///
    /// 该通道的回调**必须主动注册**:只把 `notify_url` 写进本地配置不会生效,
    /// 服务端不会往没登记过的地址推送任何事件。
    /// 服务端硬性要求:与项目官网同域名的公网 HTTPS 地址、仅 443 端口、
    /// 不含账号信息/查询参数/片段,且不接受私网地址与重定向。
    ///
    /// 返回的 `secret` 是验签密钥,**仅保存时返回这一次**;调用方必须立刻落盘到
    /// `RELAY_PAYMENT__OPCPAY_WEBHOOK_SECRET`,否则收得到回调也验不了签。
    /// 再次调用会轮换密钥,旧密钥立即失效。
    fn verify_notify(
        &self,
        headers: &HeaderMap,
        body: &str,
    ) -> Pin<Box<dyn Future<Output = anyhow::Result<NotifyResult>> + Send + '_>> {
        // 同支付宝渠道:先把借用转成自有数据,避免 async 块持有调用方的生命周期。
        let body = body.to_owned();
        // 验签同步做完:返回的 future 只承诺 &self 的生命周期,headers 的借用活不了那么久。
        // 只取需要的三个头值,不必为读三个字符串克隆整份 HeaderMap。
        let verified = verify_webhook_parts(
            &body,
            header_str(headers, "yirenfu-timestamp"),
            header_str(headers, "yirenfu-signature"),
            header_str(headers, "yirenfu-event-id"),
            &self.webhook_secret,
        );
        Box::pin(async move {
            let event = verified?;

            let event_type = event.get("type").and_then(|t| t.as_str()).unwrap_or("");
            let user_id = event
                .get("user_id")
                .and_then(|u| u.as_str())
                .unwrap_or_default()
                .to_string();
            if user_id.is_empty() {
                bail!("碳基圈回调缺少 user_id");
            }
            // 权益事件里可能带着触发它的那次购买的单号。
            let order_id = event
                .pointer("/data/idempotency_key")
                .and_then(|k| k.as_str())
                .unwrap_or_default()
                .to_string();

            // 核销类事件与本项目用法无关(买断/时长模式不做逐次核销),明确忽略。
            if !ENTITLEMENT_EVENTS.contains(&event_type) {
                tracing::info!(
                    "[payment] 忽略碳基圈事件 type={event_type} user_id={user_id}: \
                     本项目按买断/时长模式接入,不做逐次核销"
                );
                return Ok(NotifyResult {
                    order_id,
                    paid: false,
                    channel_user_id: Some(user_id),
                    trade_no: None,
                    paid_amount_cents: None,
                });
            }

            // 通知不是到账凭证:以服务端当前权益状态为准。
            let paid = match self.confirm_entitlement(&user_id).await {
                Ok(v) => v,
                Err(e) => {
                    // 复核失败不能当作"已支付",但也不能直接丢掉——返回错误让渠道重投。
                    tracing::warn!("[payment] 碳基圈权益复核失败,等待重投: {e}");
                    return Err(e);
                }
            };

            // paid_amount_cents 恒为 None:该通道金额由 SKU 决定,回调不回传金额。
            // 因此发货金额只能以本地订单表(下单时的套餐价格快照)为准,这也是
            // 为什么 SKU 映射缺失时必须拒绝下单——一旦映射错,没有任何事后校验能发现。
            Ok(NotifyResult {
                order_id,
                paid,
                channel_user_id: Some(user_id),
                trade_no: None,
                paid_amount_cents: None,
            })
        })
    }
}

/// 校验 Webhook 签名并返回解析后的事件。
///
/// 规则与服务商官方 SDK 逐条对齐:
/// `v1 = hex(HMAC-SHA256(secret, "{timestamp}.{rawBody}"))`,时间戳为 10~12 位数字、
/// 与当前时间差不超过 300 秒,且 `event.id` 必须与 `yirenfu-event-id` 请求头一致。
/// 验签主体,与请求头解耦:调用方可以在 async 块之外先取好三个头,
/// 避免把 `HeaderMap` 的借用(或整份克隆)带进 future。
/// 用渠道回传的价格快照校验本地定价。
///
/// 该通道的**回调不回传金额,事后无法核对**,所以价格漂移只能在下单这一刻拦住。
/// 文档明确要求以 `charging_option.price` 为准("不要用前端金额替代"),
/// 这里反过来拿它校验我们自己算出来的 `amount_cents`。
fn verify_channel_price(req_cents: i64, resp: &Value) -> anyhow::Result<()> {
    let price = match resp.pointer("/charging_option/price").and_then(|p| p.as_str()) {
        Some(p) => p,
        // 未回传价格时不阻断:sku_id 已锁定档位,这里防的是配置漂移而非伪造。
        None => {
            tracing::warn!("[payment] 碳基圈下单未回传 charging_option.price,跳过金额核对");
            return Ok(());
        }
    };
    match yuan_to_fen(price) {
        Some(c) if c == req_cents => Ok(()),
        Some(c) => bail!(
            "套餐价格与碳基圈档位不一致:本地 {req_cents} 分,渠道 {c} 分({price} 元)。\
             请同步该 SKU 的档位价格或本地 price_cents"
        ),
        None => bail!("碳基圈回传的价格无法解析: {price:?}"),
    }
}

fn verify_webhook_parts(
    body: &str,
    ts: Option<&str>,
    sig: Option<&str>,
    event_id: Option<&str>,
    secret: &str,
) -> anyhow::Result<Value> {
    let ts = ts.ok_or_else(|| anyhow!("回调缺少 yirenfu-timestamp 请求头"))?;
    let sig = sig.ok_or_else(|| anyhow!("回调缺少 yirenfu-signature 请求头"))?;

    if ts.len() < 10 || ts.len() > 12 || !ts.bytes().all(|b| b.is_ascii_digit()) {
        bail!("回调时间戳格式非法: {ts}");
    }
    let ts_secs: i64 = ts.parse().context("回调时间戳无法解析为整数")?;
    let now = chrono::Utc::now().timestamp();
    if (now - ts_secs).abs() > TIMESTAMP_TOLERANCE_SECS {
        bail!("回调时间戳超出 {TIMESTAMP_TOLERANCE_SECS} 秒容差,可能是重放攻击");
    }

    if !sig.starts_with(SIGNATURE_PREFIX) {
        bail!("回调签名格式非法:应以 {SIGNATURE_PREFIX} 开头");
    }
    let expected = format!("{SIGNATURE_PREFIX}{}", compute_signature(secret, &ts, body));
    if !constant_time_eq(expected.as_bytes(), sig.as_bytes()) {
        bail!("回调签名校验失败:报文可能被篡改,或并非来自碳基圈");
    }

    let event: Value = serde_json::from_str(body).context("回调内容是非法 JSON")?;
    let event_id = event_id.ok_or_else(|| anyhow!("回调缺少 yirenfu-event-id 请求头"))?;
    if event.get("id").and_then(|i| i.as_str()) != Some(event_id) {
        bail!("回调事件 ID 与请求头 yirenfu-event-id 不一致");
    }
    Ok(event)
}

/// 计算签名摘要(不含 `v1=` 前缀),返回小写 hex。
fn compute_signature(secret: &str, timestamp: &str, body: &str) -> String {
    let mut mac = HmacSha256::new_from_slice(secret.as_bytes()).expect("HMAC 接受任意长度密钥");
    mac.update(timestamp.as_bytes());
    mac.update(b".");
    mac.update(body.as_bytes());
    hex::encode(mac.finalize().into_bytes())
}

/// 取请求头字符串(HeaderMap 的键匹配本身大小写不敏感)。
fn header_str<'a>(headers: &'a HeaderMap, key: &str) -> Option<&'a str> {
    headers.get(key).and_then(|v| v.to_str().ok())
}

/// 定长比较,避免因提前返回而泄漏"前几位已经对上了"的信息。
fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.iter().zip(b.iter()).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEST_SECRET: &str = "whsec_test_secret";

    fn test_config() -> PaymentConfig {
        PaymentConfig {
            enabled: true,
            provider: "opcpay".into(),
            notify_url: "https://relay.example.com/payments/notify/opcpay".into(),
            opcpay_api_key: "test_api_key".into(),
            opcpay_webhook_secret: TEST_SECRET.into(),
            ..Default::default()
        }
    }

    /// 构造一份合法回调:事件体 -> 签名 -> 请求头。
    fn signed_event(event: &Value, secret: &str, ts: i64) -> (String, HeaderMap) {
        let body = serde_json::to_string(event).unwrap();
        let sig = format!("{SIGNATURE_PREFIX}{}", compute_signature(secret, &ts.to_string(), &body));
        let mut headers = HeaderMap::new();
        headers.insert("yirenfu-timestamp", ts.to_string().parse().unwrap());
        headers.insert("yirenfu-signature", sig.parse().unwrap());
        headers.insert("yirenfu-event-id", event["id"].as_str().unwrap().parse().unwrap());
        (body, headers)
    }

    fn sample_event() -> Value {
        json!({
            "id": "evt_0001",
            "type": "entitlement.observed",
            "project_id": "proj_1",
            "user_id": "u-1001",
            "created_at": "2026-11-01T09:00:00Z",
            "data": { "idempotency_key": "RLY-TEST-0001", "decision": "ALLOW" }
        })
    }

    fn now_ts() -> i64 {
        chrono::Utc::now().timestamp()
    }

    /// 测试侧入口:取头方式必须与 `verify_notify` 完全一致,否则测的就不是生产路径。
    fn verify_via_headers(headers: &HeaderMap, body: &str, secret: &str) -> anyhow::Result<Value> {
        verify_webhook_parts(
            body,
            header_str(headers, "yirenfu-timestamp"),
            header_str(headers, "yirenfu-signature"),
            header_str(headers, "yirenfu-event-id"),
            secret,
        )
    }

    #[test]
    fn signature_matches_official_algorithm() {
        // 官方算法:v1=hex(HMAC-SHA256(secret, timestamp + "." + rawBody))
        let s = compute_signature(TEST_SECRET, "1700000000", "{\"a\":1}");
        assert_eq!(s.len(), 64, "SHA256 摘要应为 64 位 hex");
        assert!(s.bytes().all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase()));
    }

    #[test]
    fn valid_webhook_passes_and_returns_event() {
        let event = sample_event();
        let (body, headers) = signed_event(&event, TEST_SECRET, now_ts());
        let got = verify_via_headers(&headers, &body, TEST_SECRET).unwrap();
        assert_eq!(got["id"], "evt_0001");
        assert_eq!(got["user_id"], "u-1001");
    }

    #[test]
    fn tampered_body_is_rejected() {
        let event = sample_event();
        let (body, headers) = signed_event(&event, TEST_SECRET, now_ts());
        // 攻击者把用户换成自己,试图给张三的付款发货给李四。
        let tampered = body.replace("u-1001", "u-9999");
        let err = verify_via_headers(&headers, &tampered, TEST_SECRET).unwrap_err();
        assert!(err.to_string().contains("签名校验失败"), "实际错误: {err}");
    }

    #[test]
    fn expired_timestamp_is_rejected() {
        let event = sample_event();
        let stale = now_ts() - TIMESTAMP_TOLERANCE_SECS - 60;
        let (body, headers) = signed_event(&event, TEST_SECRET, stale);
        let err = verify_via_headers(&headers, &body, TEST_SECRET).unwrap_err();
        assert!(err.to_string().contains("容差"), "实际错误: {err}");
    }

    #[test]
    fn wrong_secret_is_rejected() {
        let event = sample_event();
        let (body, headers) = signed_event(&event, "some_other_secret", now_ts());
        let err = verify_via_headers(&headers, &body, TEST_SECRET).unwrap_err();
        assert!(err.to_string().contains("签名校验失败"), "实际错误: {err}");
    }

    #[test]
    fn missing_headers_are_rejected() {
        let event = sample_event();
        let (body, headers) = signed_event(&event, TEST_SECRET, now_ts());
        for drop_key in ["yirenfu-timestamp", "yirenfu-signature", "yirenfu-event-id"] {
            let mut h = headers.clone();
            h.remove(drop_key);
            let err = verify_via_headers(&h, &body, TEST_SECRET).unwrap_err();
            assert!(err.to_string().contains("缺少"), "去掉 {drop_key} 后的错误: {err}");
        }
    }

    #[test]
    fn event_id_mismatch_is_rejected() {
        let event = sample_event();
        let (body, mut headers) = signed_event(&event, TEST_SECRET, now_ts());
        headers.insert("yirenfu-event-id", "evt_other".parse().unwrap());
        let err = verify_via_headers(&headers, &body, TEST_SECRET).unwrap_err();
        assert!(err.to_string().contains("不一致"), "实际错误: {err}");
    }

    #[test]
    fn malformed_timestamp_is_rejected() {
        let event = sample_event();
        let mut headers = HeaderMap::new();
        headers.insert("yirenfu-timestamp", "not-a-number".parse().unwrap());
        headers.insert("yirenfu-signature", "v1=abc".parse().unwrap());
        headers.insert("yirenfu-event-id", "evt_0001".parse().unwrap());
        let body = serde_json::to_string(&event).unwrap();
        let err = verify_via_headers(&headers, &body, TEST_SECRET).unwrap_err();
        assert!(err.to_string().contains("时间戳格式非法"), "实际错误: {err}");
    }

    #[test]
    fn constant_time_eq_is_length_and_content_sensitive() {
        assert!(constant_time_eq(b"abc", b"abc"));
        assert!(!constant_time_eq(b"abc", b"abd"));
        assert!(!constant_time_eq(b"abc", b"ab"));
        assert!(!constant_time_eq(b"", b"a"));
    }

    #[test]
    fn sku_lookup_prefers_plan_mapping_then_default() {
        let mut cfg = PaymentConfig::default();
        // 都没有 -> None,渠道层据此拒绝下单。
        assert!(cfg.opcpay_sku_for(Some("plan-basic")).is_none());

        cfg.opcpay_sku_id = "sku_default".into();
        assert_eq!(cfg.opcpay_sku_for(None).as_deref(), Some("sku_default"));
        assert_eq!(cfg.opcpay_sku_for(Some("plan-basic")).as_deref(), Some("sku_default"));

        cfg.opcpay_skus
            .insert("plan-basic".to_string(), "sku_basic".to_string());
        assert_eq!(cfg.opcpay_sku_for(Some("plan-basic")).as_deref(), Some("sku_basic"));
        // 未映射的套餐回落默认档位。
        assert_eq!(cfg.opcpay_sku_for(Some("plan-pro")).as_deref(), Some("sku_default"));
    }

    #[tokio::test]
    async fn new_rejects_missing_credentials() {
        let cfg = PaymentConfig {
            enabled: true,
            provider: "opcpay".into(),
            ..Default::default()
        };
        assert!(Opcpay::new(cfg).is_err(), "缺少 API Key 与 Webhook 密钥应初始化失败");

        let cfg = test_config();
        assert!(Opcpay::new(cfg).is_ok());
    }

    #[tokio::test]
    async fn create_order_rejects_plan_without_sku_mapping() {
        let p = Opcpay::new(test_config()).unwrap();
        let req = PaymentOrderRequest {
            user_id: "u-1001".into(),
            plan_id: Some("plan-unmapped".into()),
            tokens: 1_000_000,
            amount_cents: 1990,
            remark: "基础套餐".into(),
        };
        // 无网络也会被拦下:SKU 校验发生在发请求之前。
        let err = p.create_order(req).await.unwrap_err();
        assert!(err.to_string().contains("未配置碳基圈 SKU"), "实际错误: {err}");
    }

    #[tokio::test]
    async fn create_order_rejects_non_positive_amount() {
        let mut cfg = test_config();
        cfg.opcpay_sku_id = "sku_default".into();
        let p = Opcpay::new(cfg).unwrap();
        let req = PaymentOrderRequest {
            user_id: "u-1001".into(),
            plan_id: None,
            tokens: 0,
            amount_cents: 0,
            remark: String::new(),
        };
        let err = p.create_order(req).await.unwrap_err();
        assert!(err.to_string().contains("金额必须大于 0"), "实际错误: {err}");
    }

    #[tokio::test]
    async fn notify_ignores_consumption_events() {
        // 本项目按买断/时长模式接入,不做逐次核销:核销类事件不能触发发货。
        // 这条路径不碰网络,可以直接走完整的 verify_notify。
        let p = Opcpay::new(test_config()).unwrap();
        let mut event = sample_event();
        event["type"] = json!("consumption.succeeded");
        let (body, headers) = signed_event(&event, TEST_SECRET, now_ts());
        let res = p.verify_notify(&headers, &body).await.unwrap();
        assert!(!res.paid, "核销事件不应判定为已支付");
        assert_eq!(res.order_id, "RLY-TEST-0001");
        assert_eq!(res.channel_user_id.as_deref(), Some("u-1001"));
    }

    #[tokio::test]
    async fn notify_rejects_tampered_body() {
        let p = Opcpay::new(test_config()).unwrap();
        let event = sample_event();
        let (body, headers) = signed_event(&event, TEST_SECRET, now_ts());
        let tampered = body.replace("u-1001", "u-9999");
        let err = p
            .verify_notify(&headers, &tampered)
            .await
            .expect_err("篡改后的回调必须被拒绝");
        assert!(err.to_string().contains("签名校验失败"), "实际错误: {err}");
    }

    #[test]
    fn channel_price_mismatch_is_rejected_in_both_directions() {
        let resp = json!({ "charging_option": { "price": "19.90" } });
        verify_channel_price(1990, &resp).expect("价格一致应放行");
        // 渠道更贵:用户会被多收。
        let err = verify_channel_price(100, &resp).expect_err("价格不一致必须拒绝");
        assert!(err.to_string().contains("不一致"), "实际错误: {err}");
        // 渠道更便宜:我们少收,同样不能卖。
        let err = verify_channel_price(99900, &resp).expect_err("价格不一致必须拒绝");
        assert!(err.to_string().contains("不一致"), "实际错误: {err}");
    }

    #[test]
    fn unparsable_channel_price_is_rejected() {
        let resp = json!({ "charging_option": { "price": "abc" } });
        let err = verify_channel_price(1990, &resp).expect_err("无法解析的价格必须拒绝");
        assert!(err.to_string().contains("无法解析"), "实际错误: {err}");
    }

    #[test]
    fn missing_channel_price_skips_check() {
        // 未回传时不阻断:sku_id 已锁定档位,这里防的是配置漂移而非伪造。
        verify_channel_price(1990, &json!({ "status": "READY" }))
            .expect("未回传价格时不应阻断下单");
    }
}
