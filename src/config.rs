use std::collections::HashMap;

use anyhow::Context;
use url::Url;

use figment::{
    providers::{Env, Format, Toml},
    Figment,
};
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    pub server: ServerConfig,
    pub database: DatabaseConfig,
    #[serde(default)]
    pub defaults: Defaults,
    #[serde(default)]
    pub auth: AuthConfig,
    #[serde(default)]
    pub admin: AdminConfig,
    #[serde(default)]
    pub email: EmailConfig,
    #[serde(default)]
    pub cache: CacheConfig,
    /// 语义缓存(响应缓存):L1 精确哈希 + TTL,L2 embedding 预留。
    #[serde(default)]
    pub cache_semantic: CacheSemanticConfig,
    /// L2 embedding 供应商(OpenAI 兼容 /embeddings);未配置时语义缓存自动只跑 L1。
    #[serde(default)]
    pub embedding: EmbeddingConfig,
    #[serde(default)]
    pub logging: LoggingConfig,
    /// Prometheus 指标导出(GET /metrics)配置。
    #[serde(default)]
    pub metrics: MetricsConfig,
    /// 数据面代理转发超时([proxy] 段)。
    #[serde(default)]
    pub proxy: ProxyConfig,
    /// 支付对接([payment] 段,预留扩展点)。未启用时支付端点返回 501 语义错误。
    #[serde(default)]
    pub payment: PaymentConfig,
}

/// 支付宝沙箱网关地址。默认走沙箱:凭据误配时不至于产生真实扣款。
const ALIPAY_SANDBOX_GATEWAY: &str = "https://openapi-sandbox.dl.alipaydev.com/gateway.do";
/// 支付宝生产网关地址。仅在 sandbox = false 时使用。
const ALIPAY_PROD_GATEWAY: &str = "https://openapi.alipay.com/gateway.do";

/// 支付对接配置([payment] 段)。
///
/// # 安全约定
/// 私钥与支付宝公钥一律通过环境变量注入,不要写进版本库中的 toml。figment 以
/// `RELAY_` 为前缀、`__` 为分隔符,把环境变量映射成本节字段:
/// ```text
/// RELAY_PAYMENT__APP_ID=202100xxxxxxxxxxxx
/// RELAY_PAYMENT__PRIVATE_KEY="-----BEGIN PRIVATE KEY-----..."
/// RELAY_PAYMENT__ALIPAY_PUBLIC_KEY="-----BEGIN PUBLIC KEY-----..."
/// ```
/// 也可以改填 `private_key_file` / `alipay_public_key_file` 指向文件路径,便于挂载 Secret 文件。
///
/// 碳基圈(一人付)通道同理,两个凭据只走环境变量:
/// ```text
/// RELAY_PAYMENT__OPCPAY_API_KEY=...
/// RELAY_PAYMENT__OPCPAY_WEBHOOK_SECRET=...
/// ```
/// 而 `opcpay_skus` 是 `plan_id -> sku_id` 的**映射表**,建议写在 toml 里(figment 对
/// 环境变量中的嵌套 map 支持有限),例如:
/// ```toml
/// [payment.opcpay_skus]
/// plan-basic = "sku_xxx"
/// plan-pro   = "sku_yyy"
/// ```
/// 该通道下套餐与价格在服务商侧配置,因此每个本地套餐必须显式对应到一个档位,
/// 否则用户会按错误的价格付款。
#[derive(Clone, Deserialize)]
pub struct PaymentConfig {
    /// 是否启用支付;false 时支付端点返回"渠道未接入"的明确错误。
    #[serde(default)]
    pub enabled: bool,
    /// 渠道标识:none = 未接入;后续可扩展 wechatpay / alipay 等。
    #[serde(default = "default_payment_provider")]
    pub provider: String,
    /// 支付回调通知地址(渠道侧需要时填写)。
    #[serde(default)]
    pub notify_url: String,
    /// 支付宝应用 AppID(开放平台创建应用后获取)。
    #[serde(default)]
    pub app_id: String,
    /// 商户应用私钥:内联 PEM 文本。留空时回退到 [`Self::private_key_file`]。
    #[serde(default)]
    pub private_key: String,
    /// 商户应用私钥文件路径,便于以文件形式挂载 Secret。
    #[serde(default)]
    pub private_key_file: String,
    /// 支付宝公钥:用于校验异步回调签名。留空时回退到 [`Self::alipay_public_key_file`]。
    #[serde(default)]
    pub alipay_public_key: String,
    /// 支付宝公钥文件路径。
    #[serde(default)]
    pub alipay_public_key_file: String,
    /// true = 沙箱环境。默认 true,配错凭据时不会真实扣款。
    #[serde(default = "default_sandbox")]
    pub sandbox: bool,
    /// 显式覆盖网关地址,一般留空即可。
    #[serde(default)]
    pub gateway: String,

