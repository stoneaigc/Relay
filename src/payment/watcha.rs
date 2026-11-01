//! 爱猹收(watcha / pay.watcha.cn)渠道实现。
//!
//! 与碳基圈(一人付)同属支付宝 Vibe Pay「智能收款套件(即刻收)」的合作平台,
//! 资金同样「用户账户 → 创作者收款账户、**不经过平台账户、不代收不结算不分账**」,
//! 底层费率也一样(**JSAPI 支付 0.6%~1% + 即刻收商品经营 0.4%**)。差别全在工程侧:
//!
//! - **有真正的沙箱**:沙箱下单不产生真实扣款,这是它相对碳基圈最大的优势。
//!   (对应的代价:沙箱与正式的 API Key 和 entitlement_id 不通用,必须分别配置。)
//! - **没有异步回调**:这是最重要的一点。传统"下单 → 等情况方回调 → 发货"的链路
//!   在这里不存在,改由我们在用户付款返回后**主动向服务端查一次权益**确认到账。
//!   因此本实现的 [`PaymentProvider::verify_notify`] 直接返回明确错误,
//!   真正的确认入口是 [`PaymentProvider::check_paid`]。
//!   少了回调也就少了一堆约束:不需要注册回调地址、不需要公网 HTTPS 443、
//!   不需要同域、不需要验签,境外部署的自托管应用不必为此额外准备任何东西。
//! - **金额校验能力更弱**:`/v1/entitlements/access` 只回答"有没有权益",不回价格。
//!   所以这里的做法是**下单前就把 plan → entitlement 映射锁死**(缺映射拒绝下单),
//!   金额一律以本地订单表为准。
//! - 公开文档已知的限制:商品档位金额**默认上限 ¥1000**(更高需提额申请);
//!   签约初期存在**每日收款限额**(通常 T+1 解除,接口无法查询)。
//!
//! # 安全约束
//! 与其他渠道同一条底线:**Key 缺失或为空一律拒绝**。另外 `user_id` 必须是服务端
//! 身份系统中的稳定主键,绝不能信任客户端传入的用户标识——否则等于允许任意用户
//! "查到"别人的权益状态,并据此给自己的订单发货。

use std::future::Future;
use std::pin::Pin;
use std::time::Duration;

use anyhow::{anyhow, bail, Context};
use axum::http::HeaderMap;
use serde_json::{json, Value};

use super::{
    NotifyResult, PaymentCheck, PaymentOrder, PaymentOrderRequest, PaymentProvider,
};
use crate::config::PaymentConfig;

/// 爱猹收渠道。
pub struct Watcha {
    cfg: PaymentConfig,
    api_key: String,
    http: reqwest::Client,
}

