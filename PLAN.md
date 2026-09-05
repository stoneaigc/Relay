# Relay 改进计划（PLAN）

> 项目名：relay（内外统一，不再叫 runapi）。
> 本计划基于 2026-09 主流 AI 网关调研（LiteLLM / Bifrost / Kong / Envoy AI Gateway / Cloudflare / One API / Higress）。
> 后续开发直接按本计划执行，每完成一项勾一项。

***

## 0. 工作方式总纲：UI 优先（铁律）

每个功能的设计顺序固定为：**功能 → 样式 → 交互 → 主题一致性**，缺一不算完成。

1. **功能先行**：数据模型、API、状态机定义清楚，功能正确性是底线
2. **UI 即产品**：开工前先写界面形态设计（放哪个页面、用什么组件、有哪些状态、文案是什么），从「用户看什么、点什么」倒推实现
3. **样式达标**：只用主题 token，对齐现有卡片/表格/徽标的视觉语言，用色克制
4. **可交互性**：四态齐全（loading / empty / error / success），操作有反馈，危险操作有确认，专业信息有 tooltip/popover
5. **主题一致**：复用现有 shadcn 风格组件 + 语义色 token；需要新颜色时先加 token 再使用

### 每个功能的标准执行流（Definition of Done）

1. 确认本计划中该功能的 UI 设计小节
2. 后端：API + 单元/集成测试
3. 前端：组件实现（四态完整）
4. 验证：`cargo test` + `npx tsc --noEmit` + `npm run build` + 内置浏览器逐页走查
5. 过一遍 §6 视觉走查 Checklist

***

## 1. 主题与设计系统现状盘点

**技术栈**：React + Vite + Tailwind v4（`@theme` 语法）+ shadcn 风格组件，单浅色主题。

**语义色 token（已有 14 个）**：background / foreground / card / popover / primary / secondary / muted / muted-foreground / accent / destructive / success / border / input / ring（见 `frontend/admin/src/index.css`）。

**组件库存量（components/ui/）**：badge、button、card、dialog、dropdown-menu、input、password-input、row-actions、table、tooltip。

**缺口（按功能需要补建，不预建）**：toast（建议 sonner）、skeleton、switch、select、slider、progress、popover、tabs。

**已发现问题**：

- 健康徽标（HealthBadge）使用了 Tailwind 默认色 `amber-500`，绕过 token 体系 → 须统一为 `--color-warning` token

- 无暗色模式（暂不做，远期项）

***

## 2. 设计规范（Design Tokens & Patterns）

- **色彩语义**：success=健康/成功；warning=降级/告警（**新增 token**）；destructive=异常/熔断/危险操作；muted=次要信息/idle 状态

- **状态徽标语言**（已建立，所有新功能延续）：圆点 + 文案 pill（健康/有失败/异常/熔断中/无请求）

- **表格规范**：行 hover 提亮；操作列右对齐用 row-actions；危险操作红色文字

- **弹层规范**：小确认 ConfirmDialog(max-w-sm)；表单 Dialog(max-w-lg)；详情 Dialog(max-w-2xl)

- **动效**：transition 150-200ms；`animate-pulse` 只用于「熔断中」这类需要立即关注的活状态

- **文案**：中文为主；术语统一（供应商/模型组/密钥/额度）；空态必须给行动指引（如「还没有供应商，去添加 →」）

- **数字**：一律 `.mono` 等宽字体；时间格式统一

***

## 3. 阶段一 v0.2「无感容错」：Fallback 链 + 健康驱动路由

### 3.1 功能设计

- 请求主循环：首选候选失败 → 按失败分类决定换下一候选或直接报错（重试上限 2 次）

  - 可切换：连接错误 / 超时 / 429 / 5xx / 401 / 403

  - 直接报错：400 / 404（请求本身有问题）；SSE 首字节已发出（不可重放）

- 熔断半开探测：恢复后先放 1 个探测请求，成功才全量放行

- 健康降权：Weighted 组中 broken/down 的供应商权重临时置 0，恢复自动回位

- 响应头 `X-Relay-Upstream: <实际供应商>`

- 全局配置：`fallback_enabled`（默认开）+ `max_retries`

### 3.2 UI 设计

