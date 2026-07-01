# RunAPI —— LLM API 网关设计文档

> 版本:v0.1(草案) · 语言:Rust · 最后更新:2026-06-29

---

## 1. 项目概述

### 1.1 目标

构建一个高性能的大语言模型(LLM)API 网关。核心能力:

1. **对外**:同时暴露两套业界标准 API
   - OpenAI 兼容 API(`/v1/chat/completions`、`/v1/embeddings` 等)
   - Anthropic 兼容 API(`/v1/messages` 等)
2. **对内**:可接入任意上游供应商(OpenAI、Anthropic、Azure OpenAI、Google Gemini、DeepSeek、通义千问、智谱、本地 vLLM/Ollama 等)。
3. **协议互转**:客户端用 OpenAI SDK 发的请求,可路由到 Claude 后端;反之亦然。网关负责双向协议翻译。
4. **统一治理**:鉴权、配额、限流、计费、可观测、缓存、故障转移。
5. **C 端自助门户**:面向终端用户(开发者)的网站,支持**手机号 / 微信 / 支付宝**登录;登录后系统**自动为其分配 API Key**,可查看用量、账单、充值。
6. **管理后台**:面向运营/管理员,管理供应商、路由、用户、密钥、价格、用量报表、系统配置。
7. **前端**:客户端门户与管理后台均用 **React + TypeScript** 实现,分为两个独立前端应用。

### 1.2 非目标(v1 范围外)

- 不做模型训练/微调。
- 不做面向最终用户的聊天对话 UI(门户只做账号/密钥/用量/充值;聊天 Playground 可作为后续增强)。
- 不内置向量数据库(embeddings 仅做转发)。

### 1.3 关键设计原则

- **协议中立的内部表示(UIR, Unified Internal Representation)**:所有入站请求先翻译成统一中间模型,再翻译到目标供应商;避免 N×M 适配爆炸,变成 N+M。
- **流式优先**:SSE 流式是主路径,非流式是流式的特例(聚合)。
- **零拷贝/低延迟**:网关本身不应成为瓶颈,转发开销目标 < 5ms(不含上游)。
- **配置即代码 + 热加载**:供应商、路由、密钥可动态变更,不重启。
- **单机 + 零外部依赖**:不依赖 Redis,数据库用 **SQLite**;**热路径全部走内存**,数据库只做冷启动加载与异步落盘。目标:请求路径上**零(或极少)同步 DB 查询**,以支撑高并发。

---

## 2. 技术栈

| 关注点 | 选型 | 理由 |
|---|---|---|
| 语言 | Rust (2021/2024 edition) | 高并发、低延迟、内存安全 |
| 异步运行时 | `tokio` | 事实标准 |
| HTTP 服务 | `axum` | 基于 tower/hyper,流式与中间件生态好 |
| HTTP 客户端 | `reqwest`(启用 `stream`) | 上游调用,支持流式 body |
| 序列化 | `serde` / `serde_json` | |
| 流式 SSE | `axum::response::sse` + 自定义解析 | 入站出站均需处理 SSE |
| 数据库 | **SQLite**(WAL 模式)+ `sqlx`(编译期校验 SQL) | 单机、零外部依赖;密钥、用户、用量 |
| 内存缓存 | `dashmap`(并发 Map)+ `arc-swap`(配置热替换) | API Key/用户态全量驻留内存,**校验不查库** |
| 限流/并发 | `governor`(进程内令牌桶)+ `AtomicU32/I64` 计数 | **无 Redis**,纯内存 |
| 配置 | `figment`(env + TOML/YAML 合并) | 多源配置 |
| 可观测 | `tracing` + `tracing-subscriber` + `opentelemetry` + `metrics`(Prometheus exporter) | 日志/追踪/指标 |
| 密钥哈希 | `argon2` / `sha2` | 虚拟密钥存储 |
| 错误处理 | `thiserror`(库)+ `anyhow`(应用边界) | |
| 测试 | `cargo test` + `wiremock`(mock 上游) + `criterion`(基准) | |

### 2.1 Workspace 结构(Cargo workspace)

```
runapi/
├── Cargo.toml                # workspace
├── crates/
│   ├── gateway/              # 二进制:HTTP 服务入口、路由装配
│   ├── core/                 # UIR 类型、trait 定义(Provider, Router...)
│   ├── protocols/            # 入站/出站协议适配
│   │   ├── openai/           # OpenAI <-> UIR
│   │   └── anthropic/        # Anthropic <-> UIR
│   ├── providers/            # 上游连接器(openai, anthropic, gemini, azure...)
│   ├── routing/              # 路由引擎、负载均衡、故障转移
│   ├── governance/           # 鉴权、限流、配额、计费
│   ├── storage/              # sqlx(SQLite)仓储层 + 内存态/落盘任务
│   ├── observability/        # tracing/metrics 初始化、用量记录
│   └── admin/                # Admin API
├── migrations/               # sqlx 迁移
├── config/                   # 默认配置文件
└── DESIGN.md
```

---

## 3. 系统架构

### 3.1 分层架构

```
                    ┌─────────────────────────────────────────────┐
   Client SDK  ───► │  Ingress Layer (axum)                        │
 (OpenAI /         │   - TLS / HTTP                                │
  Anthropic)       │   - 路由分发: /v1/chat/completions  /v1/messages │
                    └───────────────┬─────────────────────────────┘
                                    │
                    ┌───────────────▼─────────────────────────────┐
                    │  Middleware (tower layers)                    │
                    │   认证 → 限流 → 配额 → 请求日志 → 追踪          │
                    └───────────────┬─────────────────────────────┘
                                    │
                    ┌───────────────▼─────────────────────────────┐
                    │  Inbound Protocol Adapter                     │
                    │   OpenAI Req  ─┐                              │
                    │   Anthropic Req─┴─►  UIR (统一中间表示)        │
                    └───────────────┬─────────────────────────────┘
                                    │
                    ┌───────────────▼─────────────────────────────┐
                    │  Routing Engine                               │
                    │   model 名 → 目标供应商 + 上游模型             │
                    │   负载均衡 / 故障转移 / 权重 / 灰度            │
                    └───────────────┬─────────────────────────────┘
                                    │
                    ┌───────────────▼─────────────────────────────┐
                    │  Outbound Provider Adapter                    │
                    │   UIR → OpenAI / Anthropic / Gemini / ...     │
                    └───────────────┬─────────────────────────────┘
                                    │
                    ┌───────────────▼─────────────────────────────┐
                    │  Provider Connector (reqwest, 流式)           │
                    └───────────────┬─────────────────────────────┘
                                    │  上游响应(可能是另一种协议)
                    ┌───────────────▼─────────────────────────────┐
                    │  Response Adapter: 上游 → UIR → 客户端协议     │
                    │  (流式逐 chunk 翻译)                          │
                    └───────────────┬─────────────────────────────┘
                                    │
                    ┌───────────────▼─────────────────────────────┐
                    │  Post-processing: 用量统计 / 计费 / 缓存写入    │
                    └───────────────────────────────────────────────┘
```

