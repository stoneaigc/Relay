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
    #[serde(default)]
    pub logging: LoggingConfig,
}

/// 请求日志存储后端。
#[derive(Debug, Clone, Deserialize)]
pub struct LoggingConfig {
    /// 请求链路日志存储:"sqlite"(默认,写 request_logs 表)| "elasticsearch"(预留)。
    #[serde(default = "default_log_store")]
    pub store: String,
    /// 预留:ES 连接地址(store=elasticsearch 时用,暂未实现)。
    #[serde(default)]
    pub elasticsearch_url: String,
}

fn default_log_store() -> String {
    "sqlite".to_string()
}

impl Default for LoggingConfig {
    fn default() -> Self {
        Self {
            store: default_log_store(),
            elasticsearch_url: String::new(),
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