impl Watcha {
    /// 从配置构造渠道。**凭据不完整时返回 Err**,由调用方回落到不可用的桩实现。
    pub fn new(cfg: PaymentConfig) -> anyhow::Result<Self> {
        let api_key = cfg.read_watcha_api_key()?;
        let env = if cfg.watcha_sandbox { "沙箱" } else { "正式" };
        // 该通道不需要回调地址,这里提醒的是别把别的渠道的习惯带过来:
        // 如果有人照着 carbon 圈的写法去 /payments/notify/watcha 等回调,会永远等不到。
        tracing::warn!(
            "[payment] 爱猹收通道当前为**{env}环境**且**无异步回调**:\
             到账必须由门户调用 /portal/payments/orders/:id/confirm 主动确认"
        );
        if cfg.watcha_entitlement_for(None).is_none()
            && cfg.watcha_entitlements.is_empty()
            && cfg.watcha_sandbox_entitlements.is_empty()
        {
            tracing::warn!(
                "[payment] 爱猹收未配置任何权益映射([payment.watcha_entitlements] / \
                 watcha_sandbox_entitlements),所有套餐都会拒绝下单"
            );
        }
        let http = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(5))
            .timeout(Duration::from_secs(cfg.watcha_timeout_secs.max(1)))
            .build()
            .context("构造爱猹收 HTTP 客户端失败")?;
        Ok(Self {
            cfg,
            api_key,
            http,
        })
    }

    /// 调用服务端 API。非 2xx 时把服务端返回的 `error.code` / `error.message` 一并带出。
    async fn post_json(&self, path: &str, payload: &Value) -> anyhow::Result<Value> {
        let url = format!("{}{}", self.cfg.watcha_api_base(), path);
        let resp = self
            .http
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .json(payload)
            .send()
            .await
            .with_context(|| format!("请求爱猹收 {path} 失败"))?;
        let status = resp.status();
        let text = resp
            .text()
            .await
            .with_context(|| format!("读取爱猹收 {path} 响应失败"))?;
        let v: Value = serde_json::from_str(&text)
            .unwrap_or_else(|_| json!({ "raw": text, "error": { "message": "响应不是合法 JSON" } }));
        if !status.is_success() {
            let detail = v
                .pointer("/error/message")
                .and_then(|m| m.as_str())
                .or_else(|| v.pointer("/message").and_then(|m| m.as_str()))
                .unwrap_or(&text);
            let code = v
                .pointer("/error/code")
                .and_then(|c| c.as_str())
                .unwrap_or("");
            bail!("爱猹收 {path} 返回 {status} {code}: {detail}");
        }
        Ok(v)
    }

    /// 查询某个用户对某个权益的当前状态。
    ///
    /// 这是本通道唯一的"支付结果真相来源":没有回调可依赖,只能问服务端。
    async fn entitlements_access(
        &self,
        entitlement_id: &str,
        user_id: &str,
        return_url: Option<&str>,
    ) -> anyhow::Result<Value> {
        let mut payload = json!({
            "entitlement_id": entitlement_id,
            "user_id": user_id,
        });
        if let Some(u) = return_url {
            payload["return_url"] = json!(u);
        }
        let v = self.post_json("/v1/entitlements/access", &payload).await?;
        tracing::info!(
            "[payment] 爱猹收权益查询 entitlement={entitlement_id} user={user_id} 响应={v}"
        );
        Ok(v)
    }

    /// 判断权益状态是否已生效。
    fn is_granted(resp: &Value) -> bool {
        matches!(
            resp.get("access").and_then(|a| a.as_str()),
            Some("granted")
        )
    }

    /// 取购买链接。
    fn purchase_url(resp: &Value) -> Option<String> {
        resp.pointer("/purchase/url")
            .and_then(|u| u.as_str())
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string())
    }
}

