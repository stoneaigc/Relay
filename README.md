<div align="center">

# Relay

**自托管 LLM API 网关 · 单进程 Rust 二进制 · 默认零外部依赖**

把任意多家模型供应商收敛成一个 **OpenAI / Anthropic 兼容入口**：智能路由 · 协议互转 · 故障转移 · 语义缓存 · 计费管控 · 全链路追踪，自带用户门户与运营后台，发 Key、计费、审计开箱即用。

[![CI](https://github.com/stoneaigc/Relay/actions/workflows/ci.yml/badge.svg)](https://github.com/stoneaigc/Relay/actions/workflows/ci.yml)
[![Release](https://img.shields.io/github/v/release/stoneaigc/Relay)](https://github.com/stoneaigc/Relay/releases/latest)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)
[![Stars](https://img.shields.io/github/stars/stoneaigc/Relay?style=social)](https://github.com/stoneaigc/Relay/stargazers)

[📖 完整文档（Wiki）](https://github.com/stoneaigc/Relay/wiki) · [🚀 快速开始](#-快速开始) · [💬 交流社区](#-交流社区) · [⬇️ Releases](https://github.com/stoneaigc/Relay/releases)

</div>

<!-- TODO: 在此插入两张截图（docs/images/portal.png 用户门户、docs/images/admin.png 运营后台） -->

```text
客户端 SDK ──▶ Relay 网关 ──▶ OpenAI / Anthropic / DeepSeek / 通义 / 智谱 / 私有部署 …
门户用户  ──▶ (鉴权 → 路由 → 协议互转 → 故障转移 → 计费 → 链路追踪)
运营后台 ──▶
```

---

## ✨ 核心特性

**转发核心**

- 🔌 **双协议入站**：`/v1/chat/completions`（OpenAI）与 `/v1/messages`（Anthropic），流式 / 非流式，支持工具调用与多模态图片
- 🔁 **协议互转**：客户端与上游协议不一致也透明转发，流式逐 chunk 回译——换模型不改代码
- 🎯 **智能路由**：一个对外模型名映射多个上游，六种负载策略（加权随机/加权轮询/轮询/优先级/成本优先/延迟优先）
- 🛡️ **故障转移 + 熔断**：失败/超时/429/5xx 自动切换候选，连续失败熔断、半开探测自愈
- ⚡ **语义缓存**：L1 精确 + L2 相似度命中，路由级开关，命中按折扣计费

**计费与运营**

- 💰 **计费管控**：token 计费、倍率热更新、日/月预算、RPM/TPM 限流、并发槽
- 🧑‍💻 **用户门户**：邮箱注册、API Key 管理（明文仅展示一次）、模型广场、在线对话、用量账单、奖励申领
- 🏢 **运营后台**：用户额度、供应商/模型/路由维护、奖励任务、操作审计、软件更新

**可观测与安全**

- 🔍 **请求链路追踪**：每次请求的候选顺序、failover 链、tokens、请求/响应体预览全留痕，失败标红
- 📈 **指标**：ECharts 仪表盘 + Prometheus `/metrics` 导出
- 🔐 **安全**：登录防爆破、入站校验、上游错误脱敏；链路日志支持 SQLite / PostgreSQL / Elasticsearch

---

## 🚀 快速开始

### 方式一：在线安装（推荐，一条命令）

```bash
curl -fsSL https://raw.githubusercontent.com/stoneaigc/Relay/main/deploy/install-online.sh -o install-online.sh \
  && sudo bash install-online.sh
```

自动完成：架构检测 → 下载最新 Release → 交互引导（端口/管理员密码，均有默认值，直接回车）→ 生成随机 JWT 密钥 → 注册 systemd → 健康检查。支持免交互（环境变量）与镜像加速（`DL_PREFIX`），详见 [Wiki · 安装与升级](https://github.com/stoneaigc/Relay/wiki/%E5%AE%89%E8%A3%85%E4%B8%8E%E5%8D%87%E7%BA%A7)。

### 方式二：离线安装（内网 / 手动下载）

从 [Releases](https://github.com/stoneaigc/Relay/releases/latest) 下载 `relay-<版本>-linux-<架构>.tar.gz`（`x86_64` / `arm64`，musl 静态链接），上传服务器：

```bash
tar xzf relay-*.tar.gz && cd relay-*/
sudo ./install.sh                     # 默认装 /opt/relay,自动注册 systemd
curl http://localhost:8080/healthz    # 探活
```

> ⚠️ 离线安装默认管理员 `admin / Relay@123`，生产环境启动前务必编辑 `/opt/relay/relay.env` 覆盖密码与 JWT 密钥（在线安装会自动生成）。

### 方式三：Docker Compose

```bash
docker compose up -d --build
curl http://localhost:8080/healthz
```

### 方式四：本地开发

```bash
cargo run                                            # 后端 :8080,首次启动自动建表
cd frontend/portal && npm install && npm run dev     # 用户门户 :5173
cd frontend/admin  && npm install && npm run dev     # 管理后台 :5174
```

**升级**：在线安装的执行 `sudo bash /opt/relay/upgrade.sh`（自动备份、可回滚），或管理后台「设置 → 软件更新」一键检查；也可 Docker / 离线包原地升级，详见 Wiki。

---

## ⏱️ 五分钟跑通主流程

1. 登录管理后台 → 「上游模型」**添加上游**（自带模型探测与连接测试）→ 勾选模型批量接入并填单价
2. 「模型组」**新建模型组** → **添加路由**（对外模型名 → 实际模型，可挂多条形成主备/负载池）
3. 「用户管理」**创建用户**并绑定模型组——该用户立即可见且仅可见这组模型
4. 用户打开门户注册登录 → 创建 API Key（明文仅一次，立即复制）
5. 用 Key 调用网关：

```bash
curl http://localhost:8080/v1/chat/completions \
  -H "Authorization: Bearer rk_live_..." \
  -H "Content-Type: application/json" \
  -d '{"model":"gpt-4o","messages":[{"role":"user","content":"hi"}]}'
```

6. 回到后台：「请求链路」看每次调用的候选顺序与 failover 细节，「全局用量」看费用分布——余额与计费在门户同步可见

---

## 🧭 部署后你会得到

| 入口 | 地址 | 给谁用 |
|---|---|---|
| 数据面 API | `/v1/chat/completions` `/v1/messages` `/v1/models` `/v1/embeddings` `/v1/responses` | 客户端 SDK / 应用 |
| 用户门户 | `/portal/` | 终端用户：注册、Key、对话、账单 |
| 运营后台 | `/admin/` | 运营者：用户、模型、路由、观测、审计 |
| 监控 | `/healthz` `/metrics` | 探活与 Prometheus 抓取 |

配置通过 `config/default.toml` + `RELAY_` 前缀环境变量覆盖（`RELAY_SERVER__BIND`、`RELAY_AUTH__JWT_SECRET`…），SQLite 开箱即用，生产可切 PostgreSQL + Redis。完整配置项与部署形态见 [Wiki · 架构与配置](https://github.com/stoneaigc/Relay/wiki/%E6%9E%B6%E6%9E%84%E4%B8%8E%E9%85%8D%E7%BD%AE)。

---

## 📖 完整文档（Wiki）

| 章节 | 内容 |
|---|---|
| [项目总览](https://github.com/stoneaigc/Relay/wiki) | 定位 / 核心能力 / 三个入口 |
| [安装与升级](https://github.com/stoneaigc/Relay/wiki/%E5%AE%89%E8%A3%85%E4%B8%8E%E5%8D%87%E7%BA%A7) | 在线安装、离线安装、Docker、升级回滚、卸载 |
| [Nginx 反向代理](https://github.com/stoneaigc/Relay/wiki/Nginx-%E5%8F%8D%E5%90%91%E4%BB%A3%E7%90%86) | 子路径暴露、兜底规则、SSE 注意事项 |
| [管理后台功能介绍](https://github.com/stoneaigc/Relay/wiki/%E5%8A%9F%E8%83%BD%E4%BB%8B%E7%BB%8D-%E7%AE%A1%E7%90%86%E5%90%8E%E5%8F%B0) | 概览/用户/模型组/路由/链路/缓存/用量/审计/设置 逐页说明 |
| [用户门户功能介绍](https://github.com/stoneaigc/Relay/wiki/%E5%8A%9F%E8%83%BD%E4%BB%8B%E7%BB%8D-%E7%94%A8%E6%88%B7%E9%97%A8%E6%88%B7) | 概览/模型广场/对话/奖励/对话日志/API 文档 逐页说明 |
| [架构与配置](https://github.com/stoneaigc/Relay/wiki/%E6%9E%B6%E6%9E%84%E4%B8%8E%E9%85%8D%E7%BD%AE) | 架构 / 请求生命周期 / 配置项 / 部署形态 / 性能基准 |
| [FAQ](https://github.com/stoneaigc/Relay/wiki/FAQ) | 常见问题与安全披露 |

---

## 💬 交流社区

<!-- TODO: 加群方式占位——补充群二维码图片与群号后替换此处 -->

> 交流群二维码与群号待补充，敬请期待 🙌

---

## 🆚 与同类项目的定位差异

| 项目 | 语言 / 形态 | 定位特点 |
|---|---|---|
| one-api / NewAPI | Go 单二进制 | 社区规模最大的 OpenAI 协议聚合网关生态 |
| LiteLLM | Python（Proxy + SDK） | 供应商覆盖面广，依托 Python 生态扩展 |
| **Relay** | Rust 单二进制 | OpenAI + Anthropic **双协议原生互转**、六种负载策略与 failover、全链路请求追踪、内置门户与运营闭环、默认零外部依赖 |

> 各项目均在快速演进，选型请以官方文档与实测为准。

**适合**：自建 API 站对外售卖 token、公司内部统一 LLM 网关与成本分摊、多供应商聚合容灾、给 AI 应用做模型后端。
**不适合**：需要多租户/RBAC/SSO 的平台型 SaaS、超大规模流量（单实例定位）。

---

## 📊 性能

零依赖基准脚本（内嵌零延迟 mock 上游，数字代表网关自身开销，release build，Windows 11 桌面机）：

```bash
node scripts/bench.mjs --key rk_live_xxx [--duration 10] [--concurrency 16]
```

| 场景 | RPS | P50 | P95 |
|---|---|---|---|
| `GET /healthz` | ~9900 | 1.3ms | 3.1ms |
| `POST /v1/chat/completions`（全链路重路径） | ~720 | 4.4ms | 7.7ms |

持续 4 万+ 请求后进程内存 ≈ 29MB。

---

## 🗺️ 路线图

- ✅ 双协议互转 + 六种负载策略 + 故障转移/熔断 · 语义缓存 · 计费管控 · 请求链路追踪 · 双前端全功能 · 双架构自动 Release
- [ ] 上游流式无 usage 时的本地 token 估算计费
- [ ] 第三方登录（微信 / 支付宝）
- [ ] 英文版文档

## 🤝 贡献

欢迎 Issue 与 PR：Bug 反馈请附复现步骤与版本信息；提交 PR 前请本地跑通 `cargo test`（前端改动请 `npm run build` 验证）。项目结构、CI 流水线说明见 [Wiki · 架构与配置](https://github.com/stoneaigc/Relay/wiki/%E6%9E%B6%E6%9E%84%E4%B8%8E%E9%85%8D%E7%BD%AE)。

## ⭐ Star History

[![Star History Chart](https://api.star-history.com/svg?repos=stoneaigc/Relay&type=Date)](https://star-history.com/#stoneaigc/Relay&Date)

## License 与致谢

本项目采用 [MIT License](LICENSE) 开源协议（Copyright © 2026 stoneaigc）。

Relay 受 [RunAPI](https://github.com/runify-dev/runapi) 启发并在其网关架构构想上二次开发而来，感谢开源社区的开放精神。

如果 Relay 对你有帮助，欢迎点亮 ⭐️，让更多需要 LLM 网关的团队看到它。
