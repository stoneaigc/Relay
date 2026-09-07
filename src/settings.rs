//! 系统配置的应用与敏感字段加解密。
//!
//! settings 表以 KV 存放系统配置(点分命名空间)。本模块负责:
//! - 把 DB 里的 email.* 覆盖到内存 Config(DB 优先于配置文件);
//! - SMTP 授权码的可逆加密:用 `auth.jwt_secret` 经 HKDF 派生 AES-256-GCM 密钥。

use std::collections::HashMap;

use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Nonce};
use base64::{engine::general_purpose::STANDARD as B64, Engine};
use hkdf::Hkdf;
use sha2::Sha256;

use crate::config::{Config, EmailConfig};

/// settings 表里邮箱相关 key 的前缀。
pub const EMAIL_PREFIX: &str = "email.";
pub const K_HOST: &str = "email.smtp_host";
pub const K_PORT: &str = "email.smtp_port";
pub const K_USER: &str = "email.username";
pub const K_FROM: &str = "email.from";
pub const K_PASS_ENC: &str = "email.password_enc";

/// 从 jwt_secret 派生 32 字节 AES-256-GCM 密钥(HKDF-SHA256,info 固定)。
fn derive_key(secret: &str) -> [u8; 32] {
    let hk = Hkdf::<Sha256>::new(None, secret.as_bytes());
    let mut okm = [0u8; 32];
    hk.expand(b"relay-email-encryption", &mut okm)
        .expect("hkdf expand 32 bytes");
    okm
}

/// 加密明文 -> base64(nonce(12) || ciphertext)。
pub fn encrypt(secret: &str, plain: &str) -> anyhow::Result<String> {
    let key = derive_key(secret);
    let cipher = Aes256Gcm::new(&key.into());
    let mut nonce_bytes = [0u8; 12];
    // 进程内唯一性足够;无需密码学随机(rand 已在依赖里,但避免在此引入 thread_rng 复杂度,
    // 用 SystemTime 衍生 12 字节 nonce——AES-GCM 的 nonce 仅需不重复)。
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    for (i, b) in nonce_bytes.iter_mut().enumerate() {
        *b = ((nanos >> ((i * 8) % 96)) & 0xff) as u8;
    }
    let nonce = Nonce::from_slice(&nonce_bytes);
    let ct = cipher
        .encrypt(nonce, plain.as_bytes())
        .map_err(|e| anyhow::anyhow!("aes-gcm encrypt: {e}"))?;
    let mut out = Vec::with_capacity(12 + ct.len());
    out.extend_from_slice(&nonce_bytes);
    out.extend_from_slice(&ct);
    Ok(B64.encode(&out))
}

/// 解密 base64(nonce||ciphertext) -> 明文。
pub fn decrypt(secret: &str, b64: &str) -> anyhow::Result<String> {
    let key = derive_key(secret);
    let cipher = Aes256Gcm::new(&key.into());
    let raw = B64
        .decode(b64)
        .map_err(|e| anyhow::anyhow!("base64 decode: {e}"))?;
    if raw.len() < 12 {
        anyhow::bail!("ciphertext too short");
    }
    let (nonce_bytes, ct) = raw.split_at(12);
    let nonce = Nonce::from_slice(nonce_bytes);
    let pt = cipher
        .decrypt(nonce, ct)
        .map_err(|e| anyhow::anyhow!("aes-gcm decrypt: {e}"))?;
    String::from_utf8(pt).map_err(|e| anyhow::anyhow!("utf8: {e}"))
}