| 位置           | 设计                                                                                |
| ------------ | --------------------------------------------------------------------------------- |
| 路由组卡片        | 组详情显示候选链：供应商名 + HealthBadge + 权重/优先级；组级 fallback 开关（Switch）                       |
| 供应商编辑 Dialog | 增加「参与降级顺序」说明文案                                                                    |
| 请求日志         | 新增「命中供应商」列（小 pill）；行展开显示 fallback 时间线：尝试1 → 失败原因 → 尝试2 → 成功（竖向步骤条）                |
| 健康徽标         | 点击弹出 Popover：窗口内 requests / success\_rate / avg / P95 / P99 / 熔断计数（替代现有 title 提示） |
| 仪表盘          | 新增「今日降级次数」统计卡                                                                     |
| 全局设置         | fallback\_enabled 开关 + max\_retries 输入项                                           |

***

## 4. 阶段二 v0.3「降本增效」：语义缓存 + 成本感知 + 周期预算

### 4.1 功能设计

- **语义缓存双层**：L1 精确哈希（DashMap + TTL，零依赖）；L2 embedding 相似度（供应商可配，SQLite 存向量 + 暴力余弦；未配置时自动只跑 L1）

  - 相似度阈值默认 0.8；多轮对话 >3 条消息跳过缓存；缓存按 model/provider 隔离；流式命中回放 SSE

- **模型价格表**：input/output 每 1M tokens 单价，按供应商+模型配置，带主流默认值

- **CostAware 路由策略**：过滤不健康候选 → 选最便宜 → 同价随机

- **周期预算**：日/月窗口，耗尽返回 429 `insufficient_quota`（对齐 OpenAI 语义）

### 4.2 UI 设计

| 位置      | 设计                                                |
| ------- | ------------------------------------------------- |
| 侧边导航    | 新增「缓存」页                                           |
| 缓存页顶部   | 三张统计卡：命中率（大数字+趋势）、节省 tokens、缓存条数                  |
| L2 引导态  | 未配置 embedding 时展示引导卡：「配置 embedding 供应商以启用语义缓存 →」  |
| 最近命中表格  | 时间 / 命中类型 pill（精确/语义）/ 相似度 / 模型 / 节省 tokens       |
| 设置页     | 缓存配置卡：开关 Switch、阈值 Slider（0.5-0.95 实时数值）、TTL、多轮阈值 |
| 价格表     | 模型页新增价格管理 Dialog：表格行内编辑，输入框 $ 前缀                  |
| 用量页     | 维度切换 Tabs（tokens / 费用）；按供应商/用户/密钥横向条形图            |
| 用户/密钥列表 | 行内预算 Progress 条（80% 转黄、95% 转红）；耗尽显示「已耗尽」pill      |

***

## 5. 阶段三 v0.4「透明与门面」

> 已拍板：基准先做 relay 基线（外部对比留可选）；`GET /metrics` 默认需鉴权；API 文档页 = 管理端 + 门户双份。

### 5.1 功能设计

- **LatencyAware 策略**：`Strategy::LatencyAware`；P50 快照存 `DashMap<provider名, p50_ms>`，由 main.rs 后台任务每 15s 从 MetricsStore 刷新（热路径零 await，注入 resolve，与 rr_counter 同风格）；排序按 P50 升序、无数据殿后且随机、同值随机。

- **Prometheus `GET /metrics`**：手写 exposition 文本格式（零新依赖）；复用 `provider_health()`（requests/success/fail_unavailable/fail_other/P95/P99/breaker）+ tokens/缓存命中/用户数计数器；指标族：`relay_upstream_requests_total{provider,status}`、`relay_upstream_request_duration_ms{quantile}`、`relay_upstream_success_ratio`、`relay_upstream_breaker_state`、`relay_tokens_total{direction}`、`relay_cache_hits_total{type=exact|semantic}`、`relay_users_total`。鉴权：接受 admin JWT 或新增 `[metrics] export_token` 静态 token（默认空 → 仅 admin JWT），401 文案指明配置方式。无 UI。

- **模型组导入导出**：导出 = 单组 JSON 下载（组配置 + 路由 + 时段规则 + 策略）+ 全量导出；导入 = `POST /admin/api/groups/import` upsert（按组名匹配：存在→整组覆盖，不存在→新建）。

