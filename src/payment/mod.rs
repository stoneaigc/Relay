//! 支付对接的渠道抽象层。
//!
//! [`PaymentProvider`] 是与具体渠道无关的契约:上层只用它下单和验签,
//! 换渠道等于换一个实现,路由、订单、套餐发放逻辑完全不用动。
//!
//! 已接入渠道:
//!
//! - [`alipay`] —— 支付宝「电脑网站支付」(`alipay.trade.page.pay`),RSA2 签名,有沙箱。
//! - [`opcpay`] —— 碳基圈(一人付),支付宝合作服务商通道,HMAC-SHA256 验签,**无沙箱**。
//! - [`watcha`] —— 爱猹收(观猹旗下),同为即刻收合作平台,**有沙箱、且没有异步回调**:
//!   到账靠主动查询权益确认([`PaymentProvider::check_paid`]),因此不需要回调地址。
//!
//! 三者可以共存于同一份代码,切换只改 `[payment].provider` 一个配置项。
//!
//! 设计意图:套餐/余额的"支付购买"链路只差渠道实现——路由、参数、错误语义已就位。
//! 后续接入(微信支付 / 支付宝 / Stripe 等)时:
//! 1. 为新渠道实现 [`PaymentProvider`] trait(下单 / 回调验签);
//! 2. 在 [`provider`] 的 match 里注册该渠道;
//! 3. config `[payment]` 段填 `enabled = true` 与渠道凭据。
//! 端点层(`POST /admin/api/payments/orders`、`POST /payments/notify/:provider`)
//! 无需再改动,未启用时统一返回 501 语义的明确错误。

pub mod alipay;
pub mod opcpay;
pub mod watcha;

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

/// 回调验签的结论。
///
/// 之所以不是简单的 `(order_id, paid)`:不同渠道在回调里能给出的信息并不一样。
/// 支付宝明确带 `out_trade_no`(就是下单号)和 `trade_no`;而一人付的权益事件
/// 主体是「用户」而不是「订单」,可能只带 `user_id`。少了 `channel_user_id`,
/// 单号缺失的那类回调就无法反查待发货订单。
#[derive(Debug, Clone)]
pub struct NotifyResult {
    /// 渠道侧订单号(支付宝 `out_trade_no` / 一人付 `idempotency_key`);可能为空。
    pub order_id: String,
    /// 是否确认已支付。**发货前必须再过一遍订单表的幂等闸门**。
    pub paid: bool,
    /// 渠道侧用户标识,用于单号缺失时反查该用户最近的待支付订单。
    pub channel_user_id: Option<String>,
    /// 渠道交易号(支付宝 `trade_no`),落库用于对账。
    pub trade_no: Option<String>,
    /// 渠道回传的实付金额(分)。渠道不回传时为 None(一人付的金额由 SKU 决定)。
    ///
    /// 上层**必须**拿它与本地订单金额比对后再发货:只验签不看金额,等于允许
    /// "签一份 1 分钱的订单、却让渠道按 199 元发货"这类篡改。
    pub paid_amount_cents: Option<i64>,
}

/// 主动查询某笔订单是否已付款的请求。
///
/// 存在的原因是**不是所有渠道都有异步回调**:爱猹收这类"权益式"支付根本不推送
/// 支付结果,只能由我们拿着用户标识去服务端问。把它放进 trait 而不是给某个实现
/// 加特殊方法,是为了让端点层不用 `match` 渠道类型。
#[derive(Debug, Clone)]
pub struct PaymentCheck {
    pub order_id: String,
    pub user_id: String,
    pub plan_id: Option<String>,
    pub amount_cents: i64,
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
    /// 验签并解析渠道异步回调。
    fn verify_notify(
        &self,
        headers: &axum::http::HeaderMap,
        body: &str,
    ) -> Pin<Box<dyn Future<Output = anyhow::Result<NotifyResult>> + Send + '_>>;

    /// 主动到渠道侧确认某笔订单是否已付款。
    ///
    /// 默认实现表示"本渠道不支持":带异步回调的渠道用回调就够了,不必实现它。
    /// 仅**没有回调**的权益式渠道(爱猹收)需要重写。
    fn check_paid(
        &self,
        req: PaymentCheck,
    ) -> Pin<Box<dyn Future<Output = anyhow::Result<bool>> + Send + '_>> {
        let provider = self.name();
        let _ = req;
        Box::pin(async move {
            Err(anyhow::anyhow!(
                "渠道 {provider} 不支持主动确认支付结果,请使用其异步回调"
            ))
        })
    }
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
    ) -> Pin<Box<dyn Future<Output = anyhow::Result<NotifyResult>> + Send + '_>> {
        Box::pin(async move { Err(anyhow::anyhow!("{UNCONFIGURED_MSG}")) })
    }
}

