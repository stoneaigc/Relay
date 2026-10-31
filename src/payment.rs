//! 支付对接扩展点(预留,当前未接入真实渠道)。
//!
//! 设计意图:套餐/余额的"支付购买"链路只差渠道实现——路由、参数、错误语义已就位。
//! 后续接入(微信支付 / 支付宝 / Stripe 等)时:
//! 1. 为新渠道实现 [`PaymentProvider`] trait(下单 / 回调验签);
//! 2. 在 [`provider`] 的 match 里注册该渠道;
//! 3. config `[payment]` 段填 `enabled = true` 与渠道凭据。
//! 端点层(`POST /admin/api/payments/orders`、`POST /payments/notify/:provider`)
//! 无需再改动,未启用时统一返回 501 语义的明确错误。

use std::future::Future;
use std::pin::Pin;

use serde::Serialize;

/// 一笔充值下单请求(管理端或门户发起)。
#[derive(Debug, Clone, Serialize)]
pub struct PaymentOrderRequest {
    pub user_id: String,
    /// 充值到套餐时为 Some(plan_id);充主余额时 None。
    pub plan_id: Option<String>,
    /// 购买的 token 数(渠道侧不感知,仅落业务订单)。
    pub tokens: i64,
    /// 应付金额(分)。
    pub amount_cents: i64,
    pub remark: String,
}

/// 渠道下单结果。
#[derive(Debug, Clone, Serialize)]
pub struct PaymentOrder {
    pub order_id: String,
    pub provider: String,
    pub amount_cents: i64,
    /// pending = 待支付;接入真实渠道后在此返回拉起支付所需的凭据/链接。
    pub status: String,
    pub pay_url: Option<String>,
}

/// 支付渠道抽象。
pub trait PaymentProvider: Send + Sync {
    /// 渠道标识(如 "wechatpay" / "alipay");当前仅测试断言使用。
    #[allow(dead_code)]
    fn name(&self) -> &'static str;
    /// 创建支付单。Err 时端点把错误透出给调用方。
    fn create_order(
        &self,
        req: PaymentOrderRequest,
    ) -> Pin<Box<dyn Future<Output = anyhow::Result<PaymentOrder>> + Send + '_>>;
    /// 验签并解析渠道异步回调,返回 (渠道订单号, 是否支付成功)。
    fn verify_notify(
        &self,
        headers: &axum::http::HeaderMap,
        body: &str,
    ) -> Pin<Box<dyn Future<Output = anyhow::Result<(String, bool)>> + Send + '_>>;
}

/// 未接入渠道时的桩实现:所有操作返回明确错误,端点层据此回 501。
pub struct UnconfiguredProvider;

const UNCONFIGURED_MSG: &str =
    "支付渠道未接入:请在 config [payment] 段启用并实现 PaymentProvider 后注册渠道";

impl PaymentProvider for UnconfiguredProvider {
    fn name(&self) -> &'static str {
        "none"
    }

    fn create_order(
        &self,
        _req: PaymentOrderRequest,
    ) -> Pin<Box<dyn Future<Output = anyhow::Result<PaymentOrder>> + Send + '_>> {
        Box::pin(async move { Err(anyhow::anyhow!("{UNCONFIGURED_MSG}")) })
    }

    fn verify_notify(
        &self,
        _headers: &axum::http::HeaderMap,
        _body: &str,
    ) -> Pin<Box<dyn Future<Output = anyhow::Result<(String, bool)>> + Send + '_>> {
        Box::pin(async move { Err(anyhow::anyhow!("{UNCONFIGURED_MSG}")) })
    }
}

/// 按配置选择支付渠道。未启用或渠道名未注册时回落 [`UnconfiguredProvider`]。
pub fn provider(cfg: &crate::config::PaymentConfig) -> Box<dyn PaymentProvider> {
    if !cfg.enabled {
        return Box::new(UnconfiguredProvider);
    }
    match cfg.provider.as_str() {
        // 后续在此注册真实渠道,例如:
        // "wechatpay" => Box::new(wechatpay::WechatPay::new(cfg)),
        // "alipay"    => Box::new(alipay::Alipay::new(cfg)),
        _ => Box::new(UnconfiguredProvider),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn unconfigured_provider_rejects_order() {
        let p = UnconfiguredProvider;
        let req = PaymentOrderRequest {
            user_id: "u1".into(),
            plan_id: None,
            tokens: 1000,
            amount_cents: 100,
            remark: String::new(),
        };
        let err = p.create_order(req).await.unwrap_err().to_string();
        assert!(err.contains("支付渠道未接入"), "{err}");
    }

    #[tokio::test]
    async fn disabled_config_falls_back_to_unconfigured() {
        let cfg = crate::config::PaymentConfig::default();
        assert!(!cfg.enabled);
        let p = provider(&cfg);
        assert_eq!(p.name(), "none");
    }

    #[tokio::test]
    async fn unknown_provider_name_falls_back_to_unconfigured() {
        let cfg = crate::config::PaymentConfig {
            enabled: true,
            provider: "not-a-channel".into(),
            notify_url: String::new(),
        };
        let p = provider(&cfg);
        assert_eq!(p.name(), "none");
    }
}
