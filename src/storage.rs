use std::sync::Arc;
use std::sync::OnceLock;

use dashmap::DashMap;
use sha2::{Digest, Sha256};
use sqlx::any::AnyPoolOptions;
use sqlx::Row;
use uuid::Uuid;

use crate::config::{Config, ProviderKind};
use crate::state::{KeyEntry, UserState};

/// 统一数据库句柄:运行时按连接 URL 选择 SQLite 或 Postgres。
pub type Db = sqlx::AnyPool;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Backend {
    Sqlite,
    Postgres,
}

static BACKEND: OnceLock<Backend> = OnceLock::new();

/// 当前运行的数据库后端(init_pool 时确定)。
pub fn backend() -> Backend {
    *BACKEND.get().unwrap_or(&Backend::Sqlite)
}

/// 占位符适配:Postgres 需 `$1..$n`,SQLite/其余保持 `?`。
/// 仅替换 SQL 文本之外的 `?`(跳过单引号字符串字面量内的)。
pub fn pq(sql: &str) -> String {
    if backend() != Backend::Postgres {
        return sql.to_string();
    }
    let mut out = String::with_capacity(sql.len() + 8);
    let mut n = 0usize;
    let mut in_str = false;
    for c in sql.chars() {
        match c {
            '\'' => {
                in_str = !in_str;
                out.push(c);
            }
            '?' if !in_str => {
                n += 1;
                out.push('$');
                out.push_str(&n.to_string());
            }
            _ => out.push(c),
        }
    }
    out
}

/// 所有查询统一经此宏,自动做占位符适配。等价于 `sqlx::query(&pq(sql))`。
macro_rules! q {
    ($sql:expr $(,)?) => {
        sqlx::query(&pq($sql))
    };
}

/// sha256(明文) 的十六进制串,用于 Key 校验与存储(明文绝不落库)。
pub fn hash_key(plaintext: &str) -> String {
    let mut h = Sha256::new();
    h.update(plaintext.as_bytes());
    hex::encode(h.finalize())
}

/// 生成新虚拟密钥,返回 (明文, 哈希, 掩码前缀)。
pub fn generate_key() -> (String, String, String) {
    use rand::RngCore;
    let mut bytes = [0u8; 24];
    rand::thread_rng().fill_bytes(&mut bytes);
    let plaintext = format!("rk_live_{}", hex::encode(bytes));
    let hash = hash_key(&plaintext);
    let prefix = format!("{}****", &plaintext[..16]);
    (plaintext, hash, prefix)
}

pub fn now_secs() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs() as i64
}

fn now_iso() -> String {
    format!("@{}", now_secs())
}

/// URL 百分号编码(只保留 unreserved 字符),用于把分项里的用户名/密码安全拼进连接串。
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

pub async fn init_pool(cfg: &crate::config::DatabaseConfig) -> anyhow::Result<Db> {
    sqlx::any::install_default_drivers();

    // 后端由配置的 type 决定(显式优于猜测)。
    let is_pg = match cfg.kind.to_ascii_lowercase().as_str() {
        "postgres" | "postgresql" | "pg" => true,
        "sqlite" | "" => false,
        other => anyhow::bail!("database.type 不支持: {other}(可选 sqlite | postgres)"),
    };
    let _ = BACKEND.set(if is_pg { Backend::Postgres } else { Backend::Sqlite });

    let url = if is_pg {
        // Postgres:优先用完整 url;否则用分项拼接(密码等做百分号编码,特殊字符无需手动转义)。
        if !cfg.url.is_empty() {
            anyhow::ensure!(
                cfg.url.starts_with("postgres"),
                "database.type=postgres 但 url 不是 postgres:// 开头:{}",
                cfg.url
            );
            cfg.url.clone()
        } else {
            anyhow::ensure!(!cfg.host.is_empty(), "database.host 不能为空(postgres 分项配置)");
            anyhow::ensure!(!cfg.user.is_empty(), "database.user 不能为空(postgres 分项配置)");
            anyhow::ensure!(!cfg.dbname.is_empty(), "database.dbname 不能为空(postgres 分项配置)");
            let port = cfg.port.unwrap_or(5432);
            format!(
                "postgres://{}:{}@{}:{}/{}",
                pct_encode(&cfg.user),
                pct_encode(&cfg.password),
                cfg.host,
                port,
                pct_encode(&cfg.dbname),
            )
        }
    } else {
        // SQLite:Any 不暴露 create_if_missing,用 URL 参数 mode=rwc 确保文件不存在时自动创建。
        let base = if cfg.url.is_empty() { "sqlite://relay.db" } else { cfg.url.as_str() };
        anyhow::ensure!(!base.starts_with("postgres"), "database.type=sqlite 但 url 是 postgres://:{base}");
        if base.contains("mode=") {
            base.to_string()
        } else {
            let sep = if base.contains('?') { '&' } else { '?' };
            format!("{base}{sep}mode=rwc")
        }
    };

    let pool = AnyPoolOptions::new().max_connections(8).connect(&url).await?;
    Ok(pool)
}

pub async fn init_schema(pool: &Db) -> anyhow::Result<()> {
    let is_pg = backend() == Backend::Postgres;
    let raw = if is_pg {
        include_str!("../migrations/0001_init.postgres.sql")
    } else {
        include_str!("../migrations/0001_init.sql")
    };
    // 先剥掉行内 `--` 注释(避免注释里的分号干扰切分),再按分号执行。
    let sql: String = raw
        .lines()
        .map(|l| match l.find("--") {
            Some(i) => &l[..i],
            None => l,
        })
        .collect::<Vec<_>>()
        .join("\n");
    for stmt in sql.split(';') {
        let stmt = stmt.trim();
        if stmt.is_empty() {
            continue;
        }
        q!(stmt).execute(pool).await?;
    }
    // 兼容旧库:补列(列已存在则忽略)。
    if is_pg {
        // Postgres 支持 IF NOT EXISTS,直接补。
        let _ = q!("ALTER TABLE model_groups ADD COLUMN IF NOT EXISTS is_active BIGINT NOT NULL DEFAULT 0")
            .execute(pool)
            .await;
        let _ = q!("ALTER TABLE users ADD COLUMN IF NOT EXISTS rpm_limit BIGINT")
            .execute(pool)
            .await;
        let _ = q!("ALTER TABLE users ADD COLUMN IF NOT EXISTS tpm_limit BIGINT")
            .execute(pool)
            .await;
        // 奖励申领接入任务:补 task_id,并放开旧 kind 的 NOT NULL。
        let _ = q!("ALTER TABLE reward_claims ADD COLUMN IF NOT EXISTS task_id BIGINT")
            .execute(pool)
            .await;
        let _ = q!("ALTER TABLE reward_claims ALTER COLUMN kind DROP NOT NULL")
            .execute(pool)
            .await;
    } else {
        let _ = q!("ALTER TABLE reward_claims ADD COLUMN task_id INTEGER").execute(pool).await;
        let _ = q!("ALTER TABLE users ADD COLUMN bill_multiplier REAL NOT NULL DEFAULT 1.0")
            .execute(pool)
            .await;
        let _ = q!("ALTER TABLE users ADD COLUMN group_id INTEGER").execute(pool).await;
        let _ = q!("ALTER TABLE users ADD COLUMN username TEXT").execute(pool).await;
        let _ = q!("ALTER TABLE users ADD COLUMN password_hash TEXT").execute(pool).await;
        let _ = q!("ALTER TABLE users ADD COLUMN email TEXT").execute(pool).await;
        let _ = q!("ALTER TABLE users ADD COLUMN source TEXT").execute(pool).await;
        let _ = q!("ALTER TABLE users ADD COLUMN rpm_limit INTEGER").execute(pool).await;
        let _ = q!("ALTER TABLE users ADD COLUMN tpm_limit INTEGER").execute(pool).await;
        let _ = q!("ALTER TABLE usage_logs ADD COLUMN ts INTEGER NOT NULL DEFAULT 0")
            .execute(pool)
            .await;
        let _ = q!("ALTER TABLE model_groups ADD COLUMN is_active INTEGER NOT NULL DEFAULT 0")
            .execute(pool)
            .await;
        let _ = q!("ALTER TABLE model_groups ADD COLUMN strategy TEXT NOT NULL DEFAULT 'weighted_random'")
            .execute(pool)
            .await;
        // 请求链路:旧库补 request_id 列(已存在则忽略错误)。
        let _ = q!("ALTER TABLE usage_logs ADD COLUMN request_id TEXT")
            .execute(pool)
            .await;
        // 模型防重唯一索引:旧库有重复时创建失败则跳过，幂等由 add_model 查询保证。
        let _ = q!("CREATE UNIQUE INDEX IF NOT EXISTS uniq_models_provider_model ON models(provider, upstream_model)")
            .execute(pool)
            .await;
    }
    Ok(())
}

/// 口令哈希(开发版:sha256;生产建议换 argon2)。
pub fn hash_password(pw: &str) -> String {
    hash_key(pw)
}

/// 冷启动:把 users 与未吊销的 api_keys 全量加载进内存 Map。
pub async fn load_into_memory(
    pool: &Db,
    cfg: &Config,
    users: &DashMap<Uuid, Arc<UserState>>,
    keys: &DashMap<String, KeyEntry>,
) -> anyhow::Result<()> {
    let rows = q!(
        "SELECT id, status, token_balance, concurrency_limit, bill_multiplier, group_id, rpm_limit, tpm_limit FROM users",
    )
    .fetch_all(pool)
    .await?;
    for r in rows {
        let id = Uuid::parse_str(r.get::<String, _>("id").as_str())?;
        let status: i64 = r.get("status");
        let balance: i64 = r.get("token_balance");
        let cl: Option<i64> = r.get("concurrency_limit");
        let mult: f64 = r.try_get("bill_multiplier").unwrap_or(1.0);
        let group_id: i64 = r.try_get::<Option<i64>, _>("group_id").ok().flatten().unwrap_or(0);
        let limit = cl.map(|v| v as u32).unwrap_or(cfg.defaults.concurrency_limit);
        let rpm = r.try_get::<Option<i64>, _>("rpm_limit").ok().flatten().unwrap_or(cfg.defaults.rpm_limit as i64).max(0) as u32;
        let tpm = r.try_get::<Option<i64>, _>("tpm_limit").ok().flatten().unwrap_or(cfg.defaults.tpm_limit as i64).max(0) as u32;
        let us = Arc::new(UserState::new(id, balance, limit, status as u8, mult, group_id));
        us.set_limits(rpm, tpm);
        users.insert(id, us);
    }

    let rows = q!("SELECT user_id, key_hash FROM api_keys WHERE revoked = 0")
        .fetch_all(pool)
        .await?;
    for r in rows {
        let user_id = Uuid::parse_str(r.get::<String, _>("user_id").as_str())?;
        keys.insert(r.get::<String, _>("key_hash"), KeyEntry { user_id });
    }
    Ok(())
}