/// 按配置选择支付渠道。未启用或渠道名未注册时回落 [`UnconfiguredProvider`]。
pub fn provider(cfg: &crate::config::PaymentConfig) -> Box<dyn PaymentProvider> {
    if !cfg.enabled {
        return Box::new(UnconfiguredProvider);
    }
    match normalize_provider(&cfg.provider) {
        // 后续在此注册更多渠道,例如:
        // "wechatpay" => ...,
        "alipay" => init_or_degrade("支付宝", || alipay::Alipay::new(cfg.clone()).map(|p| Box::new(p) as Box<dyn PaymentProvider>)),
        "opcpay" => init_or_degrade("碳基圈(一人付)", || {
            opcpay::Opcpay::new(cfg.clone()).map(|p| Box::new(p) as Box<dyn PaymentProvider>)
        }),
        "watcha" => init_or_degrade("爱猹收", || {
            watcha::Watcha::new(cfg.clone()).map(|p| Box::new(p) as Box<dyn PaymentProvider>)
        }),
        _ => Box::new(UnconfiguredProvider),
    }
}

/// 归一化渠道标识:把同一家服务的多种写法收敛成一个。
///
/// 这不是洁癖——回调端点是 `/payments/notify/:provider`,而配置里的 `provider` 由人填写。
/// 若配置写 `yirenfu`、回调路径却是 `opcpay`,直接字符串比较会把合法回调判成
/// "渠道未启用" 而拒收,用户表现为"付了钱不到账"。归一化让两种写法都能对上。
/// 元字符串转分,用于把渠道回传的价格与本地 `amount_cents` 比对。
///
/// 全程整数运算,不碰浮点。解析不了时返回 `None`,交给上层按"渠道未回传金额"
/// 处理——**绝不默认成 0**,否则金额比对形同虚设。
pub(crate) fn yuan_to_fen(yuan: &str) -> Option<i64> {
    let s = yuan.trim();
    if s.is_empty() {
        return None;
    }
    let (sign, body) = match s.strip_prefix('-') {
        Some(b) => (-1i64, b),
        None => (1i64, s),
    };
    let mut parts = body.splitn(2, '.');
    let int_part = parts.next()?;
    let frac_part = parts.next().unwrap_or("");
    if int_part.is_empty()
        || !int_part.bytes().all(|b| b.is_ascii_digit())
        || !frac_part.bytes().all(|b| b.is_ascii_digit())
        || frac_part.len() > 2
    {
        return None;
    }
    let whole: i64 = int_part.parse().ok()?;
    let frac: i64 = if frac_part.is_empty() {
        0
    } else {
        // "9" 补成 "90","09" 视为 9。
        let mut f = frac_part.to_string();
        while f.len() < 2 {
            f.push('0');
        }
        f.parse().ok()?
    };
    Some(sign * (whole * 100 + frac))
}

pub fn normalize_provider(name: &str) -> &str {
    match name.trim() {
        "yirenfu" | "carbon" | "carbonbased" | "一人付" => "opcpay",
        "aichashou" | "爱猹收" | "观猹" | "guan cha" => "watcha",
        other => other,
    }
}

