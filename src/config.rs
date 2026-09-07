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
}

impl Default for AdminConfig {
    fn default() -> Self {
        Self {
            username: default_admin_user(),
            password: default_admin_pass(),
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
    /// failover 总开关:关闭后仅尝试首个候选,不可用直接报错。
    #[serde(default = "default_fallback_enabled")]
    pub fallback_enabled: bool,
    /// failover 最多尝试的候选数(含首个;0 = 不限)。
    #[serde(default = "default_max_retries")]
    pub max_retries: u32,
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
fn default_fallback_enabled() -> bool { true }
fn default_max_retries() -> u32 { 5 }

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
            fallback_enabled: default_fallback_enabled(),
            max_retries: default_max_retries(),
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
    10_000_000
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
}
