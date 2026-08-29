# Relay

> **LLM API 转发网关**：单进程 Rust 二进制，对外同时兼容 **OpenAI** 与 **Anthropic** 协议，对内可接入多家供应商并做双向协议互转与故障转移；全内存热路径 + 异步落库，默认零外部依赖（SQLite + 进程内缓存）。附 C 端自助门户与运营管理后台。

> **致敬与致谢**：Relay 受 [RunAPI](https://github.com/) 的启发，并在其 API 网关的架构构想基础上二次开发而来。我们沿用了「协议中立 + 加权路由 + 内存热路径」的核心思路，并在此基础上把**请求链路追踪**、**故障转移/熔断**、**指标仪表盘**等可观测能力直接落成可用的功能。感谢 RunAPI 及其生态的开放精神，让转发网关变得可以"看得见、控得住"。

---

## 为什么叫 Relay？

**Relay（中继 / 转发）**——它做的事情正是一个 API 转发网关该做的：接收上游客户端请求（OpenAI / Anthropic 协议），经鉴权、路由（加权挑选）、协议互转、故障转移后，转发到真正提供模型能力的下游供应商。一个统一入口，多路可切换通道，每一次转发都受控、可跟踪。

---

## 特性

- **双协议入站**：`/v1/chat/completions`（OpenAI）与 `/v1/messages`（Anthropic），流式 / 非流式都支持。
- **协议互转**：上游是 OpenAI 还是 Anthropic 都透明路由；支持文本、工具调用（function calling）、多模态图片（[src/translate.rs](src/translate.rs)），流式逐 chunk 回译。
- **加权路由 / 容灾**：一个对外模型名可映射到多个上游（按权重挑选），由模型组维护。
- **故障转移 + 熔断**：可重试错误（连接失败 / 超时 / 429 / 5xx）自动切换下一候选；连续失败触发熔断，半开探测。
- **请求链路追踪**：每次请求完整记录候选顺序、权重、实际尝试（failover 链）、熔断跳过、tokens，管理后台可视化。
- **全内存热路径**：鉴权、余额、并发走 `DashMap` + 原子量，热路径零同步 DB 查询；余额与用量异步落库。
- **token 计费**：按 token 计费，支持模型级与用户级倍率（相乘），热更新即时生效。
- **可插拔存储 / 缓存**：数据库 SQLite 或 Postgres；缓存进程内或 Redis，由配置切换。
- **邮箱注册门户**：邮箱验证码注册 / 登录 / 找回密码；API Key 创建与刷新（明文仅展示一次）；余额、用量趋势、在线对话、奖励申领。
- **管理后台**：用户 CRUD、并发与额度、模型 / 模型组 / 路由维护、倍率热更新、全局用量、上游熔断管理、指标仪表盘、请求链路面板、奖励配置与审核、SMTP 邮箱设置（页面化）。
- **奖励任务**：奖励活动完全由后台配置，门户动态渲染；用户申领 → 人工审核 → 通过后 token 自动入账。

---

## 组件

| 组件 | 技术 | 默认端口 | 说明 |
|---|---|---|---|
| 网关后端 | Rust（axum 0.7） | 8080 | 数据面 `/v1/*` + 门户 `/portal/*` + 管理 `/admin/*` |
| Client Portal | React + Vite + TS | 5173 | 注册/登录、API Key、余额、用量、对话、奖励申领 |
| Admin Console | React + Vite + TS | 5174 | 用户、模型/组/路由、倍率、用量、指标、请求链路、奖励、SMTP |

---

## 架构一览

```
                 ┌────────────── 网关后端 (Rust / axum) ──────────────┐
  客户端 SDK  ──▶ │  /v1/*      鉴权→路由→(协议互转)→上游→计费→追踪      │ ──▶ OpenAI / Anthropic 上游
  Client Portal ─▶│  /portal/*  门户 API（JWT）                        │
  Admin Console ─▶│  /admin/*   管理 API（JWT）                        │
                 └─────────────────────────────────────────────────────┘
                     内存态：DashMap<用户/Key> + 原子余额/并发/熔断   ⇅ 异步落库
                     持久化：SQLite / Postgres        缓存：进程内 / Redis
```

**请求生命周期（一次请求的完整轨迹）**：

```
入站请求 ──► 鉴权(key→user) ──► resolve_all(候选:加权命中+权重) ──► 熔断检查
        ──► 占用并发槽 ──► 逐候选尝试(成功 / Unavailable 切换 / 非重试直返)
        ──► 协议互转(如 OpenAI→Anthropic) ──► 上游调用 ──► 回译
        ──► 计费 + 异步写 request_logs（候选顺序 + failover 链 + tokens）
```

---

## 运行

### 1. 后端

```bash
# 上游真实密钥通常在管理后台按「模型」配置；也可用环境变量覆盖默认配置。
cargo run
```

- 首次启动自动建表（幂等，`IF NOT EXISTS`）；若库中无奖励任务则自动播种 3 个示例（点 Star / 提 Issue / 提建议）。
- 默认管理员见 `config/default.toml` 的 `[admin]`，**生产务必用环境变量覆盖**。

### 2. 前端

```bash
cd frontend/portal && npm install && npm run dev   # http://localhost:5173
cd frontend/admin  && npm install && npm run dev   # http://localhost:5174
```

Vite 已配置代理，前端请求自动转发到 `:8080`，无需处理跨域。

---

## 快速体验

1. 打开门户 `:5173`，用邮箱注册（SMTP 未配置时为开发模式，验证码在接口响应的 `dev_code` 直接返回）→ 新用户自动赠送额度（默认 1000 万 token）。
2. 在 OpenAI / Anthropic 接口卡片点「创建 Key」→ 弹窗显示明文（仅一次，请立即复制）。
3. 用该 Key 调用网关（OpenAI 协议）：

   ```bash
   curl http://localhost:8080/v1/chat/completions \
     -H "Authorization: Bearer <你的 rk_live_...>" \
     -H "Content-Type: application/json" \
     -d '{"model":"gpt-4o-mini","messages":[{"role":"user","content":"hi"}]}'
   ```

   或 Anthropic 协议：

   ```bash
   curl http://localhost:8080/v1/messages \
     -H "x-api-key: <你的 rk_live_...>" \
     -H "anthropic-version: 2023-06-01" \
     -H "Content-Type: application/json" \
     -d '{"model":"claude-3-5-sonnet","max_tokens":64,"messages":[{"role":"user","content":"hi"}]}'
   ```

4. 打开管理后台 `:5174`：登录后添加模型（自带上游连接测试）→ 建模型组 → 加路由（对外模型名 → 模型）→ 设激活组；可改用户并发/额度、调倍率（热生效）、配置奖励任务、审核申领、查看指标仪表盘与请求链路。

### 请求链路（请求链路）

管理后台 → **请求链路**：搜索 `request_id / 模型`，可展开查看某次请求的：

- **候选顺序（负载策略）**：`# / 上游 / 模型 / 权重 / 状态`——加权命中 + failover 次序。
- **实际尝试（failover 链）**：`上游 / 模型 / 状态 / 延迟 / 错误`——成功、`503 不可用`、`熔断跳过` 一目了然。
- **Tokens 汇总**：↑输入 ↓输出 / 计费 / 是否流式。

失败（`final_status != 200`）行自动标红，便于快速定位故障。

---

## 数据面接口

| 方法 | 路径 | 说明 |
|---|---|---|
| GET | `/healthz` | 健康检查 |
| GET | `/v1/models` | 当前用户可用的对外模型名列表 |
| POST | `/v1/chat/completions` | OpenAI 协议，流式 / 非流式 |
| POST | `/v1/messages` | Anthropic 协议，流式 / 非流式 |

鉴权用数据面 Key（`Authorization: Bearer rk_live_...` 或 Anthropic 的 `x-api-key`）。

### 管理 API（部分）

| 路径 | 说明 |
|---|---|
| `/admin/api/request-logs?q=&limit=` | 请求链路日志（倒序，`q` 按 request_id/模型 LIKE 检索） |
| `/admin/api/upstreams` | 上游熔断 / 并发状态；`/upstreams/reset` 重置熔断 |
| `/admin/api/metrics?range_secs=&top_n=` | 指标大盘（聚合 + 上游 TopN + 时序） |
| `/admin/api/audit/failures` | 失败审计（最后失败候选） |
| 用户 / 模型 / 组 / 路由 / 奖励 / SMTP | 见 [src/admin.rs](src/admin.rs) |

---

## 奖励任务

奖励活动从后台配置，不再硬编码。每个任务是一条 `reward_tasks` 记录：

- **证明类型 `evidence_type`**：`screenshot`（截图）/ `link`（链接）/ `text`（文本）/ `none`（无需证明）。
- **额度模型**：`variable=false` 固定额度；`variable=true` 为区间 `[min, max]`，管理员审核时评定。
- **可见性 / 排序**：`enabled` 控制门户是否可见可申领，`sort` 控制顺序。

流程：门户按证明类型提交申领 → 后台「奖励审核」通过/驳回 → 通过后 token 自动入账（`WHERE status=0` 防重复）。

---

## 配置

配置文件 `config/default.toml`，可被环境变量覆盖：前缀 `RELAY_`，`__` 分隔层级。

```bash
RELAY_SERVER__BIND=0.0.0.0:9000
RELAY_AUTH__JWT_SECRET=your-secret
RELAY_ADMIN__PASSWORD=your-admin-password
RELAY_DATABASE__TYPE=sqlite
```

主要配置段：

| 段 | 关键项 | 说明 |
|---|---|---|
| `[server]` | `bind` | 监听地址 |
| `[database]` | `type` = `sqlite` \| `postgres` | SQLite 用 `url`；Postgres 用分项或完整 `url` |
| `[cache]` | `type` = `memory` \| `redis` | Redis 用分项或完整 `url` |
| `[auth]` | `jwt_secret` / `session_ttl_secs` | JWT 签发，生产必须覆盖 secret |
| `[admin]` | `username` / `password` | 后台登录凭据，生产必须覆盖 |
| `[email]` | `smtp_host/port/username/password/from` | 注册验证码 SMTP；`smtp_host` 空 = 开发模式。`password` 填授权码；465=SSL、587/25=STARTTLS |
| `[defaults]` | `concurrency_limit` / `signup_grant_tokens` | 全局默认并发与新用户赠额 |
| `[logging]` | `store` = `sqlite` \| `elasticsearch` | 请求链路日志后端；ES 为预留占位 |

> 供应商 / 模型 / 模型组 / 路由 / 奖励任务全部由管理后台维护并持久化到数据库，不在配置文件里。
>
> ⚠️ `config/default.toml` 内的密钥仅为占位/示例，请勿把真实密钥提交到仓库；生产用环境变量注入。

---

## 目录结构

```
src/
  main.rs        路由装配、启动、后台任务（用量/请求日志异步落库）
  handlers.rs    数据面 /v1/*（鉴权、路由、failover、计费、链路追踪）
  translate.rs   OpenAI ⇄ Anthropic 协议互转
  providers/     上游客户端（openai / anthropic）
  routing.rs     内存路由（resolve_all：加权命中 + failover 次序）
  reqlog.rs      请求链路模型 + 存储抽象（SQLite 默认 / ES 占位）
  portal.rs      门户 API
  admin.rs       管理 API
  storage.rs     数据库读写（SQLite / Postgres）
  cache.rs       缓存（memory / redis）
  auth.rs jwt.rs email.rs   鉴权 / JWT / 邮件
  config.rs state.rs error.rs
migrations/      0001_init.sql（SQLite）/ .postgres.sql
frontend/portal  C 端门户
frontend/admin   管理后台
deploy/          安装脚本 + systemd 模板
.github/workflows/release.yml   自动构建发布（多架构）
```

---

## 已实现 / 待办

- [x] 数据面：内存鉴权、加权路由、OpenAI 与 Anthropic 双协议入站、协议互转（文本/工具调用/图片）、流式与非流式、token 计费、并发限制
- [x] 故障转移 + 熔断（可重试错误自动切换、连续失败触发熔断、半开探测）
- [x] 请求链路追踪：全量记录候选顺序/权重/failover 链，管理后台可视化，与 `usage_logs` 以 `request_id` 关联
- [x] 内存态 + 异步落库；SQLite / Postgres 可切换；缓存 memory / Redis 可切换
- [x] 指标仪表盘：请求量/成功率/延迟/tokens 聚合 + 上游 TopN + 时序；上游熔断/并发状态面板；失败审计
- [x] 门户：邮箱验证码注册/登录、找回密码、Key 创建/刷新（明文一次）、余额、用量趋势、在线对话、奖励申领
- [x] 管理：用户 CRUD、并发/额度、模型倍率与用户倍率热更新、模型/组/路由维护、全局用量、奖励配置与审核、SMTP 邮箱设置（页面化）
- [ ] RPM/TPM 限流、响应缓存
- [ ] 第三方登录：微信 / 支付宝（当前为邮箱验证码）
- [ ] Elasticsearch 请求日志后端（已预留 trait 与配置开关，未实现）
- [ ] Prometheus 指标导出

---

## License

MIT（占位，发布前确认）。