    // ---- 碳基圈(一人付 / opcpay):支付宝合作服务商通道 ----
    /// API 基址。服务商可能调整,留空则用官方默认值。
    #[serde(default = "default_opcpay_api_base")]
    pub opcpay_api_base: String,
    /// 项目服务端凭证(请求头 `Authorization: Bearer ...`)。留空时回退到 [`Self::opcpay_api_key_file`]。
    #[serde(default)]
    pub opcpay_api_key: String,
    /// 项目凭证文件路径,便于以挂载 Secret 的方式注入。
    #[serde(default)]
    pub opcpay_api_key_file: String,
    /// Webhook 验签密钥(HMAC-SHA256)。留空时回退到 [`Self::opcpay_webhook_secret_file`]。
    #[serde(default)]
    pub opcpay_webhook_secret: String,
    /// Webhook 验签密钥文件路径。
    #[serde(default)]
    pub opcpay_webhook_secret_file: String,
    /// 默认收费档位 SKU:`opcpay_skus` 未覆盖的套餐回退到它。
    #[serde(default)]
    pub opcpay_sku_id: String,
    /// `plan_id -> sku_id` 映射。该通道下**套餐与价格在支付宝侧配置**,
    /// 因此每个本地套餐必须显式对应到服务商侧的档位,否则用户会按错误的价格付款。
    #[serde(default)]
    pub opcpay_skus: HashMap<String, String>,
    /// 下单与回调复核的 HTTP 超时(秒)。
    #[serde(default = "default_opcpay_timeout_secs")]
    pub opcpay_timeout_secs: u64,
    /// **项目官网地址**(必须与服务商控制台里登记的官网同域名)。
    ///
    /// 该通道的回调地址与页面返回地址都被硬性要求"与项目官网同域名的公网
    /// HTTPS 地址、仅 443 端口",所以必须知道官网域名才能正确推导/校验这两个地址。
    /// 境外服务器 + 境外域名同样满足(判定只看公网可达、不看备案),填 `https://你的域名` 即可。
    #[serde(default)]
    pub opcpay_site_url: String,
    /// 付款完成后的页面返回地址。留空时回落到 `{opcpay_site_url}/portal`。
    ///
    /// 注意它**只是页面跳转**,不是支付成功通知,不能用来发货。
    #[serde(default)]
    pub opcpay_return_url: String,

    // ---- 爱猹收(watcha):观猹旗下,同样是即刻收合作平台,但有沙箱且无需回调 ----
    /// API 基址。留空则用官方默认值 `https://pay.watcha.cn`。
    #[serde(default = "default_watcha_api_base")]
    pub watcha_api_base: String,
    /// true = 沙箱环境。默认 true。该通道**有真正的沙箱**(不产生真实扣款),
    /// 因此务必先在沙箱跑通整条链路再切正式。
    #[serde(default = "default_sandbox")]
    pub watcha_sandbox: bool,
    /// 服务端 API Key(请求头 `Authorization: Bearer ...`),**正式环境**专用。
    /// 留空时回退到 [`Self::watcha_api_key_file`]。
    #[serde(default)]
    pub watcha_api_key: String,
    /// 正式环境 API Key 文件路径。
    #[serde(default)]
    pub watcha_api_key_file: String,
    /// 沙箱环境 API Key。沙箱与正式的 Key 与权益 ID **不通用**,必须分别配置。
    #[serde(default)]
    pub watcha_sandbox_api_key: String,
    /// 沙箱 API Key 文件路径。
    #[serde(default)]
    pub watcha_sandbox_api_key_file: String,
    /// 默认权益 ID:`watcha_entitlements` 未覆盖的套餐回退到它。
    #[serde(default)]
    pub watcha_entitlement_id: String,
    /// `plan_id -> entitlement_id` 映射(**正式环境**)。
    ///
    /// 与碳基圈不同,这里映射的是「权益」而不是「档位」,价格仍在支付宝侧配置;
    /// 权益 ID 由 CREATE 商品时的响应 `entitlement.id` 给出,注意它不是商品 ID。
    #[serde(default)]
    pub watcha_entitlements: HashMap<String, String>,
    /// `plan_id -> entitlement_id` 映射(**沙箱环境**)。
    #[serde(default)]
    pub watcha_sandbox_entitlements: HashMap<String, String>,
    /// 付款完成后的页面返回地址(可选)。留空时渠道侧仍会给出购买链接,
    /// 只是用户付完不会自动跳回我们的门户。
    #[serde(default)]
    pub watcha_return_url: String,
    /// 下单与权益查询的 HTTP 超时(秒)。
    #[serde(default = "default_watcha_timeout_secs")]
    pub watcha_timeout_secs: u64,
    /// 单档金额上限(**分**),默认 ¥1,000 —— 与官方「商品档位金额上限」一致。
    ///
    /// 这个约束卡的是**单个档位的标价**,不是每日收款总额、也不是单个买家累计,
    /// 且**不会随时间自动解除**:官方要求提交「档位额度提升」反馈单独申请。
    /// 超限的档位在建商品时就会被拒,走到付款环节才暴露会很难排查,
    /// 所以这里在本侧先挡一道。若提额已获批,把它改成实际获批金额即可。
    #[serde(default = "default_watcha_tier_cap_cents")]
    pub watcha_tier_cap_cents: i64,
}

fn default_sandbox() -> bool {
    true
}

fn default_opcpay_api_base() -> String {
    "https://ai6666.com/opcpay/api/pay/v1".to_string()
}

fn default_watcha_api_base() -> String {
    "https://pay.watcha.cn".to_string()
}

fn default_watcha_timeout_secs() -> u64 {
    15
}

/// 爱猹收每个商品档位默认最高 ¥1,000(官方文档「商品档位金额上限」)。
fn default_watcha_tier_cap_cents() -> i64 {
    100_000
}

fn default_opcpay_timeout_secs() -> u64 {
    15
}

impl PaymentConfig {
    /// 当前生效的网关地址:显式 gateway > 沙箱/生产自动选择。
    pub fn gateway_url(&self) -> String {
        if !self.gateway.trim().is_empty() {
            return self.gateway.trim().to_string();
        }
        if self.sandbox {
            ALIPAY_SANDBOX_GATEWAY.to_string()
        } else {
            ALIPAY_PROD_GATEWAY.to_string()
        }
    }

    /// 读取商户私钥 PEM 文本。
    pub fn read_private_key(&self) -> anyhow::Result<String> {
        read_key_material(&self.private_key, &self.private_key_file, "商户私钥")
    }

    /// 读取支付宝公钥 PEM 文本。
    pub fn read_alipay_public_key(&self) -> anyhow::Result<String> {
        read_key_material(
            &self.alipay_public_key,
            &self.alipay_public_key_file,
            "支付宝公钥",
        )
    }

    /// 读取碳基圈项目凭证(内联优先,其次文件)。
    pub fn read_opcpay_api_key(&self) -> anyhow::Result<String> {
        let v = read_key_material(
            &self.opcpay_api_key,
            &self.opcpay_api_key_file,
            "碳基圈项目凭证",
        )?;
        Ok(v.trim().to_string())
    }

