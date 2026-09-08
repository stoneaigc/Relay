# 真实上游全链路测试记录（REAL_UPSTREAM_TEST）

> 测试日期：2026-09-06　|　测试方式：真实环境（网关 8080 + 真实上游 ai.fit2cloud.cn）
> 目的：验证「上游对接 → 路由 → 用户调用 → 日志/审计/门户回流」完整链路，并记录设计不合理点供复盘讨论。

---

## 一、测试环境

| 项 | 值 |
|---|---|
| 网关（数据面 + 管理面 + 门户面） | http://127.0.0.1:8080 |
| mock 上游 | 127.0.0.1:18901（对照组） |
| 真实上游 | https://ai.fit2cloud.cn/gateway/v1 |
| 管理员 | admin / Relay@123 |
| 门户测试账号 | demo / Demo@123（user_id 034817f0-…，Key rk_live_73ed…55f） |
| 路由组 | Production（group_id=14） |
| 真实上游模型 | 5 个（model_id 102–106，供应商 prov-1e8020c7，路由 ID 74–78，weight 100 / multiplier 1.0） |
| 测试后状态 | 已恢复：坏路由 79 / 坏模型 107 已删（供应商级联删除），multiplier 恢复 1.0，当前 10 供应商 / 16 路由 |

---

## 二、测试矩阵（t1–t6）

### t1 网关边界与鉴权 ✅
| 操作 | 预期 | 实际 | 结论 |
|---|---|---|---|
| 无 Key 调 /v1/chat/completions | 401 | 401 | ✅ |
| 伪造 Key `sk-invalid` | 401 | 401 | ✅ |
| 有效 demo Key 调 /v1/models | 仅暴露组内模型 | 9 个模型（4 个组名 + 5 个 f2c 真实模型） | ✅ 模型可见性按组隔离正确 |

### t2 真实模型调用 + 流式 ✅
| 操作 | 实际 | 结论 |
|---|---|---|
| f2c-auto 非流式（“你好”） | 200，vLLM 指纹（model f2c-deepseek-v4-flash），23 tokens，计费入账 | ✅ 真实链路打通 |
| qwen3.8-27b | 502（10.66s 后） | ⚠️ 非 Relay 缺陷：上游自身把该模型转发到内网 `sz.fit2cloud.cn:908`（dial i/o timeout）。网关行为正确：熔断计数 1/3、供应商转 degraded、charged=0 |
| 流式 SSE | 6 个 chunk，**最后一个 chunk 带 usage 尾片**（prompt 10 + completion 3 = 13），计费 13 tokens | ✅ 真实上游 usage 与 mock 不同，网关透传正确 |

### t3 错误路径 ✅（含 1 个设计问题）
| 操作 | 实际 | 结论 |
|---|---|---|
| 不存在模型 `f2c-not-exist` | 404 | ✅ |
| 坏 JSON body | 400 | ✅ |
| vision 模型（多模态输入） | 200，104 tokens，含 reasoning_content | ✅ |
| **空 messages `[]`** | **200，上游随机续写** | ❌ 设计问题②：无入站校验 |

### t4 路由与倍率计费 ✅（含 1 个设计问题）
| 操作 | 实际 | 结论 |
|---|---|---|
| PATCH /admin/api/routes/74 设 multiplier=3.0 | 首次失败：`missing field public_name`；补全全量字段后成功 | ❌ 设计问题③：PATCH 为全量替换语义 |
| multiplier=3.0 后调用 f2c-auto | charged = **69 = (21+2)×3** | ✅ 倍率计费公式正确 |
| 恢复 multiplier=1.0 | 调用恢复 charged=23 | ✅ 已恢复 |

### t5 故障注入：坏上游 failover ✅（挖出重大发现：响应缓存）
操作：注入坏供应商 `127.0.0.1:9`（connection refused，model 107）+ 坏路由（id 79，public_name=f2c-auto，weight 500 抢占主路由），连发 6 次相同请求。

