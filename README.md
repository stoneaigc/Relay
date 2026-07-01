# RunAPI

LLM API 网关:对外兼容 **OpenAI / Anthropic** 协议,对内可接多家供应商并做协议互转;鉴权与计费走全内存热路径,异步落库。含 C 端自助门户(Client Portal)与管理后台(Admin Console)。详细设计见 [DESIGN.md](DESIGN.md)。

## 特性

- **双协议入站**:`/v1/chat/completions`(OpenAI)与 `/v1/messages`(Anthropic),流式 / 非流式。
- **协议互转**:上游是 OpenAI 还是 Anthropic 都可透明路由;支持文本、工具调用(function calling)、多模态图片(见 [src/translate.rs](src/translate.rs))。
- **加权路由 / 容灾**:一个对外模型名可映射到多个上游(按权重挑选),由模型组维护。
- **全内存热路径**:鉴权、余额、并发限制走 `DashMap` + 原子量,余额变更异步回写数据库。
- **计费**:按 token 计费,支持模型级与用户级倍率(相乘),热更新即时生效。
- **可插拔存储 / 缓存**:数据库 SQLite 或 Postgres、缓存进程内或 Redis,均由配置切换。
- **自助门户**:邮箱验证码注册 / 登录、API Key 创建与刷新(明文仅展示一次)、余额、用量趋势、在线对话。
- **管理后台**:用户 CRUD、并发与额度、模型 / 模型组 / 路由维护、倍率热更新、全局用量、奖励任务配置与申领审核。
- **奖励任务**:奖励活动完全由后台配置(见下文),门户动态渲染,用户申领 → 人工审核 → 通过后 token 自动入账。

## 组件

| 组件 | 技术 | 默认端口 | 说明 |
|---|---|---|---|
| 网关后端 | Rust (axum 0.7) | 8080 | 数据面 `/v1/*` + 门户 `/portal/*` + 管理 `/admin/*` |
| Client Portal | React + Vite + TS | 5173 | 注册/登录、API Key、余额、用量、对话、奖励申领 |
| Admin Console | React + Vite + TS | 5174 | 用户、模型/组/路由、倍率、用量、奖励任务与审核 |

## 架构一览

```
                 ┌─────────────── 网关后端 (Rust / axum) ───────────────┐
  客户端 SDK  ──▶ │  /v1/*      数据面:鉴权→路由→(协议互转)→上游→计费    │ ──▶ OpenAI / Anthropic 上游
  Client Portal ─▶│  /portal/*  门户 API(JWT aud=portal)               │
  Admin Console ─▶│  /admin/*   管理 API(JWT aud=admin)                │
                 └───────────────────────────────────────────────────────┘
                     内存态:DashMap<用户/Key> + 原子余额/并发   ⇅ 异步落库
                     持久化:SQLite / Postgres        缓存:进程内 / Redis
```

## 运行

### 1. 后端

```bash
# 上游真实密钥通常在管理后台按「模型」配置;也可用环境变量覆盖默认配置。
cargo run
```

- 首次启动自动建表(幂等,`IF NOT EXISTS` + 兼容性 `ALTER`),若库中无用户会创建一个演示用户并在日志打印一把数据面 Key(仅此一次)。
- 首次启动若无奖励任务,会自动播种 3 个示例任务(点 Star / 提 Issue / 提建议),之后由后台维护。
- 默认管理员见 `config/default.toml` 的 `[admin]`,**生产务必用环境变量覆盖**。

### 2. 前端

```bash
cd frontend/portal && npm install && npm run dev   # http://localhost:5173
cd frontend/admin  && npm install && npm run dev   # http://localhost:5174
```

Vite 已配置代理,前端请求自动转发到 `:8080`,无需处理跨域。

## 快速体验

1. 打开门户 `:5173`,用邮箱注册(SMTP 未配置时为开发模式,验证码直接在接口响应 `dev_code` 返回)→ 新用户自动赠送额度(默认 1000 万 token)。
2. 在 OpenAI / Anthropic 接口卡片点「创建 Key」→ 弹窗显示明文(仅一次)→ 复制。
3. 用该 Key 调用网关(OpenAI 协议):

   ```bash
   curl http://localhost:8080/v1/chat/completions \
     -H "Authorization: Bearer <你的 rk_live_...>" \
     -H "Content-Type: application/json" \
     -d '{"model":"gpt-4o-mini","messages":[{"role":"user","content":"hi"}]}'
   ```

   或 Anthropic 协议:

   ```bash
   curl http://localhost:8080/v1/messages \
     -H "x-api-key: <你的 rk_live_...>" \
     -H "anthropic-version: 2023-06-01" \
     -H "Content-Type: application/json" \
     -d '{"model":"claude-3-5-sonnet","max_tokens":64,"messages":[{"role":"user","content":"hi"}]}'
   ```