    /// 当前生效的爱猹收 API 基址(去掉结尾斜杠,便于拼接路径)。
    pub fn watcha_api_base(&self) -> String {
        let base = self.watcha_api_base.trim();
        if base.is_empty() {
            return default_watcha_api_base();
        }
        base.trim_end_matches('/').to_string()
    }

    /// 读取当前环境对应的爱猹收服务端 Key。
    ///
    /// 沙箱与正式的 Key **不通用**,这里按 `watcha_sandbox` 二选一,避免配错环境时
    /// 拿着沙箱 Key 去查正式权益(会拿到 401 而不是"权益不存在",极易误判)。
    pub fn read_watcha_api_key(&self) -> anyhow::Result<String> {
        let v = if self.watcha_sandbox {
            read_key_material(
                &self.watcha_sandbox_api_key,
                &self.watcha_sandbox_api_key_file,
                "爱猹收沙箱 API Key",
            )?
        } else {
            read_key_material(
                &self.watcha_api_key,
                &self.watcha_api_key_file,
                "爱猹收正式 API Key",
            )?
        };
        Ok(v.trim().to_string())
    }

    /// 取套餐对应的权益 ID:先查当前环境的映射表,未命中回退到默认权益 ID。
    ///
    /// 沙箱/正式各自一张表,因为两侧的 entitlement_id 是不同对象。都为空时返回 None,
    /// 由渠道层拒绝下单——宁可下单失败,也不能让用户跳到别的商品的收银台。
    pub fn watcha_entitlement_for(&self, plan_id: Option<&str>) -> Option<String> {
        if let Some(pid) = plan_id.map(str::trim).filter(|s| !s.is_empty()) {
            let table = if self.watcha_sandbox {
                &self.watcha_sandbox_entitlements
            } else {
                &self.watcha_entitlements
            };
            if let Some(id) = table.get(pid).map(|s| s.trim().to_string()) {
                if !id.is_empty() {
                    return Some(id);
                }
            }
        }
        let fallback = self.watcha_entitlement_id.trim();
        if fallback.is_empty() {
            None
        } else {
            Some(fallback.to_string())
        }
    }

    /// 付款完成后跳回门户的地址(可选)。
    pub fn watcha_return_url(&self) -> Option<String> {
        let u = self.watcha_return_url.trim();
        if u.is_empty() {
            None
        } else {
            Some(u.to_string())
        }
    }

    /// 读取碳基圈 Webhook 验签密钥。
    pub fn read_opcpay_webhook_secret(&self) -> anyhow::Result<String> {
        let v = read_key_material(
            &self.opcpay_webhook_secret,
            &self.opcpay_webhook_secret_file,
            "碳基圈 Webhook 密钥",
        )?;
        Ok(v.trim().to_string())
    }

    /// 取套餐对应的收费档位 SKU:先查 `opcpay_skus` 映射,未命中回退到默认 SKU。
    ///
    /// 都为空时返回 None,由渠道层拒绝下单——宁可下单失败,也不能让套餐 A 的购买者
    /// 跳到套餐 B 的收银台(价格与权益都不对)。
    pub fn opcpay_sku_for(&self, plan_id: Option<&str>) -> Option<String> {
        if let Some(pid) = plan_id.map(str::trim).filter(|s| !s.is_empty()) {
            if let Some(sku) = self.opcpay_skus.get(pid).map(|s| s.trim().to_string()) {
                if !sku.is_empty() {
                    return Some(sku);
                }
            }
        }
        let default_sku = self.opcpay_sku_id.trim();
        if default_sku.is_empty() {
            None
        } else {
            Some(default_sku.to_string())
        }
    }

    /// 当前生效的碳基圈 API 基址(去掉结尾斜杠,便于拼接路径)。
    pub fn opcpay_api_base(&self) -> String {
        let base = self.opcpay_api_base.trim();
        if base.is_empty() {
            return default_opcpay_api_base();
        }
        base.trim_end_matches('/').to_string()
    }

    /// 项目官网地址(去结尾斜杠)。未配置时返回空串,由调用方决定回落策略。
    pub fn opcpay_site_url(&self) -> String {
        self.opcpay_site_url.trim().trim_end_matches('/').to_string()
    }

    /// 由官网地址推导出的回调注册地址。官网未配置时返回 None。
    pub fn opcpay_webhook_url(&self) -> Option<String> {
        let site = self.opcpay_site_url();
        if site.is_empty() {
            return None;
        }
        Some(format!("{site}{OPCPAY_WEBHOOK_PATH}"))
    }

    /// 付款完成后的页面返回地址:优先取显式配置,否则回落到官网下的门户首页。
    pub fn opcpay_callback_url(&self) -> Option<String> {
        let explicit = self.opcpay_return_url.trim();
        if !explicit.is_empty() {
            return Some(explicit.to_string());
        }
        let site = self.opcpay_site_url();
        if site.is_empty() {
            return None;
        }
        Some(format!("{site}{OPCPAY_RETURN_PATH}"))
    }

