# Relay

> **LLM API 转发网关**：单进程 Rust 二进制，对外同时兼容 **OpenAI** 与 **Anthropic** 协议，对内可接入多家供应商并做双向协议互转与故障转移；全内存热路径 + 异步落库，默认零外部依赖（SQLite + 进程内缓存）。附 C 端自助门户与运营管理后台。

> **致敬与致谢**：Relay 受 [RunAPI](https://github.com/runify-dev/runapi) 的启发，并在其 API 网关的架构构想基础上二次开发而来。我们沿用了「协议中立 + 加权路由 + 内存热路径」的核心思路，并在此基础上把**请求链路追踪**、**故障转移/熔断**、**指标仪表盘**等可观测能力直接落成可用的功能。感谢 RunAPI 及其生态的开放精神，让转发网关变得可以"看得见、控得住"。

---

## 为什么叫 Relay？

**Relay（中继 / 转发）**——它做的事情正是一个 API 转发网关该做的：接收上游客户端请求（OpenAI / Anthropic 协议），经鉴权、路由（加权挑选）、协议互转、故障转移后，转发到真正提供模型能力的下游供应商。一个统一入口，多路可切换通道，每一次转发都受控、可跟踪。

---

## 特性

- **双协议入站**：`/v1/chat/completions`（OpenAI）与 `/v1/messages`（Anthropic），流式 / 非流式都支持。
- **协议互转**：上游是 OpenAI 还是 Anthropic 都透明路由；支持文本、工具调用（function calling）、多模态图片（[src/translate.rs](src/translate.rs)），流式逐 chunk 回译。
- **加权路由 / 容灾**：一个对外模型名可映射到多个上游（按权重挑选），由模型组维护；支持权重 / 优先级 / 成本优先 / 轮询 / **延迟优先**（P50 快照）五种负载策略。
- **故障转移 + 熔断**：可重试错误（连接失败 / 超时 / 429 / 5xx）自动切换下一候选；连续失败触发熔断，半开探测。
- **请求链路追踪**：每次请求完整记录候选顺序、权重、实际尝试（failover 链）、熔断跳过、tokens 与请求/响应体预览，管理后台可视化；日志支持 SQLite / PostgreSQL / Elasticsearch 三种后端。
- **全内存热路径**：鉴权、余额、并发走 `DashMap` + 原子量，热路径零同步 DB 查询；余额与用量异步落库。
- **token 计费**：按 token 计费，支持模型级与用户级倍率（相乘），热更新即时生效。
- **可插拔存储 / 缓存**：数据库 SQLite 或 Postgres；缓存进程内或 Redis，由配置切换。
- **语义缓存**：L1 精确命中 + L2 embedding 余弦相似度命中（阈值可调），命中免费回放；后台一键开关、命中率可视。
- **邮箱注册门户**：邮箱验证码注册 / 登录 / 找回密码；API Key 创建与刷新（明文仅展示一次）；余额、用量趋势、在线对话、奖励申领。
- **管理后台**：用户 CRUD、并发与额度、模型 / 模型组 / 路由维护、模型组 JSON 导入/导出与路由批量编辑、倍率热更新、全局用量、上游熔断管理、指标仪表盘、请求链路面板、奖励配置与审核、SMTP 邮箱设置（页面化）、API 文档页（双协议示例一键复制）。
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

## 部署形态

| 形态 | 数据库 | 缓存 | 请求链路日志 | 适用场景 |
|---|---|---|---|---|
| **All-in-One（默认）** | SQLite（单文件） | 进程内 | SQLite `request_logs` 表 | 本地开发、自用、小规模部署；零外部依赖，`cargo run` 即起 |
| **生产** | PostgreSQL | Redis（可选） | PostgreSQL 或 Elasticsearch | 多实例 / 高并发，需要更强的并发写入、备份与日志检索能力 |

- **数据库切换**：`[database] type = "postgres"` + 连接分项或完整 `url`；表结构见 `migrations/*.postgres.sql`，首次启动自动幂等建表。All-in-One 直接用 SQLite 即可，无需任何外部服务。
- **请求日志对接 ES**：`[logging] store = "elasticsearch"` 并配置 `elasticsearch_url`（可选 Basic 认证 `elasticsearch_username/password`），链路日志按日滚动写入 `<prefix>-YYYY.MM.DD`。请求/响应体预览**跟随存储后端**：ES 模式落 ES 文档同名字段；SQLite/PG 模式落 `request_logs.req_body / resp_body` 列（上限 `body_preview_max_bytes`，0=不采集）。
- **保留策略**：`[logging] retention_days = 30` 表示链路/用量日志保留 30 天，后台任务每 10 分钟分批清理过期数据；`0`（默认）= 永久保留。
- **缓存切换**：`[cache] type = "redis"` 供多实例共享语义缓存与热点数据；单实例保持 `memory` 即可。

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
- **请求/响应体预览**：展开行内直接查看（按 `body_preview_max_bytes` 截断），便于复盘 prompt 与上游返回。

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
| `/admin/api/request-logs?q=&page=&page_size=&failed=&hours=` | 请求链路日志（倒序分页；`q` 按 request_id/模型检索，`failed` 只看失败/成功，`hours` 只看最近 N 小时；超大表下 `total` 在 20000 封顶，分页到此为止） |
| `/admin/api/upstreams` | 上游熔断 / 并发状态；`/upstreams/reset` 重置熔断 |
| `/admin/api/metrics?range_secs=&top_n=` | 指标大盘（聚合 + 上游 TopN + 时序） |
| `GET /metrics` | Prometheus 文本指标导出（统一 48h 窗口：请求/状态/延迟 P95/P99/熔断/tokens/缓存/用户数）；管理端 JWT 优先，失败时 `Bearer <metrics.export_token>`（配置非空才放行），否则 401 |
| `/admin/api/audit/failures` | 失败审计（最后失败候选） |
| 用户 / 模型 / 组 / 路由 / 奖励 / SMTP | 见 [src/admin.rs](src/admin.rs) |

---

## 性能基准

数据面基准脚本 [scripts/bench.mjs](scripts/bench.mjs)（零依赖，Node 18+），内嵌 mock OpenAI 上游：

```bash
node scripts/bench.mjs --key rk_live_xxx [--base http://localhost:8080] [--duration 10] [--concurrency 16]
```

参考数字（release build，`--duration 10 --concurrency 16`，Windows 11 桌面机，两轮取稳定值）：

| 场景 | RPS | avg | P50 | P95 | P99 | 错误 |
|---|---|---|---|---|---|---|
| `GET /healthz`（纯 HTTP 栈） | ~9900 | 1.6ms | 1.3ms | 3.1ms | 4.3ms | 0 |
| `GET /v1/models`（内存鉴权） | ~9600 | 1.7ms | 1.5ms | 3.0ms | 3.8ms | 0 |
| `POST /v1/chat/completions`（全链路） | ~720 | 22ms | 4.4ms | 7.7ms | 300–390ms | 0 |

- chat 场景为语义缓存 L1 **全未命中**重路径（每请求随机 nonce）：鉴权 → 路由 → 缓存查写 → 上游转发 → 计费扣减 → 异步落库；P50 仍在 5ms 内，P99 毛刺主要来自落库批刷与回环 mock 抖动。
- 内存：持续 4 万+ 请求（缓存 1536 条驻留）后进程 WorkingSet ≈ 29MB。
- 环境注记：本机回环 mock 上游；压测用户并发上限需 ≥ `--concurrency`（默认 16）。

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

也可以把变量放进项目根目录的 **`.env` 文件**（模板见 `.env.example`；`.env` 已被 `.gitignore` 排除，不会误提交）。加载优先级：**进程环境变量 > `.env` > `config/default.toml`**（同名时高优先级覆盖低优先级；`.env` 不存在时静默跳过）。

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
| `[logging]` | `store` = `sqlite` \| `elasticsearch` | 链路日志后端：`sqlite` 写主库（SQLite/PG）`request_logs` 表；`elasticsearch` 按日索引 `<prefix>-YYYY.MM.DD` 写 ES（配 `elasticsearch_url/index_prefix/username/password`）。`body_preview_max_bytes` 请求/响应体预览上限（0=不采集，默认 8192）；`retention_days` 保留天数（0=永久） |
| `[metrics]` | `export_token` | `/metrics` 导出令牌：非空时 `Bearer <token>` 可代替管理端 JWT 抓取（供 Prometheus 抓取器使用）；为空时仅管理端 JWT 可访问 |

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
  reqlog.rs      请求链路模型 + 存储抽象（SQLite / PostgreSQL / Elasticsearch）
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
- [x] 请求链路追踪：全量记录候选顺序/权重/failover 链，请求/响应体预览，失败/时间窗筛选与服务端搜索分页，管理后台可视化，与 `usage_logs` 以 `request_id` 关联
- [x] 内存态 + 异步落库；SQLite / Postgres 可切换；缓存 memory / Redis 可切换
- [x] 指标仪表盘：请求量/成功率/延迟/tokens 聚合 + 上游 TopN + 时序；上游熔断/并发状态面板；失败审计
- [x] 门户：邮箱验证码注册/登录、找回密码、Key 创建/刷新（明文一次）、余额、用量趋势、在线对话、奖励申领
- [x] 管理：用户 CRUD、并发/额度、模型倍率与用户倍率热更新、模型/组/路由维护、全局用量、奖励配置与审核、SMTP 邮箱设置（页面化）
- [ ] RPM/TPM 限流（响应缓存已实现，见上）
- [ ] 第三方登录：微信 / 支付宝（当前为邮箱验证码）
- [x] Elasticsearch 请求日志后端：按日索引写入 + Basic 认证；请求/响应体预览跟随存储后端，`retention_days` 过期清理
- [x] Prometheus 指标导出：`GET /metrics` 文本格式（管理端 JWT 或 `metrics.export_token` 鉴权）
- [x] 语义缓存：L1 精确 + L2 embedding 余弦相似度双层命中（命中免费回放），后台开关 + 命中率可视
- [x] 模型组 JSON 导入/导出（两步向导：干跑预览 diff + 缺失上游跳过）与路由批量编辑（多选删除/改倍率/改权重）
- [x] API 文档页：管理端全量（快速开始/鉴权/协议/流式/错误码/Prometheus 接入）+ 门户精简版，代码块一键复制与语言 Tabs

---

## License

本项目采用 [MIT License](LICENSE) 开源协议（Copyright © 2026 stoneaigc）。