4. 打开管理后台 `:5174`,登录后:添加模型(自带上游连接)→ 建模型组 → 加路由(对外模型名 → 模型)→ 设为激活组;可改用户并发/额度、调倍率(热生效)、配置奖励任务、审核申领。

## 数据面接口

| 方法 | 路径 | 说明 |
|---|---|---|
| GET | `/healthz` | 健康检查 |
| GET | `/v1/models` | 当前用户可用的对外模型名列表 |
| POST | `/v1/chat/completions` | OpenAI 协议,流式 / 非流式 |
| POST | `/v1/messages` | Anthropic 协议,流式 / 非流式 |

鉴权用数据面 Key(`Authorization: Bearer rk_live_...` 或 Anthropic 的 `x-api-key`)。

## 奖励任务

奖励活动从后台配置,不再硬编码。每个任务是一条 `reward_tasks` 记录:

- **证明类型 `evidence_type`**:`screenshot`(上传截图)/ `link`(填链接)/ `text`(纯文本)/ `none`(无需证明)。
- **额度模型**:`variable=false` 为固定额度 `reward_tokens`;`variable=true` 为区间 `[reward_min, reward_max]`,由管理员在审核时评定入账数。
- **可见性 / 排序**:`enabled` 控制门户是否可见可申领,`sort` 控制展示顺序。

流程:门户按任务的证明类型提交申领 → 后台「奖励审核」通过/驳回 → 通过后 token 自动入账(内存余额与库同步,`WHERE status=0` 防重复入账)。管理入口:后台「奖励设置」(任务 CRUD)与「奖励审核」(申领列表)。

## 配置

配置文件 `config/default.toml`,可被环境变量覆盖:前缀 `RUNAPI_`,`__` 分隔层级。

```bash
RUNAPI_SERVER__BIND=0.0.0.0:9000
RUNAPI_AUTH__JWT_SECRET=your-secret
RUNAPI_ADMIN__PASSWORD=your-admin-password
RUNAPI_DATABASE__TYPE=sqlite
```

主要配置段:

| 段 | 关键项 | 说明 |
|---|---|---|
| `[server]` | `bind` | 监听地址 |
| `[database]` | `type` = `sqlite` \| `postgres` | SQLite 用 `url`;Postgres 用 `host/port/user/password/dbname` 分项(特殊字符自动编码),或直接给完整 `url` |
| `[cache]` | `type` = `memory` \| `redis` | Redis 用分项或完整 `url` |
| `[auth]` | `jwt_secret` / `session_ttl_secs` | JWT 签发,生产必须覆盖 secret |
| `[admin]` | `username` / `password` | 后台登录凭据,生产必须覆盖 |
| `[email]` | `smtp_host/port/username/password/from` | 注册验证码 SMTP;`smtp_host` 为空即开发模式(验证码走日志并在响应里返回)。`password` 填客户端授权码;端口 465=SSL、587/25=STARTTLS |
| `[defaults]` | `concurrency_limit` / `signup_grant_tokens` | 全局默认并发与新用户赠额 |

> 供应商 / 模型 / 模型组 / 路由全部由管理后台维护并持久化到数据库,不在配置文件里。
>
> ⚠️ `config/default.toml` 内的密钥仅为占位/示例,请勿把真实密钥提交到仓库;生产用环境变量注入。

## 目录结构

```
src/
  main.rs        路由装配、启动、后台任务
  handlers.rs    数据面 /v1/*(鉴权、路由、计费)
  translate.rs   OpenAI ⇄ Anthropic 协议互转
  providers/     上游客户端(openai / anthropic)
  routing.rs     内存路由图(供应商/模型/组/路由)
  portal.rs      门户 API
  admin.rs       管理 API
  storage.rs     数据库读写(SQLite / Postgres)
  cache.rs       缓存(memory / redis)
  auth.rs jwt.rs email.rs   鉴权 / JWT / 邮件
  config.rs state.rs error.rs
migrations/      0001_init.sql(SQLite)/ .postgres.sql
frontend/portal  C 端门户
frontend/admin   管理后台
```

## 已实现 / 待办

- [x] 数据面:内存鉴权、加权路由、OpenAI 与 Anthropic 双协议入站、协议互转(文本/工具调用/图片)、流式与非流式、token 计费、并发限制
- [x] 内存态 + 异步落库(余额、用量);SQLite / Postgres 可切换;缓存 memory / Redis 可切换
- [x] 门户:邮箱验证码注册/登录、找回密码、Key 创建/刷新(明文一次)、余额、用量趋势、在线对话
- [x] 管理:登录、用户 CRUD、并发/额度、模型倍率与用户倍率热更新、模型/组/路由维护、全局用量
- [x] 奖励任务:后台可配置(证明类型 + 固定/区间额度 + 启用/排序)、门户动态渲染与申领、后台审核入账
- [ ] failover 自动重试、RPM/TPM 限流、响应缓存
- [ ] 第三方登录:微信 / 支付宝