    /// 校验准备提交给碳基圈的公网地址(回调地址 / 页面返回地址)。
    ///
    /// 服务端硬性限制(与 OpenAPI 中 `WebhookInput.url` 的描述逐条对齐):
    /// 与项目官网同域名的公网 HTTPS 地址、仅 443 端口、不含账号信息/查询参数/片段,
    /// 且不接受私网地址。这里在本地先跑一遍同样的规则,把服务商那句
    /// `400 INVALID_WEBHOOK_URL` 变成启动时就能看见的明确原因,而不是联调时才去猜。
    ///
    /// 官网地址未配置时跳过同域校验(无法判断),只做格式与可达性检查。
    pub fn validate_opcpay_public_url(&self, url: &str) -> anyhow::Result<()> {
        let raw = url.trim();
        anyhow::ensure!(!raw.is_empty(), "地址为空");
        anyhow::ensure!(raw.len() <= 512, "地址超过 512 字节限制:{} 字节", raw.len());
        let parsed = Url::parse(raw).with_context(|| format!("地址不是合法 URL: {raw}"))?;
        anyhow::ensure!(parsed.scheme() == "https", "地址必须是 HTTPS(实测 scheme={}): {raw}", parsed.scheme());
        anyhow::ensure!(
            parsed.username().is_empty() && parsed.password().is_none(),
            "地址不能包含账号信息: {raw}"
        );
        anyhow::ensure!(parsed.query().is_none(), "地址不能包含查询参数: {raw}");
        anyhow::ensure!(parsed.fragment().is_none(), "地址不能包含片段(#): {raw}");
        anyhow::ensure!(
            matches!(parsed.port(), None | Some(443)),
            "地址端口必须是 443(HTTPS 默认端口): {raw}"
        );
        match parsed.host() {
            Some(url::Host::Domain(d)) if d.eq_ignore_ascii_case("localhost") => {
                anyhow::bail!("地址不能使用 localhost: {raw}")
            }
            Some(url::Host::Ipv4(ip)) if is_private_ipv4(ip) => {
                anyhow::bail!("地址不能使用私网/回环 IPv4: {raw}")
            }
            Some(url::Host::Ipv6(ip)) if is_private_ipv6(ip) => {
                anyhow::bail!("地址不能使用私网/回环 IPv6: {raw}")
            }
            _ => {}
        }
        let site = self.opcpay_site_url();
        if !site.is_empty() {
            let site_parsed = Url::parse(&site)
                .with_context(|| format!("[payment] opcpay_site_url 不是合法 URL: {site}"))?;
            let same = match (parsed.host(), site_parsed.host()) {
                (Some(url::Host::Domain(a)), Some(url::Host::Domain(b))) => a.eq_ignore_ascii_case(b),
                (a, b) => a == b,
            };
            anyhow::ensure!(
                same,
                "地址必须与项目官网({site})同域名: {raw} —— 服务端会直接拒绝异域回调"
            );
        }
        Ok(())
    }
}

/// 回调路由路径(与 `main.rs` 中注册的公开路由一致)。
pub const OPCPAY_WEBHOOK_PATH: &str = "/admin/api/payments/opcpay/webhook";
/// 门户首页路径:付款完成后把用户送回自己的控制台。
pub const OPCPAY_RETURN_PATH: &str = "/portal";

/// 私网/回环/未指定 IPv4 —— 服务商明确不接受这类回调地址。
fn is_private_ipv4(ip: std::net::Ipv4Addr) -> bool {
    ip.is_private() || ip.is_loopback() || ip.is_unspecified() || ip.is_link_local()
}

/// IPv6 的等价判断:`::1` 回环、`::` 未指定、`fc00::/7` 唯一本地地址。
fn is_private_ipv6(ip: std::net::Ipv6Addr) -> bool {
    ip.is_loopback() || ip.is_unspecified() || (ip.segments()[0] & 0xfe00) == 0xfc00
}

/// 优先返回内联 PEM,其次读取指定文件;两者都为空则报错。
fn read_key_material(inline: &str, file: &str, label: &str) -> anyhow::Result<String> {
    let inline = inline.trim();
    if !inline.is_empty() {
        return Ok(inline.to_string());
    }
    let path = file.trim();
    if path.is_empty() {
        anyhow::bail!("{label}未配置:请通过环境变量 RELAY_PAYMENT__* 注入 PEM,或配置文件路径");
    }
    std::fs::read_to_string(path)
        .map_err(|e| anyhow::anyhow!("读取{label}文件失败({path}): {e}"))
}

impl Default for PaymentConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            provider: default_payment_provider(),
            notify_url: String::new(),
            app_id: String::new(),
            private_key: String::new(),
            private_key_file: String::new(),
            alipay_public_key: String::new(),
            alipay_public_key_file: String::new(),
            sandbox: default_sandbox(),
            gateway: String::new(),
            opcpay_api_base: default_opcpay_api_base(),
            opcpay_api_key: String::new(),
            opcpay_api_key_file: String::new(),
            opcpay_webhook_secret: String::new(),
            opcpay_webhook_secret_file: String::new(),
            opcpay_sku_id: String::new(),
            opcpay_skus: HashMap::new(),
            opcpay_timeout_secs: default_opcpay_timeout_secs(),
            opcpay_site_url: String::new(),
            opcpay_return_url: String::new(),
            watcha_api_base: default_watcha_api_base(),
            watcha_sandbox: default_sandbox(),
            watcha_api_key: String::new(),
            watcha_api_key_file: String::new(),
            watcha_sandbox_api_key: String::new(),
            watcha_sandbox_api_key_file: String::new(),
            watcha_entitlement_id: String::new(),
            watcha_entitlements: HashMap::new(),
            watcha_sandbox_entitlements: HashMap::new(),
            watcha_return_url: String::new(),
            watcha_timeout_secs: default_watcha_timeout_secs(),
            watcha_tier_cap_cents: default_watcha_tier_cap_cents(),
        }
    }
}

