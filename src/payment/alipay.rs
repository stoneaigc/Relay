//! 支付宝「电脑网站支付」(alipay.trade.page.pay)渠道实现。
//!
//! 支付宝没有官方 Rust SDK,这里按开放平台规范自行实现两件事:
//! 1. **下单**:参数按 key 字典序拼接待签名串,用商户 RSA 私钥做
//!    RSA2(RSASSA-PKCS1-v1_5 + SHA256)签名,再拼出可供浏览器直接跳转的收银台 URL。
//! 2. **回调**:用支付宝公钥验签异步通知,确认消息确实来自支付宝而非伪造。
//!
//! # 安全约束
//! 这两条不是纸上谈兵,有同类项目的真实事故作为前车之鉴:
//!
//! - **凭据缺失或为空时一律拒绝,绝不"跳过校验"**。密钥为空时攻击者可以用空串算出
//!   合法签名,等于敞开门给自己充值(参见 New API CVE-2026-41432,可伪造充值)。
//!   因此 [`Alipay::new`] 在凭据不完整时返回 Err,注册失败则整体回落 501,而不是带病运行。
//! - **验签通过后仍要核对金额**。本渠道的回调只负责回答"这笔通知是谁发的、成功与否",
//!   实付金额与业务订单的比对由上层负责,避免订单号或金额被篡改后照样发货。

use std::collections::BTreeMap;
use std::future::Future;
use std::pin::Pin;

use anyhow::{bail, Context};
use axum::http::HeaderMap;
use base64::Engine;
use chrono::Local;
use percent_encoding::{percent_decode_str, utf8_percent_encode, AsciiSet, NON_ALPHANUMERIC};
use rsa::pkcs1::DecodeRsaPrivateKey;
use rsa::pkcs1v15::{SigningKey, VerifyingKey};
use rsa::pkcs8::{DecodePrivateKey, DecodePublicKey};
use rsa::sha2::Sha256;
use rsa::signature::{SignatureEncoding, Signer, Verifier};
use rsa::{RsaPrivateKey, RsaPublicKey};

use super::{yuan_to_fen, NotifyResult, PaymentOrder, PaymentOrderRequest, PaymentProvider};
use crate::config::PaymentConfig;

/// 电脑网站支付的产品码,固定值。
const PRODUCT_CODE_PAGE_PAY: &str = "FAST_INSTANT_TRADE_PAY";
/// 回调中表示支付成功的两种终态。
const PAID_STATUSES: [&str; 2] = ["TRADE_SUCCESS", "TRADE_FINISHED"];

/// 支付宝电脑网站支付渠道。
pub struct Alipay {
    cfg: PaymentConfig,
    signing_key: SigningKey<Sha256>,
    verifying_key: VerifyingKey<Sha256>,
}

impl Alipay {
    /// 从配置构造渠道。**凭据不完整时返回 Err**,由调用方回落到不可用的桩实现。
    pub fn new(cfg: PaymentConfig) -> anyhow::Result<Self> {
        if cfg.app_id.trim().is_empty() {
            bail!("支付宝缺少 app_id:请配置 payment.app_id 或环境变量 RELAY_PAYMENT__APP_ID");
        }
        let private_pem = cfg.read_private_key()?;
        let private_key = parse_private_key(&private_pem)?;
        let public_pem = cfg.read_alipay_public_key()?;
        let public_key = RsaPublicKey::from_public_key_pem(&public_pem)
            .context("解析支付宝公钥失败:应为 PEM 格式的 PUBLIC KEY")?;
        if cfg.notify_url.trim().is_empty() {
            tracing::warn!(
                "[payment] alipay 未配置 notify_url,将无法收到支付结果回调,订单不会自动到账"
            );
        }
        Ok(Self {
            cfg,
            signing_key: SigningKey::<Sha256>::new(private_key),
            verifying_key: VerifyingKey::<Sha256>::new(public_key),
        })
    }

    /// 对一段待签名内容做 RSA2 签名,返回 base64 文本。
    fn sign_content(&self, content: &str) -> anyhow::Result<String> {
        let sig = self
            .signing_key
            .try_sign(content.as_bytes())
            .context("RSA2 签名失败")?;
        Ok(base64::engine::general_purpose::STANDARD.encode(sig.to_bytes()))
    }
}

