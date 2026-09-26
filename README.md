# Relay

[![CI](https://github.com/stoneaigc/Relay/actions/workflows/ci.yml/badge.svg)](https://github.com/stoneaigc/Relay/actions/workflows/ci.yml)
[![Release](https://img.shields.io/github/v/release/stoneaigc/Relay)](https://github.com/stoneaigc/Relay/releases/latest)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)
![Rust](https://img.shields.io/badge/Rust-stable-orange)

> **LLM API 转发网关**：单进程 Rust 二进制，对外同时兼容 **OpenAI** 与 **Anthropic** 协议，对内可接入多家供应商并做双向协议互转与故障转移；全内存热路径 + 异步落库，默认零外部依赖。附 C 端自助门户与运营管理后台。

<!-- TODO: 建议在此处补充两张截图（docs/images/portal.png 用户门户、docs/images/admin.png 运营后台），让访客直观看到双前端门面 -->

[核心能力](#核心能力) · [快速开始](#快速开始) · [数据面接口](#数据面接口) · [为什么选 Relay](#为什么选-relay) · [可观测](#可观测) · [性能基准](#性能基准) · [配置](#配置) · [部署形态](#部署形态) · [开发与 CI](#开发与-ci) · [路线图](#路线图) · [FAQ 与安全](#faq-与安全) · [Wiki](docs/wiki/Home.md)

---

## 核心能力

**转发核心**

- **双协议入站**：`/v1/chat/completions`（OpenAI）与 `/v1/messages`（Anthropic），流式 / 非流式。
- **协议互转**：上游是 OpenAI 还是 Anthropic 都透明路由，支持文本、工具调用、多模态图片，流式逐 chunk 回译。
- **智能路由**：一个对外模型名映射多个上游；加权随机 / 加权轮询 / 轮询 / 优先级 / 成本优先 / 延迟优先（P50）六种负载策略。
- **故障转移 + 熔断**：连接失败 / 超时 / 429 / 5xx 自动切换下一候选；连续失败熔断、半开探测自动恢复。
- **语义缓存**：L1 精确命中 + L2 embedding 相似度命中；路由级开关（默认关）、仅缓存确定性请求（temperature=0 或带 seed）、命中折扣率可配、调用方 `no-cache` 逃生口。

**计费与运营**

- **计费与管控**：token 计费、模型/用户级倍率热更新、日/月预算（耗尽返回 429 `insufficient_quota`）、RPM/TPM 限流（429 结构化响应）、并发槽。
- **用户门户**：邮箱验证码注册/登录/找回、API Key 创建与轮换（明文仅展示一次）、余额与用量趋势、在线对话、奖励申领。
- **运营后台**：用户与额度管理、供应商/模型/模型组/路由维护、模型组 JSON 导入导出与路由批量编辑、未定价提醒、操作审计、奖励任务配置与审核、SMTP 页面化配置、内置 API 文档页。

**可观测与安全**

- **请求链路追踪**：每次请求完整记录候选顺序、failover 链、熔断跳过、tokens 与请求/响应体预览；链路日志支持 SQLite / PostgreSQL / Elasticsearch 三后端，保留期自动清理。
- **指标**：仪表盘（请求量/成功率/延迟分位/Tokens 吞吐/时序）、上游熔断与并发面板、Prometheus `GET /metrics` 文本导出。
- **安全**：登录防爆破（三入口滑动窗口）、入站 payload 校验、上游错误统一脱敏（对外抹除内网地址与密钥）、全局 401 踢回登录。

---

## 快速开始

### 环境要求

- **Release 包**：Linux x86_64 / arm64（musl 静态链接，兼容任意发行版），无其他依赖
- **Docker**：任意支持 Docker Compose v2 的环境
- **本地开发**：Rust stable（edition 2021）+ Node ≥ 20（构建前端）；基准脚本需 Node ≥ 18

### 方式一：在线安装（推荐，一条命令）

参考 1Panel 的快速安装体验——**一条命令 + 交互引导**，自动完成下载、配置、注册 systemd 服务与健康检查：

```bash
curl -fsSL https://raw.githubusercontent.com/stoneaigc/Relay/main/deploy/install-online.sh -o install-online.sh \
  && sudo bash install-online.sh
```

交互引导项均有默认值（安装目录 `/opt/relay`、端口 `8080`、管理员 `admin` + 自动生成的强密码），直接回车即可。脚本同时自动生成随机 JWT 密钥（`relay.env`，`chmod 600`），比手写默认配置更安全。支持免交互（环境变量）与镜像加速（`DL_PREFIX`），详见 [Wiki · 安装与升级](docs/wiki/安装与升级.md)。

### 方式二：离线安装（内网 / 手动下载）

服务器无法访问 GitHub 时，从 [GitHub Releases](https://github.com/stoneaigc/Relay/releases/latest) 下载对应架构的 `relay-<版本>-linux-<架构>.tar.gz`（架构后缀为 `x86_64` / `arm64`，musl 静态链接），上传服务器后：

```bash
tar xzf relay-*.tar.gz && cd relay-*/
sudo ./install.sh                     # 默认装 /opt/relay,自动注册 systemd 服务
curl http://localhost:8080/healthz    # 探活
```

> ⚠️ 离线安装默认管理员为 `admin / Relay@123`、JWT 密钥为开发默认值，生产环境启动前务必编辑 `/opt/relay/relay.env` 覆盖（在线安装会自动生成）。

### 方式三：Docker Compose

```bash
docker compose up -d --build        # 默认 SQLite,数据落 named volume
curl http://localhost:8080/healthz
```

> **注意**：compose 中的 `RELAY_AUTH__JWT_SECRET` / `RELAY_ADMIN__PASSWORD` 默认是注释态，对外部署前务必取消注释并覆盖默认口令，详见下方[部署形态](#部署形态)。

### 方式四：本地开发

```bash
cargo run                                            # 后端 :8080,首次启动自动建表
cd frontend/portal && npm install && npm run dev     # 用户门户 :5173
cd frontend/admin  && npm install && npm run dev     # 管理后台 :5174
```

> 首次启动自动播种 3 个示例奖励任务；默认管理员与初始配置见 `config/default.toml`，**生产务必用环境变量覆盖**。

### 升级与子路径反代

- **升级**：在线装的直接 `sudo bash /opt/relay/upgrade.sh`（自动备份、可回滚）；离线装的下载新包重新执行 `install.sh`（配置与数据自动保留）。也可在管理后台「设置 → 软件更新」一键检查。
- **Nginx 子路径对外暴露**（如 `https://域名/relay/admin/`）：完整配置与兜底规则见 [Wiki · Nginx 反向代理](docs/wiki/Nginx-反向代理.md)。

### 五分钟走完主流程

1. 管理后台登录 → 「模型」页**添加上游**（自带模型列表探测与连接测试）→ 勾选模型批量接入（可顺带填单价）。
2. 「模型组」页**新建模型组** → **添加路由**（对外模型名 → 实际模型，支持批量与倍率）。
3. 「用户管理」**创建用户**并绑定模型组——该用户立即可见且仅可见这组模型。
4. 用户打开门户注册/登录 → 创建 API Key（明文仅一次，请立即复制）。
5. 用 Key 调用网关：

```bash
curl http://localhost:8080/v1/chat/completions \
  -H "Authorization: Bearer <你的 rk_live_...>" \
  -H "Content-Type: application/json" \
  -d '{"model":"<对外模型名>","messages":[{"role":"user","content":"hi"}]}'
```

6. 回到管理后台：**请求链路**看每次调用的候选顺序与 failover 细节，**接口指标**看实时流量，**全局用量**看费用分布——余额与计费在门户同步可见。

---

## 数据面接口

| 方法 | 路径 | 说明 |
|---|---|---|
| GET | `/healthz` | 健康检查 |
| GET | `/v1/models` | 当前用户可见的对外模型列表（按组隔离） |
| POST | `/v1/chat/completions` | OpenAI Chat Completions 协议，流式 / 非流式 |
| POST | `/v1/responses` | OpenAI Responses 协议转发 |
| POST | `/v1/messages` | Anthropic Messages 协议，流式 / 非流式 |
| POST | `/v1/embeddings` | OpenAI 兼容 embeddings 转发 |
| GET | `/metrics` | Prometheus 指标导出（管理端 JWT 或 `metrics.export_token` 鉴权） |

入站校验（messages 非空、类型合法）与按组模型隔离默认开启；完整参数与错误码见管理后台内置的 **API 文档页**（含 curl / Python / Node 示例一键复制）。

---

## 为什么选 Relay

接入多家模型供应商时，团队通常会撞上同一批问题：不同供应商协议不一致、某家挂了业务就断、用量和成本没人看得清、给外部发 Key 之后更是黑盒。Relay 把这些收敛成一个入口：客户端按 OpenAI 或 Anthropic 协议发请求，网关完成鉴权、按策略在多家供应商间路由、必要时做协议互转、某条通道挂了自动切换——每一次转发都计费、留痕、可观测。

```
客户端 SDK ──▶ Relay 网关 ──▶ OpenAI / Anthropic / DeepSeek / 通义 / 智谱 / 私有部署 …
门户用户  ──▶ (鉴权 → 路由 → 协议互转 → 故障转移 → 计费 → 链路追踪)
运营后台 ──▶
```

**一句话定位：面向个人开发者与中小团队的自托管 LLM API 网关——把任意多家模型供应商收敛成一个 OpenAI / Anthropic 兼容入口，并且天生自带「运营」属性：发 Key、计费、限流、审计、门户，开箱即用。**

它同时扮演三个角色：

| 角色 | 价值 |
|---|---|
| **对调用方：统一的模型入口** | 一套 OpenAI（或 Anthropic）协议打天下，后面接哪家供应商随便换——换模型不改代码，某家挂了自动切，语义缓存还能省钱 |
| **对运营者：可售卖的 API 站点** | 邮箱注册发 Key、额度与倍率、日月预算、限流、奖励任务拉新、用量账单——对外提供模型服务（或对内做成本分摊）所需的商业闭环是现成的 |
| **对运维者：可观测的流量枢纽** | 每一次请求的候选顺序、failover 链、计费明细全部留痕；指标仪表盘 + Prometheus 导出 + 熔断面板，出问题 30 秒定位到是哪家上游 |

### 适合这些场景

- **自建 API 站对外提供服务 / 售卖 token**：门户注册 → 创建 Key → 计费扣减 → 奖励活动拉新，商业闭环开箱即用。
- **公司内部统一 LLM 网关**：各部门共用一个入口，按模型组隔离可用模型，按用户统计用量做成本分摊，日/月预算管住失控调用。
- **多供应商聚合与容灾**：主备通道加权分流，供应商故障自动切换、熔断自愈；成本优先 / 延迟优先策略按需切换。
- **给你的 AI 应用做模型后端**：应用只对接一个 OpenAI 兼容地址，底层模型随时增删替换，调用方无感。
- **需要 prompt 与成本可追溯的团队**：每次调用的请求/响应预览、tokens、计费、failover 链全留痕，复盘与审计有据可查。

### 不适合这些场景（诚实边界）

- 需要**多租户 / 组织架构 / RBAC / SSO** 的平台型 SaaS——产品形态固定为「一个运营者 + N 个用户」两级，刻意不做团队体系。
- **超大规模流量**（单实例定位：多实例可共享 PostgreSQL / Redis，但未做大规模并发压测与专项调优）。
- 需要**Key 级模型白名单**的精细化管控——现有「用户绑定模型组」已覆盖隔离需求，Key 级粒度判定不做。

### 与同类项目的定位差异

| 项目 | 语言 / 形态 | 定位特点 |
|---|---|---|
| one-api / NewAPI | Go 单二进制 | 社区规模最大的 OpenAI 协议聚合网关生态，渠道与令牌体系成熟 |
| LiteLLM | Python（Proxy + SDK） | 供应商覆盖面广，OpenAI 格式统一，依托 Python 生态扩展 |
| **Relay** | Rust 单二进制 | OpenAI + Anthropic 双协议原生入站互转、六种负载策略与 failover、全链路请求追踪、内置门户与运营闭环、默认零外部依赖 |

> 以上为定位概述，各项目均在快速演进，选型请以官方文档与实测为准。

---

## 可观测

### 请求链路追踪（亮点功能）

管理后台 → **请求链路**，展开任意一次请求可看到：

- **候选顺序**：负载策略命中的上游排序与权重。
- **实际尝试链**：每跳上游的状态、延迟、错误，含熔断跳过标记——为什么走这条路由、失败切到哪，一目了然。
- **Tokens 与计费**：输入/输出/计费 tokens、是否流式。
- **请求/响应体预览**：按上限截断存储，便于复盘 prompt（默认上限 8192 字节、保留 30 天，均可在后台配置，超期自动清理）。

失败请求自动标红。

### 指标与告警

- 管理后台内置仪表盘：请求量 / 成功率 / 延迟分位 / Tokens 吞吐 / 时序，以及上游熔断与并发面板。
- `GET /metrics` 提供 Prometheus 文本格式导出，可直接接入 Prometheus + Grafana。

---

## 性能基准

内置基准脚本（零依赖，内嵌零延迟 mock 上游，数字代表网关自身开销）：

```bash
node scripts/bench.mjs --key rk_live_xxx [--base http://localhost:8080] [--duration 10] [--concurrency 16]
```

参考数字（release build，`--duration 10 --concurrency 16`，Windows 11 桌面机，仅供横向参考，请以你的环境自测为准）：

| 场景 | RPS | avg | P50 | P95 | 错误 |
|---|---|---|---|---|---|
| `GET /healthz`（纯 HTTP 栈） | ~9900 | 1.6ms | 1.3ms | 3.1ms | 0 |
| `GET /v1/models`（内存鉴权） | ~9600 | 1.7ms | 1.5ms | 3.0ms | 0 |
| `POST /v1/chat/completions`（全链路） | ~720 | 22ms | 4.4ms | 7.7ms | 0 |

- chat 为语义缓存全未命中重路径（鉴权 → 路由 → 缓存查写 → 上游转发 → 计费 → 异步落库），P50 仍在 5ms 内。
- 内存：持续 4 万+ 请求后进程 WorkingSet ≈ 29MB。

---

## 配置

配置文件 `config/default.toml`，可被环境变量覆盖（前缀 `RELAY_`，`__` 分隔层级）：

```bash
RELAY_SERVER__BIND=0.0.0.0:9000
RELAY_AUTH__JWT_SECRET=your-secret
RELAY_ADMIN__PASSWORD=your-admin-password
RELAY_DATABASE__TYPE=postgres
```

| 配置段 | 说明 |
|---|---|
| `[server]` | 监听地址 |
| `[database]` | `sqlite`（默认，单文件零依赖）或 `postgres`（分项或完整 url） |
| `[cache]` | `memory`（默认）或 `redis`（多实例共享） |
| `[auth]` / `[admin]` | JWT 密钥与后台凭据，生产必须覆盖 |
| `[email]` | 注册验证码 SMTP；留空为开发模式（验证码打日志并随响应返回） |
| `[defaults]` | 全局默认限额：并发槽、新用户注册赠送 token |
| `[proxy]` | 转发超时：非流式总超时 / 连接超时可配，流式不设总超时 |
| `[logging]` | 链路日志后端（SQLite/PG/ES）、正文预览上限（0=不采集）、保留天数（0=永久，默认 30 天） |
| `[metrics]` | `/metrics` 导出令牌（供 Prometheus 抓取器使用） |

> `[proxy]` 与 `[metrics]` 为代码内置默认值，未出现在 `config/default.toml`，按需用环境变量覆盖即可。供应商 / 模型 / 模型组 / 路由 / 奖励任务全部由管理后台维护并持久化到数据库，不需要改配置文件。

---

## 部署形态

| 形态 | 数据库 | 缓存 | 链路日志 | 适用场景 |
|---|---|---|---|---|
| **All-in-One（默认）** | SQLite | 进程内 | SQLite | 自用 / 小规模，零外部依赖 |
| **生产** | PostgreSQL | Redis（可选） | PostgreSQL 或 Elasticsearch | 多实例 / 高并发 / 日志检索 |

两种部署通道（详见[快速开始](#快速开始)与仓库 `deploy/` 脚本）：

- **在线安装（推荐）**：`install-online.sh` 一条命令交互式安装，自动下载双架构 Release 包、生成随机 JWT 密钥与管理员凭据、注册 systemd；后续 `upgrade.sh` 一键升级（自动备份、可回滚）。
- **离线安装 / Release 包 → systemd**：单二进制 + 静态前端，专用系统用户运行，幂等升级；敏感变量可写入 `/opt/relay/relay.env`（systemd EnvironmentFile 自动注入）。
- **Docker Compose**：`docker compose up -d --build`，SQLite 默认、`--profile pg` 一键切 Postgres。

部署细节（在线 / 离线 / 升级 / 卸载 / Nginx 子路径反代完整配置）见 [Wiki · 安装与升级](docs/wiki/安装与升级.md) 与 [Wiki · Nginx 反向代理](docs/wiki/Nginx-反向代理.md)。

**生产检查清单**：

- [ ] `RELAY_AUTH__JWT_SECRET` 换成长随机串；`RELAY_ADMIN__USERNAME/PASSWORD` 首次启动前覆盖默认口令
- [ ] 敏感变量统一写入 `/opt/relay/relay.env`（systemd 部署）或 compose environment（Docker 部署），不落代码与镜像
- [ ] 反向代理终结 TLS；`GET /metrics` 仅对内网暴露或配置导出令牌
- [ ] 数据备份计划（SQLite 库文件 / `pg_dump`）
- [ ] 邮箱功能按需配置 `RELAY_EMAIL__SMTP_*`

---

## 开发与 CI

### 项目结构

```
src/              Rust 后端(数据面/门户/管理面/路由/计费/缓存/链路存储)
frontend/portal   C 端用户门户(React + Vite)
frontend/admin    运营管理后台(React + Vite)
migrations/       数据库迁移(SQLite / PostgreSQL)
tests/            集成测试(路由与负载策略)
deploy/           build.sh 源码打包 + install.sh 安装 + systemd 单元
scripts/          数据面基准脚本 / 本地 mock 上游
docs/             设计文档与内部记录
.github/          CI 质量门禁 + 多架构自动发布
```

### CI 流水线

| 流水线 | 触发 | 作用 |
|---|---|---|
| 质量门禁 | push 到 main / 任意 PR | cargo test 全量 + 双前端构建，不过不许合入 |
| 自动发布 | 推送 `v*` tag | 双架构（x86_64 / arm64，musl 静态）构建 → 打包 → 自动创建 GitHub Release |

版本迭代三步：同步修改 `Cargo.toml` 的 `version` → 提交推送 → 打 `v*` tag。tag 与代码版本不一致时发布流水线会拒绝构建。也可用 `deploy/build.sh` 在本地打包（与 CI 产物结构一致；本地包为宿主动态链接，musl 静态包以 CI Release 为准）。

### 贡献

欢迎 Issue 与 PR：

- Bug 反馈请附复现步骤、期望与实际行为、版本信息。
- 提交 PR 前请在本地跑通 `cargo test`（CI 会额外检查双前端构建，前端改动请本地 `npm run build` 验证）。

---

## 路线图

已完成的核心里程碑：

- ✅ 双协议入站 + 协议互转 + 流式,六种负载策略与故障转移/熔断
- ✅ 请求链路追踪(三存储后端) + 指标仪表盘 + Prometheus 导出 + 操作审计
- ✅ 语义缓存(路由级 opt-in / 确定性请求判定 / 折扣计费 / no-cache 逃生口)
- ✅ 计费管控(倍率热更新 / 日月预算 / RPM·TPM 限流 / 并发槽)
- ✅ 用户门户与运营后台全功能 + 登录防爆破 + 奖励任务体系
- ✅ 双通道部署(Docker Compose / systemd) + CI 门禁 + 双架构自动 Release

计划中：

- [ ] 上游流式响应无 usage 时的本地 token 估算计费
- [ ] 拒绝类请求(未授权模型等)落链路日志,补全安全审计视角
- [ ] 第三方登录(微信 / 支付宝)
- [ ] 大规模并发压测与调优报告
- [ ] 英文版文档

---

## FAQ 与安全

**常见问题**

- **如何新增 / 更换供应商？** 管理后台「模型」页添加上游（自带连接测试），在「模型组」里调整路由即可，调用方代码零改动。
- **语义缓存为什么默认关？** 相似度命中存在轻微误差风险，因此按路由 opt-in，且仅缓存确定性请求（temperature=0 或带 seed）；确认业务可接受后再开启。
- **需要多实例 / 更高并发怎么办？** 多个 Relay 实例共享 PostgreSQL（数据）与 Redis（缓存）即可横向扩展；链路日志建议切换 PostgreSQL 或 Elasticsearch 后端。

**安全披露**

发现安全漏洞请勿公开 Issue，优先通过 GitHub 私密安全通告（Security Advisories）或私信联系维护者报告，我们会在确认后尽快修复并在 Release 中致谢。

---

## License 与致谢

本项目采用 [MIT License](LICENSE) 开源协议（Copyright © 2026 stoneaigc）。

**致谢**：Relay 受 [RunAPI](https://github.com/runify-dev/runapi) 的启发，并在其 API 网关的架构构想基础上二次开发而来。感谢 RunAPI 及其生态的开放精神，让转发网关变得可以"看得见、控得住"。

如果 Relay 对你有帮助，欢迎点亮 ⭐️，让更多需要 LLM 网关的团队看到它。