/// 手写 Debug 而非 derive:私钥与公钥属敏感凭据,绝不能出现在日志里。
impl std::fmt::Debug for PaymentConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PaymentConfig")
            .field("enabled", &self.enabled)
            .field("provider", &self.provider)
            .field("sandbox", &self.sandbox)
            .field("notify_url", &self.notify_url)
            .field("app_id", &mask_secret(&self.app_id))
            .field(
                "private_key",
                &secret_state(&self.private_key, &self.private_key_file),
            )
            .field(
                "alipay_public_key",
                &secret_state(&self.alipay_public_key, &self.alipay_public_key_file),
            )
            .field("opcpay_api_base", &self.opcpay_api_base)
            .field(
                "opcpay_api_key",
                &secret_state(&self.opcpay_api_key, &self.opcpay_api_key_file),
            )
            .field(
                "opcpay_webhook_secret",
                &secret_state(&self.opcpay_webhook_secret, &self.opcpay_webhook_secret_file),
            )
            .field("opcpay_sku_count", &self.opcpay_skus.len())
            .field("opcpay_site_url", &self.opcpay_site_url)
            .field("watcha_api_base", &self.watcha_api_base)
            .field("watcha_sandbox", &self.watcha_sandbox)
            .field(
                "watcha_api_key",
                &secret_state(&self.watcha_api_key, &self.watcha_api_key_file),
            )
            .field(
                "watcha_sandbox_api_key",
                &secret_state(&self.watcha_sandbox_api_key, &self.watcha_sandbox_api_key_file),
            )
            .field("watcha_entitlement_count", &self.watcha_entitlements.len())
            .field(
                "watcha_sandbox_entitlement_count",
                &self.watcha_sandbox_entitlements.len(),
            )
            .finish_non_exhaustive()
    }
}

/// 已配置的值只暴露末 4 位,够用于排障又不泄露。
fn mask_secret(v: &str) -> String {
    let s = v.trim();
    if s.is_empty() {
        return "未配置".to_string();
    }
    let tail: String = s.chars().rev().take(4).collect::<Vec<_>>().into_iter().rev().collect();
    format!("已配置(...{tail})")
}

/// 凭据是否存在(只回答有无,不输出内容)。
fn secret_state(inline: &str, file: &str) -> &'static str {
    if !inline.trim().is_empty() || !file.trim().is_empty() {
        "已配置"
    } else {
        "未配置"
    }
}

fn default_payment_provider() -> String {
    "none".to_string()
}

/// 数据面代理转发超时配置。
#[derive(Debug, Clone, Deserialize)]
pub struct ProxyConfig {
    /// 非流式请求的总超时(秒),默认 300;流式请求不设总超时。
    #[serde(default = "default_upstream_timeout_secs")]
    pub upstream_timeout_secs: u64,
    /// 连接建立超时(秒),全局生效(含流式),默认 10。
    #[serde(default = "default_connect_timeout_secs")]
    pub connect_timeout_secs: u64,
}

fn default_upstream_timeout_secs() -> u64 {
    300
}

fn default_connect_timeout_secs() -> u64 {
    10
}

impl Default for ProxyConfig {
    fn default() -> Self {
        Self {
            upstream_timeout_secs: default_upstream_timeout_secs(),
            connect_timeout_secs: default_connect_timeout_secs(),
        }
    }
}

/// 请求日志存储后端。
#[derive(Debug, Clone, Deserialize)]
pub struct LoggingConfig {
    /// 请求链路日志存储:"sqlite"(默认,写 request_logs 表)| "elasticsearch"(按日索引写入 ES)。
    #[serde(default = "default_log_store")]
    pub store: String,
    /// ES 连接地址(store=elasticsearch 时用,如 http://127.0.0.1:9200)。
    #[serde(default)]
    pub elasticsearch_url: String,
    /// ES 索引前缀,按日滚动为 <prefix>-YYYY.MM.DD,默认 relay-logs。
    #[serde(default)]
    pub elasticsearch_index_prefix: String,
    /// ES Basic 认证用户名(可选,留空=匿名)。
    #[serde(default)]
    pub elasticsearch_username: String,
    /// ES Basic 认证密码(可选)。
    #[serde(default)]
    pub elasticsearch_password: String,
    /// 请求/响应体预览采集上限(字节);0=不采集,默认 8192。
    /// 预览跟随存储后端:SQLite/PG 写 request_logs.req_body/resp_body 列,ES 写文档同名字段。
    #[serde(default = "default_body_preview")]
    pub body_preview_max_bytes: usize,
    /// 请求链路/用量日志保留天数,超期由后台任务分批删除;0=永久保留(默认 30 天)。
    #[serde(default = "default_retention_days")]
    pub retention_days: u32,
}

fn default_log_store() -> String {
    "sqlite".to_string()
}

fn default_body_preview() -> usize {
    8192
}

fn default_retention_days() -> u32 {
    30
}

impl Default for LoggingConfig {
    fn default() -> Self {
        Self {
            store: default_log_store(),
            elasticsearch_url: String::new(),
            elasticsearch_index_prefix: String::new(),
            elasticsearch_username: String::new(),
            elasticsearch_password: String::new(),
            body_preview_max_bytes: default_body_preview(),
            retention_days: default_retention_days(),
        }
    }
}

/// Prometheus 指标导出配置。
#[derive(Debug, Clone, Deserialize)]
pub struct MetricsConfig {
    /// /metrics 导出令牌:非空时,Bearer <token> 可代替管理端 JWT 访问 /metrics(供 Prometheus 抓取器使用);为空时仅管理端 JWT 可访问。
    #[serde(default)]
    pub export_token: String,
}