// ---- 供应商 / 模型 / 模型组 / 路由(持久化)----
use crate::routing::{ModelDef, ProviderConn, Routing, Target};

pub async fn upsert_provider(
    pool: &Db,
    name: &str,
    kind: &str,
    base_url: &str,
    api_key: Option<&str>,
) -> anyhow::Result<()> {
    q!(
        "INSERT INTO providers (name, kind, base_url, api_key, created_at)
         VALUES (?, ?, ?, ?, ?)
         ON CONFLICT(name) DO UPDATE SET
           kind = excluded.kind, base_url = excluded.base_url, api_key = excluded.api_key",
    )
    .bind(name).bind(kind).bind(base_url).bind(api_key).bind(now_iso())
    .execute(pool).await?;
    Ok(())
}

/// 按 base_url + api_key 查找已有 provider;不存在则创建。返回 provider name。
///
/// 同一个 base_url 用不同 API key 视为不同供应商（可持有不同模型集）。
pub async fn find_or_create_provider(
    pool: &Db,
    kind: &str,
    base_url: &str,
    api_key: Option<&str>,
) -> anyhow::Result<(String, bool)> {
    // 按 base_url + api_key 精确匹配已有 provider
    let existing: Option<String> = q!("SELECT name FROM providers WHERE base_url = ? AND api_key IS NOT DISTINCT FROM ?")
        .bind(base_url).bind(api_key.filter(|k| !k.is_empty()))
        .fetch_optional(pool).await?.map(|r| r.get("name"));
    if let Some(name) = existing {
        return Ok((name, false));
    }
    // 不存在则创建
    let name = format!("prov-{}", &uuid::Uuid::new_v4().to_string()[..8]);
    upsert_provider(pool, &name, kind, base_url, api_key).await?;
    Ok((name, true))
}

/// 按 base_url + api_key 精确查找已有 provider。
pub async fn find_provider_by_credentials(
    pool: &Db,
    base_url: &str,
    api_key: Option<&str>,
) -> anyhow::Result<Option<String>> {
    let name: Option<String> = q!("SELECT name FROM providers WHERE base_url = ? AND api_key IS NOT DISTINCT FROM ?")
        .bind(base_url).bind(api_key.filter(|k| !k.is_empty()))
        .fetch_optional(pool).await?.map(|r| r.get("name"));
    Ok(name)
}

/// 列出所有 provider，附带模型数量。支持分页。
pub async fn list_providers(pool: &Db, page: Option<u32>, page_size: Option<u32>) -> anyhow::Result<(Vec<serde_json::Value>, u64)> {
    let total: i64 = q!("SELECT COUNT(*) as cnt FROM providers").fetch_one(pool).await?.get("cnt");
    let limit = page_size.unwrap_or(20).clamp(1, 100) as i64;
    let current_page = page.unwrap_or(1).max(1);
    let offset = ((current_page - 1) as i64 * limit).max(0);
    let rows = q!(
        "SELECT p.name, p.kind, p.base_url,
                (SELECT COUNT(*) FROM models m WHERE m.provider = p.name) as model_count
         FROM providers p ORDER BY p.name LIMIT ? OFFSET ?"
    ).bind(limit).bind(offset).fetch_all(pool).await?;
    Ok((rows.iter().map(|r| serde_json::json!({
        "name": r.get::<String,_>("name"),
        "kind": r.get::<String,_>("kind"),
        "base_url": r.get::<String,_>("base_url"),
        "model_count": r.get::<i64,_>("model_count"),
    })).collect(), total as u64))
}

/// 级联删除 provider 及其所有模型和关联路由。
pub async fn delete_provider_cascade(pool: &Db, provider_name: &str) -> anyhow::Result<()> {
    // 先删该 provider 下所有模型的路由引用
    q!("DELETE FROM group_routes WHERE model_id IN (SELECT id FROM models WHERE provider = ?)")
        .bind(provider_name).execute(pool).await?;
    // 删模型
    q!("DELETE FROM models WHERE provider = ?")
        .bind(provider_name).execute(pool).await?;
    // 删 provider
    q!("DELETE FROM providers WHERE name = ?")
        .bind(provider_name).execute(pool).await?;
    Ok(())
}

/// 更新供应商的 base_url 和 api_key。
/// api_key 为 None 或空字符串时保持原值不更新。
pub async fn update_provider(
    pool: &Db,
    name: &str,
    base_url: &str,
    api_key: Option<&str>,
) -> anyhow::Result<()> {
    let key = api_key.filter(|k| !k.is_empty());
    if let Some(k) = key {
        q!("UPDATE providers SET base_url = ?, api_key = ? WHERE name = ?")
            .bind(base_url).bind(k).bind(name).execute(pool).await?;
    } else {
        q!("UPDATE providers SET base_url = ? WHERE name = ?")
            .bind(base_url).bind(name).execute(pool).await?;
    }
    Ok(())
}

/// 列出指定 provider 下的所有模型。
pub async fn list_models_by_provider(pool: &Db, provider_name: &str) -> anyhow::Result<Vec<serde_json::Value>> {
    let rows = q!(
        "SELECT m.id, m.upstream_model, m.label
         FROM models m WHERE m.provider = ? ORDER BY m.id"
    ).bind(provider_name).fetch_all(pool).await?;
    Ok(rows.iter().map(|r| serde_json::json!({
        "id": r.get::<i64,_>("id"),
        "upstream_model": r.get::<String,_>("upstream_model"),
        "label": r.get::<Option<String>,_>("label"),
    })).collect())
}

pub async fn add_model(pool: &Db, provider: &str, upstream_model: &str, label: Option<&str>) -> anyhow::Result<i64> {
    let existing: Option<i64> = q!("SELECT id FROM models WHERE provider = ? AND upstream_model = ?")
        .bind(provider).bind(upstream_model)
        .fetch_optional(pool).await?
        .map(|r| r.get("id"));
    if let Some(id) = existing {
        return Ok(id);
    }
    let r = q!("INSERT INTO models (provider, upstream_model, label, created_at) VALUES (?, ?, ?, ?) RETURNING id")
        .bind(provider).bind(upstream_model).bind(label).bind(now_iso())
        .fetch_one(pool).await?;
    Ok(r.get::<i64, _>("id"))
}

/// 添加模型并报告是否新建（用于批量添加时区分「新增」与「已存在」）。
pub async fn add_model_returning(pool: &Db, provider: &str, upstream_model: &str, label: Option<&str>) -> anyhow::Result<(i64, bool)> {
    let existing: Option<i64> = q!("SELECT id FROM models WHERE provider = ? AND upstream_model = ?")
        .bind(provider).bind(upstream_model)
        .fetch_optional(pool).await?
        .map(|r| r.get("id"));
    if let Some(id) = existing {
        return Ok((id, false));
    }
    let r = q!("INSERT INTO models (provider, upstream_model, label, created_at) VALUES (?, ?, ?, ?) RETURNING id")
        .bind(provider).bind(upstream_model).bind(label).bind(now_iso())
        .fetch_one(pool).await?;
    Ok((r.get::<i64, _>("id"), true))
}

/// 更新模型:改其上游连接(provider 的 kind/base_url,密钥仅在提供时覆盖)+ 上游模型名/备注。
pub async fn update_model(
    pool: &Db,
    id: i64,
    kind: &str,
    base_url: &str,
    api_key: Option<&str>,
    upstream_model: &str,
    label: Option<&str>,
) -> anyhow::Result<()> {
    let prov: Option<String> = q!("SELECT provider FROM models WHERE id = ?")
        .bind(id).fetch_optional(pool).await?.map(|r| r.get("provider"));
    if let Some(p) = prov {
        q!("UPDATE providers SET kind = ?, base_url = ? WHERE name = ?")
            .bind(kind).bind(base_url).bind(&p).execute(pool).await?;
        if let Some(k) = api_key.filter(|k| !k.is_empty()) {
            q!("UPDATE providers SET api_key = ? WHERE name = ?")
                .bind(k).bind(&p).execute(pool).await?;
        }
    }
    q!("UPDATE models SET upstream_model = ?, label = ? WHERE id = ?")
        .bind(upstream_model).bind(label).bind(id).execute(pool).await?;
    Ok(())
}

/// 删除模型 + 它的上游连接 + 引用它的组内路由。
pub async fn delete_model_cascade(pool: &Db, id: i64) -> anyhow::Result<()> {
    let prov: Option<String> = q!("SELECT provider FROM models WHERE id = ?")
        .bind(id).fetch_optional(pool).await?.map(|r| r.get("provider"));
    q!("DELETE FROM group_routes WHERE model_id = ?").bind(id).execute(pool).await?;
    q!("DELETE FROM models WHERE id = ?").bind(id).execute(pool).await?;
    // 只有当 provider 下无其他模型时才删除 provider
    if let Some(ref p) = prov {
        let remaining: i64 = q!("SELECT COUNT(*) as cnt FROM models WHERE provider = ?")
            .bind(p).fetch_one(pool).await?.get("cnt");
        if remaining == 0 {
            q!("DELETE FROM providers WHERE name = ?").bind(p).execute(pool).await?;
        }
    }
    Ok(())
}