### 3.2 请求生命周期(以「OpenAI 请求 → Claude 后端」为例)

1. 客户端用 OpenAI SDK 调 `POST /v1/chat/completions`,`model: "claude-opus-4-8"`。
2. 中间件:校验虚拟密钥 → 限流 → 配额检查。
3. Inbound adapter:OpenAI ChatCompletion 请求 → UIR。
4. Routing:查路由表,`claude-opus-4-8` → provider `anthropic-prod`,上游模型 `claude-opus-4-8`。
5. Outbound adapter:UIR → Anthropic Messages 请求(system 提取、role 映射、tools schema 转换)。
6. Connector:带 Anthropic key 发起流式请求。
7. 上游返回 Anthropic SSE 事件流。
8. Response adapter:Anthropic `message_start/content_block_delta/...` → UIR delta → OpenAI `chat.completion.chunk` SSE。
9. 边转发边累计 token,结束后写用量、扣配额、记审计、(可选)写缓存。

### 3.3 三个认证平面(关键概念)

系统存在三种相互独立的身份与鉴权,切勿混淆:

| 平面 | 使用者 | 凭证 | 入口 | 说明 |
|---|---|---|---|---|
| **数据面 Data Plane** | 调用方程序 / SDK | **虚拟 API Key**(`rk-...`) | `/v1/*` 网关端点 | 真正调模型用的,登录后自动发放 |
| **门户面 Portal Plane** | C 端终端用户(开发者) | **会话 JWT**(手机号/微信/支付宝登录换取) | `/portal/*` + React 客户端 | 管理自己的 Key、看用量、充值 |
| **管理面 Admin Plane** | 运营 / 管理员 | **管理员账号 + RBAC**(JWT) | `/admin/*` + React 管理端 | 管供应商、路由、用户、价格 |

> 区别要点:**登录(门户面)≠ 调用(数据面)**。用户用手机号登录拿到的是会话 JWT,只能操作门户;真正调用大模型用的是登录后系统发给他的虚拟 API Key。两套凭证生命周期、撤销、限流维度都不同。

### 3.4 部署拓扑

```
                 ┌────────────────────┐     ┌────────────────────┐
                 │  Client Portal      │     │  Admin Console      │
                 │  (React, C 端)      │     │  (React, 管理端)    │
                 └─────────┬──────────┘     └─────────┬──────────┘
                           │ /portal/* (JWT)          │ /admin/* (JWT+RBAC)
                           ▼                           ▼
   SDK ──/v1/*(API Key)──►┌──────────────────────────────────────┐
                          │        RunAPI Gateway (单进程, Rust)    │
                          │  ┌────────────────────────────────┐    │
                          │  │ 内存态(热路径):              │    │
                          │  │  DashMap<key→KeyEntry>(校验)   │    │
                          │  │  DashMap<user→UserState>       │    │
                          │  │   token余额(Atomic)/并发/限流  │    │
                          │  │  ArcSwap<配置>(路由/供应商)    │    │
                          │  └────────────────────────────────┘    │
                          │  data plane + portal API + admin API   │
                          └──────┬───────────────────┬─────────────┘
                  冷启动加载/异步落盘 │               │ 上游调用
                          ┌──────▼─────┐         ┌────▼─────────┐
                          │  SQLite    │         │ 上游供应商    │
                          │ (WAL,单文件)│         └──────────────┘
                          └────────────┘
                                 ▲
                  ┌──────────────┴───────────────┐
                  │ 外部:短信(阿里/腾讯)、       │
                  │ 微信开放平台、支付宝开放平台    │
                  └──────────────────────────────┘
```

---

## 4. 统一中间表示(UIR)

UIR 是协议无关的核心数据模型,位于 `core` crate。所有翻译都经过它。

### 4.1 请求 UIR(简化示意)

```rust
pub struct ChatRequest {
    pub model: String,                  // 客户端请求的逻辑模型名
    pub messages: Vec<Message>,
    pub system: Option<String>,         // 抽出的 system prompt(Anthropic 独立字段)
    pub max_tokens: Option<u32>,
    pub temperature: Option<f32>,
    pub top_p: Option<f32>,
    pub stop: Vec<String>,
    pub stream: bool,
    pub tools: Vec<ToolDef>,
    pub tool_choice: ToolChoice,
    pub response_format: Option<ResponseFormat>, // json_schema / json_object
    pub metadata: RequestMetadata,      // user_id、request_id 等
    pub extra: serde_json::Map<String, Value>, // 透传供应商专有参数
}

pub struct Message {
    pub role: Role,                     // System | User | Assistant | Tool
    pub content: Vec<ContentBlock>,     // 多模态:文本/图片/工具调用/工具结果
}

pub enum ContentBlock {
    Text(String),
    Image { source: ImageSource },      // url / base64
    ToolUse { id: String, name: String, input: Value },
    ToolResult { tool_use_id: String, content: Vec<ContentBlock>, is_error: bool },
}

pub struct ToolDef {
    pub name: String,
    pub description: Option<String>,
    pub input_schema: Value,            // JSON Schema
}
```

### 4.2 响应 UIR(流式增量)

```rust
pub enum StreamEvent {
    Start { id: String, model: String },
    TextDelta { text: String },
    ToolUseStart { id: String, name: String },
    ToolUseDelta { partial_json: String },
    ToolUseStop,
    Usage(Usage),
    Stop { reason: StopReason },         // EndTurn | MaxTokens | StopSequence | ToolUse
    Error(ProviderError),
}

pub struct Usage {
    pub input_tokens: u32,
    pub output_tokens: u32,
    pub cache_read_tokens: u32,
    pub cache_write_tokens: u32,
}
```

> 设计要点:UIR 的内容块模型刻意贴近 Anthropic 的「content blocks」结构,因为它比 OpenAI 的扁平 `content + tool_calls` 表达力更强,OpenAI→UIR 是无损升维,UIR→OpenAI 是降维(可控)。

---

## 5. 协议适配(Protocol Adapters)

每种对外协议实现两个方向;每个上游供应商实现两个方向。统一 trait:

```rust
/// 入站:客户端协议 <-> UIR
pub trait InboundProtocol {
    fn parse_request(&self, raw: Bytes) -> Result<ChatRequest>;
    fn render_response(&self, uir: &ChatResponse) -> Result<Bytes>;            // 非流式
    fn render_stream_event(&self, ev: &StreamEvent, st: &mut RenderState) -> Option<SseEvent>; // 流式
}

/// 出站:UIR <-> 上游供应商协议
pub trait OutboundProvider {
    fn build_request(&self, uir: &ChatRequest, route: &Route) -> Result<reqwest::Request>;
    fn parse_stream_chunk(&self, raw: &[u8], st: &mut ParseState) -> Vec<StreamEvent>;
    fn parse_response(&self, raw: Bytes) -> Result<ChatResponse>;             // 非流式
}
```

### 5.1 关键翻译难点与对策