impl Default for MetricsConfig {
    fn default() -> Self {
        Self {
            export_token: String::new(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct CacheConfig {
    /// 缓存类型:"memory"(默认,进程内)| "redis"(多实例共享)。
    #[serde(default = "default_cache_type", rename = "type")]
    pub kind: String,
    /// 完整 redis url(优先于下面分项)。
    #[serde(default)]
    pub url: String,
    // ---- Redis 分项(url 为空时使用)----
    #[serde(default)]
    pub host: String,
    pub port: Option<u16>,
    #[serde(default)]
    pub password: String,
    pub db: Option<i64>,
}

fn default_cache_type() -> String {
    "memory".to_string()
}

impl Default for CacheConfig {
    fn default() -> Self {
        Self {
            kind: default_cache_type(),
            url: String::new(),
            host: String::new(),
            port: None,
            password: String::new(),
            db: None,
        }
    }
}

/// 语义缓存(响应缓存)配置:进程内 RwLock<HashMap> + TTL;L2 embedding 预留。
#[derive(Debug, Clone, Deserialize)]
pub struct CacheSemanticConfig {
    /// 总开关。
    #[serde(default = "default_cs_enabled")]
    pub enabled: bool,
    /// 缓存条目 TTL(秒)。
    #[serde(default = "default_cs_ttl")]
    pub ttl_secs: u64,
    /// 语义相似度阈值(L2 命中使用;L1 精确哈希不使用,先行落位以便热切换)。
    #[serde(default = "default_cs_threshold")]
    pub similarity_threshold: f64,
    /// 超过该消息条数的多轮对话跳过缓存。
    #[serde(default = "default_cs_multi_turn")]
    pub multi_turn_max: usize,
    /// 命中计费折扣率(0.0=免费,1.0=全额;与模型倍率/用户倍率相乘)。
    #[serde(default = "default_cs_billing_ratio")]
    pub billing_ratio: f64,
}

fn default_cs_enabled() -> bool { true }
fn default_cs_ttl() -> u64 { 3600 }
fn default_cs_threshold() -> f64 { 0.8 }
fn default_cs_multi_turn() -> usize { 3 }
fn default_cs_billing_ratio() -> f64 { 0.0 }

/// L2 embedding 供应商配置(OpenAI 兼容接口通吃):POST {base_url}/embeddings。
#[derive(Debug, Clone, Deserialize)]
pub struct EmbeddingConfig {
    /// L2 开关:关闭或 base_url 为空时语义缓存只跑 L1 精确哈希。
    #[serde(default)]
    pub enabled: bool,
    /// 供应商基础地址(如 https://api.openai.com/v1),不带尾斜杠。
    #[serde(default)]
    pub base_url: String,
    /// Bearer 密钥(本地供应商可为空)。
    #[serde(default)]
    pub api_key: String,
    /// embedding 模型名(如 text-embedding-3-small)。
    #[serde(default)]
    pub model: String,
}

impl Default for EmbeddingConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            base_url: String::new(),
            api_key: String::new(),
            model: String::new(),
        }
    }
}

impl EmbeddingConfig {
    /// 是否已具备调用条件(开关开启且 base_url/model 齐备)。
    pub fn usable(&self) -> bool {
        self.enabled && self.service_ready()
    }

