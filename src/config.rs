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
    #[serde(default = "default_tz")]
    pub tz_offset_hours: i64,
}

impl Default for Defaults {
    fn default() -> Self {
        Self {
            concurrency_limit: default_concurrency(),
            signup_grant_tokens: default_grant(),
            tz_offset_hours: default_tz(),
        }
    }
}

fn default_tz() -> i64 {
    8
}

fn default_concurrency() -> u32 {
    16
}
fn default_grant() -> i64 {
    10_000_000
}

/// 上游供应商的协议类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
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
            .merge(Env::prefixed("RUNAPI_").split("__"))
            .extract()?;
        Ok(cfg)
    }
}