| 难点 | OpenAI | Anthropic | 对策 |
|---|---|---|---|
| system prompt | messages 中 role=system | 顶层 `system` 字段 | UIR 单独存 `system`,翻译时按需提取/下沉 |
| 工具调用 | `tool_calls`(数组,放在 assistant message) | `tool_use` content block | 统一为 `ContentBlock::ToolUse` |
| 工具结果 | role=tool 的独立 message | `tool_result` content block(在 user message 内) | 统一为 `ContentBlock::ToolResult`,翻译时重组 message 边界 |
| 流式分帧 | `chat.completion.chunk`,`delta` | `content_block_delta` 等多事件类型 | 用 `RenderState` 跟踪状态机映射 |
| 停止原因 | `finish_reason: stop/length/tool_calls` | `stop_reason: end_turn/max_tokens/tool_use` | UIR 的 `StopReason` 双向枚举映射 |
| 多模态图片 | `image_url`(url 或 data URI) | `source.{type,media_type,data}` | UIR 的 `ImageSource` 归一 |
| token 用量 | `usage.{prompt,completion}_tokens` | `usage.{input,output}_tokens` + cache | UIR `Usage` 超集 |
| 参数差异 | `frequency_penalty` 等 | 无对应 | 不支持的参数:丢弃 + 警告日志(可配置为报错) |

### 5.2 流式状态机示例(Anthropic 上游 → OpenAI 客户端)

```
Anthropic 事件                     UIR                      OpenAI chunk
message_start              →  Start                  →  {choices:[{delta:{role:"assistant"}}]}
content_block_start(text)  →  (记录 index)           →  (不输出)
content_block_delta(text)  →  TextDelta              →  {choices:[{delta:{content:"..."}}]}
content_block_start(tool)  →  ToolUseStart           →  {choices:[{delta:{tool_calls:[{id,function:{name}}]}}]}
content_block_delta(json)  →  ToolUseDelta           →  {choices:[{delta:{tool_calls:[{function:{arguments}}]}}]}
message_delta(stop_reason) →  Stop                   →  {choices:[{finish_reason:"stop"}]}
message_stop               →  (end)                  →  data: [DONE]
```

---

## 6. 路由引擎(Routing)

### 6.1 职责

- 逻辑模型名 → 一个或多个上游目标(Route)。
- 多目标时:负载均衡(加权轮询 / 最少连接 / 一致性哈希)。
- 故障转移(failover):上游 5xx/超时/限流 时按优先级切换。
- 灰度/金丝雀:按百分比分流到不同上游或不同模型。
- 模型别名:把 `gpt-4o` 映射成内部某个 deployment。

#### 6.1.1 路由决策全流程(核心)

**路由键是请求里的 `model` 名,不是用户。** 用户身份只决定鉴权/白名单/扣费;去哪个上游由 model 名经 `model_routes` 决定。三者解耦:

```
客户端 model 名(逻辑名) ──model_routes映射──► [(provider, upstream_model, weight)...]
                                                      │ 按权重选 / 失败 failover
                                                      ▼
                                          provider(自带协议 kind)
                                                      │ 入站协议≠provider协议 → 互转
                                                      ▼
                                          用 upstream_model 调上游
```

**示例:用户用 OpenAI Key 调,但 model 是 Claude**

```
POST /v1/chat/completions
Authorization: Bearer rk_live_xxx
{ "model": "claude-opus-4-8", "messages": [...] }
```

1. **鉴权**:`rk_live_xxx` → 内存 `DashMap` → `user_id`、`interface_kind=openai`。
2. **白名单**:`claude-opus-4-8` 是否在该用户 `allowed_models`(空=全部)。
3. **查表**:内存配置 `model_routes["claude-opus-4-8"]` → `[{anthropic-prod,80},{bedrock,20}]`。
4. **负载均衡**:按权重选中 `anthropic-prod`。
5. **协议互转**:入站 OpenAI、provider `kind=anthropic` → OpenAI→UIR→Anthropic。
6. **调上游**:provider base_url + 真实 key,模型名用 `upstream_model`。
7. **failover**:超时/5xx 换 `bedrock` 重试。
8. **回译**:Anthropic 响应 → UIR → OpenAI 格式返回。

**要点**
- **逻辑名 ≠ 上游真名**:逻辑名可抽象(如 `fast-cheap`),后端随时可换,客户端无感。
- **入站接口与上游供应商无关**:OpenAI 接口进来可路由到 Anthropic 上游(靠互转),反之亦然。
- **可调模型清单**:`GET /v1/models` 返回逻辑模型(按用户白名单过滤),用户从中选 `model`。
- **(可选)按用户路由**:给路由规则加用户分组/标签条件,实现"同一 model 名、不同用户走不同通道"(如 VIP 专属通道)。默认不启用。

### 6.2 路由配置示例(YAML,可热加载)

```yaml
models:
  - name: "claude-opus-4-8"           # 客户端看到的模型名
    targets:
      - provider: "anthropic-prod"
        upstream_model: "claude-opus-4-8"
        weight: 80
      - provider: "bedrock-anthropic" # 同模型不同通道做容灾
        upstream_model: "anthropic.claude-opus-4-8-v1"
        weight: 20
    failover: [ "anthropic-prod", "bedrock-anthropic" ]

  - name: "gpt-4o"
    targets:
      - provider: "azure-openai-eastus"
        upstream_model: "gpt-4o-2024-deploy"

  - name: "fast-cheap"                 # 抽象模型名,屏蔽底层
    targets:
      - provider: "deepseek"
        upstream_model: "deepseek-chat"

providers:
  anthropic-prod:
    kind: anthropic
    base_url: "https://api.anthropic.com"
    api_key_ref: "secret://ANTHROPIC_KEY"
    timeout_ms: 60000
    max_retries: 2
  azure-openai-eastus:
    kind: azure_openai
    base_url: "https://xxx.openai.azure.com"
    api_version: "2024-10-01"
    api_key_ref: "secret://AZURE_KEY"
  deepseek:
    kind: openai            # OpenAI 兼容协议,直接复用 openai 连接器
    base_url: "https://api.deepseek.com"
    api_key_ref: "secret://DEEPSEEK_KEY"
```

### 6.3 故障转移与重试策略

- **可重试错误**:连接超时、429、500/502/503/504。
- **不可重试**:400(请求本身错)、401/403(鉴权)、模型不存在。
- **流式中途失败**:若已输出 token,默认不重试(避免重复内容),记录并向客户端发 error 事件;可配置「未发首 token 前可重试」。
- 退避:指数退避 + 抖动,受 `max_retries` 限制。

---

## 7. 治理(Governance)

### 7.1 鉴权

- **虚拟密钥(Virtual Key)**:网关发给客户端的 key(如 `rk-...`),与真实上游 key 解耦。
- 存储:仅存哈希(`argon2` 或 `sha256`,带前缀便于检索),原文只在创建时返回一次。
- 一个虚拟密钥可绑定:允许的模型集合、配额、限流、过期时间、所属团队/项目。
- 头部兼容:OpenAI 用 `Authorization: Bearer`,Anthropic 用 `x-api-key` —— 两者都接受。