| 观察点 | 实际 | 结论 |
|---|---|---|
| 调用方结果 | **6/6 全 200**，无失败 | ✅ failover 对调用方无感 |
| 坏路由是否被尝试 | 是（weight 500 优先），失败后回落好路由 | ✅ 加权故障转移生效 |
| 日志 final_kind | 6 条**全部为 `cache`**，latency≈2s 的记录也是缓存兜底（stale-while-revalidate） | ⚠️ 发现响应缓存机制（此前未文档化） |
| 响应体 | 6 次返回**同一 chatcmpl id** | ⚠️ 缓存命中复用响应 |
| cache/stats | hits=6，tokens_saved=138 | 缓存统计可用 |
| attempts 字段 | **6 条全为 []**（仅最终失败请求才记 attempts） | ❌ 设计问题⑤：failover 过程不留痕 |
| 好供应商真实流量 | requests 5→10、success 3→8（6 连发期间真实转发了 5 次） | ❌ 设计问题④：后台刷新的真实上游调用无计费痕迹 |
| 清理 | DELETE /routes/79 + models/batch-delete → 供应商级联删除 | ✅ 级联删除实证生效 |

### t6 日志 / 审计 / 门户回流 ✅（含 3 个设计问题）
| 观察点 | 实际 | 结论 |
|---|---|---|
| request-logs | 每次调用可查：模型/路由/供应商/latency/charged/final_kind/req_body/resp_body | ✅ 全链路可追溯 |
| audit-logs | total=49，本测试 5 条管理操作全记录（PATCH routes/74、POST models/batch、POST groups/14/routes/batch、DELETE routes/79、POST models/batch-delete），含 actor/ip/latency/status | ✅ 管理操作审计完整 |
| overview | demo 账号 10 calls / 307 tokens，by_model 含 f2c 系列 | ✅ |
| 门户 /me | balance=9,999,693（已扣 307），**used_total=0** | ❌ 设计问题⑦：字段口径矛盾 |
| 门户 usage 明细 | 全部 f2c 调用已计费，**cost_usd 恒 0.0** | ❌ 设计问题⑧：新模型无单价 |
| /portal/api/series?days=7 | 返回整段历史，days 参数未生效 | ❌ 设计问题⑫ |

---

## 三、设计不合理点清单（复盘讨论用）

> 按严重度组织。每条含：现象 → 证据 → 影响 → 复盘讨论方向。

### A. 正确性 / 语义类

**① 上游失败的错误包装语义误导（严重）**
- 现象：上游 connection refused 时，客户端收到 `502 "no healthy upstream target"`；日志 attempts 里记的却是上游 503。
- 证据：t5 坏供应商注入；t2 qwen 502（实际是上游 dial timeout）。
- 影响：三处状态不一致（上游错误 / attempts 503 / 客户端 502），排查时误导；「无候选」与「候选全失败」两种根因不可区分。
- 复盘方向：区分错误码语义（无候选=503？全失败=502？）；是否透传上游错误原文（需权衡信息泄露）；客户端错误信息里附 trace_id。
- ✅ **已拍板（2026-09-06）**：新增 AllFailed(502) 区分「候选全失败」与「无候选」；错误体带末次上游错误**脱敏摘要**（抹 base_url/api_key，仅留状态码+message，内嵌 URL 二次清洗）。并入批 M（M1-3）。

**② 空 messages 无入站校验（严重）**
- 现象：`messages: []` 直接转发上游，返回 200 + 随机续写内容，且**正常计费**。
- 证据：t3。
- 影响：调用方无感拿到无意义响应还被扣费；上游浪费。
- 复盘方向：入站 JSON Schema 校验（messages 非空、role 合法、max_tokens 范围等）；校验失败返回 400 而非透传。

**③ PATCH /routes/:id 全量替换语义（中）**
- 现象：PATCH 缺省 weight → 重置为默认 100；缺省 multiplier → 重置 1.0；必须带全 public_name/model_id 才能成功。
- 证据：t4 两次失败后 grep `update_route` 确认 `weight.unwrap_or(100), multiplier.unwrap_or(1.0)`。
- 影响：管理端误操作会静默重置路由参数；与 REST PATCH 语义相悖。
- 复盘方向：改为真正的部分更新（Option 字段区分「未传」与「传 null」），或前端强制提交全量并明示。