    /// 对外 /v1/embeddings 端点就绪条件:服务地址与模型已配置(enabled 仅约束 L2 语义缓存)。
    pub fn service_ready(&self) -> bool {
        !self.base_url.is_empty() && !self.model.is_empty()
    }
}

impl Default for CacheSemanticConfig {
    fn default() -> Self {
        Self {
            enabled: default_cs_enabled(),
            ttl_secs: default_cs_ttl(),
            similarity_threshold: default_cs_threshold(),
            multi_turn_max: default_cs_multi_turn(),
            billing_ratio: default_cs_billing_ratio(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct EmailConfig {
    #[serde(default)]
    pub smtp_host: String,
    #[serde(default = "default_smtp_port")]
    pub smtp_port: u16,
    #[serde(default)]
    pub username: String,
    #[serde(default)]
    pub password: String,
    #[serde(default)]
    pub from: String,
}

impl Default for EmailConfig {
    fn default() -> Self {
        Self {
            smtp_host: String::new(),
            smtp_port: default_smtp_port(),
            username: String::new(),
            password: String::new(),
            from: String::new(),
        }
    }
}

fn default_smtp_port() -> u16 {
    587
}

impl EmailConfig {
    pub fn enabled(&self) -> bool {
        !self.smtp_host.is_empty()
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct AuthConfig {
    #[serde(default = "default_secret")]
    pub jwt_secret: String,
    #[serde(default = "default_ttl")]
    pub session_ttl_secs: u64,
}

impl Default for AuthConfig {
    fn default() -> Self {
        Self {
            jwt_secret: default_secret(),
            session_ttl_secs: default_ttl(),
        }
    }
}

fn default_secret() -> String {
    "dev-secret-change-me".to_string()
}
fn default_ttl() -> u64 {
    7 * 24 * 3600
}

#[derive(Debug, Clone, Deserialize)]
pub struct AdminConfig {
    #[serde(default = "default_admin_user")]
    pub username: String,
    #[serde(default = "default_admin_pass")]
    pub password: String,
    /// 后台改密后的口令哈希(argon2 PHC,存 settings.admin.*)。存在时登录校验以它为准,
    /// env/config 明文 password 仅作首次引导;不参与序列化输出。
    #[serde(default, skip_serializing)]
    pub password_hash: Option<String>,
}

impl Default for AdminConfig {
    fn default() -> Self {
        Self {
            username: default_admin_user(),
            password: default_admin_pass(),
            password_hash: None,
        }
    }
}

fn default_admin_user() -> String {
    "admin".to_string()
}
fn default_admin_pass() -> String {
    "admin123".to_string()
}

#[derive(Debug, Clone, Deserialize)]
pub struct ServerConfig {
    pub bind: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DatabaseConfig {
    /// 数据库类型:"sqlite"(默认)| "postgres"。决定建表 schema 与占位符方言。
    #[serde(default = "default_db_type", rename = "type")]
    pub kind: String,
    /// SQLite 文件 URL;Postgres 也可直接给完整 url(优先于下面分项)。
    #[serde(default)]
    pub url: String,
    // ---- Postgres 分项(url 为空时使用,密码含特殊字符无需转义)----
    #[serde(default)]
    pub host: String,
    pub port: Option<u16>,
    #[serde(default)]
    pub user: String,
    #[serde(default)]
    pub password: String,
    #[serde(default)]
    pub dbname: String,
}

fn default_db_type() -> String {
    "sqlite".to_string()
}

#[derive(Debug, Clone, Deserialize)]
pub struct Defaults {
    #[serde(default = "default_concurrency")]
    pub concurrency_limit: u32,
    #[serde(default = "default_grant")]
    pub signup_grant_tokens: i64,
    /// IANA 时区名(高峰/低谷时段判断用),如 "Asia/Shanghai"。解析失败时回退 tz_offset_hours。
    #[serde(default = "default_timezone")]
    pub timezone: String,
    #[serde(default = "default_tz")]
    pub tz_offset_hours: i64,
    /// 用户级默认限流(0 = 不限):每分钟最大请求数。
    #[serde(default = "default_rpm")]
    pub rpm_limit: u32,
    /// 用户级默认限流(0 = 不限):每分钟最大 token 消耗。
    #[serde(default = "default_tpm")]
    pub tpm_limit: u32,
    /// 单个上游连接(一个 key)的默认最大在途并发。0 表示不限。
    #[serde(default = "default_upstream_concurrency")]
    pub upstream_concurrency: u32,
    /// 用户并发满员时的排队等待时长(毫秒):排队期内有空位即放行,超时报 429。
    /// 0 = 关闭排队(满员立即拒绝,旧行为)。
    #[serde(default = "default_user_queue_wait")]
    pub user_queue_wait_ms: u64,
    /// failover 总开关:关闭后仅尝试首个候选,不可用直接报错。
    #[serde(default = "default_fallback_enabled")]
    pub fallback_enabled: bool,
    /// failover 最多尝试的候选数(含首个;0 = 不限)。
    #[serde(default = "default_max_retries")]
    pub max_retries: u32,
    /// 费用显示币种:"usd"(默认) | "cny"(人民币)。仅影响前端展示单位,
    /// 计价与落库的 cost_usd 不变;切换为 cny 时按 cny_rate 换算显示。
    #[serde(default = "default_currency")]
    pub currency: String,
    /// 币种切换为 cny 时使用的美元→人民币汇率(展示换算用,默认 7.2)。
    #[serde(default = "default_cny_rate")]
    pub cny_rate: f64,
    /// 上游熔断阈值:连续 N 次 Unavailable 级失败后熔断(默认 3)。
    #[serde(default = "default_breaker_fails")]
    pub breaker_fail_threshold: u8,
    /// 熔断窗时长(秒):窗内直接跳过该上游,窗后半开探活(默认 30)。
    #[serde(default = "default_breaker_window")]
    pub breaker_open_window_secs: u64,
    /// 半开探活在飞超时(秒):超过视为探活丢失,允许放新探活(默认 30)。
    #[serde(default = "default_breaker_window")]
    pub breaker_probe_window_secs: u64,
}

impl Defaults {
    /// 本次请求允许尝试的候选上限。
    pub fn attempt_cap(&self, candidates: usize) -> usize {
        if !self.fallback_enabled {
            return candidates.min(1);
        }
        if self.max_retries == 0 {
            candidates
        } else {
            candidates.min(self.max_retries as usize)
        }
    }
}

fn default_upstream_concurrency() -> u32 { 32 }
fn default_user_queue_wait() -> u64 { 3000 }
fn default_fallback_enabled() -> bool { true }
fn default_max_retries() -> u32 { 5 }
fn default_currency() -> String { "usd".to_string() }
fn default_cny_rate() -> f64 { 7.2 }
fn default_breaker_fails() -> u8 { 3 }
fn default_breaker_window() -> u64 { 30 }

impl Default for Defaults {
    fn default() -> Self {
        Self {
            concurrency_limit: default_concurrency(),
            signup_grant_tokens: default_grant(),
            timezone: default_timezone(),
            tz_offset_hours: default_tz(),
            rpm_limit: default_rpm(),
            tpm_limit: default_tpm(),
            upstream_concurrency: default_upstream_concurrency(),
            user_queue_wait_ms: default_user_queue_wait(),
            fallback_enabled: default_fallback_enabled(),
            max_retries: default_max_retries(),
            currency: default_currency(),
            cny_rate: default_cny_rate(),
            breaker_fail_threshold: default_breaker_fails(),
            breaker_open_window_secs: default_breaker_window(),
            breaker_probe_window_secs: default_breaker_window(),
        }
    }
}

fn default_rpm() -> u32 {
    0
}
fn default_tpm() -> u32 {
    0
}

fn default_tz() -> i64 {
    8
}

fn default_timezone() -> String {
    "Asia/Shanghai".to_string()
}

fn default_concurrency() -> u32 {
    16
}
fn default_grant() -> i64 {
    1_000_000
}

/// 上游供应商的协议类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderKind {
    Openai,
    Anthropic,
}

impl Config {
    /// 从 config/default.toml + 环境变量加载。
    pub fn load() -> anyhow::Result<Self> {
        let cfg: Config = Figment::new()
            .merge(Toml::file("config/default.toml"))
            .merge(Env::prefixed("RELAY_").split("__"))
            .extract()?;
        Ok(cfg)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn attempt_cap_defaults_limit_to_five() {
        let d = Defaults::default();
        assert!(d.fallback_enabled);
        assert_eq!(d.max_retries, 5);
        assert_eq!(d.attempt_cap(2), 2);
        assert_eq!(d.attempt_cap(20), 5);
    }

    #[test]
    fn attempt_cap_disabled_means_first_only() {
        let d = Defaults { fallback_enabled: false, ..Defaults::default() };
        assert_eq!(d.attempt_cap(0), 0);
        assert_eq!(d.attempt_cap(7), 1);
    }

    #[test]
    fn attempt_cap_zero_retries_means_unlimited() {
        let d = Defaults { max_retries: 0, ..Defaults::default() };
        assert_eq!(d.attempt_cap(20), 20);
    }

    /// 境外域名 + HTTPS 的默认形态:官网地址合法即可推出回调与返回地址。
    #[test]
    fn opcpay_urls_derive_from_site_url() {
        let cfg = PaymentConfig {
            opcpay_site_url: "https://relay.example.com/".into(),
            ..Default::default()
        };
        assert_eq!(cfg.opcpay_site_url(), "https://relay.example.com");
        assert_eq!(
            cfg.opcpay_webhook_url().as_deref(),
            Some("https://relay.example.com/admin/api/payments/opcpay/webhook")
        );
        assert_eq!(
            cfg.opcpay_callback_url().as_deref(),
            Some("https://relay.example.com/portal")
        );
        // 推导出来的地址必须通过服务端那条同域规则。
        cfg.validate_opcpay_public_url(&cfg.opcpay_webhook_url().unwrap())
            .expect("官网推导出的回调地址应当合规");
    }

    #[test]
    fn opcpay_return_url_overrides_derived_default() {
        let cfg = PaymentConfig {
            opcpay_site_url: "https://relay.example.com".into(),
            opcpay_return_url: "https://relay.example.com/pay/return".into(),
            ..Default::default()
        };
        assert_eq!(
            cfg.opcpay_callback_url().as_deref(),
            Some("https://relay.example.com/pay/return")
        );
    }

    #[test]
    fn opcpay_urls_empty_without_site_url() {
        let cfg = PaymentConfig::default();
        assert!(cfg.opcpay_webhook_url().is_none());
        assert!(cfg.opcpay_callback_url().is_none());
    }

    /// 逐条覆盖服务端会返回 INVALID_WEBHOOK_URL 的情形。
    #[test]
    fn opcpay_rejects_non_compliant_public_urls() {
        let cfg = PaymentConfig {
            opcpay_site_url: "https://relay.example.com".into(),
            ..Default::default()
        };
        let bad = [
            "http://relay.example.com/webhook",           // 非 HTTPS
            "https://relay.example.com:8443/webhook",     // 非 443 端口
            "https://user:pw@relay.example.com/webhook",  // 含账号信息
            "https://relay.example.com/webhook?a=1",      // 含查询参数
            "https://relay.example.com/webhook#x",        // 含片段
            "https://other.example.com/webhook",          // 异域(与官网不同)
            "https://127.0.0.1/webhook",                  // 回环地址
            "https://192.168.1.10/webhook",               // 私网地址
            "https://localhost/webhook",                  // localhost
            "not-a-url",                                  // 非法 URL
        ];
        for url in bad {
            let err = match cfg.validate_opcpay_public_url(url) {
                Err(e) => e,
                Ok(()) => panic!("应被拒绝: {url}"),
            };
            assert!(!err.to_string().is_empty());
        }
        // 未配置官网时跳过同域校验,但仍拦住格式问题。
        let bare = PaymentConfig::default();
        assert!(bare.validate_opcpay_public_url("https://any.example.com/hook").is_ok());
        assert!(bare.validate_opcpay_public_url("http://any.example.com/hook").is_err());
    }

    /// 域名大小写不同应视为同域:DNS 本身不区分大小写。
    #[test]
    fn opcpay_same_domain_check_is_case_insensitive() {
        let cfg = PaymentConfig {
            opcpay_site_url: "https://Relay.Example.com".into(),
            ..Default::default()
        };
        cfg.validate_opcpay_public_url("https://relay.example.com/webhook")
            .expect("域名大小写差异不应影响同域判定");
    }

    /// 沙箱与正式的权益表必须各查各的:两边的 entitlement_id 是不同对象,
    /// 混用会导致"沙箱下单、正式发货"这类串环境问题。
    #[test]
    fn watcha_entitlement_lookup_follows_environment() {
        let cfg = PaymentConfig {
            watcha_sandbox: true,
            watcha_entitlement_id: "ent_default".into(),
            watcha_entitlements: HashMap::from([("plan-a".into(), "ent_live".into())]),
            watcha_sandbox_entitlements: HashMap::from([("plan-a".into(), "ent_sandbox".into())]),
            ..Default::default()
        };
        assert_eq!(
            cfg.watcha_entitlement_for(Some("plan-a")),
            Some("ent_sandbox".into())
        );
        let live = PaymentConfig {
            watcha_sandbox: false,
            ..cfg
        };
        assert_eq!(
            live.watcha_entitlement_for(Some("plan-a")),
            Some("ent_live".into())
        );
        // 未命中的套餐回退到默认权益。
        assert_eq!(
            live.watcha_entitlement_for(Some("plan-unknown")),
            Some("ent_default".into())
        );
        // 什么都没配时必须返回 None —— 渠道层据此拒绝下单,而不是让用户
        // 跳到一个权益不对的收银台。
        let empty = PaymentConfig::default();
        assert_eq!(empty.watcha_entitlement_for(Some("plan-a")), None);
        assert_eq!(empty.watcha_entitlement_for(None), None);
    }

    /// Key 也必须按环境二选一:沙箱 Key 不能拿去查正式权益。
    #[test]
    fn watcha_api_key_follows_environment() {
        let cfg = PaymentConfig {
            watcha_sandbox: true,
            watcha_sandbox_api_key: "sandbox".into(),
            watcha_api_key: "live".into(),
            ..Default::default()
        };
        assert_eq!(cfg.read_watcha_api_key().unwrap(), "sandbox");
        let live = PaymentConfig {
            watcha_sandbox: false,
            ..cfg
        };
        assert_eq!(live.read_watcha_api_key().unwrap(), "live");
        // 凭据缺失一律报错:不能带着空 Key 上线。
        let bare = PaymentConfig::default();
        assert!(bare.read_watcha_api_key().is_err());
    }

    #[test]
    fn watcha_api_base_trims_trailing_slash_and_has_default() {
        assert_eq!(PaymentConfig::default().watcha_api_base(), default_watcha_api_base());
        let cfg = PaymentConfig {
            watcha_api_base: "https://pay.watcha.cn///".into(),
            ..Default::default()
        };
        assert_eq!(cfg.watcha_api_base(), "https://pay.watcha.cn");
        let cfg = PaymentConfig {
            watcha_api_base: "  ".into(),
            ..Default::default()
        };
        assert_eq!(cfg.watcha_api_base(), default_watcha_api_base());
    }
}