### 7.2 限流(Rate Limiting)

- **主维度:每用户(`user_id`)**。token 余额、并发、RPM/TPM 都以用户为计量主体(一个用户的两把 Key 合并计算)。
- 指标:**并发数(in-flight 请求)**、RPM(请求/分钟)、TPM(token/分钟)。
- **全部走进程内存,无 Redis**:
  - 并发:`UserState` 内 `AtomicU32`,请求进入 `fetch_add`、结束 `fetch_sub`(用 RAII guard 保证断流/panic 也释放),超 `concurrency_limit` 立即拒绝;流式连接占用到流结束。
  - RPM/TPM:`governor`(进程内令牌桶)按用户一个限流器,存于 `DashMap<user_id, RateLimiter>`。
- 限额取值:用户级设置优先,未设则取全局默认(管理后台配,存内存)。
- 超限返回 429 + `Retry-After`(按客户端协议格式)。

> 关于「并发按用户还是按 Key」的取舍见 §7.2.1。

#### 7.2.1 并发按用户 vs 按 Key —— 决策:**按用户**

- 本系统里 **Key 不是独立租户**,只是同一用户的两个协议入口(OpenAI / Anthropic),且**共用同一 token 余额**。计量主体天然是用户。
- 若按 Key 限并发,用户用两把 Key 就能突破限制,失去公平性与成本保护意义;管理后台设的也是「用户并发」。
- 因此:**并发、RPM/TPM、配额都以用户为准**(跨该用户所有 Key 累加)。
- 预留扩展:`api_keys` 可保留可选的 `concurrency_limit` 作为**用户额度内的子上限**(默认不启用),将来若演进出「团队/多项目」场景再开启 Key 级隔离。

### 7.3 配额与计费(Token 制)

**本系统以 token 为计费与配额单位(非美元)。**

- 每个用户有一个**全局 token 余额**(`users.token_balance`),所有接口、所有 Key 的调用都从这同一个池子里扣减。
- **新用户注册赠送 1000 万 token(10,000,000)**。
- 计量口径:每次调用结束后,按 `input_tokens + output_tokens`(可对不同模型设权重系数,见下)从余额扣减。
- **模型权重系数**:不同模型成本差异大,每个模型可设 `token_multiplier`(如贵模型 1 token 记 3),实际扣减 = 原始 token × 系数。默认 1。**该系数在管理后台可视化配置**(`model_prices.token_multiplier`),热更新到内存。
- **内存计数 + 异步落盘(关键)**:余额是 `UserState.token_balance`(`AtomicI64`),扣费在内存原子操作完成,**不同步写库**。后台任务周期性(如每 5s 或累计变更达阈值)批量回写 SQLite;进程退出时强制 flush。
  - 余额校验在请求入口读内存,**零 DB 查询**。
  - 崩溃容忍:极端情况下最多丢失最近一个 flush 周期内的扣减(用户少量"白嫖"),单机场景可接受;flush 周期越短窗口越小。
- 余额 ≤ 0 时,数据面调用拒绝(按客户端协议返回 402/429);门户提示充值。
- 软阈值(如剩余 10%)触发提醒。

### 7.4 用量与报表

- 用量明细落库(`usage_logs`),记录每次调用的 input/output token、接口类型、模型、上游。
- 门户:用户看自己的「剩余额度卡片 + 用量趋势」。
- 管理后台:按用户/模型/供应商聚合,看消耗与剩余额度。
- 价格表 `model_prices` 仍保留(用于内部成本核算/对账),但对 C 端展示以 token 为准。

---

## 8. 数据模型(SQLite)

> **类型说明**:以下 DDL 用通用 SQL 表达概念;在 SQLite 中 `UUID`→`TEXT`、`BIGINT/INT`→`INTEGER`、`NUMERIC`→`REAL` 或 `TEXT`(高精度金额用 TEXT 存)、`TIMESTAMPTZ`→`TEXT`(ISO8601)或 `INTEGER`(unix 秒)、`JSONB`→`TEXT`(JSON 字符串)、`TEXT[]`→ `TEXT`(JSON 数组)。开启 **WAL 模式**(`PRAGMA journal_mode=WAL; synchronous=NORMAL`)以支持多读单写并发。