### B. 计费 / 一致性类

**④ 响应缓存：语义与计费口径（需重点讨论）**

> ⚠️ 复盘前代码审查 + 实时复测修正：t5 原始记录有三处误判，已纠正——
> a) 缓存命中/未命中**有**标识头 `x-relay-cache: HIT|MISS`（当时未检查响应头；实测第 1 发 MISS、第 2 发 HIT）；
> b) TTL/开关/多轮上限/相似度阈值**可配置**（TOML `[cache_semantic]` + 管理端设置 DB `cache.*`，DB 优先，管理接口已有）；
> c) **不存在** stale-while-revalidate 后台刷新：代码里过期即 miss、走正常计费转发。t5 观察到的「好供应商 requests 5→10」实为滑动时间窗（range_secs）统计叠加首次真实转发的误读。
> 复测实证：同一请求连发 3 次——第 1 发 MISS（供应商 requests 0→1），第 2/3 发 HIT 且供应商计数不变（requests=1）→ **命中零上游调用、零成本，charged=0 与成本自洽**。

真正成立的问题（代码依据 semantic_cache.rs / handlers.rs）：
- a) **缓存不看随机性参数**：temperature>0 的相同请求照样缓存回放，「重新生成」语义失效（temperature 变化会换键，但同参数重复请求必命中）；
- b) **命中完全免费（emit_success(0,0,0)）**：成本 0 免费 0 虽自洽，但重复流量白嫖是否合理是产品决策；且命中日志 input/output/charged 全 0，门户 usage 无法区分「免费命中」与「异常零计费」；
- c) **流式响应整条缓存原字节回放**：同一 chatcmpl id / created 时间戳多次出现，调用方去重/审计困惑；cache_wrap 把整个流缓冲进内存后才判 4MB 上限，极端响应有内存放大风险；
- d) **L2 语义缓存**：不同 prompt 命中相似问题的答案（正确性风险），且 L1 miss 时旁路调 embedding 上游——该调用无计费、无日志痕迹（这才是真正存在的「无计费上游调用」）；
- e) **粒度**：只有全局开关（默认开启！），无路由/模型级 opt-in；缓存是有语义风险的能力，当前所有用户所有路由被动生效。
- 复盘方向：路由级 opt-in（默认关）+ 仅缓存 temperature=0/带 seed 请求 + 命中计费口径拍板（免费 vs 折扣率，Entry 里已存原始 input/output tokens 可支撑折扣计费）+ 调用方 no-cache 逃生头。
- ✅ **已拍板（2026-09-06）**：路由级 opt-in 默认关 / 仅缓存确定性请求 / 可配折扣率默认免费 / 细节并入 PLAN.md 批 L（L1–L4）。

**⑦ 门户 /me 的 used_total=0 与 balance 扣减矛盾（中）**
- 现象：balance 已扣 307，`used_total` 仍为 0。
- 证据：t6。
- 影响：门户展示自相矛盾，用户不信任。
- 复盘方向：used_total 的统计来源（是否只统计某张表/某种状态）；与 usage 明细、overview 三处口径统一。

**⑧ 新模型默认无单价，cost_usd 恒 0.0（中）**
- 现象：门户 usage 明细所有 f2c 调用 cost_usd=0.0（tokens/charged 正常）。
- 证据：t6。
- 影响：成本/毛利核算缺失；多模型比价不可用。
- 复盘方向：模型/路由级单价配置（进价 + 售价）；无单价时明确显示「未定价」而非 0。
- 定性修正（复盘审查）：定价体系**本已齐全**——models 表有 input_price/output_price、admin API 可设价、pricing.rs 有回退价目表；真因是真实上游导入的模型**未填单价**（纯配置缺失，非代码缺陷）。
- ✅ **已拍板（2026-09-06）**：模型表格「未定价」徽标 + 成本概览提示「N 个模型未设价」+ 导入表单批量填价；计算链不动。并入批 M（M1-4）。

### C. 可观测性 / 安全类