/// 把 DB 里的 email.* 覆盖到 Config.email。
/// - 普通字段:DB 有则覆盖,无则保持配置文件原值。
/// - password_enc:解密后填入 password;解密失败记 warn、按空处理,不阻断。
pub fn apply_email_settings(cfg: &mut Config, kv: &HashMap<String, String>, secret: &str) {
    let mut email = cfg.email.clone();

    if let Some(v) = kv.get(K_HOST) {
        email.smtp_host = v.clone();
    }
    if let Some(v) = kv.get(K_PORT) {
        if let Ok(p) = v.parse::<u16>() {
            email.smtp_port = p;
        }
    }
    if let Some(v) = kv.get(K_USER) {
        email.username = v.clone();
    }
    if let Some(v) = kv.get(K_FROM) {
        email.from = v.clone();
    }
    if let Some(v) = kv.get(K_PASS_ENC) {
        match decrypt(secret, v) {
            Ok(pw) => email.password = pw,
            Err(e) => {
                tracing::warn!("解密 email.password_enc 失败,按未配置处理: {e}");
                email.password = String::new();
            }
        }
    }
    cfg.email = email;
}

/// 供管理后台 GET 时判断「是否已设置授权码」。
pub fn has_email_password(kv: &HashMap<String, String>) -> bool {
    kv.get(K_PASS_ENC).map(|s| !s.is_empty()).unwrap_or(false)
}

/// 从 DB 的 email.* 构造一份 EmailConfig(测试发信用;密码可选从库解密补全)。
/// `password_override` 非空时用它,否则尝试从库解密。
pub fn email_config_from_kv(
    base: &EmailConfig,
    kv: &HashMap<String, String>,
    secret: &str,
    password_override: Option<&str>,
) -> EmailConfig {
    let mut email = base.clone();
    if let Some(v) = kv.get(K_HOST) {
        email.smtp_host = v.clone();
    }
    if let Some(v) = kv.get(K_PORT) {
        if let Ok(p) = v.parse::<u16>() {
            email.smtp_port = p;
        }
    }
    if let Some(v) = kv.get(K_USER) {
        email.username = v.clone();
    }
    if let Some(v) = kv.get(K_FROM) {
        email.from = v.clone();
    }
    match password_override.filter(|s| !s.is_empty()) {
        Some(pw) => email.password = pw.to_string(),
        None => {
            if let Some(v) = kv.get(K_PASS_ENC) {
                email.password = decrypt(secret, v).unwrap_or_default();
            }
        }
    }
    email
}

/// settings 表里 failover 相关 key 的前缀。
pub const FALLBACK_PREFIX: &str = "fallback.";
pub const K_FB_ENABLED: &str = "fallback.enabled";
pub const K_FB_MAX_RETRIES: &str = "fallback.max_retries";

/// 把 DB 里的 fallback.* 覆盖到内存 Config.defaults(DB 优先于配置文件)。
/// 解析失败时静默保持原值,与 email.* 行为一致。
pub fn apply_fallback_settings(cfg: &mut Config, kv: &HashMap<String, String>) {
    let mut defaults = cfg.defaults.clone();

    if let Some(v) = kv.get(K_FB_ENABLED) {
        if let Ok(b) = v.parse::<bool>() {
            defaults.fallback_enabled = b;
        }
    }
    if let Some(v) = kv.get(K_FB_MAX_RETRIES) {
        if let Ok(n) = v.parse::<u32>() {
            defaults.max_retries = n;
        }
    }
    cfg.defaults = defaults;
}

/// settings 表里语义缓存相关 key 的前缀。
pub const CACHE_PREFIX: &str = "cache.";
pub const K_CACHE_ENABLED: &str = "cache.enabled";
pub const K_CACHE_TTL: &str = "cache.ttl_secs";
pub const K_CACHE_THRESHOLD: &str = "cache.similarity_threshold";
pub const K_CACHE_MULTI_TURN: &str = "cache.multi_turn_max";
/// L3 命中计费折扣率(0.0=命中不扣费,1.0=照常计费)。
pub const K_CACHE_BILLING: &str = "cache.billing_ratio";