- **路由批量编辑**：当前页内多选（checkbox 列 + 表头全选）。

- **性能基准（relay 基线）**：`bench/` 脚本 + mock 上游固定响应 + oha 压测 → RPS/P99/内存进 README。

### 5.2 UI 设计

| 位置 | 设计 |
| --- | --- |
| 策略下拉 | 加「延迟优先」选项 + hint：「延迟优先模式:按窗口内 P50 延迟从低到高选择;无数据候选殿后随机分摊。」（对齐成本优先文案风格） |
| 模型组页顶部 | 「导入」「导出全部」按钮，与搜索/新建并列 |
| 导入 Dialog(max-w-2xl) | 两步：① 来源（粘贴 JSON / 选 .json 文件）→ 解析反馈「识别到 N 组 · M 路由 · K 时段规则」；② diff 只读预览表：组名 / 动作 pill（新建=success、覆盖=warning + 「将替换现有 N 条路由」）/ 路由数 / 策略 → 确认导入（pending 态，成功刷新列表）。四态：JSON 解析错误给具体行提示、空文件给指引 |
| 组详情卡 | 卡头小按钮「导出」（单组 JSON 下载） |
| 路由表 | 首列 checkbox + 全选；选中>0 浮出批量操作栏（「已选 N」badge + dropdown：批量删除〔ConfirmDialog destructive〕/ 批量改倍率 / 批量改权重） |
| API 文档页（管理端 + 门户精简版） | 分区：快速开始 / 鉴权 / OpenAI·Anthropic 协议 / 流式 / 错误码 / `/metrics` 接入（含 Prometheus bearer_token 配置示例）；代码块 mono + 一键复制 + curl/Python/Node 语言 Tabs；门户版去掉管理向内容 |
| README | 勾掉「Prometheus 指标导出」待办；补阶段三特性；基准数字表格 |

### 5.3 执行批次

- **批F 透明**：F1 后端（LatencyAware + /metrics 端点 + 测试）→ F2 前端（策略下拉/标签/hint）→ F3 验证+提交
- **批G 门面一**：G1 后端（export/import 端点 + 测试）→ G2 前端（导入导出向导 + 批量编辑）→ G3 验证+提交
- **批H 门面二**：H1 管理端 API 文档页 → H2 门户精简版 + 文档统一（runapi 残留、README 特性）→ H3 验证+提交
- **批I 基准**：bench 脚本 + relay 基线 + README 数字（外部网关对比为可选项，默认不做）

***

## 6. 视觉走查 Checklist（每个功能交付前过一遍）

- [ ] 四态：loading（骨架/转圈）、空（指引文案+行动按钮）、错误（可重试提示）、成功

- [ ] 所有点击 <100ms 有视觉响应；异步提交按钮有 pending 态（disabled + spinner）

- [ ] 危险操作（删除/清空）必经 ConfirmDialog，确认按钮用 destructive

- [ ] tooltip/popover 覆盖所有专业术语与统计口径

- [ ] 颜色只用 token；新色先进 `@theme` 再用

- [ ] 数字用 `.mono`；时间格式统一

- [ ] 1440 / 1024 / 768 三档宽度不破版

- [ ] 键盘可用：Dialog Esc 关闭、表单 Enter 提交

***

## 7. 执行顺序

1. **前置小任务**：

   - 提交当前积压改动（供应商健康展示 + 批量删除模型 + flaky 测试修复，7 文件）

   - HealthBadge 的 `amber-500` → `--color-warning` token

   - 补建 toast 基础设施（sonner）
2. **阶段一**：Fallback 链 + 健康驱动路由（后端 → UI → 走查）
3. **阶段二**：语义缓存（先 L1 → UI → 再 L2）+ 周期预算 + 成本感知
4. **阶段三**：按 §5 顺序

## 8. 待拍板决策点

1. 语义缓存 L2 的 embedding 供应商：OpenAI 兼容接口通吃（推荐）还是先只做 L1？
2. 周期预算 vs 成本感知，谁先进阶段二？（倾向：预算先行，管控刚需）
3. `fallback_enabled` 默认值：开（推荐，配合可关）？