pub async fn list_models(pool: &Db) -> anyhow::Result<Vec<serde_json::Value>> {
    let rows = q!(
        "SELECT m.id, m.upstream_model, m.label, p.name as provider_name, p.kind, p.base_url, p.api_key
         FROM models m LEFT JOIN providers p ON p.name = m.provider ORDER BY p.name, m.id")
        .fetch_all(pool).await?;
    Ok(rows.iter().map(|r| serde_json::json!({
        "id": r.get::<i64,_>("id"),
        "upstream_model": r.get::<String,_>("upstream_model"),
        "label": r.get::<Option<String>,_>("label"),
        "provider": r.get::<Option<String>,_>("provider_name"),
        "kind": r.get::<Option<String>,_>("kind"),
        "base_url": r.get::<Option<String>,_>("base_url"),
        "api_key": r.get::<Option<String>,_>("api_key"),
    })).collect())
}

/// 某供应商下已存在的模型名集合（前端勾选时用于标记「已添加」）。
pub async fn list_model_names_by_provider(pool: &Db, provider_name: &str) -> anyhow::Result<Vec<String>> {
    let rows = q!("SELECT upstream_model FROM models WHERE provider = ?")
        .bind(provider_name).fetch_all(pool).await?;
    Ok(rows.iter().map(|r| r.get::<String, _>("upstream_model")).collect())
}

pub async fn add_group(pool: &Db, name: &str) -> anyhow::Result<i64> {
    let r = q!("INSERT INTO model_groups (name, created_at) VALUES (?, ?) RETURNING id")
        .bind(name).bind(now_iso()).fetch_one(pool).await?;
    Ok(r.get::<i64, _>("id"))
}

pub async fn delete_group(pool: &Db, id: i64) -> anyhow::Result<()> {
    q!("DELETE FROM group_routes WHERE group_id = ?").bind(id).execute(pool).await?;
    q!("DELETE FROM model_groups WHERE id = ?").bind(id).execute(pool).await?;
    Ok(())
}

pub async fn list_groups(pool: &Db) -> anyhow::Result<Vec<serde_json::Value>> {
    let rows = q!("SELECT id, name, is_active, strategy FROM model_groups ORDER BY id")
        .fetch_all(pool).await?;
    Ok(rows.iter().map(|r| serde_json::json!({
        "id": r.get::<i64,_>("id"),
        "name": r.get::<String,_>("name"),
        "is_active": r.get::<i64,_>("is_active") != 0,
        "strategy": r.get::<Option<String>,_>("strategy").unwrap_or_else(|| "weighted_random".to_string()),
    })).collect())
}

/// 设激活(默认)组:同时只能有一个。注册新用户默认绑定到它。
pub async fn set_active_group(pool: &Db, id: i64) -> anyhow::Result<()> {
    q!("UPDATE model_groups SET is_active = 0 WHERE is_active <> 0").execute(pool).await?;
    q!("UPDATE model_groups SET is_active = 1 WHERE id = ?").bind(id).execute(pool).await?;
    Ok(())
}

/// 设置某组的负载策略。
pub async fn set_group_strategy(pool: &Db, id: i64, strategy: &str) -> anyhow::Result<()> {
    q!("UPDATE model_groups SET strategy = ? WHERE id = ?")
        .bind(strategy).bind(id).execute(pool).await?;
    Ok(())
}

/// 当前激活(默认)组 id;无则 None。
pub async fn active_group_id(pool: &Db) -> anyhow::Result<Option<i64>> {
    let row = q!("SELECT id FROM model_groups WHERE is_active <> 0 ORDER BY id LIMIT 1")
        .fetch_optional(pool).await?;
    Ok(row.map(|r| r.get::<i64, _>("id")))
}

pub async fn add_route(pool: &Db, group_id: i64, public_name: &str, model_id: i64, weight: i64, multiplier: f64) -> anyhow::Result<i64> {
    let r = q!(
        "INSERT INTO group_routes (group_id, public_name, model_id, weight, multiplier, created_at) VALUES (?, ?, ?, ?, ?, ?) RETURNING id")
        .bind(group_id).bind(public_name).bind(model_id).bind(weight).bind(multiplier).bind(now_iso())
        .fetch_one(pool).await?;
    Ok(r.get::<i64, _>("id"))
}

pub async fn update_route(pool: &Db, id: i64, public_name: &str, model_id: i64, weight: i64, multiplier: f64) -> anyhow::Result<()> {
    q!("UPDATE group_routes SET public_name = ?, model_id = ?, weight = ?, multiplier = ? WHERE id = ?")
        .bind(public_name).bind(model_id).bind(weight).bind(multiplier).bind(id)
        .execute(pool).await?;
    Ok(())
}

pub async fn delete_route(pool: &Db, id: i64) -> anyhow::Result<()> {
    q!("DELETE FROM group_routes WHERE id = ?").bind(id).execute(pool).await?;
    Ok(())
}

pub async fn list_routes(pool: &Db, group_id: i64) -> anyhow::Result<Vec<serde_json::Value>> {
    let rows = q!(
        "SELECT r.id, r.public_name, r.model_id, r.weight, r.multiplier, m.provider, m.upstream_model, m.label
         FROM group_routes r JOIN models m ON m.id = r.model_id
         WHERE r.group_id = ? ORDER BY r.public_name")
        .bind(group_id).fetch_all(pool).await?;
    Ok(rows.iter().map(|r| serde_json::json!({
        "id": r.get::<i64,_>("id"), "public_name": r.get::<String,_>("public_name"),
        "model_id": r.get::<i64,_>("model_id"), "weight": r.get::<i64,_>("weight"),
        "multiplier": r.try_get::<f64,_>("multiplier").unwrap_or(1.0),
        "provider": r.get::<String,_>("provider"), "upstream_model": r.get::<String,_>("upstream_model"),
        "label": r.get::<Option<String>,_>("label"),
    })).collect())
}

// ---- 高峰/低谷时段规则 ----
pub struct TimeRuleInput {
    pub group_id: i64,
    pub name: String,
    pub weekdays: String,
    pub start_time: String,
    pub end_time: String,
    pub multiplier: f64,
    pub weight_map: Option<String>,
    pub active: bool,
}

pub async fn list_time_rules(pool: &Db, group_id: i64) -> anyhow::Result<Vec<serde_json::Value>> {
    let rows = q!(
        "SELECT id, group_id, name, weekdays, start_time, end_time, multiplier, weight_map, active, created_at
         FROM time_rules WHERE group_id = ? ORDER BY id")
        .bind(group_id).fetch_all(pool).await?;
    Ok(rows.iter().map(|r| serde_json::json!({
        "id": r.get::<i64,_>("id"), "group_id": r.get::<i64,_>("group_id"),
        "name": r.get::<String,_>("name"), "weekdays": r.get::<String,_>("weekdays"),
        "start_time": r.get::<String,_>("start_time"), "end_time": r.get::<String,_>("end_time"),
        "multiplier": r.try_get::<f64,_>("multiplier").unwrap_or(1.0),
        "weight_map": r.get::<Option<String>,_>("weight_map"),
        "active": r.try_get::<i64,_>("active").unwrap_or(1) != 0,
        "created_at": r.get::<String,_>("created_at"),
    })).collect())
}

pub async fn add_time_rule(pool: &Db, rule: &TimeRuleInput) -> anyhow::Result<i64> {
    let r = q!(
        "INSERT INTO time_rules (group_id, name, weekdays, start_time, end_time, multiplier, weight_map, active, created_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?) RETURNING id")
        .bind(rule.group_id).bind(&rule.name).bind(&rule.weekdays)
        .bind(&rule.start_time).bind(&rule.end_time).bind(rule.multiplier)
        .bind(&rule.weight_map).bind(rule.active as i64).bind(now_iso())
        .fetch_one(pool).await?;
    Ok(r.get::<i64, _>("id"))
}

pub async fn update_time_rule(pool: &Db, id: i64, rule: &TimeRuleInput) -> anyhow::Result<()> {
    q!("UPDATE time_rules SET name = ?, weekdays = ?, start_time = ?, end_time = ?, multiplier = ?, weight_map = ?, active = ? WHERE id = ?")
        .bind(&rule.name).bind(&rule.weekdays).bind(&rule.start_time).bind(&rule.end_time)
        .bind(rule.multiplier).bind(&rule.weight_map).bind(rule.active as i64).bind(id)
        .execute(pool).await?;
    Ok(())
}

pub async fn delete_time_rule(pool: &Db, id: i64) -> anyhow::Result<()> {
    q!("DELETE FROM time_rules WHERE id = ?").bind(id).execute(pool).await?;
    Ok(())
}

pub async fn set_user_group(pool: &Db, user_id: Uuid, group_id: Option<i64>) -> anyhow::Result<()> {
    q!("UPDATE users SET group_id = ? WHERE id = ?")
        .bind(group_id).bind(user_id.to_string()).execute(pool).await?;
    Ok(())
}

/// 从 DB 构建完整内存路由图。
pub async fn load_routing(pool: &Db) -> anyhow::Result<Routing> {
    let mut routing = Routing::default();

    for r in q!("SELECT name, kind, base_url, api_key FROM providers").fetch_all(pool).await? {
        let kind = match r.get::<String, _>("kind").as_str() {
            "anthropic" => ProviderKind::Anthropic,
            _ => ProviderKind::Openai,
        };
        routing.providers.insert(r.get::<String, _>("name"), ProviderConn {
            kind, base_url: r.get("base_url"), api_key: r.get("api_key"), concurrency: None,
        });
    }
    for r in q!("SELECT id, provider, upstream_model FROM models").fetch_all(pool).await? {
        routing.models.insert(r.get::<i64, _>("id"), ModelDef {
            provider: r.get("provider"),
            upstream_model: r.get("upstream_model"),
        });
    }
    for r in q!("SELECT id, name, strategy FROM model_groups").fetch_all(pool).await? {
        let id: i64 = r.get("id");
        routing.group_names.insert(id, r.get("name"));
        routing.group_strategy.insert(id, crate::routing::strategy_from_str(r.try_get("strategy").unwrap_or(None).unwrap_or("weighted_random")));
        routing.groups.entry(id).or_default();
    }
    for r in q!("SELECT group_id, public_name, model_id, weight, multiplier FROM group_routes").fetch_all(pool).await? {
        let gid: i64 = r.get("group_id");
        routing.groups.entry(gid).or_default()
            .entry(r.get::<String, _>("public_name")).or_default()
            .push(Target {
                model_id: r.get("model_id"),
                weight: r.get::<i64, _>("weight") as u32,
                multiplier: r.try_get("multiplier").unwrap_or(1.0),
            });
    }
    // 加载每组的「高峰/低谷」时段规则(倍率 + 权重覆盖)。
    for r in q!("SELECT id, group_id, name, weekdays, start_time, end_time, multiplier, weight_map, active FROM time_rules").fetch_all(pool).await? {
        let gid: i64 = r.get("group_id");
        let wm: std::collections::HashMap<String, std::collections::HashMap<i64, u32>> = r
            .try_get::<Option<String>, _>("weight_map")
            .unwrap_or(None)
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default();
        routing.time_rules.entry(gid).or_default().push(crate::routing::TimeRule {
            id: r.get("id"),
            name: r.get("name"),
            weekdays: r.try_get("weekdays").unwrap_or("0-6".to_string()),
            start_time: r.try_get("start_time").unwrap_or("00:00".to_string()),
            end_time: r.try_get("end_time").unwrap_or("23:59".to_string()),
            multiplier: r.try_get("multiplier").unwrap_or(1.0),
            weight_map: wm,
            active: r.try_get::<i64, _>("active").unwrap_or(1) != 0,
        });
    }
    Ok(routing)
}

/// 把内存中余额回写一条用户记录。
pub async fn flush_balance(pool: &Db, user_id: Uuid, balance: i64) -> anyhow::Result<()> {
    q!("UPDATE users SET token_balance = ? WHERE id = ?")
        .bind(balance)
        .bind(user_id.to_string())
        .execute(pool)
        .await?;
    Ok(())
}

// ---- 用户 ----

pub struct UserRow {
    pub id: Uuid,
    pub username: Option<String>,
    pub email: Option<String>,
    pub phone: Option<String>,
    pub status: i64,
    pub token_balance: i64,
    pub token_used_total: i64,
    pub concurrency_limit: Option<i64>,
    pub bill_multiplier: f64,
    pub group_id: Option<i64>,
    pub rpm_limit: Option<i64>,
    pub tpm_limit: Option<i64>,
    pub source: Option<String>,
    pub created_at: String,
}

/// 管理端创建用户(用户名/密码/邮箱/手机号/模型组)。
#[allow(clippy::too_many_arguments)]
#[allow(clippy::too_many_arguments)]
pub async fn admin_create_user(
    pool: &Db,
    username: &str,
    password_hash: &str,
    email: Option<&str>,
    phone: Option<&str>,
    grant: i64,
    group_id: Option<i64>,
    source: &str,
) -> anyhow::Result<Uuid> {
    let id = Uuid::new_v4();
    q!(
        "INSERT INTO users (id, username, password_hash, email, phone, status, token_balance, group_id, source, created_at)
         VALUES (?, ?, ?, ?, ?, 0, ?, ?, ?, ?)",
    )
    .bind(id.to_string())
    .bind(username)
    .bind(password_hash)
    .bind(email)
    .bind(phone)
    .bind(grant)
    .bind(group_id)
    .bind(source)
    .bind(now_iso())
    .execute(pool)
    .await?;
    Ok(id)
}

/// 删除用户(连同其 API Key);返回被删 Key 的哈希(供内存 map 清理)。
pub async fn delete_user(pool: &Db, id: Uuid) -> anyhow::Result<Vec<String>> {
    let rows = q!("SELECT key_hash FROM api_keys WHERE user_id = ?")
        .bind(id.to_string()).fetch_all(pool).await?;
    let hashes: Vec<String> = rows.iter().map(|r| r.get::<String, _>("key_hash")).collect();
    q!("DELETE FROM api_keys WHERE user_id = ?").bind(id.to_string()).execute(pool).await?;
    q!("DELETE FROM users WHERE id = ?").bind(id.to_string()).execute(pool).await?;
    Ok(hashes)
}

/// 更新用户资料字段(提供了才改;密码传哈希)。
pub async fn update_user_fields(
    pool: &Db,
    id: Uuid,
    username: Option<&str>,
    email: Option<&str>,
    phone: Option<&str>,
    password_hash: Option<&str>,
) -> anyhow::Result<()> {
    let ids = id.to_string();
    if let Some(v) = username {
        q!("UPDATE users SET username = ? WHERE id = ?").bind(v).bind(&ids).execute(pool).await?;
    }
    if let Some(v) = email {
        q!("UPDATE users SET email = ? WHERE id = ?").bind(v).bind(&ids).execute(pool).await?;
    }
    if let Some(v) = phone {
        q!("UPDATE users SET phone = ? WHERE id = ?").bind(v).bind(&ids).execute(pool).await?;
    }
    if let Some(v) = password_hash {
        q!("UPDATE users SET password_hash = ? WHERE id = ?").bind(v).bind(&ids).execute(pool).await?;
    }
    Ok(())
}

/// 按邮箱查用户 id(忘记密码用)。
pub async fn find_user_by_email(pool: &Db, email: &str) -> anyhow::Result<Option<Uuid>> {
    let row = q!("SELECT id FROM users WHERE email = ?")
        .bind(email)
        .fetch_optional(pool)
        .await?;
    match row {
        Some(r) => Ok(Some(Uuid::parse_str(r.get::<String, _>("id").as_str())?)),
        None => Ok(None),
    }
}

/// 按用户名查 (id, 密码哈希),供门户口令登录。
pub async fn find_user_by_username(pool: &Db, username: &str) -> anyhow::Result<Option<(Uuid, Option<String>)>> {
    let row = q!("SELECT id, password_hash FROM users WHERE username = ?")
        .bind(username)
        .fetch_optional(pool)
        .await?;
    match row {
        Some(r) => Ok(Some((Uuid::parse_str(r.get::<String, _>("id").as_str())?, r.get("password_hash")))),
        None => Ok(None),
    }
}

pub async fn get_user(pool: &Db, id: Uuid) -> anyhow::Result<Option<UserRow>> {
    let row = q!(
        "SELECT id, username, email, phone, status, token_balance, token_used_total, concurrency_limit, bill_multiplier, group_id, rpm_limit, tpm_limit, source, created_at
         FROM users WHERE id = ?",
    )
    .bind(id.to_string())
    .fetch_optional(pool)
    .await?;
    row.map(|r| row_to_user(&r)).transpose()
}

// 旧的全量版保留(避免影响其它潜在调用方);当前列表接口走下方 *_page 分页版。
#[allow(dead_code)]
pub async fn list_users(pool: &Db) -> anyhow::Result<Vec<UserRow>> {
    let rows = q!(
        "SELECT id, username, email, phone, status, token_balance, token_used_total, concurrency_limit, bill_multiplier, group_id, rpm_limit, tpm_limit, source, created_at
         FROM users ORDER BY created_at DESC",
    )
    .fetch_all(pool)
    .await?;
    rows.iter().map(row_to_user).collect()
}

/// 分页版用户列表。返回(当前页, 总条数)。
pub async fn list_users_page(
    pool: &Db,
    limit: i64,
    offset: i64,
) -> anyhow::Result<(Vec<UserRow>, i64)> {
    let total: i64 = q!("SELECT COUNT(*) AS c FROM users").fetch_one(pool).await?.get("c");
    let rows = q!(
        "SELECT id, username, email, phone, status, token_balance, token_used_total, concurrency_limit, bill_multiplier, group_id, rpm_limit, tpm_limit, source, created_at
         FROM users ORDER BY created_at DESC LIMIT ? OFFSET ?",
    )
    .bind(limit)
    .bind(offset)
    .fetch_all(pool)
    .await?;
    Ok((rows.iter().map(row_to_user).collect::<anyhow::Result<_>>()?, total))
}

fn row_to_user(r: &sqlx::any::AnyRow) -> anyhow::Result<UserRow> {
    Ok(UserRow {
        id: Uuid::parse_str(r.get::<String, _>("id").as_str())?,
        username: r.try_get("username").unwrap_or(None),
        email: r.try_get("email").unwrap_or(None),
        phone: r.get("phone"),
        status: r.get("status"),
        token_balance: r.get("token_balance"),
        token_used_total: r.get("token_used_total"),
        concurrency_limit: r.get("concurrency_limit"),
        bill_multiplier: r.try_get("bill_multiplier").unwrap_or(1.0),
        group_id: r.try_get::<Option<i64>, _>("group_id").unwrap_or(None),
        rpm_limit: r.try_get::<Option<i64>, _>("rpm_limit").unwrap_or(None),
        tpm_limit: r.try_get::<Option<i64>, _>("tpm_limit").unwrap_or(None),
        source: r.try_get("source").unwrap_or(None),
        created_at: r.get("created_at"),
    })
}

pub async fn update_user(
    pool: &Db,
    id: Uuid,
    concurrency_limit: Option<i64>,
    status: Option<i64>,
    bill_multiplier: Option<f64>,
    rpm_limit: Option<i64>,
    tpm_limit: Option<i64>,
) -> anyhow::Result<()> {
    if let Some(cl) = concurrency_limit {
        q!("UPDATE users SET concurrency_limit = ? WHERE id = ?")
            .bind(cl)
            .bind(id.to_string())
            .execute(pool)
            .await?;
    }
    if let Some(st) = status {
        q!("UPDATE users SET status = ? WHERE id = ?")
            .bind(st)
            .bind(id.to_string())
            .execute(pool)
            .await?;
    }
    if let Some(m) = bill_multiplier {
        q!("UPDATE users SET bill_multiplier = ? WHERE id = ?")
            .bind(m)
            .bind(id.to_string())
            .execute(pool)
            .await?;
    }
    if let Some(rpm) = rpm_limit {
        q!("UPDATE users SET rpm_limit = ? WHERE id = ?")
            .bind(rpm)
            .bind(id.to_string())
            .execute(pool)
            .await?;
    }
    if let Some(tpm) = tpm_limit {
        q!("UPDATE users SET tpm_limit = ? WHERE id = ?")
            .bind(tpm)
            .bind(id.to_string())
            .execute(pool)
            .await?;
    }
    Ok(())
}

pub async fn add_tokens(pool: &Db, id: Uuid, delta: i64) -> anyhow::Result<()> {
    q!("UPDATE users SET token_balance = token_balance + ? WHERE id = ?")
        .bind(delta)
        .bind(id.to_string())
        .execute(pool)
        .await?;
    Ok(())
}

// ---- 奖励任务(后台可配置)----

/// 审核 / 申领校验所需的任务字段。
pub struct RewardTaskRow {
    pub id: i64,
    pub evidence_type: String,
    pub variable: bool,
    pub reward_tokens: i64,
    pub enabled: bool,
}

fn task_row_json(r: &sqlx::any::AnyRow) -> serde_json::Value {
    serde_json::json!({
        "id": r.get::<i64,_>("id"),
        "title": r.get::<String,_>("title"),
        "description": r.get::<Option<String>,_>("description"),
        "evidence_type": r.get::<String,_>("evidence_type"),
        "variable": r.get::<i64,_>("variable") != 0,
        "reward_tokens": r.get::<i64,_>("reward_tokens"),
        "reward_min": r.get::<i64,_>("reward_min"),
        "reward_max": r.get::<i64,_>("reward_max"),
        "link_url": r.get::<Option<String>,_>("link_url"),
        "enabled": r.get::<i64,_>("enabled") != 0,
        "sort": r.get::<i64,_>("sort"),
    })
}

/// 列出奖励任务;`enabled_only` 时仅取启用的(门户用)。
pub async fn list_reward_tasks(pool: &Db, enabled_only: bool) -> anyhow::Result<Vec<serde_json::Value>> {
    let sql = if enabled_only {
        "SELECT id, title, description, evidence_type, variable, reward_tokens, reward_min, reward_max, link_url, enabled, sort
         FROM reward_tasks WHERE enabled <> 0 ORDER BY sort, id"
    } else {
        "SELECT id, title, description, evidence_type, variable, reward_tokens, reward_min, reward_max, link_url, enabled, sort
         FROM reward_tasks ORDER BY sort, id"
    };
    let rows = q!(sql).fetch_all(pool).await?;
    Ok(rows.iter().map(task_row_json).collect())
}

pub async fn get_reward_task(pool: &Db, id: i64) -> anyhow::Result<Option<RewardTaskRow>> {
    let row = q!(
        "SELECT id, evidence_type, variable, reward_tokens, enabled FROM reward_tasks WHERE id = ?",
    )
    .bind(id)
    .fetch_optional(pool)
    .await?;
    Ok(row.map(|r| RewardTaskRow {
        id: r.get("id"),
        evidence_type: r.get("evidence_type"),
        variable: r.get::<i64, _>("variable") != 0,
        reward_tokens: r.get("reward_tokens"),
        enabled: r.get::<i64, _>("enabled") != 0,
    }))
}

#[allow(clippy::too_many_arguments)]
pub async fn create_reward_task(
    pool: &Db,
    title: &str,
    description: Option<&str>,
    evidence_type: &str,
    variable: bool,
    reward_tokens: i64,
    reward_min: i64,
    reward_max: i64,
    link_url: Option<&str>,
    enabled: bool,
    sort: i64,
) -> anyhow::Result<i64> {
    let r = q!(
        "INSERT INTO reward_tasks (title, description, evidence_type, variable, reward_tokens, reward_min, reward_max, link_url, enabled, sort, created_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?) RETURNING id",
    )
    .bind(title)
    .bind(description)
    .bind(evidence_type)
    .bind(variable as i64)
    .bind(reward_tokens)
    .bind(reward_min)
    .bind(reward_max)
    .bind(link_url)
    .bind(enabled as i64)
    .bind(sort)
    .bind(now_iso())
    .fetch_one(pool)
    .await?;
    Ok(r.get::<i64, _>("id"))
}

#[allow(clippy::too_many_arguments)]
pub async fn update_reward_task(
    pool: &Db,
    id: i64,
    title: &str,
    description: Option<&str>,
    evidence_type: &str,
    variable: bool,
    reward_tokens: i64,
    reward_min: i64,
    reward_max: i64,
    link_url: Option<&str>,
    enabled: bool,
    sort: i64,
) -> anyhow::Result<()> {
    q!(
        "UPDATE reward_tasks SET title = ?, description = ?, evidence_type = ?, variable = ?, reward_tokens = ?, reward_min = ?, reward_max = ?, link_url = ?, enabled = ?, sort = ? WHERE id = ?",
    )
    .bind(title)
    .bind(description)
    .bind(evidence_type)
    .bind(variable as i64)
    .bind(reward_tokens)
    .bind(reward_min)
    .bind(reward_max)
    .bind(link_url)
    .bind(enabled as i64)
    .bind(sort)
    .bind(id)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn delete_reward_task(pool: &Db, id: i64) -> anyhow::Result<()> {
    q!("DELETE FROM reward_tasks WHERE id = ?").bind(id).execute(pool).await?;
    Ok(())
}

/// 首次启动:若无任务,播种 3 个默认任务(star / issue / 提建议)。
/// 仅作为开箱即用的示例,之后由管理员在后台增删改。
pub async fn seed_reward_tasks_if_empty(pool: &Db) -> anyhow::Result<()> {
    let c: i64 = q!("SELECT COUNT(*) AS c FROM reward_tasks").fetch_one(pool).await?.get("c");
    if c > 0 {
        return Ok(());
    }
    let repo = "https://github.com/stoneaigc/Relay";
    create_reward_task(
        pool,
        "给 Relay 点 Star",
        Some("为项目点亮 GitHub Star,上传 star 后的截图即可申领。"),
        "screenshot",
        false,
        5_000_000,
        0,
        0,
        Some(repo),
        true,
        1,
    )
    .await?;
    create_reward_task(
        pool,
        "使用 Relay 并提交 Issue",
        Some("在 GitHub 提交一个有效 issue,填写 issue 链接即可申领。"),
        "link",
        false,
        10_000_000,
        0,
        0,
        Some(repo),
        true,
        2,
    )
    .await?;
    create_reward_task(
        pool,
        "给 Relay 提建议",
        Some("用纯文本写下你的产品建议,额度由管理员评定后入账。"),
        "text",
        true,
        0,
        1_000_000,
        10_000_000,
        Some(repo),
        true,
        3,
    )
    .await?;
    Ok(())
}

// ---- 奖励申领(关联任务,后台人工审核)----

/// 审核所需的最小字段(含所属任务的额度规则)。
pub struct RewardClaimRow {
    pub user_id: Uuid,
    pub reward_tokens: i64,
    pub status: i64,
    pub variable: bool,
    pub reward_min: i64,
    pub reward_max: i64,
}

/// 是否已有未被驳回(待审 0 或已通过 1)的同任务申领,避免重复提交/重复领取。
pub async fn has_open_reward_claim(pool: &Db, user_id: Uuid, task_id: i64) -> anyhow::Result<bool> {
    let c: i64 = q!(
        "SELECT COUNT(*) AS c FROM reward_claims WHERE user_id = ? AND task_id = ? AND status IN (0, 1)",
    )
    .bind(user_id.to_string())
    .bind(task_id)
    .fetch_one(pool)
    .await?
    .get("c");
    Ok(c > 0)
}

/// 创建一条待审申领(kind 为兼容旧列写空串)。
pub async fn create_reward_claim(
    pool: &Db,
    user_id: Uuid,
    task_id: i64,
    evidence: Option<&str>,
    reward_tokens: i64,
) -> anyhow::Result<Uuid> {
    let id = Uuid::new_v4();
    q!(
        "INSERT INTO reward_claims (id, user_id, task_id, kind, evidence, reward_tokens, status, created_at)
         VALUES (?, ?, ?, '', ?, ?, 0, ?)",
    )
    .bind(id.to_string())
    .bind(user_id.to_string())
    .bind(task_id)
    .bind(evidence)
    .bind(reward_tokens)
    .bind(now_iso())
    .execute(pool)
    .await?;
    Ok(id)
}

/// 某用户的申领记录(门户「我的奖励」),带任务标题。
pub async fn list_user_reward_claims(pool: &Db, user_id: Uuid) -> anyhow::Result<Vec<serde_json::Value>> {
    let rows = q!(
        "SELECT c.id, c.task_id, COALESCE(t.title, NULLIF(c.kind, '')) AS title, c.evidence, c.reward_tokens, c.status, c.review_note, c.created_at, c.reviewed_at
         FROM reward_claims c LEFT JOIN reward_tasks t ON t.id = c.task_id
         WHERE c.user_id = ? ORDER BY c.created_at DESC",
    )
    .bind(user_id.to_string())
    .fetch_all(pool)
    .await?;
    Ok(rows.iter().map(|r| serde_json::json!({
        "id": r.get::<String,_>("id"),
        "task_id": r.get::<Option<i64>,_>("task_id"),
        "title": r.get::<Option<String>,_>("title"),
        "evidence": r.get::<Option<String>,_>("evidence"),
        "reward_tokens": r.get::<i64,_>("reward_tokens"),
        "status": r.get::<i64,_>("status"),
        "review_note": r.get::<Option<String>,_>("review_note"),
        "created_at": r.get::<String,_>("created_at"),
        "reviewed_at": r.get::<Option<String>,_>("reviewed_at"),
    })).collect())
}

/// 管理端列出申领(可按状态过滤),带用户标识与任务信息。
/// 列出奖励申领(全量版,保留);分页请用 list_reward_claims_page。
#[allow(dead_code)]
pub async fn list_reward_claims(pool: &Db, status: Option<i64>) -> anyhow::Result<Vec<serde_json::Value>> {
    let base = "SELECT c.id, c.user_id, c.task_id, COALESCE(t.title, NULLIF(c.kind, '')) AS title, t.evidence_type, t.variable, t.reward_min, t.reward_max,
                    c.evidence, c.reward_tokens, c.status, c.review_note, c.created_at, c.reviewed_at,
                    COALESCE(u.username, u.email, u.phone) AS name
             FROM reward_claims c
             LEFT JOIN users u ON u.id = c.user_id
             LEFT JOIN reward_tasks t ON t.id = c.task_id";
    let rows = if let Some(st) = status {
        q!(&format!("{base} WHERE c.status = ? ORDER BY c.created_at DESC"))
            .bind(st)
            .fetch_all(pool)
            .await?
    } else {
        q!(&format!("{base} ORDER BY c.created_at DESC")).fetch_all(pool).await?
    };
    Ok(rows.iter().map(|r| serde_json::json!({
        "id": r.get::<String,_>("id"),
        "user_id": r.get::<String,_>("user_id"),
        "name": r.get::<Option<String>,_>("name"),
        "task_id": r.get::<Option<i64>,_>("task_id"),
        "title": r.get::<Option<String>,_>("title"),
        "evidence_type": r.get::<Option<String>,_>("evidence_type"),
        "variable": r.get::<Option<i64>,_>("variable").map(|v| v != 0).unwrap_or(false),
        "reward_min": r.get::<Option<i64>,_>("reward_min").unwrap_or(0),
        "reward_max": r.get::<Option<i64>,_>("reward_max").unwrap_or(0),
        "evidence": r.get::<Option<String>,_>("evidence"),
        "reward_tokens": r.get::<i64,_>("reward_tokens"),
        "status": r.get::<i64,_>("status"),
        "review_note": r.get::<Option<String>,_>("review_note"),
        "created_at": r.get::<String,_>("created_at"),
        "reviewed_at": r.get::<Option<String>,_>("reviewed_at"),
    })).collect())
}

/// 分页版奖励申领列表(保留 status 过滤)。返回(当前页, 总条数)。
pub async fn list_reward_claims_page(
    pool: &Db,
    status: Option<i64>,
    limit: i64,
    offset: i64,
) -> anyhow::Result<(Vec<serde_json::Value>, i64)> {
    let base = "SELECT c.id, c.user_id, c.task_id, COALESCE(t.title, NULLIF(c.kind, '')) AS title, t.evidence_type, t.variable, t.reward_min, t.reward_max,
                    c.evidence, c.reward_tokens, c.status, c.review_note, c.created_at, c.reviewed_at,
                    COALESCE(u.username, u.email, u.phone) AS name
             FROM reward_claims c
             LEFT JOIN users u ON u.id = c.user_id
             LEFT JOIN reward_tasks t ON t.id = c.task_id";
    let total: i64 = if let Some(st) = status {
        q!("SELECT COUNT(*) AS c FROM reward_claims c WHERE c.status = ?")
            .bind(st)
            .fetch_one(pool)
            .await?
            .get("c")
    } else {
        q!("SELECT COUNT(*) AS c FROM reward_claims c").fetch_one(pool).await?.get("c")
    };
    let rows = if let Some(st) = status {
        q!(&format!("{base} WHERE c.status = ? ORDER BY c.created_at DESC LIMIT ? OFFSET ?"))
            .bind(st)
            .bind(limit)
            .bind(offset)
            .fetch_all(pool)
            .await?
    } else {
        q!(&format!("{base} ORDER BY c.created_at DESC LIMIT ? OFFSET ?"))
            .bind(limit)
            .bind(offset)
            .fetch_all(pool)
            .await?
    };
    Ok((
        rows.iter()
            .map(|r| {
                serde_json::json!({
                    "id": r.get::<String,_>("id"),
                    "user_id": r.get::<String,_>("user_id"),
                    "name": r.get::<Option<String>,_>("name"),
                    "task_id": r.get::<Option<i64>,_>("task_id"),
                    "title": r.get::<Option<String>,_>("title"),
                    "evidence_type": r.get::<Option<String>,_>("evidence_type"),
                    "variable": r.get::<Option<i64>,_>("variable").map(|v| v != 0).unwrap_or(false),
                    "reward_min": r.get::<Option<i64>,_>("reward_min").unwrap_or(0),
                    "reward_max": r.get::<Option<i64>,_>("reward_max").unwrap_or(0),
                    "evidence": r.get::<Option<String>,_>("evidence"),
                    "reward_tokens": r.get::<i64,_>("reward_tokens"),
                    "status": r.get::<i64,_>("status"),
                    "review_note": r.get::<Option<String>,_>("review_note"),
                    "created_at": r.get::<String,_>("created_at"),
                    "reviewed_at": r.get::<Option<String>,_>("reviewed_at"),
                })
            })
            .collect(),
        total,
    ))
}

/// 取申领的审核字段(含所属任务的额度规则)。
pub async fn get_reward_claim(pool: &Db, id: Uuid) -> anyhow::Result<Option<RewardClaimRow>> {
    let row = q!(
        "SELECT c.user_id, c.reward_tokens, c.status,
                COALESCE(t.variable, 0) AS variable, COALESCE(t.reward_min, 0) AS reward_min, COALESCE(t.reward_max, 0) AS reward_max
         FROM reward_claims c LEFT JOIN reward_tasks t ON t.id = c.task_id WHERE c.id = ?",
    )
    .bind(id.to_string())
    .fetch_optional(pool)
    .await?;
    match row {
        Some(r) => Ok(Some(RewardClaimRow {
            user_id: Uuid::parse_str(r.get::<String, _>("user_id").as_str())?,
            reward_tokens: r.get("reward_tokens"),
            status: r.get("status"),
            variable: r.get::<i64, _>("variable") != 0,
            reward_min: r.get("reward_min"),
            reward_max: r.get("reward_max"),
        })),
        None => Ok(None),
    }
}

/// 审核:仅当前为待审(status=0)时改写,返回受影响行数(1=本次确实完成审核)。
/// 用 `WHERE status = 0` 保证并发/重复点击下奖励不会被入账两次。
/// `reward_tokens` 为 Some 时一并改写入账额度(提建议由管理员评定)。
pub async fn review_reward_claim(
    pool: &Db,
    id: Uuid,
    status: i64,
    note: Option<&str>,
    reward_tokens: Option<i64>,
) -> anyhow::Result<u64> {
    let r = if let Some(amt) = reward_tokens {
        q!(
            "UPDATE reward_claims SET status = ?, review_note = ?, reviewed_at = ?, reward_tokens = ? WHERE id = ? AND status = 0",
        )
        .bind(status)
        .bind(note)
        .bind(now_iso())
        .bind(amt)
        .bind(id.to_string())
        .execute(pool)
        .await?
    } else {
        q!(
            "UPDATE reward_claims SET status = ?, review_note = ?, reviewed_at = ? WHERE id = ? AND status = 0",
        )
        .bind(status)
        .bind(note)
        .bind(now_iso())
        .bind(id.to_string())
        .execute(pool)
        .await?
    };
    Ok(r.rows_affected())
}

// ---- API Key ----

pub struct KeyRow {
    pub id: Uuid,
    pub interface_kind: String,
    pub key_prefix: String,
    pub revoked: i64,
    pub created_at: String,
}

/// 创建一把 Key,写库;返回 (明文, 哈希, KeyRow)。明文只此一次。
pub async fn create_key(
    pool: &Db,
    user_id: Uuid,
    interface_kind: &str,
) -> anyhow::Result<(String, String, KeyRow)> {
    let (plaintext, hash, prefix) = generate_key();
    let id = Uuid::new_v4();
    let created = now_iso();
    q!(
        "INSERT INTO api_keys (id, user_id, interface_kind, key_hash, key_prefix, created_at)
         VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(id.to_string())
    .bind(user_id.to_string())
    .bind(interface_kind)
    .bind(&hash)
    .bind(&prefix)
    .bind(&created)
    .execute(pool)
    .await?;
    Ok((
        plaintext,
        hash,
        KeyRow {
            id,
            interface_kind: interface_kind.to_string(),
            key_prefix: prefix,
            revoked: 0,
            created_at: created,
        },
    ))
}

/// 吊销某用户某接口下所有有效 Key,返回被吊销的哈希(供内存 map 删除)。
pub async fn revoke_active_keys(
    pool: &Db,
    user_id: Uuid,
    interface_kind: &str,
) -> anyhow::Result<Vec<String>> {
    let rows = q!(
        "SELECT key_hash FROM api_keys WHERE user_id = ? AND interface_kind = ? AND revoked = 0",
    )
    .bind(user_id.to_string())
    .bind(interface_kind)
    .fetch_all(pool)
    .await?;
    let hashes: Vec<String> = rows.iter().map(|r| r.get::<String, _>("key_hash")).collect();
    q!("UPDATE api_keys SET revoked = 1 WHERE user_id = ? AND interface_kind = ?")
        .bind(user_id.to_string())
        .bind(interface_kind)
        .execute(pool)
        .await?;
    Ok(hashes)
}

pub async fn list_keys(pool: &Db, user_id: Uuid) -> anyhow::Result<Vec<KeyRow>> {
    let rows = q!(
        "SELECT id, interface_kind, key_prefix, revoked, created_at
         FROM api_keys WHERE user_id = ? ORDER BY created_at DESC",
    )
    .bind(user_id.to_string())
    .fetch_all(pool)
    .await?;
    rows.iter()
        .map(|r| {
            Ok(KeyRow {
                id: Uuid::parse_str(r.get::<String, _>("id").as_str())?,
                interface_kind: r.get("interface_kind"),
                key_prefix: r.get("key_prefix"),
                revoked: r.get("revoked"),
                created_at: r.get("created_at"),
            })
        })
        .collect()
}

// ---- 用量查询 ----

/// 用量日志(全量版,保留);分页请用 usage_rows_page。
#[allow(dead_code)]
pub async fn usage_rows(
    pool: &Db,
    user_id: Option<Uuid>,
    limit: i64,
) -> anyhow::Result<Vec<serde_json::Value>> {
    let rows = if let Some(uid) = user_id {
        q!(
            "SELECT user_id, model, provider, input_tokens, output_tokens, charged_tokens, status, created_at
             FROM usage_logs WHERE user_id = ? ORDER BY id DESC LIMIT ?",
        )
        .bind(uid.to_string())
        .bind(limit)
        .fetch_all(pool)
        .await?
    } else {
        q!(
            "SELECT user_id, model, provider, input_tokens, output_tokens, charged_tokens, status, created_at
             FROM usage_logs ORDER BY id DESC LIMIT ?",
        )
        .bind(limit)
        .fetch_all(pool)
        .await?
    };
    Ok(rows
        .iter()
        .map(|r| {
            serde_json::json!({
                "user_id": r.get::<String, _>("user_id"),
                "model": r.get::<Option<String>, _>("model"),
                "provider": r.get::<Option<String>, _>("provider"),
                "input_tokens": r.get::<Option<i64>, _>("input_tokens"),
                "output_tokens": r.get::<Option<i64>, _>("output_tokens"),
                "charged_tokens": r.get::<Option<i64>, _>("charged_tokens"),
                "status": r.get::<Option<i64>, _>("status"),
                "created_at": r.get::<String, _>("created_at"),
            })
        })
        .collect())
}

/// 分页版用量日志。返回(当前页, 总条数)。
pub async fn usage_rows_page(
    pool: &Db,
    user_id: Option<Uuid>,
    page_size: i64,
    offset: i64,
) -> anyhow::Result<(Vec<serde_json::Value>, i64)> {
    let total: i64 = if let Some(uid) = user_id {
        q!("SELECT COUNT(*) AS c FROM usage_logs WHERE user_id = ?")
            .bind(uid.to_string())
            .fetch_one(pool)
            .await?
            .get("c")
    } else {
        q!("SELECT COUNT(*) AS c FROM usage_logs").fetch_one(pool).await?.get("c")
    };
    let rows = if let Some(uid) = user_id {
        q!(
            "SELECT user_id, model, provider, input_tokens, output_tokens, charged_tokens, status, created_at
             FROM usage_logs WHERE user_id = ? ORDER BY id DESC LIMIT ? OFFSET ?",
        )
        .bind(uid.to_string())
        .bind(page_size)
        .bind(offset)
        .fetch_all(pool)
        .await?
    } else {
        q!(
            "SELECT user_id, model, provider, input_tokens, output_tokens, charged_tokens, status, created_at
             FROM usage_logs ORDER BY id DESC LIMIT ? OFFSET ?",
        )
        .bind(page_size)
        .bind(offset)
        .fetch_all(pool)
        .await?
    };
    Ok((
        rows.iter()
            .map(|r| {
                serde_json::json!({
                    "user_id": r.get::<String, _>("user_id"),
                    "model": r.get::<Option<String>, _>("model"),
                    "provider": r.get::<Option<String>, _>("provider"),
                    "input_tokens": r.get::<Option<i64>, _>("input_tokens"),
                    "output_tokens": r.get::<Option<i64>, _>("output_tokens"),
                    "charged_tokens": r.get::<Option<i64>, _>("charged_tokens"),
                    "status": r.get::<Option<i64>, _>("status"),
                    "created_at": r.get::<String, _>("created_at"),
                })
            })
            .collect(),
        total,
    ))
}

// ---- 统计聚合 ----

/// 某用户累计已用 token(SUM charged)。
pub async fn user_used(pool: &Db, user_id: Uuid) -> anyhow::Result<i64> {
    let r = q!("SELECT CAST(COALESCE(SUM(charged_tokens),0) AS BIGINT) AS used FROM usage_logs WHERE user_id = ?")
        .bind(user_id.to_string()).fetch_one(pool).await?;
    Ok(r.get("used"))
}

/// 管理端概览统计(累计)。
pub async fn overview(pool: &Db) -> anyhow::Result<serde_json::Value> {
    let total_users: i64 = q!("SELECT COUNT(*) AS c FROM users").fetch_one(pool).await?.get("c");
    let active: i64 = q!("SELECT COUNT(*) AS c FROM users WHERE status = 0").fetch_one(pool).await?.get("c");
    let total_balance: i64 = q!("SELECT CAST(COALESCE(SUM(token_balance),0) AS BIGINT) AS s FROM users").fetch_one(pool).await?.get("s");
    let agg = q!("SELECT CAST(COALESCE(SUM(charged_tokens),0) AS BIGINT) AS used, COUNT(*) AS reqs FROM usage_logs").fetch_one(pool).await?;
    let total_used: i64 = agg.get("used");
    let total_requests: i64 = agg.get("reqs");

    let top = q!(
        "SELECT l.user_id AS uid, COALESCE(u.username, u.phone) AS name,
                CAST(COALESCE(SUM(l.charged_tokens),0) AS BIGINT) AS used, COUNT(*) AS calls
         FROM usage_logs l LEFT JOIN users u ON u.id = l.user_id
         GROUP BY l.user_id, u.username, u.phone ORDER BY used DESC LIMIT 20")
        .fetch_all(pool).await?;
    let top_users: Vec<serde_json::Value> = top.iter().map(|r| serde_json::json!({
        "user_id": r.get::<String,_>("uid"),
        "name": r.get::<Option<String>,_>("name"),
        "used": r.get::<i64,_>("used"),
        "calls": r.get::<i64,_>("calls"),
    })).collect();

    let bm = q!(
        "SELECT model, CAST(COALESCE(SUM(charged_tokens),0) AS BIGINT) AS used, COUNT(*) AS calls
         FROM usage_logs GROUP BY model ORDER BY used DESC LIMIT 50")
        .fetch_all(pool).await?;
    let by_model: Vec<serde_json::Value> = bm.iter().map(|r| serde_json::json!({
        "model": r.get::<Option<String>,_>("model"),
        "used": r.get::<i64,_>("used"),
        "calls": r.get::<i64,_>("calls"),
    })).collect();

    Ok(serde_json::json!({
        "total_users": total_users,
        "active_users": active,
        "total_balance": total_balance,
        "total_used": total_used,
        "total_requests": total_requests,
        "top_users": top_users,
        "by_model": by_model,
    }))
}

/// 写一条用量日志。
pub async fn insert_usage(
    pool: &Db,
    ev: &crate::state::UsageEvent,
) -> anyhow::Result<()> {
    q!(
        "INSERT INTO usage_logs
         (user_id, model, provider, upstream_model, input_tokens, output_tokens, charged_tokens, status, ts, created_at, request_id)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(ev.user_id.to_string())
    .bind(&ev.model)
    .bind(&ev.provider)
    .bind(&ev.upstream_model)
    .bind(ev.input_tokens as i64)
    .bind(ev.output_tokens as i64)
    .bind(ev.charged_tokens)
    .bind(ev.status as i64)
    .bind(now_secs())
    .bind(now_iso())
    .bind(ev.request_id.as_deref())
    .execute(pool)
    .await?;
    Ok(())
}

/// 各用户消耗汇总:user_id -> sum(charged)。since 为 None 取全部,Some(ts) 取该时间之后。
pub async fn usage_by_user(pool: &Db, since: Option<i64>) -> anyhow::Result<std::collections::HashMap<String, i64>> {
    let rows = if let Some(ts) = since {
        q!("SELECT user_id, CAST(COALESCE(SUM(charged_tokens),0) AS BIGINT) AS s FROM usage_logs WHERE ts >= ? GROUP BY user_id")
            .bind(ts).fetch_all(pool).await?
    } else {
        q!("SELECT user_id, CAST(COALESCE(SUM(charged_tokens),0) AS BIGINT) AS s FROM usage_logs GROUP BY user_id")
            .fetch_all(pool).await?
    };
    Ok(rows.iter().map(|r| (r.get::<String, _>("user_id"), r.get::<i64, _>("s"))).collect())
}

/// 本地「今日 00:00」对应的 unix 秒(按时区偏移小时)。
pub fn today_start(tz_offset_hours: i64) -> i64 {
    let offset = tz_offset_hours * 3600;
    let now = now_secs();
    now - ((now + offset).rem_euclid(86400))
}

/// 公历日期 ⇄ 自 1970-01-01 起的天数(Howard Hinnant 算法,无需 chrono)。
fn days_from_civil(y: i64, m: u32, d: u32) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let mi = if m > 2 { m - 3 } else { m + 9 } as i64;
    let doy = (153 * mi + 2) / 5 + d as i64 - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe - 719468
}

fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

/// 全站消耗趋势(按 日 / 周 / 月 聚合,含 0 值补齐)。
/// `granularity`: "day" | "week" | "month";`periods` 为返回的桶数量。
/// 返回 [{ts: 桶起始本地0点的unix秒, tokens, calls}],按时间升序。
pub async fn global_series(
    pool: &Db,
    granularity: &str,
    periods: i64,
    tz_offset_hours: i64,
) -> anyhow::Result<Vec<serde_json::Value>> {
    let offset = tz_offset_hours * 3600;
    let today0 = today_start(tz_offset_hours);
    let today_idx = (today0 + offset) / 86400; // 本地「今天」距 1970-01-01 的天数
    let periods = periods.max(1);

    // 各桶的「起始日索引」(升序);最后一个桶即当前(可能未满)的周期。
    let mut starts: Vec<i64> = Vec::new();
    match granularity {
        "week" => {
            // 周一为一周起点:周一的日索引 ≡ 4 (mod 7)。
            let end = today_idx - (today_idx - 4).rem_euclid(7);
            let mut d = end - (periods - 1) * 7;
            while d <= end {
                starts.push(d);
                d += 7;
            }
        }
        "month" => {
            let (ey, em, _) = civil_from_days(today_idx);
            let end_ym = ey * 12 + (em as i64 - 1);
            let mut k = end_ym - (periods - 1);
            while k <= end_ym {
                let y = k.div_euclid(12);
                let m = (k.rem_euclid(12) + 1) as u32;
                starts.push(days_from_civil(y, m, 1));
                k += 1;
            }
        }
        _ => {
            let mut d = today_idx - (periods - 1);
            while d <= today_idx {
                starts.push(d);
                d += 1;
            }
        }
    }

    // 一次性取范围内每日聚合,再按桶累加。
    let range_start = starts[0] * 86400 - offset;
    let rows = q!(
        "SELECT ((ts + ?) / 86400) AS d, CAST(COALESCE(SUM(charged_tokens),0) AS BIGINT) AS tok, COUNT(*) AS c
         FROM usage_logs WHERE ts >= ? GROUP BY d",
    )
    .bind(offset)
    .bind(range_start)
    .fetch_all(pool)
    .await?;

    let mut daily: std::collections::HashMap<i64, (i64, i64)> = std::collections::HashMap::new();
    for r in rows {
        daily.insert(r.get::<i64, _>("d"), (r.get::<i64, _>("tok"), r.get::<i64, _>("c")));
    }

    let mut out = Vec::with_capacity(starts.len());
    for (i, &lo) in starts.iter().enumerate() {
        let hi = if i + 1 < starts.len() { starts[i + 1] - 1 } else { today_idx };
        let (mut tok, mut calls) = (0i64, 0i64);
        let mut d = lo;
        while d <= hi {
            if let Some(&(t, c)) = daily.get(&d) {
                tok += t;
                calls += c;
            }
            d += 1;
        }
        out.push(serde_json::json!({ "ts": lo * 86400 - offset, "tokens": tok, "calls": calls }));
    }
    Ok(out)
}

/// 某用户最近 `days` 天的每日消耗序列(含 0 值,按本地日切分)。
/// 返回 [{ts: 当天本地0点的unix秒, tokens, calls}]。
pub async fn user_daily_series(
    pool: &Db,
    user_id: Uuid,
    days: i64,
    tz_offset_hours: i64,
) -> anyhow::Result<Vec<serde_json::Value>> {
    let offset = tz_offset_hours * 3600;
    let today0 = today_start(tz_offset_hours);
    let start = today0 - (days - 1).max(0) * 86400;

    let rows = q!(
        "SELECT ((ts + ?) / 86400) AS d, CAST(COALESCE(SUM(charged_tokens),0) AS BIGINT) AS tok, COUNT(*) AS c
         FROM usage_logs WHERE user_id = ? AND ts >= ? GROUP BY d",
    )
    .bind(offset)
    .bind(user_id.to_string())
    .bind(start)
    .fetch_all(pool)
    .await?;

    let mut map: std::collections::HashMap<i64, (i64, i64)> = std::collections::HashMap::new();
    for r in rows {
        map.insert(r.get::<i64, _>("d"), (r.get::<i64, _>("tok"), r.get::<i64, _>("c")));
    }

    let start_d = (start + offset) / 86400;
    let today_d = (today0 + offset) / 86400;
    let mut out = Vec::new();
    let mut d = start_d;
    while d <= today_d {
        let (tok, c) = map.get(&d).copied().unwrap_or((0, 0));
        out.push(serde_json::json!({ "ts": d * 86400 - offset, "tokens": tok, "calls": c }));
        d += 1;
    }
    Ok(out)
}

// ---- 系统配置(settings KV 表)----

/// 读取某命名空间下所有配置(prefix 如 "email.",匹配 key 前缀)。返回 key(去前缀) -> value。
pub async fn load_settings(pool: &Db, prefix: &str) -> anyhow::Result<std::collections::HashMap<String, String>> {
    let like = format!("{prefix}%");
    let rows = q!("SELECT key, value FROM settings WHERE key LIKE ?")
        .bind(&like)
        .fetch_all(pool)
        .await?;
    let mut out = std::collections::HashMap::new();
    for r in rows {
        let k: String = r.get("key");
        let v: String = r.get("value");
        // 存完整 key(含前缀),由调用方按需裁剪。
        out.insert(k, v);
    }
    Ok(out)
}

/// upsert 单个配置项。
pub async fn set_setting(pool: &Db, key: &str, value: &str) -> anyhow::Result<()> {
    q!(
        "INSERT INTO settings (key, value, updated_at) VALUES (?, ?, ?)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at",
    )
    .bind(key)
    .bind(value)
    .bind(now_iso())
    .execute(pool)
    .await?;
    Ok(())
}

/// 批量 upsert(逐条;SQLite/Postgres 均支持上述 ON CONFLICT 语法)。
pub async fn set_settings(pool: &Db, items: &[(String, String)]) -> anyhow::Result<()> {
    for (k, v) in items {
        set_setting(pool, k, v).await?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 创建内存 SQLite 数据库并初始化 schema，供测试使用。
    async fn test_db() -> Db {
        sqlx::any::install_default_drivers();
        let _ = BACKEND.set(Backend::Sqlite);
        let pool = AnyPoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:?mode=rwc")
            .await
            .expect("failed to connect in-memory sqlite");
        init_schema(&pool).await.expect("init_schema failed");
        pool
    }

    // ---- find_or_create_provider ----

    #[tokio::test]
    async fn find_or_create_provider_creates_new() {
        let db = test_db().await;
        let (name, created) = find_or_create_provider(&db, "openai", "https://api.test.com/v1", Some("sk-aaa"))
            .await.unwrap();
        assert!(created);
        assert!(name.starts_with("prov-"));
    }

    #[tokio::test]
    async fn find_or_create_provider_reuses_same_credentials() {
        let db = test_db().await;
        let (n1, c1) = find_or_create_provider(&db, "openai", "https://api.test.com/v1", Some("sk-aaa"))
            .await.unwrap();
        let (n2, c2) = find_or_create_provider(&db, "openai", "https://api.test.com/v1", Some("sk-aaa"))
            .await.unwrap();
        assert!(c1);
        assert!(!c2);
        assert_eq!(n1, n2);
    }

    #[tokio::test]
    async fn find_or_create_provider_different_key_creates_new() {
        let db = test_db().await;
        let (n1, _) = find_or_create_provider(&db, "openai", "https://api.test.com/v1", Some("sk-aaa"))
            .await.unwrap();
        let (n2, created) = find_or_create_provider(&db, "openai", "https://api.test.com/v1", Some("sk-bbb"))
            .await.unwrap();
        assert!(created);
        assert_ne!(n1, n2);
    }

    #[tokio::test]
    async fn find_or_create_provider_both_empty_key() {
        let db = test_db().await;
        let (n1, _) = find_or_create_provider(&db, "openai", "https://api.test.com/v1", None)
            .await.unwrap();
        let (n2, created) = find_or_create_provider(&db, "openai", "https://api.test.com/v1", None)
            .await.unwrap();
        assert!(!created);
        assert_eq!(n1, n2);
    }

    // ---- find_provider_by_credentials ----

    #[tokio::test]
    async fn find_provider_by_credentials_match() {
        let db = test_db().await;
        let (name, _) = find_or_create_provider(&db, "openai", "https://api.test.com/v1", Some("sk-aaa"))
            .await.unwrap();
        let found = find_provider_by_credentials(&db, "https://api.test.com/v1", Some("sk-aaa"))
            .await.unwrap();
        assert_eq!(found.as_deref(), Some(name.as_str()));
    }

    #[tokio::test]
    async fn find_provider_by_credentials_no_match_different_key() {
        let db = test_db().await;
        let _ = find_or_create_provider(&db, "openai", "https://api.test.com/v1", Some("sk-aaa"))
            .await.unwrap();
        let found = find_provider_by_credentials(&db, "https://api.test.com/v1", Some("sk-bbb"))
            .await.unwrap();
        assert!(found.is_none());
    }

    #[tokio::test]
    async fn find_provider_by_credentials_no_match_different_url() {
        let db = test_db().await;
        let _ = find_or_create_provider(&db, "openai", "https://api.test.com/v1", Some("sk-aaa"))
            .await.unwrap();
        let found = find_provider_by_credentials(&db, "https://other.com/v1", Some("sk-aaa"))
            .await.unwrap();
        assert!(found.is_none());
    }

    // ---- add_model_returning 幂等性 ----

    #[tokio::test]
    async fn add_model_returning_dedup() {
        let db = test_db().await;
        let (provider, _) = find_or_create_provider(&db, "openai", "https://api.test.com/v1", Some("sk-x"))
            .await.unwrap();
        let (_, is_new1) = add_model_returning(&db, &provider, "gpt-4o", None).await.unwrap();
        let (_, is_new2) = add_model_returning(&db, &provider, "gpt-4o", None).await.unwrap();
        assert!(is_new1);
        assert!(!is_new2);
        // 第三个不同模型
        let (_, is_new3) = add_model_returning(&db, &provider, "gpt-4o-mini", None).await.unwrap();
        assert!(is_new3);
    }

    // ---- list_providers 分页 ----

    #[tokio::test]
    async fn list_providers_empty() {
        let db = test_db().await;
        let (data, total) = list_providers(&db, None, None).await.unwrap();
        assert_eq!(total, 0);
        assert!(data.is_empty());
    }

    #[tokio::test]
    async fn list_providers_returns_all_within_page() {
        let db = test_db().await;
        // 创建 5 个 provider
        for i in 0..5 {
            let _ = find_or_create_provider(&db, "openai", &format!("https://api{i}.test.com/v1"), Some(&format!("sk-{i}")))
                .await.unwrap();
        }
        let (data, total) = list_providers(&db, None, None).await.unwrap();
        assert_eq!(total, 5);
        assert_eq!(data.len(), 5);
    }

    #[tokio::test]
    async fn list_providers_pagination_page1() {
        let db = test_db().await;
        for i in 0..12 {
            let _ = find_or_create_provider(&db, "openai", &format!("https://api{i}.test.com/v1"), Some(&format!("sk-{i}")))
                .await.unwrap();
        }
        let (data, total) = list_providers(&db, Some(1), Some(5)).await.unwrap();
        assert_eq!(total, 12);
        assert_eq!(data.len(), 5);
    }

    #[tokio::test]
    async fn list_providers_pagination_page2() {
        let db = test_db().await;
        for i in 0..12 {
            let _ = find_or_create_provider(&db, "openai", &format!("https://api{i}.test.com/v1"), Some(&format!("sk-{i}")))
                .await.unwrap();
        }
        let (data, total) = list_providers(&db, Some(2), Some(5)).await.unwrap();
        assert_eq!(total, 12);
        assert_eq!(data.len(), 5);
    }

    #[tokio::test]
    async fn list_providers_pagination_last_page_partial() {
        let db = test_db().await;
        for i in 0..12 {
            let _ = find_or_create_provider(&db, "openai", &format!("https://api{i}.test.com/v1"), Some(&format!("sk-{i}")))
                .await.unwrap();
        }
        let (data, total) = list_providers(&db, Some(3), Some(5)).await.unwrap();
        assert_eq!(total, 12);
        assert_eq!(data.len(), 2); // 12 = 5+5+2
    }

    #[tokio::test]
    async fn list_providers_pagination_beyond_last_page() {
        let db = test_db().await;
        for i in 0..3 {
            let _ = find_or_create_provider(&db, "openai", &format!("https://api{i}.test.com/v1"), Some(&format!("sk-{i}")))
                .await.unwrap();
        }
        let (data, total) = list_providers(&db, Some(10), Some(20)).await.unwrap();
        assert_eq!(total, 3);
        assert!(data.is_empty());
    }

    #[tokio::test]
    async fn list_providers_model_count() {
        let db = test_db().await;
        let (p, _) = find_or_create_provider(&db, "openai", "https://api.test.com/v1", Some("sk-x"))
            .await.unwrap();
        let _ = add_model(&db, &p, "model-a", None).await.unwrap();
        let _ = add_model(&db, &p, "model-b", None).await.unwrap();
        let _ = add_model(&db, &p, "model-c", None).await.unwrap();
        let (data, _) = list_providers(&db, None, None).await.unwrap();
        assert_eq!(data.len(), 1);
        assert_eq!(data[0]["model_count"], 3);
    }

    #[tokio::test]
    async fn list_providers_page_size_clamped() {
        let db = test_db().await;
        for i in 0..3 {
            let _ = find_or_create_provider(&db, "openai", &format!("https://api{i}.test.com/v1"), Some(&format!("sk-{i}")))
                .await.unwrap();
        }
        // page_size=0 应被 clamp 到 1
        let (data, total) = list_providers(&db, Some(1), Some(0)).await.unwrap();
        assert_eq!(total, 3);
        assert_eq!(data.len(), 1);
    }
}