**⑤ 成功请求 attempts=[]（中）**
- 现象：仅最终失败请求记录 attempts；failover 中间尝试、成功请求过程全部不留痕。
- 证据：t5（6 条日志 attempts 全空）。
- 影响：无法回答「这次请求为什么走了这条路由/那个供应商」「failover 花了多少时间」。
- 复盘方向：每次请求记录 attempts 数组（候选顺序、每跳状态码/延迟/是否命中），日志里以 JSON 展示。

**⑥ req_body/resp_body 全量明文记录对话内容（严重，合规）**
- 现象：request-logs 存储完整请求与响应正文（含用户对话明文）。
- 证据：t6 日志详情。
- 影响：合规/隐私风险（对话属敏感数据）；库体积膨胀。
- 复盘方向：默认截断/脱敏 + 可配置保留策略；按租户/Key 级开关；过期自动清理 job。

**⑨ 上游超时 10s 固定且不可配置（低）**
- 现象：qwen 等待 10.66s 才 502；超时硬编码，不可按路由/模型配置。
- 复盘方向：超时进路由/模型级配置；流式与非流式分离（流式首包超时 vs 总超时）。

### D. UI / 前端类

**⑩ 添加路由弹窗按钮非 sticky footer（低）**
- 现象：路由较多时「添加路由」弹窗确认按钮在视口外，需滚动才能点到（此前 r3 UI 走查受阻的根因）。
- 复盘方向：弹窗统一 sticky footer + 内容区滚动。
- ✅ **已拍板（2026-09-06）**：Modal 组件统一 sticky footer（一处 CSS）。并入批 M（M3-3）。

**⑪ 门户 JWT 过期不自动跳登录（低，遗留）**
- 现象：token 过期后接口 401，前端停留在空白/报错，不跳登录页。
- 复盘方向：全局 401 拦截 → 清 token → 跳登录。
- ✅ **已拍板（2026-09-06）**：admin/portal 两处 api.ts 全局 401 拦截 → 清 token → 跳登录（排除登录接口）。并入批 M（M3-4）。

**⑫ /portal/api/series days 参数未生效（低）**
- 现象：`?days=7` 返回整段历史。
- 复盘方向：后端解析 days 参数并限制最大窗口。
- 定性修正（复盘审查）：不是「参数无效」而是**压根没有 days 参数**——portal.rs 与 admin.rs 的 series 都硬编码 30 天，前端也未传参。
- ✅ **已拍板（2026-09-06）**：两端点加 `?days=`（clamp 7~90，默认 30），门户仪表盘 + 管理端用户详情加 7/30/90 切换。并入批 M（M3-2）。

---

## 四、上游侧问题（非 Relay 缺陷）

- qwen3.8-27b：ai.fit2cloud.cn 自身将该模型转发至内网 `sz.fit2cloud.cn:908`（dial i/o timeout），恒定 502。已通过直连复测确认。
- 处置建议：该模型保留但标记 degraded/down，或从路由中摘除，避免调用方踩坑。

## 五、未覆盖项（可作后续测试计划）

1. `/v1/messages`（Anthropic 兼容面）端到端
2. `/v1/embeddings`
3. 并发压测（限流、熔断阈值 3/30s 的实际触发）
4. Key 轮换 `/keys/rotate` 后旧 Key 失效时序
5. rewards 兑换流程
6. 缓存边界：不同 temperature / 流式 / 长上下文的命中规则

## 六、复盘重点议题（✅ 全部拍板完成，2026-09-06）

1. **缓存策略与计费口径**（④）——✅ 已拍板 → 批 L（L1–L4）
2. **错误语义透传**（①）——✅ 已拍板 → 批 M（M1-3）
3. **入站校验**（②）——✅ 已拍板 → 批 M（M1-1）
4. **日志脱敏与保留策略**（⑥）——✅ 已拍板 → 批 M（M3-1）
5. **可观测性增强**（⑤）——✅ 已拍板 → 批 M（M2-2）
6. 其余项（③⑦⑧⑨⑩⑪⑫）——✅ 已拍板 → 批 M（M1-2/M1-4/M2-1/M2-3/M3-2/M3-3/M3-4）