impl PaymentProvider for Alipay {
    fn name(&self) -> &'static str {
        "alipay"
    }

    fn create_order(
        &self,
        req: PaymentOrderRequest,
    ) -> Pin<Box<dyn Future<Output = anyhow::Result<PaymentOrder>> + Send + '_>> {
        Box::pin(async move {
            if req.amount_cents <= 0 {
                bail!("下单金额必须大于 0:amount_cents={}", req.amount_cents);
            }
            let out_trade_no = format!("RLY{}", uuid::Uuid::new_v4().simple());
            let subject = if req.remark.trim().is_empty() {
                "额度充值".to_string()
            } else {
                req.remark.trim().to_string()
            };
            let biz_content = serde_json::json!({
                "out_trade_no": out_trade_no,
                "total_amount": fen_to_yuan(req.amount_cents),
                "subject": subject,
                "product_code": PRODUCT_CODE_PAGE_PAY,
            });

            // 公共请求参数 + 业务参数。注意:待签名串里的值是未经 URL 编码的原值。
            let mut params: BTreeMap<String, String> = BTreeMap::new();
            params.insert("app_id".into(), self.cfg.app_id.trim().to_string());
            params.insert("method".into(), "alipay.trade.page.pay".into());
            params.insert("format".into(), "JSON".into());
            params.insert("charset".into(), "utf-8".into());
            params.insert("sign_type".into(), "RSA2".into());
            params.insert(
                "timestamp".into(),
                Local::now().format("%Y-%m-%d %H:%M:%S").to_string(),
            );
            params.insert("version".into(), "1.0".into());
            params.insert("biz_content".into(), biz_content.to_string());
            if !self.cfg.notify_url.trim().is_empty() {
                params.insert("notify_url".into(), self.cfg.notify_url.trim().to_string());
            }

            // 此刻 params 尚未插入 sign,直接整串签名即可。
            let sign_content = build_sign_content(&params);
            let sign = self.sign_content(&sign_content)?;
            params.insert("sign".into(), sign);

            // 发送时才做 URL 编码。NON_ALPHANUMERIC 会把空格编成 %20 而非 +,符合要求。
            let query = params
                .iter()
                .map(|(k, v)| format!("{}={}", query_encode(k), query_encode(v)))
                .collect::<Vec<_>>()
                .join("&");
            let pay_url = format!("{}?{}", self.cfg.gateway_url(), query);

            Ok(PaymentOrder {
                order_id: out_trade_no,
                provider: self.name().to_string(),
                amount_cents: req.amount_cents,
                status: "pending".to_string(),
                pay_url: Some(pay_url),
            })
        })
    }

    fn verify_notify(
        &self,
        _headers: &HeaderMap,
        body: &str,
    ) -> Pin<Box<dyn Future<Output = anyhow::Result<NotifyResult>> + Send + '_>> {
        // 先把切片转成自有 String,让 future 不再持有调用方的借用,
        // 否则返回类型里的 '_ 会同时被 &'self 和 body 的生命周期夹住。
        let body = body.to_owned();
        Box::pin(async move {
            let pairs = parse_form_body(&body);
            if pairs.is_empty() {
                bail!("支付宝回调内容为空,不是合法的通知报文");
            }

            let sign_b64 = pairs.get("sign").cloned().unwrap_or_default();
            if sign_b64.trim().is_empty() {
                bail!("支付宝回调缺少 sign 参数");
            }
            let sign_type = pairs
                .get("sign_type")
                .map(|s| s.to_uppercase())
                .unwrap_or_else(|| "RSA2".to_string());
            if sign_type != "RSA2" {
                bail!("不支持的 sign_type={sign_type},本实现仅支持 RSA2");
            }

            // 待验签串按官方规则剔除 sign 与 sign_type,其余按 key 字典序拼接。
            let content = build_verify_content(&pairs);

            let sig_bytes = base64::engine::general_purpose::STANDARD
                .decode(sign_b64.trim())
                .context("回调 sign 不是合法 base64")?;
            let sig = rsa::pkcs1v15::Signature::try_from(sig_bytes.as_slice())
                .context("回调 sign 不是合法 RSA 签名")?;

            self.verifying_key
                .verify(content.as_bytes(), &sig)
                .map_err(|_| {
                    anyhow::anyhow!("支付宝回调验签失败:消息可能被篡改,或并非来自支付宝")
                })?;

            let out_trade_no = pairs.get("out_trade_no").cloned().unwrap_or_default();
            if out_trade_no.is_empty() {
                bail!("支付宝回调缺少 out_trade_no");
            }
            let trade_status = pairs.get("trade_status").cloned().unwrap_or_default();
            let paid = PAID_STATUSES.contains(&trade_status.as_str());
            // trade_no 是支付宝侧流水号,落库用于对账;下单号 out_trade_no 用于反查业务订单。
            let trade_no = pairs.get("trade_no").cloned().filter(|s| !s.is_empty());

            // 实付金额回传上层比对:验签只证明"消息来自支付宝",不证明金额没被动过。
            let paid_amount_cents = pairs.get("total_amount").and_then(|s| yuan_to_fen(s));

            Ok(NotifyResult {
                order_id: out_trade_no,
                paid,
                channel_user_id: None,
                trade_no,
                paid_amount_cents,
            })
        })
    }
}