```sql
-- C 端终端用户
CREATE TABLE users (
  id            UUID PRIMARY KEY,
  phone         TEXT UNIQUE,               -- 手机号(可空,纯第三方登录时)
  nickname      TEXT,
  avatar_url    TEXT,
  status        TEXT NOT NULL DEFAULT 'active', -- active|disabled|banned
  token_balance BIGINT NOT NULL DEFAULT 10000000, -- 剩余 token 余额;新用户默认赠 1000 万
  token_granted_total BIGINT NOT NULL DEFAULT 10000000, -- 累计授予(注册赠送+充值)
  token_used_total    BIGINT NOT NULL DEFAULT 0,        -- 累计消耗
  concurrency_limit   INT,                  -- 并发上限(NULL=用全局默认),管理后台可设
  rpm_limit     INT,                        -- 每分钟请求数(NULL=全局默认)
  tpm_limit     INT,                        -- 每分钟 token 数(NULL=全局默认)
  allowed_models TEXT[] DEFAULT '{}',       -- 模型白名单(空=全部)
  created_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
  last_login_at TIMESTAMPTZ
);
-- 注:token_balance 实时扣减走内存 AtomicI64,周期性批量回写本表(见 §8.1)。

-- 第三方身份绑定(微信/支付宝),一个用户可绑多个
CREATE TABLE user_identities (
  id            UUID PRIMARY KEY,
  user_id       UUID NOT NULL REFERENCES users(id),
  provider      TEXT NOT NULL,             -- wechat | alipay
  -- 微信:优先用 unionid 跨应用统一身份;openid 按 app 维度
  external_id   TEXT NOT NULL,             -- unionid / alipay user_id
  union_id      TEXT,                      -- 微信 unionid(若有)
  raw_profile   JSONB,                     -- 第三方返回的原始资料
  created_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
  UNIQUE(provider, external_id)
);

-- 短信验证码(放内存 Map 带 TTL 即可;此表用于审计/风控,可选)
CREATE TABLE verification_codes (
  id            UUID PRIMARY KEY,
  phone         TEXT NOT NULL,
  code_hash     TEXT NOT NULL,             -- 不存明文
  purpose       TEXT NOT NULL,             -- login | bind | ...
  attempts      INT DEFAULT 0,
  expires_at    TIMESTAMPTZ NOT NULL,
  consumed_at   TIMESTAMPTZ,
  created_at    TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- 管理员
CREATE TABLE admins (
  id            UUID PRIMARY KEY,
  username      TEXT UNIQUE NOT NULL,
  password_hash TEXT NOT NULL,             -- argon2
  role          TEXT NOT NULL DEFAULT 'operator', -- superadmin|operator|viewer
  created_at    TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- 团队/项目(B 端可选;C 端自助场景下用户即默认归属)
CREATE TABLE teams (
  id           UUID PRIMARY KEY,
  name         TEXT NOT NULL,
  created_at   TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- 虚拟密钥
CREATE TABLE api_keys (
  id            UUID PRIMARY KEY,
  user_id       UUID REFERENCES users(id),  -- C 端密钥归属用户
  interface_kind TEXT NOT NULL,             -- openai | anthropic(对应门户两张接口卡片)
  team_id       UUID REFERENCES teams(id),
  key_hash      TEXT NOT NULL UNIQUE,      -- argon2/sha256
  key_prefix    TEXT NOT NULL,             -- 'rk-abc...' 前几位,便于展示/检索
  name          TEXT,
  allowed_models TEXT[] DEFAULT '{}',      -- 空=全部
  rpm_limit     INT,
  tpm_limit     INT,
  budget_usd    NUMERIC(12,4),
  spent_usd     NUMERIC(12,4) DEFAULT 0,
  expires_at    TIMESTAMPTZ,
  disabled      BOOLEAN DEFAULT false,
  created_at    TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- 上游供应商(也可纯配置文件,DB 用于动态管理)
CREATE TABLE providers (
  id           UUID PRIMARY KEY,
  name         TEXT UNIQUE NOT NULL,
  kind         TEXT NOT NULL,              -- openai|anthropic|azure_openai|gemini...
  base_url     TEXT NOT NULL,
  config       JSONB NOT NULL,            -- 协议/版本/超时等
  secret_ref   TEXT NOT NULL,             -- 指向密钥管理(env/vault)
  enabled      BOOLEAN DEFAULT true
);

-- 模型路由
CREATE TABLE model_routes (
  id            UUID PRIMARY KEY,
  model_name    TEXT NOT NULL,
  targets       JSONB NOT NULL,           -- [{provider, upstream_model, weight}]
  failover      JSONB,
  enabled       BOOLEAN DEFAULT true
);

-- 用量日志(高写入,建议分区表 by day)
CREATE TABLE usage_logs (
  id             BIGSERIAL PRIMARY KEY,
  request_id     UUID NOT NULL,
  api_key_id     UUID,
  team_id        UUID,
  client_protocol TEXT,                    -- openai|anthropic
  model_name     TEXT,                     -- 逻辑模型
  provider       TEXT,                     -- 实际上游
  upstream_model TEXT,
  input_tokens   INT,
  output_tokens  INT,
  cache_read_tokens  INT,
  cache_write_tokens INT,
  cost_usd       NUMERIC(12,6),
  status         INT,                      -- HTTP 状态
  latency_ms     INT,
  ttft_ms        INT,                      -- 首 token 时间
  stream         BOOLEAN,
  error_code     TEXT,
  created_at     TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- 模型价格表
CREATE TABLE model_prices (
  model_name        TEXT PRIMARY KEY,
  input_per_mtok    NUMERIC(12,6),         -- 内部成本核算用(美元)
  output_per_mtok   NUMERIC(12,6),
  cache_read_per_mtok  NUMERIC(12,6),
  cache_write_per_mtok NUMERIC(12,6),
  token_multiplier  NUMERIC(8,3) NOT NULL DEFAULT 1.0  -- C 端扣费倍率,管理后台可改
);
```

> 用量写入用「异步批量管道」:请求结束发到内存 channel,后台任务批量 INSERT,避免阻塞主路径。

### 8.1 内存缓存与持久化策略(单机高并发核心)

设计目标:**请求热路径上零同步 DB 查询、零同步 DB 写**;SQLite 仅用于冷启动加载与异步落盘。

**内存态(`AppState`,`Arc` 共享)**

```rust
pub struct AppState {
    // key 明文哈希 → Key 信息(校验入口,O(1),不查库)
    keys: DashMap<String, KeyEntry>,        // key_hash -> {user_id, interface_kind, revoked}
    // 用户运行态
    users: DashMap<Uuid, Arc<UserState>>,
    // 配置:路由/供应商/全局默认限额/模型倍率,热替换
    config: ArcSwap<RuntimeConfig>,
    // 异步落盘通道
    usage_tx: mpsc::Sender<UsageEvent>,
}

pub struct UserState {
    token_balance: AtomicI64,               // 实时扣减
    in_flight: AtomicU32,                    // 并发计数
    rate: governor::RateLimiter<...>,        // RPM/TPM
    status: AtomicU8,                        // active/disabled
    dirty: AtomicBool,                       // 余额是否需回写
    // 限额快照(用户级覆盖)
    concurrency_limit: AtomicU32,
}
```

**生命周期**

1. **启动**:从 SQLite 全量加载 `users`、未吊销的 `api_keys`、配置到内存(几万~几十万行,秒级)。
2. **API Key 校验**:取请求里的 key → 哈希 → `keys.get(hash)` → 拿到 `user_id` → `users.get`。**全程内存,无 DB**。
3. **扣费/限流/并发**:全在 `UserState` 的原子量上操作。
4. **写回(后台任务)**:
   - **余额**:定时(如每 5s)扫描 `dirty=true` 的用户,批量 `UPDATE` 回 SQLite;退出时强制 flush。
   - **用量明细**:请求结束发 `UsageEvent` 到 mpsc channel,后台**批量事务 INSERT**(如每 200 条或每 1s 一批),避开 SQLite 单写瓶颈。
5. **变更(创建用户/建 Key/改限额/改倍率)**:这类操作**低频**,采用「**先写 SQLite,成功后更新内存**」,保证持久与一致;不在热路径。

**SQLite 并发要点**

- WAL 模式:多读不阻塞写,写串行化 → 所以**所有写集中到少数后台任务**,批量提交。
- 连接池:读用多连接,**写用单连接串行**(或一个专职写任务),避免 `SQLITE_BUSY`。
- 热路径几乎不碰库,SQLite 的单写限制就不再是瓶颈。

**取舍**:崩溃会丢失最近一个 flush 周期的余额扣减与未落盘用量。单机产品可接受;若要更强持久,可缩短 flush 周期或对充值/扣大额走同步写。

---

## 9. 对外 API 规格

### 9.1 OpenAI 兼容端点

| 方法 | 路径 | 说明 |
|---|---|---|
| POST | `/v1/chat/completions` | 聊天补全(流式/非流式) |
| POST | `/v1/embeddings` | 向量嵌入(转发) |
| POST | `/v1/completions` | 旧版文本补全(可选) |
| GET  | `/v1/models` | 列出网关暴露的逻辑模型 |

### 9.2 Anthropic 兼容端点

| 方法 | 路径 | 说明 |
|---|---|---|
| POST | `/v1/messages` | Messages API(流式/非流式) |
| POST | `/v1/messages/count_tokens` | token 计数(可选) |
| GET  | `/v1/models` | 与上面共享 |

> 注意:`/v1/models` 在两套规范里响应结构略不同,按请求路径/头部判断返回格式。也可用不同前缀区分,如 `/openai/v1/...` 与 `/anthropic/v1/...`,推荐**同时支持**:既支持标准路径(方便 SDK base_url 直接替换),也支持带前缀路径(消除歧义)。

### 9.3 错误响应

