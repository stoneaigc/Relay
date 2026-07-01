-- RunAPI 初始 schema(SQLite)。运行时由 storage::init_schema 以 IF NOT EXISTS 执行。
PRAGMA journal_mode = WAL;
PRAGMA synchronous = NORMAL;

CREATE TABLE IF NOT EXISTS users (
  id                TEXT PRIMARY KEY,
  username          TEXT UNIQUE,
  password_hash     TEXT,
  email             TEXT,
  phone             TEXT UNIQUE,
  status            INTEGER NOT NULL DEFAULT 0,   -- 0=active 1=disabled
  token_balance     INTEGER NOT NULL,
  token_used_total  INTEGER NOT NULL DEFAULT 0,
  concurrency_limit INTEGER,                       -- NULL=用全局默认
  bill_multiplier   REAL NOT NULL DEFAULT 1.0,     -- 用户级计费倍率(与模型倍率相乘)
  group_id          INTEGER,                       -- 绑定的模型组
  source            TEXT,                          -- 注册来源:admin|phone|wechat|alipay
  created_at        TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS api_keys (
  id             TEXT PRIMARY KEY,
  user_id        TEXT NOT NULL,
  interface_kind TEXT NOT NULL,                    -- openai | anthropic
  key_hash       TEXT NOT NULL UNIQUE,             -- sha256(明文)
  key_prefix     TEXT NOT NULL,                    -- 掩码展示用
  revoked        INTEGER NOT NULL DEFAULT 0,
  created_at     TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS usage_logs (
  id             INTEGER PRIMARY KEY AUTOINCREMENT,
  user_id        TEXT NOT NULL,
  model          TEXT,
  provider       TEXT,
  upstream_model TEXT,
  input_tokens   INTEGER,
  output_tokens  INTEGER,
  charged_tokens INTEGER,
  status         INTEGER,
  ts             INTEGER NOT NULL DEFAULT 0,   -- unix 秒,便于按时间聚合
  created_at     TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_usage_user ON usage_logs(user_id, ts);

CREATE TABLE IF NOT EXISTS providers (
  name        TEXT PRIMARY KEY,
  kind        TEXT NOT NULL,               -- openai | anthropic
  base_url    TEXT NOT NULL,
  api_key     TEXT,                         -- 直接保存的真实密钥
  created_at  TEXT NOT NULL
);

-- 模型 = 某供应商上的真实模型
CREATE TABLE IF NOT EXISTS models (
  id             INTEGER PRIMARY KEY AUTOINCREMENT,
  provider       TEXT NOT NULL,             -- providers.name
  upstream_model TEXT NOT NULL,             -- 供应商上的真实模型名
  label          TEXT,                       -- 可选备注
  created_at     TEXT NOT NULL
);

-- 模型组
CREATE TABLE IF NOT EXISTS model_groups (
  id         INTEGER PRIMARY KEY AUTOINCREMENT,
  name       TEXT NOT NULL UNIQUE,
  is_active  INTEGER NOT NULL DEFAULT 0,   -- 1=激活(默认组),新用户注册默认绑定
  created_at TEXT NOT NULL
);

-- 组内路由:对外模型名 -> 指定模型(同名多条=加权/容灾)。按名精确路由。
CREATE TABLE IF NOT EXISTS group_routes (
  id          INTEGER PRIMARY KEY AUTOINCREMENT,
  group_id    INTEGER NOT NULL,
  public_name TEXT NOT NULL,                 -- 用户请求 & 响应展示的模型名
  model_id    INTEGER NOT NULL,              -- models.id
  weight      INTEGER NOT NULL DEFAULT 100,
  multiplier  REAL NOT NULL DEFAULT 1.0,
  created_at  TEXT NOT NULL
);

-- 奖励任务:由管理员在后台配置的可申领活动(star / issue / 提建议 等)。
CREATE TABLE IF NOT EXISTS reward_tasks (
  id            INTEGER PRIMARY KEY AUTOINCREMENT,
  title         TEXT NOT NULL,                 -- 展示标题
  description   TEXT,                          -- 说明 / 申领要求
  evidence_type TEXT NOT NULL DEFAULT 'text',  -- screenshot | link | text | none
  variable      INTEGER NOT NULL DEFAULT 0,    -- 0=固定额度 1=区间(管理员审核时评定)
  reward_tokens INTEGER NOT NULL DEFAULT 0,    -- 固定额度(variable=0 时用)
  reward_min    INTEGER NOT NULL DEFAULT 0,    -- 区间下限(variable=1 时用)
  reward_max    INTEGER NOT NULL DEFAULT 0,    -- 区间上限(variable=1 时用)
  link_url      TEXT,                          -- 展示给用户的相关链接(如仓库地址)
  enabled       INTEGER NOT NULL DEFAULT 1,    -- 1=门户可见可申领
  sort          INTEGER NOT NULL DEFAULT 0,    -- 展示排序(小在前)
  created_at    TEXT NOT NULL
);

-- 奖励申领:用户对某个奖励任务的申领,后台人工审核后入账。
CREATE TABLE IF NOT EXISTS reward_claims (
  id            TEXT PRIMARY KEY,
  user_id       TEXT NOT NULL,
  task_id       INTEGER,                        -- reward_tasks.id(旧数据可能为空)
  kind          TEXT,                           -- 兼容旧字段:star | issue | suggestion
  evidence      TEXT,                          -- 用户提交的证明:截图 data URL / 链接 / 文本
  reward_tokens INTEGER NOT NULL,             -- 通过后入账的 token 数
  status        INTEGER NOT NULL DEFAULT 0,   -- 0=pending 1=approved 2=rejected
  review_note   TEXT,                          -- 审核备注(驳回原因等)
  reviewed_at   TEXT,
  created_at    TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_reward_user ON reward_claims(user_id);