/// 拼接待签名串:key 字典序、value 原样、`&` 连接。
///
/// 由于 `BTreeMap` 本身有序,遍历即满足字典序要求。
fn build_sign_content(params: &BTreeMap<String, String>) -> String {
    params
        .iter()
        .map(|(k, v)| format!("{k}={v}"))
        .collect::<Vec<_>>()
        .join("&")
}

/// 拼接待验签串:规则同 [`build_sign_content`],但剔除 sign 与 sign_type 本身。
fn build_verify_content(params: &BTreeMap<String, String>) -> String {
    params
        .iter()
        .filter(|(k, _)| k.as_str() != "sign" && k.as_str() != "sign_type")
        .map(|(k, v)| format!("{k}={v}"))
        .collect::<Vec<_>>()
        .join("&")
}

/// 兼容两种私钥格式:PKCS#8(`BEGIN PRIVATE KEY`)与 PKCS#1(`BEGIN RSA PRIVATE KEY`)。
fn parse_private_key(pem: &str) -> anyhow::Result<RsaPrivateKey> {
    if let Ok(k) = RsaPrivateKey::from_pkcs8_pem(pem) {
        return Ok(k);
    }
    if let Ok(k) = RsaPrivateKey::from_pkcs1_pem(pem) {
        return Ok(k);
    }
    bail!(
        "解析商户私钥失败:需要 PKCS#8(-----BEGIN PRIVATE KEY-----)或 \
         PKCS#1(-----BEGIN RSA PRIVATE KEY-----)格式的 PEM"
    )
}

/// 分值转成支付宝要求的元字符串。**全程整数运算**,避免浮点误差。
fn fen_to_yuan(cents: i64) -> String {
    let sign = if cents < 0 { "-" } else { "" };
    let abs = cents.abs();
    format!("{sign}{}.{:02}", abs / 100, abs % 100)
}

/// 查询串编码字符集。
///
/// 相对 [`NON_ALPHANUMERIC`] 额外放出 RFC 3986 的 unreserved 字符,避免把时间戳、
/// 金额里的 `-`、`.` 之类通通编成 `%2D` `%2E`,让 URL 保持可读且贴近官方示例。
const QUERY_ENCODE_SET: &AsciiSet = &NON_ALPHANUMERIC
    .remove(b'-')
    .remove(b'_')
    .remove(b'.')
    .remove(b'~');

/// URL 编码:空格编成 %20 而不是 +,unreserved 字符原样保留。
fn query_encode(s: &str) -> String {
    utf8_percent_encode(s, QUERY_ENCODE_SET).to_string()
}

/// 解析 application/x-www-form-urlencoded 报文。
fn parse_form_body(body: &str) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    for part in body.split('&') {
        if part.is_empty() {
            continue;
        }
        let mut kv = part.splitn(2, '=');
        let key = kv.next().unwrap_or("");
        let val = kv.next().unwrap_or("");
        out.insert(decode_form_value(key), decode_form_value(val));
    }
    out
}

/// 表单解码:先把 `+` 还原为空格,再做百分号解码。
fn decode_form_value(s: &str) -> String {
    percent_decode_str(&s.replace('+', " "))
        .decode_utf8_lossy()
        .to_string()
}

#[cfg(test)]
mod tests {
    use rsa::pkcs8::{EncodePrivateKey, EncodePublicKey, LineEnding};

    use super::*;

    /// 运行时生成一对临时密钥用于测试。
    ///
    /// 刻意不把任何 PEM 硬编码进仓库:既是测试隔离,也杜绝误提交真实凭据的可能。
    fn test_key_pair() -> (String, String) {
        let mut rng = rand::thread_rng();
        let private = RsaPrivateKey::new(&mut rng, 2048).expect("生成测试 RSA 密钥失败");
        let public = RsaPublicKey::from(&private);
        let private_pem = private
            .to_pkcs8_pem(LineEnding::LF)
            .expect("私钥 PEM 编码失败")
            .to_string();
        let public_pem = public
            .to_public_key_pem(LineEnding::LF)
            .expect("公钥 PEM 编码失败");
        (private_pem, public_pem)
    }