/// 渠道初始化失败时**整体降级为不可用**,而不是带病放行。
///
/// 凭据不完整(尤其密钥为空)时,空串往往也能算出「合法」签名,等于敞开门让伪造回调
/// 直接充值——历史上已有同类项目因此出过可伪造充值的高危漏洞。因此这里统一记 error
/// 并回落到 [`UnconfiguredProvider`],让端点回 501,而不是让一个验不了签的渠道上线。
fn init_or_degrade(
    label: &str,
    init: impl FnOnce() -> anyhow::Result<Box<dyn PaymentProvider>>,
) -> Box<dyn PaymentProvider> {
    match init() {
        Ok(p) => p,
        Err(e) => {
            tracing::error!("[payment] {label} 渠道初始化失败,支付端点将不可用: {e}");
            Box::new(UnconfiguredProvider)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

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
            ..Default::default()
        };
        let p = provider(&cfg);
        assert_eq!(p.name(), "none");
    }

    #[test]
    fn normalize_provider_maps_aliases_to_canonical_name() {
        for (input, expected) in [
            ("opcpay", "opcpay"),
            ("yirenfu", "opcpay"),
            ("carbon", "opcpay"),
            ("carbonbased", "opcpay"),
            ("  yirenfu  ", "opcpay"),
            ("alipay", "alipay"),
            ("not-a-channel", "not-a-channel"),
        ] {
            assert_eq!(normalize_provider(input), expected, "input=`{input}`");
        }
    }

    #[test]
    fn notify_path_alias_matches_config_alias() {
        // 回归:配置写 yirenfu、回调路径是 opcpay,必须判为同一渠道。
        // 否则回调被当成未启用拒收,用户表现为付了钱却不到账。
        assert_eq!(normalize_provider("yirenfu"), normalize_provider("opcpay"));
    }

    #[tokio::test]
    async fn opcpay_registers_under_all_aliases() {
        // 同一家服务在文档里出现过多种叫法,配置时写哪个都该生效。
        for name in ["opcpay", "yirenfu", "carbon"] {
            let cfg = crate::config::PaymentConfig {
                enabled: true,
                provider: name.into(),
                opcpay_api_key: "test_key".into(),
                opcpay_webhook_secret: "test_secret".into(),
                ..Default::default()
            };
            let p = provider(&cfg);
            assert_eq!(p.name(), "opcpay", "渠道名 `{name}` 应注册到 opcpay");
        }
    }

    #[tokio::test]
    async fn incomplete_opcpay_credentials_degrade_to_unconfigured() {
        let cfg = crate::config::PaymentConfig {
            enabled: true,
            provider: "opcpay".into(),
            ..Default::default()
        };
        assert_eq!(
            provider(&cfg).name(),
            "none",
            "凭据不全必须整体降级,不能上线一个验不了签的渠道"
        );
    }

    #[tokio::test]
    async fn watcha_registers_under_all_aliases() {
        for name in ["watcha", "aichashou", "爱猹收", "观猹"] {
            let cfg = crate::config::PaymentConfig {
                enabled: true,
                provider: name.into(),
                watcha_sandbox_api_key: "sandbox_key".into(),
                watcha_api_key: "live_key".into(),
                ..Default::default()
            };
            let p = provider(&cfg);
            assert_eq!(p.name(), "watcha", "渠道名 `{name}` 应注册到 watcha");
        }
    }

    #[tokio::test]
    async fn watcha_requires_the_key_of_the_active_environment() {
        // 回归:沙箱默认开启,只配了正式 Key 时必须降级。
        // 否则会拿着空 Key 去请求 —— 服务端回 401,而排查时会误判成"权益不存在"。
        let cfg = crate::config::PaymentConfig {
            enabled: true,
            provider: "watcha".into(),
            watcha_api_key: "live_key".into(),
            watcha_sandbox: true,
            ..Default::default()
        };
        assert_eq!(provider(&cfg).name(), "none");

        let cfg = crate::config::PaymentConfig {
            watcha_sandbox_api_key: "sandbox_key".into(),
            ..cfg
        };
        assert_eq!(provider(&cfg).name(), "watcha");
    }

    #[tokio::test]
    async fn channels_without_callback_reject_verify_notify_but_support_check_paid() {
        let cfg = crate::config::PaymentConfig {
            enabled: true,
            provider: "watcha".into(),
            watcha_sandbox_api_key: "k".into(),
            watcha_sandbox_entitlements: HashMap::from([("plan-basic".into(), "ent_1".into())]),
            ..Default::default()
        };
        let p = provider(&cfg);
        let err = p
            .verify_notify(&axum::http::HeaderMap::new(), "{}")
            .await
            .unwrap_err()
            .to_string();
        assert!(
            err.contains("无异步回调") && err.contains("confirm"),
            "验签入口必须显式告知运维去走主动确认,而不是让对方干等: {err}"
        );
    }

    #[tokio::test]
    async fn check_paid_default_impl_rejects_for_callback_channels() {
        // 有回调的渠道不该被主动查询绕过:否则有人配错渠道时会得到"永远未付款"。
        let cfg = crate::config::PaymentConfig {
            enabled: true,
            provider: "opcpay".into(),
            opcpay_api_key: "k".into(),
            opcpay_webhook_secret: "s".into(),
            ..Default::default()
        };
        let p = provider(&cfg);
        let err = p
            .check_paid(PaymentCheck {
                order_id: "RLY1".into(),
                user_id: "u1".into(),
                plan_id: None,
                amount_cents: 100,
            })
            .await
            .unwrap_err()
            .to_string();
        assert!(err.contains("不支持主动确认"), "{err}");
    }
}