impl PaymentProvider for Watcha {
    fn name(&self) -> &'static str {
        "watcha"
    }

    fn create_order(
        &self,
        req: PaymentOrderRequest,
    ) -> Pin<Box<dyn Future<Output = anyhow::Result<PaymentOrder>> + Send + '_>> {
        Box::pin(async move {
            if req.amount_cents <= 0 {
                bail!("下单金额必须大于 0:amount_cents={}", req.amount_cents);
            }
            // 单档金额上限:官方默认每个档位最高 ¥1,000,且**不会随 T+1 自动解除**,
            // 必须单独提交「档位额度提升」。超限档位要到用户付款那一步才被渠道拒绝,
            // 排查成本很高,所以在本侧先拦一道。提额获批后调高 `watcha_tier_cap_cents`。
            if req.amount_cents > self.cfg.watcha_tier_cap_cents {
                bail!(
                    "套餐金额 {:.2} 元超出爱猹收单档上限 {:.2} 元:\
                     该上限针对**单个档位标价**(非单日总额、非单用户累计),且不会自动解除,\
                     需向平台提交「档位额度提升」反馈;获批后把 [payment] watcha_tier_cap_cents \
                     调整为实际获批金额",
                    req.amount_cents as f64 / 100.0,
                    self.cfg.watcha_tier_cap_cents as f64 / 100.0,
                );
            }
            // 这里有意识地**不**拿 amount_cents 去和渠道侧比价:该接口不回价格,
            // 无从比对。替代防线是"缺映射就拒单"(见下),把错配挡在收银台之前。
            let entitlement = self.cfg.watcha_entitlement_for(req.plan_id.as_deref()).ok_or_else(|| {
                anyhow!(
                    "套餐 {:?} 未配置爱猹收权益 ID:请在 [payment.watcha_entitlements] \
                     (沙箱用 watcha_sandbox_entitlements)中映射 plan_id -> entitlement_id, \
                     或设置 payment.watcha_entitlement_id 作为默认权益",
                    req.plan_id
                )
            })?;

            let resp = self
                .entitlements_access(
                    &entitlement,
                    &req.user_id,
                    self.cfg.watcha_return_url().as_deref(),
                )
                .await?;

            // 已经买过(买断/时长未过期)时服务端不再给购买链接。这时若还返回 pay_url,
            // 用户会重复付款——明确报错让上层提示,而不是"静静地再生一单"。
            let pay_url = Self::purchase_url(&resp);
            if pay_url.is_none() {
                if Self::is_granted(&resp) {
                    bail!("该用户已持有该权益且在有效期内,无需再次购买");
                }
                bail!(
                    "爱猹收未返回购买链接:可能是商品档位未配置、处于审核中,或商品已下架\
                     (详见服务端返回的权益状态)"
                );
            }

            // 注意 order_id 是我们自己的单号,渠道侧没有"订单"这个概念——
            // 它以"用户 + 权益"来标识一次持有关系。发货仍以本地订单表为准。
            let order_id = format!("RLY{}", uuid::Uuid::new_v4().simple());
            Ok(PaymentOrder {
                order_id,
                provider: self.name().to_string(),
                amount_cents: req.amount_cents,
                status: "pending".to_string(),
                pay_url,
            })
        })
    }

    /// 该通道**没有异步回调**,永远收不到渠道推送。
    ///
    /// 这里不返回"验签失败"而是返回一段说明:不少运维同学会习惯性地去
    /// `/payments/notify/watcha` 等事件,等不到又以为是签名错了。
    fn verify_notify(
        &self,
        _headers: &HeaderMap,
        _body: &str,
    ) -> Pin<Box<dyn Future<Output = anyhow::Result<NotifyResult>> + Send + '_>> {
        Box::pin(async move {
            Err(anyhow!(
                "爱猹收通道无异步回调:请在用户付款返回后调用 \
                 POST /portal/payments/orders/:id/confirm 主动确认权益"
            ))
        })
    }

    /// 主动到服务端确认用户是否已经持有该权益。
    fn check_paid(
        &self,
        req: PaymentCheck,
    ) -> Pin<Box<dyn Future<Output = anyhow::Result<bool>> + Send + '_>> {
        Box::pin(async move {
            let entitlement = self
                .cfg
                .watcha_entitlement_for(req.plan_id.as_deref())
                .ok_or_else(|| {
                    anyhow!(
                        "套餐 {:?} 未配置爱猹收权益 ID,无法确认到账",
                        req.plan_id
                    )
                })?;
            tracing::info!(
                "[payment] 爱猹收到账确认 order_id={} user={} plan={:?} amount_cents={} entitlement={}",
                req.order_id, req.user_id, req.plan_id, req.amount_cents, entitlement
            );
            let resp = self
                .entitlements_access(&entitlement, &req.user_id, None)
                .await?;
            // 时长权益若在有效期内,服务端给的 access 就是 granted;
            // 过期后回到 purchase_required,此时如果还在调 confirm,说明用户确实没付款。
            Ok(Self::is_granted(&resp))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// 只有 `access == "granted"` 才算已持有。其余一律视为未付款。
    ///
    /// 这条边界决定了是否会发货:把 `purchase_required` 误判成 granted 等于白送套餐。
    #[test]
    fn granted_only_when_access_is_granted() {
        assert!(Watcha::is_granted(&json!({ "access": "granted" })));
        assert!(!Watcha::is_granted(&json!({ "access": "purchase_required" })));
        assert!(!Watcha::is_granted(&json!({ "access": "unavailable" })));
        assert!(!Watcha::is_granted(&json!({})));
        // 额度包 `granted` 会同时返回 remaining,不影响判定。
        assert!(Watcha::is_granted(&json!({
            "access": "granted",
            "entitlement": { "type": "quota", "remaining": 9 }
        })));
    }

    /// 购买链接可能为 HTTP(S) 也可能是 alipays://  scheme,
    /// 不能按 https 过滤,也不能返回空串。
    #[test]
    fn purchase_url_accepts_https_and_alipays_scheme() {
        assert_eq!(
            Watcha::purchase_url(&json!({ "purchase": { "url": "https://x.example/pay" } })),
            Some("https://x.example/pay".into())
        );
        assert_eq!(
            Watcha::purchase_url(&json!({
                "purchase": { "url": "alipays://platformapi/startapp?x=1" }
            })),
            Some("alipays://platformapi/startapp?x=1".into())
        );
        // 未购买/已下架时不返回 purchase,必须得到 None 而不是 panic。
        assert_eq!(Watcha::purchase_url(&json!({ "access": "granted" })), None);
        assert_eq!(Watcha::purchase_url(&json!({ "purchase": { "url": "" } })), None);
    }

    /// 单档金额上限默认 ¥1,000:卡的是**单个档位标价**,不是单日总额、也不是
    /// 单个买家累计,且**不会随 T+1 自动解除**。超限必须在本地就挡掉。
    #[tokio::test]
    async fn order_above_tier_cap_is_rejected_before_charging() {
        let cfg = crate::config::PaymentConfig {
            watcha_sandbox: true,
            watcha_sandbox_api_key: "sk_test_key".into(),
            watcha_sandbox_entitlements: std::collections::HashMap::from([(
                "plan-demo".to_string(),
                "ent_demo".to_string(),
            )]),
            watcha_entitlement_id: "ent_demo".into(),
            ..Default::default()
        };
        let w = Watcha::new(cfg).expect("凭据齐全应可构造");
        let err = w
            .create_order(PaymentOrderRequest {
                user_id: "u1".into(),
                plan_id: Some("plan-pro".into()),
                tokens: 0,
                amount_cents: 200_000,
                remark: String::new(),
            })
            .await
            .expect_err("¥2000 档位应被拒绝");
        let msg = err.to_string();
        assert!(msg.contains("2000.00"), "应报出实际金额: {msg}");
        assert!(msg.contains("1000.00"), "应报出上限: {msg}");
        assert!(msg.contains("watcha_tier_cap_cents"), "应给出调整入口: {msg}");
    }

    /// 提额获批后抬高上限即可放行——不要把这当成永久封死的 ¥1,000。
    #[tokio::test]
    async fn raised_tier_cap_allows_higher_amount() {
        let cfg = crate::config::PaymentConfig {
            watcha_sandbox: true,
            watcha_sandbox_api_key: "sk_test_key".into(),
            watcha_entitlement_id: "ent_demo".into(),
            watcha_tier_cap_cents: 500_000,
            ..Default::default()
        };
        let w = Watcha::new(cfg).expect("凭据齐全应可构造");
        let err = w
            .create_order(PaymentOrderRequest {
                user_id: "u1".into(),
                plan_id: Some("plan-pro".into()),
                tokens: 0,
                amount_cents: 200_000,
                remark: String::new(),
            })
            .await
            .expect_err("本地无 HTTP mock,必然失败于渠道调用");
        assert!(
            !err.to_string().contains("超出爱猹收单档上限"),
            "抬高上限后不应再报金额超限: {err}"
        );
    }

    /// 边界:正好等于上限应放行(上限是含在内的)。
    #[tokio::test]
    async fn amount_exactly_at_cap_passes_the_guard() {
        let cfg = crate::config::PaymentConfig {
            watcha_sandbox: true,
            watcha_sandbox_api_key: "sk_test_key".into(),
            watcha_sandbox_entitlements: std::collections::HashMap::from([(
                "plan-demo".to_string(),
                "ent_demo".to_string(),
            )]),
            watcha_entitlement_id: "ent_demo".into(),
            ..Default::default()
        };
        let w = Watcha::new(cfg).expect("凭据齐全应可构造");
        let err = w
            .create_order(PaymentOrderRequest {
                user_id: "u1".into(),
                plan_id: Some("plan-basic".into()),
                tokens: 0,
                amount_cents: 100_000,
                remark: String::new(),
            })
            .await
            .expect_err("本地无 HTTP mock,必然失败于渠道调用");
        assert!(
            !err.to_string().contains("超出爱猹收单档上限"),
            "等于上限应放行: {err}"
        );
    }
}