    fn test_config() -> PaymentConfig {
        let (private_key, alipay_public_key) = test_key_pair();
        PaymentConfig {
            enabled: true,
            provider: "alipay".into(),
            notify_url: "https://relay.example.com/payments/notify/alipay".into(),
            app_id: "2021000000000000".into(),
            private_key,
            alipay_public_key,
            sandbox: true,
            ..Default::default()
        }
    }

    fn sample_order_request(cents: i64) -> PaymentOrderRequest {
        PaymentOrderRequest {
            user_id: "u1".into(),
            plan_id: Some("plan-basic".into()),
            tokens: 1_000_000,
            amount_cents: cents,
            remark: "基础套餐".into(),
        }
    }

    /// 把一组参数拼成回调报文:先算 sign(按"排除 sign/sign_type"规则),再整体编码。
    fn sign_as_callback(p: &Alipay, params: &BTreeMap<String, String>) -> String {
        let content = build_verify_content(params);
        let sig = p.signing_key.sign(content.as_bytes());
        let sign = base64::engine::general_purpose::STANDARD.encode(sig.to_bytes());
        let mut all = params.clone();
        all.insert("sign".into(), sign);
        all.iter()
            .map(|(k, v)| format!("{}={}", query_encode(k), query_encode(v)))
            .collect::<Vec<_>>()
            .join("&")
    }

    #[test]
    fn fen_to_yuan_keeps_two_decimals() {
        assert_eq!(fen_to_yuan(1990), "19.90");
        assert_eq!(fen_to_yuan(1), "0.01");
        assert_eq!(fen_to_yuan(100), "1.00");
        assert_eq!(fen_to_yuan(0), "0.00");
        assert_eq!(fen_to_yuan(200_000), "2000.00");
        assert_eq!(fen_to_yuan(-500), "-5.00");
    }

    #[test]
    fn yuan_to_fen_roundtrips_without_float_error() {
        assert_eq!(yuan_to_fen("19.90"), Some(1990));
        assert_eq!(yuan_to_fen("0.01"), Some(1));
        assert_eq!(yuan_to_fen("1"), Some(100));
        assert_eq!(yuan_to_fen("1.9"), Some(190));
        assert_eq!(yuan_to_fen("2000.00"), Some(200_000));
        assert_eq!(yuan_to_fen("-5.00"), Some(-500));
        // 与 fen_to_yuan 互为逆运算(非负场景)。
        for cents in [0i64, 1, 99, 100, 1990, 200_000, 1_234_567] {
            assert_eq!(yuan_to_fen(&fen_to_yuan(cents)), Some(cents), "cents={cents}");
        }
    }

    #[test]
    fn yuan_to_fen_rejects_garbage_instead_of_defaulting_to_zero() {
        assert_eq!(yuan_to_fen(""), None);
        assert_eq!(yuan_to_fen("abc"), None);
        assert_eq!(yuan_to_fen("1.234"), None, "超过两位小数应拒绝");
        assert_eq!(yuan_to_fen(".50"), None);
        assert_eq!(yuan_to_fen("1.-2"), None);
        // 关键:解析失败绝不能返回 Some(0),否则金额比对会被绕过。
        assert_ne!(yuan_to_fen("abc"), Some(0));
    }

    #[test]
    fn fen_conversion_never_uses_float_rounding() {
        // 0.1+0.2 类浮点误差的经典取值:整数运算必须精确。
        assert_eq!(yuan_to_fen("0.30"), Some(30));
        assert_eq!(yuan_to_fen("1.15"), Some(115));
        assert_eq!(yuan_to_fen("8.20"), Some(820));
    }

    #[test]
    fn form_body_decodes_plus_and_percent() {
        let m = parse_form_body("a=1&b=hello+world&c=%E4%BD%A0%E5%A5%BD");
        assert_eq!(m.get("a").map(String::as_str), Some("1"));
        assert_eq!(m.get("b").map(String::as_str), Some("hello world"));
        assert_eq!(m.get("c").map(String::as_str), Some("你好"));
    }

    #[test]
    fn query_encode_uses_percent_twenty_not_plus() {
        assert_eq!(query_encode("2026-11-01 12:00:00"), "2026-11-01%2012%3A00%3A00");
    }

    #[test]
    fn sign_content_sorts_by_key() {
        let mut m = BTreeMap::new();
        m.insert("z".into(), "1".into());
        m.insert("a".into(), "2".into());
        assert_eq!(build_sign_content(&m), "a=2&z=1");
    }

    #[test]
    fn verify_content_drops_sign_and_sign_type() {
        let mut m = BTreeMap::new();
        m.insert("a".into(), "1".into());
        m.insert("sign".into(), "SKIP".into());
        m.insert("sign_type".into(), "RSA2".into());
        m.insert("z".into(), "2".into());
        assert_eq!(build_verify_content(&m), "a=1&z=2");
    }