- 透传上游错误时,翻译成客户端协议对应的错误结构(OpenAI 的 `{error:{type,message,code}}` vs Anthropic 的 `{type:"error",error:{type,message}}`)。
- 网关自身错误(限流/配额/路由失败)也按客户端协议格式返回,保证 SDK 能正确解析。

### 9.4 Admin API(独立鉴权,管理员 token)

```
GET    /admin/users                所有用户列表(状态、剩余/已用 token)
POST   /admin/users                创建用户(手机号/初始额度)
GET    /admin/users/{id}           用户详情
PATCH  /admin/users/{id}           设并发/RPM/TPM/模型白名单/状态、增减额度
GET    /admin/users/{id}/usage     该用户 token 消耗明细与趋势
DELETE /admin/users/{id}/keys      重置/吊销该用户全部 Key
POST   /admin/keys                 创建虚拟密钥
GET    /admin/keys                 列表(可按 user 过滤)
DELETE /admin/keys/{id}            吊销
POST   /admin/providers            增删改上游
POST   /admin/routes               配置模型路由
PATCH  /admin/models/{name}/multiplier  设模型扣费倍率(热更新)
GET    /admin/usage                全局用量/成本(按 user/model/provider/时间)
POST   /admin/reload               热加载配置
GET    /healthz /readyz /metrics   健康检查与 Prometheus 指标
```

---

## 10. 可观测性

- **日志**:`tracing` 结构化日志,每请求一个 span,带 `request_id`、`model`、`provider`。**默认不记录 prompt/response 正文**(隐私),可按密钥/团队开启采样记录。
- **指标(Prometheus)**:
  - `requests_total{protocol,model,provider,status}`
  - `request_duration_seconds`(直方图)
  - `ttft_seconds`(首 token 延迟)
  - `tokens_total{type=input|output}`
  - `upstream_errors_total{provider,code}`
  - `ratelimit_rejections_total`
- **追踪**:OpenTelemetry,贯穿入站→路由→上游调用。
- **告警**:错误率、p99 延迟、预算耗尽、上游不可用。

---

## 11. 缓存(可选模块)

- **精确缓存**:对完全相同的(model + messages + 参数)请求,key 为规范化哈希,命中直接返回。需谨慎处理流式回放与 `temperature>0` 场景(默认仅缓存确定性请求)。
- **存储**:进程内 LRU(如 `moka`/`lru`),带 TTL 与容量上限;单机无需 Redis。
- 命中时仍记用量(标记 `cached=true`,成本计 0 或折扣)。
- 语义缓存(向量相似)列为后续增强,默认不开。

---

## 12. 安全

- 虚拟密钥与真实上游密钥隔离;上游密钥由 secret 管理(env / Vault / KMS),不入库明文。
- 全链路 TLS;Admin API 独立强鉴权 + IP allowlist。
- 输入大小限制、超时、最大并发,防资源耗尽。
- 可选:内容审核钩子(请求/响应过审核接口)、PII 脱敏。
- 审计日志:谁在何时改了路由/密钥/供应商。
- 依赖审计:`cargo audit` 接入 CI。

---

## 13. 部署与运维

- **形态**:**单进程单二进制**(static musl)+ 一个 SQLite 文件;Docker 镜像或裸机直接跑,**零外部中间件**。
- **依赖**:仅 SQLite(随二进制,无需独立部署);无 Redis、无 PostgreSQL。
- **状态**:有状态(内存态 + SQLite 文件),**单实例运行**;不做水平扩展(单机定位)。需要更大规模时再演进为「内存态外置 + 共享 DB」。
- **配置**:环境变量 + 配置文件(`figment` 合并),敏感项(上游 key、微信/支付宝 secret)走 env/secret 文件。
- **持久化备份**:定期备份 SQLite 文件(WAL checkpoint 后复制,或 `VACUUM INTO`)。
- **优雅退出**:收到 SIGTERM 时 flush 内存余额与用量队列再退出,避免丢数据。
- **进程守护**:systemd / supervisor 拉起;崩溃自动重启后从 SQLite 重建内存态。

---

## 14. 性能目标(初版)

| 指标 | 目标 |
|---|---|
| 网关转发额外延迟(p99,不含上游) | < 5 ms |
| 单实例并发流式连接 | > 10k(取决于上游与内存) |
| 吞吐 | 主要受上游限制;网关不应是瓶颈 |
| 内存 | 空闲 < 100MB,随连接数线性增长 |

---

## 15. 用户体系与 C 端登录(手机号 / 微信 / 支付宝)

### 15.1 登录后的整体流程

```
用户在门户选择登录方式
   ├─ 手机号:输入手机号 → 收短信验证码 → 校验 → 找到/创建 user
   ├─ 微信:扫码授权 → code → 换 access_token + openid/unionid → 找到/创建 user
   └─ 支付宝:授权 → auth_code → 换 access_token + user_id → 找到/创建 user
        │
        ▼
   颁发会话 JWT(门户面凭证,含 user_id)
        │
        ▼
   首次登录:创建 user,赠送 1000 万 token 余额(不自动建 Key)
        │
        ▼
   门户首页:顶部「剩余额度卡片」+ 下方两张接口卡片(OpenAI / Anthropic)
   每张卡片有「创建」按钮,Key 由用户手动创建
```

### 15.2 账号合一(Identity Merge)

同一个人可能既用手机号又用微信登录,需要避免产生重复账号:

- **微信**:以 `unionid`(同主体下跨公众号/小程序/网站应用唯一)为主键关联;无 unionid 时退化用 `openid`。
- **支付宝**:以 `alipay user_id` 关联。
- **绑定策略**:登录成功后,若当前会话已是某 user,则把第三方身份**绑定**到该 user;若该第三方身份已存在则直接登录对应 user。
- 允许用户在门户「账号设置」里绑定/解绑手机号、微信、支付宝,实现多方式登录同一账号。
- 手机号是软主键:若第三方资料里能拿到手机号(支付宝授权可申请),可提示用户合并。

### 15.3 手机号登录(短信验证码)

- **发码端点** `POST /portal/auth/sms/send`:参数 `phone`。
  - 风控:同手机号 60s 内限发 1 次、单日上限;同 IP 限频;图形/滑块验证码前置(防刷)。
  - 生成 6 位数字码,存内存 Map(`DashMap<phone, {hash, expire}>`,TTL 5 分钟);只存哈希。
  - 调短信服务商(阿里云 SMS / 腾讯云 SMS)发送。
- **校验登录** `POST /portal/auth/sms/verify`:参数 `phone`、`code`。
  - 校验码、次数限制(≤5 次,超限失效);成功后:`users` 中按 phone upsert;颁发 JWT;首登触发建 Key。

### 15.4 微信登录(H5 / 公众号网页授权)

**采用方案:H5(公众号网页授权,`snsapi_userinfo`)** —— 在微信内置浏览器中打开门户 H5 完成授权。

前置条件:已认证的**微信公众号(服务号)**,配置网页授权域名(`redirect_uri` 的域名需在公众号后台白名单)。

