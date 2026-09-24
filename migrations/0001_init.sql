-- Relay 初始 schema(SQLite)。运行时由 storage::init_schema 以 IF NOT EXISTS 执行。
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
  rpm_limit         INTEGER,                       -- 每分钟请求上限(NULL=用全局默认)
  tpm_limit         INTEGER,                       -- 每分钟 token 上限(NULL=用全局默认)
  budget_daily_tokens  INTEGER,                    -- 日预算(charged tokens,NULL=不限)
  budget_monthly_tokens INTEGER,                    -- 月预算(charged tokens,NULL=不限)
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
  key_id         TEXT,                         -- 发起调用的 API Key(api_keys.id),门户调用为 NULL
  cost_usd       REAL,                         -- 计费口径成本(USD),消费端折算
  status         INTEGER,
  ts             INTEGER NOT NULL DEFAULT 0,   -- unix 秒,便于按时间聚合
  created_at     TEXT NOT NULL,
  request_id     TEXT                          -- 关联 request_logs 的请求链路 ID
);
CREATE INDEX IF NOT EXISTS idx_usage_user ON usage_logs(user_id, ts);

-- 请求链路追踪:记录每次请求(成功+失败)的路由候选顺序、权重、实际选中、failover 链。
CREATE TABLE IF NOT EXISTS request_logs (
  id                 INTEGER PRIMARY KEY AUTOINCREMENT,
  request_id         TEXT NOT NULL,            -- 本次请求唯一 ID(UUID)
  user_id            TEXT NOT NULL,
  path               TEXT,                     -- chat | messages
  requested_model    TEXT,                     -- 用户请求的对外模型名
  stream             INTEGER NOT NULL DEFAULT 0,
  candidates         TEXT,                     -- JSON:[{kind,base_url,model,weight}] 候选顺序(加权命中+failover)
  attempts           TEXT,                     -- JSON:[{kind,base_url,model,status,latency_ms,error,skipped}]
  final_kind         TEXT,                     -- 最终命中/失败的上游 kind
  final_upstream_model TEXT,                   -- 最终命中/失败的上游模型名
  final_status       INTEGER,                  -- 200=成功;非 200=失败原因
  latency_ms         INTEGER NOT NULL DEFAULT 0,
  input_tokens       INTEGER NOT NULL DEFAULT 0,
  output_tokens      INTEGER NOT NULL DEFAULT 0,
  charged_tokens     INTEGER NOT NULL DEFAULT 0,
  req_body           TEXT,                     -- 请求体内容预览(跟随存储后端,UTF-8 安全截断)
  resp_body          TEXT,                     -- 响应体内容预览(流式路径暂不采集)
  created_at         TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_reqlog_user ON request_logs(user_id, created_at);
CREATE INDEX IF NOT EXISTS idx_reqlog_reqid ON request_logs(request_id);
CREATE INDEX IF NOT EXISTS idx_reqlog_status_created ON request_logs(final_status, created_at);
CREATE INDEX IF NOT EXISTS idx_reqlog_created ON request_logs(created_at);
CREATE INDEX IF NOT EXISTS idx_usage_user_tokens ON usage_logs(user_id, charged_tokens);

CREATE TABLE IF NOT EXISTS providers (
  name         TEXT PRIMARY KEY,
  kind         TEXT NOT NULL,               -- openai | anthropic
  base_url     TEXT NOT NULL,
  api_key      TEXT,                        -- 直接保存的真实密钥
  display_name TEXT,                        -- 自定义显示名(空=按域名展示)
  created_at   TEXT NOT NULL
);

-- 模型 = 某供应商上的真实模型
CREATE TABLE IF NOT EXISTS models (
  id             INTEGER PRIMARY KEY AUTOINCREMENT,
  provider       TEXT NOT NULL,             -- providers.name
  upstream_model TEXT NOT NULL,             -- 供应商上的真实模型名
  label          TEXT,                       -- 可选备注
  input_price    REAL,                       -- 输入单价 $/1M tokens(NULL=未定价,回退内置默认价表)
  output_price   REAL,                       -- 输出单价 $/1M tokens(NULL=未定价)
  created_at     TEXT NOT NULL,
  UNIQUE (provider, upstream_model)         -- 同一供应商下模型名唯一,防止重复添加
);

-- 模型组
CREATE TABLE IF NOT EXISTS model_groups (
  id         INTEGER PRIMARY KEY AUTOINCREMENT,
  name       TEXT NOT NULL UNIQUE,
  is_active  INTEGER NOT NULL DEFAULT 0,   -- 1=激活(默认组),新用户注册默认绑定
  strategy   TEXT NOT NULL DEFAULT 'weighted_random', -- 负载策略:weighted_random(加权随机,当前)|(可扩展 weighted_round_robin 等)
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
  cache_enabled INTEGER NOT NULL DEFAULT 0,    -- 路由级语义缓存 opt-in(1=参与缓存判定)
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

-- 系统配置(KV)。点分命名空间,如 email.smtp_host。value 统一 TEXT(标量直存,复杂值 JSON)。
CREATE TABLE IF NOT EXISTS settings (
  key        TEXT PRIMARY KEY,
  value      TEXT NOT NULL,
  updated_at TEXT NOT NULL
);

-- 高峰/低谷时段策略:按「星期 + 时间段」驱动 计费倍率 与 路由权重覆盖。
CREATE TABLE IF NOT EXISTS time_rules (
  id          INTEGER PRIMARY KEY AUTOINCREMENT,
  group_id    INTEGER NOT NULL,                 -- 绑定的模型组
  name        TEXT NOT NULL,                    -- 时段名:高峰/平峰/低谷/自定义
  weekdays    TEXT NOT NULL DEFAULT '0-6',      -- 生效星期:0-6 或 "1-5" / "0,6" / "0-6"
  start_time  TEXT NOT NULL,                    -- 开始 HH:MM
  end_time    TEXT NOT NULL,                    -- 结束 HH:MM
  multiplier  REAL NOT NULL DEFAULT 1.0,        -- 该时段计费倍率系数(与模型倍率/用户倍率相乘)
  weight_map  TEXT,                             -- 该时段路由权重覆盖 JSON {"public_name":{"model_id":weight}}
  active      INTEGER NOT NULL DEFAULT 1,
  created_at  TEXT NOT NULL
);

-- 语义缓存 L2:请求文本的 embedding 向量(按 model+provider 隔离,暴力余弦扫描)。
CREATE TABLE IF NOT EXISTS cache_vectors (
  cache_key  TEXT PRIMARY KEY,              -- 对应内存缓存的 key(L2 命中后回查 L1 取响应体)
  model      TEXT NOT NULL,                 -- 公开模型名(隔离维度)
  provider   TEXT NOT NULL,                 -- 供应商名(隔离维度)
  embedding  TEXT NOT NULL,                 -- f32 向量 base64(little-endian 字节)
  created_at INTEGER NOT NULL               -- unix 秒(用于按龄清理)
);
CREATE INDEX IF NOT EXISTS idx_cache_vectors_scope ON cache_vectors(model, provider);

-- 管理操作审计:admin 面所有变更请求(POST/PUT/PATCH/DELETE)统一落库。
CREATE TABLE IF NOT EXISTS admin_audit_logs (
  id         INTEGER PRIMARY KEY AUTOINCREMENT,
  actor      TEXT NOT NULL,                -- 操作者(admin 用户名);无 token 请求(登录)为 anonymous
  method     TEXT NOT NULL,                -- POST | PUT | PATCH | DELETE
  path       TEXT NOT NULL,                -- 请求路径(如 /admin/api/users)
  status     INTEGER NOT NULL,             -- 响应状态码
  ip         TEXT NOT NULL DEFAULT '',     -- 客户端 IP
  latency_ms INTEGER NOT NULL DEFAULT 0,
  created_at TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_audit_created ON admin_audit_logs(created_at);