    #[test]
    fn new_rejects_incomplete_credentials() {
        // 缺 app_id
        let cfg = PaymentConfig {
            enabled: true,
            provider: "alipay".into(),
            ..Default::default()
        };
        assert!(Alipay::new(cfg).is_err());
        // 有 app_id 但缺私钥与公钥
        let cfg = PaymentConfig {
            enabled: true,
            provider: "alipay".into(),
            app_id: "2021000000000000".into(),
            ..Default::default()
        };
        assert!(Alipay::new(cfg).is_err());
    }

    #[tokio::test]
    async fn pay_url_defaults_to_sandbox_and_carries_verifiable_sign() {
        let cfg = test_config();
        let p = Alipay::new(cfg).unwrap();
        let order = p.create_order(sample_order_request(1990)).await.unwrap();

        // 默认必须是沙箱网关:配错凭据时不至于真实扣款。
        let url = order.pay_url.expect("应返回收银台 URL");
        assert!(
            url.starts_with("https://openapi-sandbox.dl.alipaydev.com/gateway.do?"),
            "默认应走沙箱网关,实际: {url}"
        );
        assert_eq!(order.status, "pending");
        assert_eq!(order.amount_cents, 1990);

        // 还原 pay_url 参数,用支付宝公钥复算签名:签出来的东西必须能被自己验过。
        let query = url.split_once('?').expect("pay_url 缺少查询串").1;
        let pairs = parse_form_body(query);
        assert_eq!(pairs.get("method").map(String::as_str), Some("alipay.trade.page.pay"));
        assert_eq!(pairs.get("sign_type").map(String::as_str), Some("RSA2"));
        assert!(pairs.get("biz_content").unwrap().contains("19.90"));

        let sign = pairs.get("sign").expect("缺少 sign");
        let signed_map: BTreeMap<String, String> = pairs
            .iter()
            .filter(|(k, _)| k.as_str() != "sign")
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        let content = build_sign_content(&signed_map);
        let raw = base64::engine::general_purpose::STANDARD
            .decode(sign.trim())
            .expect("sign 应为合法 base64");
        let sig =
            rsa::pkcs1v15::Signature::try_from(raw.as_slice()).expect("sign 应为合法 RSA 签名");
        p.verifying_key
            .verify(content.as_bytes(), &sig)
            .expect("下单签名应能通过支付宝公钥验签");
    }

    #[tokio::test]
    async fn create_order_rejects_non_positive_amount() {
        let p = Alipay::new(test_config()).unwrap();
        let err = p.create_order(sample_order_request(0)).await.unwrap_err();
        assert!(err.to_string().contains("金额必须大于 0"));
    }

    #[tokio::test]
    async fn notify_verifies_when_intact_and_rejects_when_tampered() {
        let p = Alipay::new(test_config()).unwrap();
        let mut params = BTreeMap::new();
        params.insert("app_id".into(), "2021000000000000".into());
        params.insert("out_trade_no".into(), "RLY-TEST-0001".into());
        params.insert("trade_status".into(), "TRADE_SUCCESS".into());
        params.insert("total_amount".into(), "19.90".into());
        params.insert("trade_no".into(), "2026110122001001".into());
        params.insert("sign_type".into(), "RSA2".into());

        let body = sign_as_callback(&p, &params);
        let res = p
            .verify_notify(&HeaderMap::new(), &body)
            .await
            .expect("未被篡改的回调应验签通过");
        assert_eq!(res.order_id, "RLY-TEST-0001");
        assert!(res.paid, "TRADE_SUCCESS 应判定为已支付");
        assert_eq!(res.trade_no.as_deref(), Some("2026110122001001"));

        // 攻击者把 19.90 改成 0.01 却复用原签名 —— 必须被拦下。
        let tampered = body.replace("total_amount=19.90", "total_amount=0.01");
        let err = p
            .verify_notify(&HeaderMap::new(), &tampered)
            .await
            .expect_err("篡改后的回调必须验签失败");
        assert!(err.to_string().contains("验签失败"), "实际错误: {err}");
    }

    #[tokio::test]
    async fn notify_without_sign_is_rejected() {
        let p = Alipay::new(test_config()).unwrap();
        let err = p
            .verify_notify(&HeaderMap::new(), "out_trade_no=X&trade_status=TRADE_SUCCESS")
            .await
            .expect_err("缺少 sign 必须被拒绝");
        assert!(err.to_string().contains("缺少 sign"), "实际错误: {err}");
    }
}