服务端流程:
1. 前端引导跳转授权页:
   `https://open.weixin.qq.com/connect/oauth2/authorize?appid=APPID&redirect_uri=ENC&response_type=code&scope=snsapi_userinfo&state=STATE#wechat_redirect`
2. 微信回调带 `code` 到后端 `GET /portal/auth/wechat/callback`。
3. 后端用 `code` + `appid` + `secret` 换网页授权 `access_token` + `openid`。
4. 用 `access_token` + `openid` 拉用户资料(昵称/头像),并取 `unionid`(公众号需绑定开放平台才有 unionid;有则用 unionid 做账号合一,无则退化用 openid)。
5. 按 `unionid`/`openid` 在 `user_identities` 找/建用户 → 颁发 JWT,前端跳回门户。

> 仅做 H5,不做 PC 扫码 / App。`appid`/`secret` 走 secret 管理,不入库明文;回调 `state` 用一次性随机值并校验防 CSRF。
> 注意非微信浏览器(如 PC 端)打开门户时,微信登录入口应隐藏或提示「请在微信中打开」,手机号/支付宝登录仍可用。

### 15.5 支付宝登录(OAuth2)

1. 前端跳转支付宝授权页(`alipay.user.info.share` 或第三方登录 scope `auth_user`)。
2. 回调带 `auth_code` → 后端 `GET /portal/auth/alipay/callback`。
3. 用 `auth_code` 调 `alipay.system.oauth.token` 换 `access_token` + `user_id`。
4. (可选)`alipay.user.info.share` 拉资料。
5. 按 `user_id` 在 `user_identities` 找/建用户 → 颁发 JWT。

> 需要支付宝开放平台应用 + RSA2 密钥对(应用私钥签名、支付宝公钥验签)。SDK 侧用社区 crate 或自实现签名。

### 15.6 会话与安全

- **JWT**:短期 access token(如 2h)+ 刷新 token(如 30d,可旋转);需即时吊销时用内存会话黑名单/白名单(`DashMap`,随进程,单机够用)。
- Cookie(HttpOnly + Secure + SameSite)或 Authorization 头,二选一并防 CSRF。
- 风控:登录异常告警、设备/IP 记录、验证码防刷、第三方回调 `state` 校验。
- 门户面 JWT **不能**直接调 `/v1/*`(数据面),反之亦然。

### 15.7 API Key:手动创建、明文只显示一次、刷新即轮换

**额度与 Key 解耦**:token 余额是用户全局的(注册即送 1000 万);Key 只是调用凭证,本身不带额度,所有 Key 共用同一余额。

门户布局:
- 顶部一张**额度卡片**:显示剩余 token 总额(如 `剩余 98,234,100 / 100,000,000`)。
- 下方两张**接口卡片**:`OpenAI` 与 `Anthropic`,各自独立一把 Key(`interface_kind` 区分),各带 base_url 和调用示例。

**Key 生命周期(每张接口卡片各自独立)**:

1. **创建**:卡片初始为空,显示「创建 Key」按钮。点击后:
   - 后端生成 `rk_live_` + 高熵随机串(如 32 字节 base62)。
   - **仅存哈希**(`key_hash`)+ 前缀(`key_prefix`,如 `rk_live_a1b2…`)入库;明文**不落库**。
   - 接口**仅在本次响应中返回一次明文**,前端**弹窗**展示,提示用户「请立即复制,关闭后无法再次查看」。
2. **展示**:之后页面只显示**掩码密文**(如 `rk_live_a1b2****************wxyz`),无法再看到完整明文。
3. **刷新(轮换)**:卡片上有「刷新」按钮。点击后:
   - 生成**一把新 Key**,旧 Key **立即失效**(撤销),新 Key 哈希入库。
   - 与创建一致:**弹窗显示新明文一次**。
   - 风险提示:刷新会使正在使用旧 Key 的应用立刻失效,需二次确认。

**接口设计**:
- `POST /portal/keys`(body: `interface_kind`)→ 创建,响应含明文(唯一一次)。
- `POST /portal/keys/{id}/rotate` 或 `POST /portal/keys/rotate`(body: `interface_kind`)→ 轮换,响应含新明文。
- `GET /portal/keys` → 列表,仅返回掩码 `key_prefix` + 状态,**绝不返回明文**。

**约束**:`UNIQUE(user_id, interface_kind) WHERE revoked = false` —— 每用户每接口同时只有一把有效 Key。
余额耗尽时数据面调用返回 402/429(按客户端协议格式)。

### 15.8 门户面 API(Portal Plane)

```
POST /portal/auth/sms/send             发送短信验证码
POST /portal/auth/sms/verify           验证码登录
GET  /portal/auth/{wechat|alipay}/url  获取第三方授权跳转地址
GET  /portal/auth/{wechat|alipay}/callback  第三方回调
POST /portal/auth/refresh              刷新会话
POST /portal/auth/logout               登出
GET  /portal/me                        当前用户资料
POST /portal/identities/bind           绑定第三方/手机号
GET  /portal/keys                      我的 Key 列表(仅掩码,不含明文)
POST /portal/keys                      创建 Key(body: interface_kind;响应含明文一次)
POST /portal/keys/rotate               刷新/轮换 Key(body: interface_kind;响应含新明文一次)
DELETE /portal/keys/{id}               吊销 Key
GET  /portal/balance                   我的剩余 token 额度(顶部卡片数据)
GET  /portal/usage                     我的用量趋势/明细
POST /portal/recharge                  充值 token(对接支付,后续)
```

---

## 16. 前端架构(React)

### 16.1 两个独立应用

| 应用 | 受众 | 路径/域名(示例) | 重点 |
|---|---|---|---|
| **Client Portal** | C 端开发者 | `portal.example.com` | 登录、API Key 管理、用量/账单、文档、充值 |
| **Admin Console** | 运营/管理员 | `admin.example.com` | 供应商、路由、用户、密钥、价格、报表、配置 |

两者共享 UI 基础库,但独立构建、独立部署、独立鉴权域。

### 16.2 技术选型

| 关注点 | 选型 |
|---|---|
| 框架 | React 18 + TypeScript |
| 构建 | Vite |
| 路由 | React Router |
| 数据请求/缓存 | TanStack Query(React Query) |
| 全局状态 | Zustand(轻量)|
| UI 组件库 | Ant Design(中后台场景成熟,中文友好) |
| 表单/校验 | React Hook Form + Zod |
| 图表 | ECharts / Recharts(用量趋势、成本) |
| HTTP | axios / fetch 封装,统一注入 JWT、错误处理 |
| 国际化 | i18next(中/英) |

### 16.3 Client Portal 页面

- 登录页:手机号(发码/验证)、微信扫码、支付宝按钮三种入口。
- **仪表盘(首页核心布局)**:
  - **顶部额度卡片**:剩余 token / 总授予 token(进度条),今日消耗。
  - **下方两张接口卡片**:`OpenAI` 与 `Anthropic`,每张含:
    - base_url、Key(掩码显示)、状态。
    - 「创建」按钮(无 Key 时)/「刷新」按钮(有 Key 时,二次确认)。
    - 创建/刷新成功 → **明文弹窗**(一次性,带「复制」按钮 + 不可再看提示)。
    - 调用示例代码片段(对应 SDK 的 base_url 替换写法)。