/// 把 DB 里的 cache.* 覆盖到内存 Config.cache_semantic(DB 优先于配置文件)。
/// 解析失败时静默保持原值,与 fallback.* 行为一致。
pub fn apply_cache_settings(cfg: &mut Config, kv: &HashMap<String, String>) {
    let mut cs = cfg.cache_semantic.clone();

    if let Some(v) = kv.get(K_CACHE_ENABLED) {
        if let Ok(b) = v.parse::<bool>() {
            cs.enabled = b;
        }
    }
    if let Some(v) = kv.get(K_CACHE_TTL) {
        if let Ok(n) = v.parse::<u64>() {
            cs.ttl_secs = n;
        }
    }
    if let Some(v) = kv.get(K_CACHE_THRESHOLD) {
        if let Ok(f) = v.parse::<f64>() {
            cs.similarity_threshold = f;
        }
    }
    if let Some(v) = kv.get(K_CACHE_MULTI_TURN) {
        if let Ok(n) = v.parse::<usize>() {
            cs.multi_turn_max = n;
        }
    }
    if let Some(v) = kv.get(K_CACHE_BILLING) {
        if let Ok(f) = v.parse::<f64>() {
            if (0.0..=1.0).contains(&f) {
                cs.billing_ratio = f;
            }
        }
    }
    cfg.cache_semantic = cs;
}

/// settings 表里 L2 embedding 供应商相关 key 的前缀。
pub const EMB_PREFIX: &str = "embedding.";
pub const K_EMB_ENABLED: &str = "embedding.enabled";
pub const K_EMB_BASE_URL: &str = "embedding.base_url";
pub const K_EMB_API_KEY_ENC: &str = "embedding.api_key_enc";
pub const K_EMB_MODEL: &str = "embedding.model";

/// 把 DB 里的 embedding.* 覆盖到内存 Config.embedding(DB 优先于配置文件)。
/// api_key 以 email.password_enc 同款 AES-256-GCM 加密存放,解密失败按未配置处理。
pub fn apply_embedding_settings(cfg: &mut Config, kv: &HashMap<String, String>, secret: &str) {
    let mut ec = cfg.embedding.clone();

    if let Some(v) = kv.get(K_EMB_ENABLED) {
        if let Ok(b) = v.parse::<bool>() {
            ec.enabled = b;
        }
    }
    if let Some(v) = kv.get(K_EMB_BASE_URL) {
        ec.base_url = v.trim_end_matches('/').to_string();
    }
    if let Some(v) = kv.get(K_EMB_MODEL) {
        ec.model = v.clone();
    }
    if let Some(v) = kv.get(K_EMB_API_KEY_ENC) {
        match decrypt(secret, v) {
            Ok(k) => ec.api_key = k,
            Err(e) => {
                tracing::warn!("解密 embedding.api_key_enc 失败,按未配置处理: {e}");
                ec.api_key = String::new();
            }
        }
    }
    cfg.embedding = ec;
}

/// 供管理后台 GET 时判断「是否已设置 api_key」。
pub fn has_embedding_key(kv: &HashMap<String, String>) -> bool {
    kv.get(K_EMB_API_KEY_ENC).map(|s| !s.is_empty()).unwrap_or(false)
}

/// settings 表里日志相关 key 的前缀。
pub const LOG_PREFIX: &str = "logging.";
pub const K_LOG_PREVIEW: &str = "logging.body_preview_max_bytes";
pub const K_LOG_RETENTION: &str = "logging.retention_days";

/// 把 DB 里的 logging.* 覆盖到内存 Config.logging(DB 优先于配置文件)。
/// 解析失败时静默保持原值,与 fallback.* 行为一致。
pub fn apply_logging_settings(cfg: &mut Config, kv: &HashMap<String, String>) {
    let mut lc = cfg.logging.clone();

    if let Some(v) = kv.get(K_LOG_PREVIEW) {
        if let Ok(n) = v.parse::<usize>() {
            lc.body_preview_max_bytes = n;
        }
    }
    if let Some(v) = kv.get(K_LOG_RETENTION) {
        if let Ok(n) = v.parse::<u32>() {
            lc.retention_days = n;
        }
    }
    cfg.logging = lc;
}