> 复盘结论：12 项全部拍板，整改方案分别落入 [PLAN.md](PLAN.md) 批 L（缓存）与批 M（其余 11 项），待排期实施。
> （2026-09-07 更新：批 L/M 已全部实施并通过真实环境复测，见下「七」。）

---

## 七、真实环境复测记录（2026-09-07，批 L/M 整改验收 + 未覆盖项补测）

> 范围：批 L（缓存 L1–L4）与批 M（11 项整改）共 12 项验收 + 「五、未覆盖项」6 项补测。
> 环境：同「一」（网关 127.0.0.1:8080 + 真实上游 ai.fit2cloud.cn/gateway/v1）。管理面探针 admin/Relay@123；数据面主测试 Key rk_live_248d…e8d（用户 cachetest09072108）；主测路由 74（f2c-auto，weight 100 / multiplier 1.0 / cache_enabled=true）。
> 方法：PowerShell API 探针 + 浏览器 UI 走查；发现缺陷当场修复并回归（cargo test）。
> 结论：**12 项整改全部验收通过；未覆盖项补测 5/6（rewards 兑换未测，留后续）；复测新发现并当场修复 2 个缺陷（FIX-1/FIX-2），cargo test 176 项全绿；注入测试状态已全部清理（见 7.6）。**

### 7.1 复测新发现并修复（2 个）

**FIX-1 Anthropic 面（/v1/messages）漏调入站校验（② M1-1 的遗漏面）**
- 发现：双协议探针——同一非法 payload（空 messages）在 OpenAI 面 400，在 `/v1/messages` 仍透传上游。
- 根因：M1-1 的 `validate_chat_payload` 只挂在 OpenAI 面 `chat_completions`（handlers.rs L408），Anthropic 入口 `messages → run_messages` 未调用。
- 修复：`run_messages` 补 `validate_chat_payload(&req)?`（handlers.rs L1107）。
- 回归：新增 5 个单测（缺 messages / 空数组 / 非数组 / stream 与 max_tokens 类型错 / 合法通过，handlers.rs L2141-2184），cargo test 176 项全绿；探针复测 ANT 面空 messages → 400 ✅。

**FIX-2 /v1/embeddings 上游错误未脱敏（与 ① 同源）**
- 发现：embeddings 探针注入坏上游，503 错误体携带上游原始 URL（`embedding 上游失败: … http://127.0.0.1:9 …`），与 M1-3 AllFailed 的脱敏口径不一致。
- 修复：`embeddings_inner` 错误路径套 `sanitize_upstream_error`（handlers.rs L292-298）。
- 回归：cargo test 176 项全绿；复测 503 错误体 `[url]` 化、无 `sk-` 泄露 ✅。

### 7.2 批 L 复测（④ → L1–L4）

| 整改项 | 复测证据 | 结果 |
|---|---|---|
| L1 路由级 opt-in（默认关） | 主测路由 74 显式 `cache_enabled=true` 后缓存链路按预期生效；该字段随路由 CRUD 正常读写 | ✅ |
| L2 仅缓存确定性请求 | 探针缺省 temperature 的相同请求不命中（MISS，符合 deterministic 设计）；显式 `temperature=0` / 带 `seed` 后 6 组场景全过（非流式命中、流式整条回放、参数变化换键等） | ✅ |
| L3 命中计费折扣率 | 本轮未重复端到端复测（批 L 提交 50e1936 含单测与走查） | 批内覆盖 |
| L4 no-cache 逃生口 | 本轮未重复端到端复测（批 L 提交含 `no_cache_requested` 等单测） | 批内覆盖 |

> 探针教训：语义缓存按「确定性请求」判定——只认 `temperature≤0.001` 或非空 `seed`。**缺省 temperature 不缓存是保守正确设计**（缺省≠明确要求确定性，宁可不命中也不误回放）。测试缓存前探针必须显式带确定性参数。

### 7.3 批 M 复测（11 项）