- 用量明细:按模型/时间维度图表与列表。
- 充值:token 套餐充值(对接支付,后续)。
- 账号设置:绑定/解绑手机号、微信、支付宝。
- 文档/快速开始。

### 16.4 Admin Console 页面

- 概览:全局 QPS、错误率、各供应商健康、成本。
- 供应商管理:增删改、密钥引用、健康探测。
- 路由管理:模型名 → 目标(权重/failover/灰度)可视化编辑。
- 用户管理(核心):
  - 查看**所有用户**列表:注册方式、注册时间、状态、剩余/已用 token。
  - 查看单用户**token 消耗明细**(按时间/模型/接口)与趋势。
  - **手动创建用户**(预置手机号/初始额度),用于内部/对公开通。
  - **设置用户并发上限**、RPM/TPM、模型白名单。
  - 调整额度(增/减赠送 token)、禁用/封禁、重置/吊销其 Key。
- 密钥管理:全局视角、查询某 Key 归属用户、批量吊销。
- 价格/倍率管理:模型 `token_multiplier`(扣费倍率)、内部成本单价维护,热更新。
- 用量报表:按用户/模型/供应商聚合。
- 系统配置:限流默认值、缓存开关、热加载。

#### 16.4.1 管理端鉴权与权限(RBAC)

- 管理员登录:用户名 + `argon2` 密码 → 颁发**管理员 JWT**(管理面,独立于门户/数据面)。
- 角色:
  - `superadmin`:全部权限(含供应商上游密钥、删用户)。
  - `operator`:日常运营(改限额、调额度、看用量),**不能**查看/修改供应商上游密钥。
  - `viewer`:只读。
- 敏感操作(吊销 Key、删用户、改供应商、改倍率)需二次确认并写**审计日志**(谁、何时、改了什么)。
- 建议加 IP allowlist;管理端不暴露公网或仅内网可达。

#### 16.4.2 管理端改动如何生效(单机内存架构关键)

管理操作都是**低频写**,统一走「**先写 SQLite,成功后更新内存态**」,改完即时生效、无需重启:

| 管理操作 | 持久化 | 内存生效方式 |
|---|---|---|
| 改路由 / 供应商 / 模型倍率 / 全局默认 | 写 SQLite | 原子替换 `ArcSwap<RuntimeConfig>`,下一个请求即用新配置 |
| 改用户限额 / 额度 / 状态 | 写 SQLite | 更新该用户 `UserState` 的原子量(`concurrency_limit`/`token_balance`/`status`) |
| 创建用户 | 写 SQLite | 插入 `DashMap<user_id, UserState>` |
| 创建 / 吊销 Key | 写 SQLite | 增删 `DashMap<key_hash, KeyEntry>` |

> 因此管理端**完全不碰请求热路径**,与数据面的高并发互不影响。

### 16.5 前后端协作约定

- 鉴权:Portal 用门户 JWT;Admin 用管理员 JWT(RBAC),互不通用。
- 第三方登录回调:后端处理 `code/auth_code` 换取身份后,通过前端回调页带短期票据交换 JWT,避免 token 出现在 URL。
- API 契约:后端用 `utoipa` 等生成 OpenAPI,前端据此生成 TS 类型(`openapi-typescript`),保证类型一致。
- 跨域:同源部署或配置 CORS;敏感操作二次确认。

---

## 17. 里程碑(Roadmap)

**M1 — 最小可用(MVP)**
- axum 服务骨架、配置加载
- OpenAI 入站 + OpenAI/兼容上游(含 DeepSeek 等)直通,流式打通
- 虚拟密钥鉴权(配置文件级)、基础日志

**M2 — 双协议 + 互转**
- Anthropic 入站、Anthropic 上游
- OpenAI ⇄ Anthropic 双向协议翻译(含工具调用、多模态、流式状态机)
- 路由引擎(权重 + failover)

**M3 — 治理 + 用户体系后端**
- SQLite 接入 + 内存态(DashMap/Atomic)+ 异步落盘任务:用户/身份/密钥/用量/价格/路由
- 内存限流(governor)、并发计数、token 余额扣减
- C 端登录后端:手机号短信、微信、支付宝 OAuth;会话 JWT;登录自动发 Key
- Portal API + Admin API + 热加载

**M4 — 前端(React)**
- Client Portal:三种登录、仪表盘、API Key 管理、用量/账单、账号设置
- Admin Console:供应商/路由/用户/密钥/价格/报表
- OpenAPI → TS 类型契约

**M5 — 可观测与生产化**
- Prometheus 指标、OTel 追踪
- 缓存模块、内容审核钩子
- 更多上游:Gemini、Azure、Bedrock、本地 vLLM
- 充值/支付对接
- 压测与调优、Helm chart

---

## 18. 决策记录与待确认

### 18.1 已确认(本轮拍板)

- ✅ **计费单位**:按 token,非美元。门户顶部显示**全局剩余 token 余额**(所有接口/Key 共用)。
- ✅ **新用户赠送**:1000 万 token(10,000,000)。
- ✅ **Key 模型**:每用户每接口(OpenAI / Anthropic)各一把 Key,共用同一 token 余额。
- ✅ **Key 创建**:不自动建,用户手动点「创建」;**明文仅创建时弹窗显示一次**,之后只显示掩码。
- ✅ **Key 刷新**:「刷新」按钮轮换 Key,旧 Key 立即失效,新明文同样弹窗显示一次。
- ✅ **账号体系**:纯 C 端个人账号;管理后台可查看所有用户。
- ✅ **模型扣费倍率**:每模型 `token_multiplier`,**在管理后台配置**、可热更新。
- ✅ **微信登录**:仅 **H5(公众号网页授权 `snsapi_userinfo`)**,不做 PC 扫码/App。
- ✅ **并发限制维度**:**按用户**(非按 Key);管理后台可设。
- ✅ **部署形态**:**单机单进程,无 Redis**;数据库用 **SQLite(WAL)**;热路径全走内存,DB 仅冷启动加载 + 异步落盘。

### 18.2 仍待确认

1. **首批必须支持的上游供应商**有哪些?(决定 M1/M2 连接器优先级)
2. **资质准备**:微信服务号(且绑定开放平台以拿 unionid)、支付宝开放平台应用、短信服务商账号——是否已就绪?
3. **充值 / 付费模式**:除赠送额度外,后续充值是「买 token 套餐」还是「充值金额按用量折算 token」?
4. **额度耗尽行为**:直接拒绝,还是允许少量透支 / 自动提醒充值?
5. **部署目标**:K8s / 裸机 / Serverless?是否区分国内合规区域?
6. **是否记录 prompt 正文**(调试/审计)及其合规边界?
7. **embeddings / 图像 / 语音**等非 chat 端点是否在 v1 范围内?
```
