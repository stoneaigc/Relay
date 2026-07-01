//! 带 TTL 的轻量键值缓存。默认进程内存(单实例),可切 Redis(多实例共享)。
//! 当前用于:OAuth CSRF state、邮箱注册验证码。

use std::collections::HashMap;
use std::sync::Mutex;

use crate::config::CacheConfig;
use crate::storage::now_secs;

/// URL 百分号编码(只保留 unreserved 字符),用于把密码安全拼进 redis url。
fn pct_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => out.push(b as char),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

fn build_redis_url(cfg: &CacheConfig) -> String {
    if !cfg.url.is_empty() {
        return cfg.url.clone();
    }
    let host = if cfg.host.is_empty() { "127.0.0.1" } else { cfg.host.as_str() };
    let port = cfg.port.unwrap_or(6379);
    let db = cfg.db.unwrap_or(0);
    let auth = if cfg.password.is_empty() {
        String::new()
    } else {
        format!(":{}@", pct_encode(&cfg.password))
    };
    format!("redis://{auth}{host}:{port}/{db}")
}

/// 统一缓存句柄:运行时按配置选 Memory 或 Redis。
pub enum Cache {
    /// 进程内:key -> (value, 过期 unix 秒)。读取时惰性过期。
    Memory(Mutex<HashMap<String, (String, i64)>>),
    Redis(redis::aio::ConnectionManager),
}

impl Cache {
    pub async fn init(cfg: &CacheConfig) -> anyhow::Result<Self> {
        match cfg.kind.to_ascii_lowercase().as_str() {
            "redis" => {
                let client = redis::Client::open(build_redis_url(cfg))?;
                let mgr = redis::aio::ConnectionManager::new(client).await?;
                Ok(Cache::Redis(mgr))
            }
            "memory" | "" => Ok(Cache::Memory(Mutex::new(HashMap::new()))),
            other => anyhow::bail!("cache.type 不支持: {other}(可选 memory | redis)"),
        }
    }

    /// 写入并设置 TTL(秒)。
    pub async fn set_ex(&self, key: &str, val: &str, ttl_secs: u64) -> anyhow::Result<()> {
        match self {
            Cache::Memory(m) => {
                m.lock()
                    .unwrap()
                    .insert(key.to_string(), (val.to_string(), now_secs() + ttl_secs as i64));
                Ok(())
            }
            Cache::Redis(mgr) => {
                let mut c = mgr.clone();
                redis::cmd("SET")
                    .arg(key)
                    .arg(val)
                    .arg("EX")
                    .arg(ttl_secs)
                    .query_async::<()>(&mut c)
                    .await?;
                Ok(())
            }
        }
    }

    /// 读取;不存在或已过期返回 None。
    pub async fn get(&self, key: &str) -> anyhow::Result<Option<String>> {
        match self {
            Cache::Memory(m) => {
                let mut g = m.lock().unwrap();
                match g.get(key) {
                    Some((v, exp)) if *exp >= now_secs() => Ok(Some(v.clone())),
                    Some(_) => {
                        g.remove(key);
                        Ok(None)
                    }
                    None => Ok(None),
                }
            }
            Cache::Redis(mgr) => {
                let mut c = mgr.clone();
                let v: Option<String> = redis::cmd("GET").arg(key).query_async(&mut c).await?;
                Ok(v)
            }
        }
    }

    /// 删除。
    pub async fn del(&self, key: &str) -> anyhow::Result<()> {
        match self {
            Cache::Memory(m) => {
                m.lock().unwrap().remove(key);
                Ok(())
            }
            Cache::Redis(mgr) => {
                let mut c = mgr.clone();
                redis::cmd("DEL").arg(key).query_async::<()>(&mut c).await?;
                Ok(())
            }
        }
    }
}