| 原问题 | 整改（批） | 复测证据 | 结果 |
|---|---|---|---|
| ① 错误包装误导 | M1-3 AllFailed + 脱敏摘要 | 注入坏供应商：客户端 502 AllFailed，错误体上游 URL `[url]` 化、无 `sk-` 泄露；attempts 保留末次上游错误内部明细 | ✅ |
| ② 入站校验 | M1-1 validate_chat_payload | 双协议探针（见 FIX-1）：修复后 OpenAI/ANT 两面空 messages、非数组、类型错全部 400 | ✅（含 FIX-1） |
| ③ PATCH 全量替换 | M2-1 真部分更新 | 单字段 PATCH `{"weight":250}`、`{"multiplier":2.0}` 直接 `{"ok":true}`（老语义报 `missing field public_name`，t4 铁证）；三次 PATCH 后 `cache_enabled` 仍 true、weight/multiplier 恢复 100/1.0——缺省字段零覆盖 | ✅ |
| ⑤ 成功请求 attempts=[] | M2-2 全程留痕 | failover 链完整留痕（候选顺序/每跳状态/熔断 skip），成功与失败请求均有 attempts | ✅ |
| ⑥ 日志明文 | M3-1 预览+保留可配 | 设置页「正文预览上限 8192 / 保留 30 天」→ 改 4096 保存 → `GET /admin/settings/logging` 回读 4096 → 恢复 8192 回读 ✅（settings 表持久化 + 内存 Config 即时重建）；后台清理 job 每 10 分钟按保留期删超期日志 | ✅ |
| ⑦ used_total=0 | M1-2 token_used_total 回填 | 门户 /me used_total 与 usage 明细、overview 口径一致（探针比对） | ✅ |
| ⑧ 未定价显示 | M1-4 未定价可见性 | 全局用量页提示「34 个模型未设价」+「去定价」入口；模型未定价徽标 | ✅ |
| ⑨ 超时固定 | M2-3 超时可配 | `ProxyConfig`：非流式总超时 `upstream_timeout_secs`（默认 300s）、连接超时 `connect_timeout_secs`（默认 10s，含流式），TOML/RELAY_* 可配；流式不设总超时（config.rs L39-54） | ✅（配置证据） |
| ⑩ Modal footer | M3-3 一处 CSS | index.css L48-58 统一 sticky footer；真实「添加路由」弹窗（22 模型 batch 场景，scrollH 791 / clientH 580 可滚动）`getComputedStyle(footer).position === "sticky"`；容器 `max-h-[85dvh] overflow-y-auto` | ✅ |
| ⑪ 401 不跳登录 | M3-4 全局拦截 | admin/portal 双侧 api.ts 401 拦截（排除登录口）→ 清 token → 跳登录；实测注入无效 token reload → 渲染登录页、`relay_admin_token` 已清除 | ✅ |
| ⑫ series days | M3-2 days 参数 | API：portal/admin series `?days=`（clamp 7~90 默认 30）；UI：用户详情趋势 Modal 7/30/90 切换，network 铁证 `GET /admin/api/users/:id/series?days=30`、`?days=90`（90 天视图 x 轴 6/10→9/7，峰值 34/天与当日实测吻合） | ✅ |

### 7.4 未覆盖项补测（对应「五」1–6）

1. `/v1/messages` 端到端 → ✅ ANT 面真实上游调用打通（非流式 + 流式），期间发现并修复 FIX-1。
2. `/v1/embeddings` → ✅ 对外端点真实上游 200；坏上游 503 脱敏缺陷修复（FIX-2）后复测 `[url]` ✅。
3. 并发压测（限流/熔断）→ ✅（功能性）：熔断 3/30s 触发与 skip 留痕、RPM 精确计数 + 429 结构化均已验证；**未做大规模并发压测**。
4. Key 轮换时序 → ✅ `/keys/rotate` 后旧 Key 下一请求即 401、新 Key 即 200（即时生效）。
5. rewards 兑换流程 → ❌ 本轮未测，留后续计划。
6. 缓存边界 → ✅ 温度/seed 边界 + 流式回放 6 组全过（见 7.2 L2）。

### 7.5 设计说明与观察项（不整改，留档）

- **缺省 temperature 不缓存**：deterministic 判定只认 temperature≤0.001 或非空 seed，缺省不缓存是保守正确设计（见 7.2 教训）。
- **RPM 计数含失败请求**：限流按全部请求（含上游失败）精确计数，防失败重试打穿上游；与「成功才计数」的直觉不同，属有意设计。
- **rotate 即时失效**：Key 轮换无宽限窗口，旧 Key 下一请求即 401；需平滑迁移时应先建新 Key、切换后再废旧 Key。
- **双口径脱敏**：对外错误体统一 `sanitize_upstream_error`（抹 base_url/api_key，内嵌 URL 二次清洗）；attempts/审计等内部留痕保留原文供管理员排查——**对内不脱敏、对外必脱敏**，属有意设计。
- **观察项（429 文案）**：429 响应体已结构化，但限流文案中「频次超限」与「配额耗尽」两类语义表述有歧义，易混淆；不改行为，留待文案优化批次。

### 7.6 收尾清理核销

| 注入状态 | 清理方式 | 验证 |
|---|---|---|
| 路由 84/85（坏路由抢占） | DELETE /admin/api/routes/:id | 列表 0 残留 |
| 模型 113/114（坏上游模型） | DELETE /admin/api/models/:id | 列表 0 残留 |
| 供应商 prov-5ae82d19（坏上游） | DELETE /admin/api/providers/:name | 0 引用 |
| 一次性用户 rotatetest0907 | DELETE /admin/api/users/:id | users total 12→11，GET 已删 ID → 400 |
| 路由 74 参数 | PATCH 恢复 weight 100 / multiplier 1.0（cache_enabled 全程未动） | 列表复核 ✅ |

> 复测全程网关行为与批内验收一致，无回归。容器化三件套（Dockerfile / docker-compose.yml / .dockerignore）按拍板留工作区，随交付阶段一并处理。

---

## 八、全流程 UI 走查记录（2026-09-08，对抗式走查）

> 范围：admin 14 个页面 + portal 5 个页面全流程走查（登录/退出/CRUD/表单校验/空态/分页/401/奖励全流程），PowerShell API 探针 + 浏览器实测 + console/network 全程监控。
> 结果：**发现 8 个问题（2 中 2 轻偏安全/一致性 + 4 低观察项），当场修复 6 个并验证，cargo test 178 项全绿（新增 2 单测）+ 前端构建通过**；顺带补测完成「五」未覆盖项 5（奖励申领→审核→入账全流程闭环）。

### 8.1 走查通过项（摘要）

- 双端登录/退出/token 清理、错误行内提示、显示密码切换、注入式用户名（`admin' OR '1'='1`）被参数化查询正确拒绝 ✅
- admin 14 页全部正常渲染，无空壳页；空态文案完整（上游治理「首次调用后才会记录连接」、缓存「暂无命中记录」、奖励审核「暂无申领」、接口指标/用量明细「暂无数据」）✅
- 供应商添加/删除（含级联警告确认弹窗「将删除该上游下的 N 个模型及其所有路由」）、模型定价表单负数/非数字拦截 ✅
- 概览趋势按天/周/月切换（`granularity=day|week|month`，峰值随粒度变化总数恒定自洽）；portal 趋势 7/30/90（`series?days=7|30|90` 网络铁证）✅
- 创建用户弹窗按钮初始 disabled、重复用户名行内报错；奖励评定额度范围校验（1M~10M 之外按钮禁用）✅
- Key 生命周期：每用户每接口类型恒一把有效 Key（重复创建被拒并提示 rotate）、有 Key 后创建按钮隐藏、列表掩码显示 ✅
- portal 401 跳登录实测（无效 token → 登录页 + token 清除），与 admin 侧对称 ✅
- **console 全会话仅 1 个低级错误，零 JS 异常** ✅

### 8.2 走查问题清单与处置

| # | 严重度 | 问题 | 证据 | 处置 |
|---|---|---|---|---|
| W1 | 中(安全) | **创建用户无密码强度校验**：密码 `123` 经 admin 创建成功并可登录 portal | 走查实测创建 + 登录成功 | ✅ 修复：admin 创建/改用户与 portal 注册/重置统一 ≥8 位（admin.rs/portal.rs），探针复测 400 ✅ |
| W2 | 中 | **供应商 base_url 无格式校验**：`not a url!!` 直接保存成功污染配置 | 添加上游探针实证 | ✅ 修复：`valid_base_url`（http/https 可解析）挂 update_provider/add_model/add_models_batch/update_model 四端点 + 2 单测，探针复测 400 ✅ |
| W3 | 中 | **请求链路时间列无日期**：仅 `HH:MM:SS.mmm`，跨天记录歧义；与操作审计页（完整日期）不一致 | 昨日 23:09 记录与今日无法区分 | ✅ 修复：`fmtTime` 补 `MM-DD` 前缀；上游治理失败审计副时间同步补全（App.tsx），复测显示 `09-07 23:09:35.000` ✅ |
| W4 | 中 | **全局用量显示裸 ID**：按供应商/按用户显示 `prov-xxx`/完整 UUID 而非友好名称 | 用量页 top10 全为 prov-xxx/UUID | ✅ 修复：前端拉 providers/users 构建映射字典（`providerDisplayName` 提升为模块级共用），复测现存供应商/用户显示友好名 ✅；已删除供应商/用户的历史聚合数据无映射源，回退显示原 ID（合理，见 W8） |
| W5 | 轻 | **后端错误原文直出**：表单报错带 `bad request: ` 技术前缀（添加上游/创建用户实证） | 空提交/重复用户名弹窗文案 | ✅ 修复：error.rs `BadRequest` 去前缀，消息人话直达前端 ✅ |
| W6 | 轻 | **admin 登录空提交无前置拦截**：可发出空请求且文案误导为「用户名或密码错误」；portal 侧初始 disabled 更优，两端不一致 | 空提交实测发请求 | ✅ 修复：admin 登录按钮 `disabled={busy \|\| !u.trim() \|\| !p}` 对齐 portal，实测空值禁用/填后启用 ✅ |
| W7 | 低(观察) | 供应商卡片整行 div onClick 展开，无 button/role 语义，键盘用户不可达（无障碍） | 快照中不可交互 | 记录：建议后续统一加 role/tabindex |
| W8 | 低(观察) | 概览排行/用量分布含**已删除**供应商/用户的 ID 形式历史数据 | 排行含 f575d9da（已删用户） | 记录：历史用量数据保留合理（审计需要），但仅显示 ID 可读性差；可考虑「已删除」徽标或跳转兜底 |
| W9 | 低(观察) | Key 无备注/名称能力（单 kind 单 Key 设计下影响有限） | 创建 Key 直接弹明文 | 记录：多 Key 需求出现时一并评估命名与列表 |
| W10 | 低(观察) | 单次 console 错误 `net::ERR_INVALID_URL data:image/png;base64,…`，源码与 DOM 均未定位到来源 | console 瞬时报错 | 记录：无功能影响，出现条件待捕捉 |

**非缺陷排除**：`GET /@vite/client` 加载失败为浏览器会话缓存残留（昨天走查曾连 dev server），构建产物（dist/index.html 与 assets）经 grep 确认无 dev 引用，非生产缺陷。

### 8.3 修复验证与回归

- cargo test：**178 项全绿**（含新增 `valid_base_url` 2 单测）
- 前端 `npm run build` 通过（admin 端构建产物更新）
- 后端探针：弱密码建用户 → 400；坏 base_url 建模型 → 400 ✅
- 浏览器复测：登录空值禁用/填后启用 ✅；请求链路时间带日期 ✅；用量页现存供应商/用户显示友好名 ✅
- 走查注入状态清理：坏供应商 `not a url!!`（含级联模型）、弱密码用户 weakpw-test 均已删除；奖励走查产生的申领（uiwalk0908，已通过入账 2M）与 uiwalk0908 账号按流程保留

### 8.4 走查副产物：未覆盖项⑤（rewards）补测闭环

portal 提建议申领（文本 10~5000 字校验）→ admin「奖励审核」评定入账（1M~10M 范围校验）→ portal 余额 10,000,000 → 12,000,000 实时到账 ✅。全流程闭环，「五」未覆盖项清单仅余大规模并发压测一项。
