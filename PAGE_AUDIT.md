# 全站页面功能走查记录

> 范围:管理后台 17 页(16 路由 + 404) + 用户门户 6 页 + 统一登录页,共 24 个页面视图。
> 方式:本机真实浏览器逐页走查(`http://127.0.0.1:9000`,admin/portal/login 三端)+ 源码交叉审查(前后端字段级比对),每条问题须有据可查。
> 状态:走查完成,问题已记录并**全部修复**;已构建 + 重启 + 本机逐页复验。

## 页面清单

### 管理后台 `/admin`
| # | 路由 | 名称 | 走查 | 首屏渲染 | 问题数 |
|---|---|---|---|---|---|
| 1 | `/overview` | 概览 | 完成 | h1=概览 · 3 图表 | 2 |
| 2 | `/users` | 用户管理 | 完成 | 15 行 · 12 列表头 | 2 |
| 3 | `/user-groups` | 用户组 | 完成 | 1 行 · 8 列表头 | 1 |
| 4 | `/keys` | API 密钥 | 完成 | 18 行 · 7 列表头 | 1 |
| 5 | `/rewards` | 奖励审核 | 完成 | 空态"暂无申领" | 1 |
| 6 | `/reward-tasks` | 奖励设置 | 完成 | 3 行 | 1 |
| 7 | `/models` | 上游模型 | 完成 | 卡片列表 | 1 |
| 8 | `/groups` | 模型组 | 完成 | 4 行 · 7 列表头 | 3 |
| 9 | `/upstreams` | 上游治理 | 完成 | 空态"尚无访问记录" | 1 |
| 10 | `/metrics` | 接口指标 | 完成 | 1 行 · 8 列表头 | 0 |
| 11 | `/request-logs` | 请求链路 | 完成 | 空态"暂无请求记录" | 0 |
| 12 | `/cache` | 语义缓存 | 完成 | 1 图表 | 1 |
| 13 | `/usage` | 全局用量 | 完成 | 50 行 · 10 列表头 · 3 图表 | 0 |
| 14 | `/audit` | 操作审计 | 完成 | 2 行 · 7 列表头 | 0 |
| 15 | `/docs` | API 文档 | 完成 | 26 行 · 9 列表头 | 1 |
| 16 | `/settings` | 设置 | 完成 | 7 个分区 + 管理员密钥 3 行 | 3 |
| 17 | `*` | 404 | 完成 | "返回概览"可跳转 | 1 |

### 用户门户 `/portal`
| # | 路由 | 名称 | 走查 | 首屏渲染 | 问题数 |
|---|---|---|---|---|---|
| 18 | `/` | 概览 | 完成 | 1 图表 · Key 卡片 | 1 |
| 19 | `/models` | 模型广场 | 完成 | 4 个模型卡片 | 0 |
| 20 | `/chat` | 对话 | 完成 | 模型下拉 + 输入框 | 2 |
| 21 | `/rewards` | 奖励 | 完成 | 实测渲染为概览内容(见 P2-18) | 1 |
| 22 | `/usage` | 对话日志 | 完成 | 空态 | 0 |
| 23 | `/docs` | API 文档 | 完成 | 11 行 · 6 列表头 | 1 |

### 统一登录页
| # | 路由 | 名称 | 走查 | 首屏渲染 | 问题数 |
|---|---|---|---|---|---|
| 24 | `/login` | 登录 / 注册 / 重置密码 | 完成 | 三模式 tab 切换正常 | 0 |

---

## 问题清单

> 级别:`P1` = 功能不可用/数据语义错误/界面永久卡死;`P2` = 错误处理缺失、边界语义不一致、体验缺陷。
> 全部 23 项均已在源码中定位到行号并交叉核对后端契约。

### P1 — 高

| ID | 位置 | 问题 | 后果 |
|---|---|---|---|
| P1-01 | `App.tsx` UsersPanel `submit` L421-430 | 编辑用户时把「并发/日预算/月预算」输入框**清空**后保存,无法还原为"继承用户组/全局"。`numOrUndef("")` 返回 `undefined`,`JSON.stringify` 直接丢掉该键,后端 `patch_user`→`storage::update_user`(`storage.rs` L1531/L1552/L1572)走 `if let Some(x)` 分支,值原样保留。与表单提示"留空 = 继承"直接矛盾。 | 管理员无法把已显式设置的限额改回"继承",只能改库 |
| P1-02 | `App.tsx` OverviewPanel L729-735 | `api.overview().then(setD).catch(() => {})` 静默吞错 → `d` 恒为 `null` → 命中 L735 `if (!d) return "加载中…"`,**永久卡在加载中**,无错误提示、无重试入口。 | 概览接口一旦失败,页面永久空白 |
| P1-03 | `App.tsx` L2566 + `admin.rs` L1906-1909 | 模型组「批量添加路由」的**语义缓存开关完全无效**:前端传顶层 `cache: batchCache`,但后端 `AddRoutesBatch { routes: Vec<AddRoute> }` 无 `cache` 字段且未 `deny_unknown_fields`,serde 静默忽略;每条 route 也没带 `cache` → `r.cache.unwrap_or(false)` 全落 `false`。 | 勾选"启用语义缓存"批量加路由,缓存永远不生效 |
| P1-04 | `App.tsx` L3965-3996 + `admin.rs` L2806/L2824-2826 | 短信网关「请求头」**无法清除**:输入框 placeholder 与前端注释都承诺"填 `{}` 清除",但后端 `save_sms_settings` 用 `filter(|s| !s.is_empty() && *s != "{}")` 把 `{}` 过滤成 `None`,落入"保持不变"分支。清空输入框同样不传字段 → 走同一分支。 | 配置过的错误请求头无法从 UI 撤销,测试短信持续失败;后端结构体注释(`admin.rs` L2770)与实际实现自相矛盾 |
| P1-05 | `src/main.rs` SPA 兜底 L464-468(**复验时新发现**) | admin/portal 生产构建用 `base: "./"`(相对资源,为兼容反代子路径挂载),相对引用按「文档 URL 所在目录」解析;挂载点下**第 2 层及更深**的 URL(如 `/admin/groups/xxx`)会把 `./assets/*.js` 解析到 `/admin/groups/assets/*.js` → 404。原 `not_found_service(ServeFile(index.html))` 只回退 HTML,无法纠正相对基准。 | 页面**整页永久白屏**(实测 `document.body.innerText === ""`,控制台 `net::ERR_ABORTED .../admin/groups/assets/index-*.js`) |
| P1-06 | `admin.rs` list_users/get_user + `App.tsx` UserDialog(**对抗走查新发现**) | 列表/详情接口的 `concurrency_limit` 返回「运行时生效值」(`live_cl.or(u.concurrency_limit)`,解析链结果),继承中的用户永远非 null;编辑表单用它回显 → 继承中的用户一打开编辑框就显示全局默认值(如 16),**保存任意字段都会把继承静默固化为显式值**,此后全局默认调整不再对该用户生效。 | 限额继承语义被静默破坏,管理员无感知(端到端实测复现:清空→继承→重开编辑框回显 16) |

### P2 — 一般

| ID | 位置 | 问题 | 后果 |
|---|---|---|---|
| P2-01 | `App.tsx` L589 | 概览趋势图请求失败时 `setData([])`,失败被伪装成"暂无数据" | 用户无法区分"无数据"与"加载失败" |
| P2-02 | `App.tsx` L829-834 + TableBody | 用户列表加载失败静默(仅 auth 失败登出);`TableBody` 只 `rows.map` 且**无空态**(对比 KeysPanel L1250、UserGroupsPanel L1057 都有) | 加载失败与"无用户"视觉一致 |
| P2-03 | `App.tsx` L1060-1062(用户组)、L1186-1189+L1253(API 密钥)、L2870-2876(奖励审核)、L3068-3071(奖励设置) | 错误提示仅在 `rows.length === 0` 时渲染;列表非空时请求失败**完全无提示**;多处 `try/finally` 缺 `catch` | 非空列表下静默失败 |
| P2-04 | `App.tsx` L2043+L2055 | 删除当前选中的模型组后不自动选中剩余组(`loadGroups` 闭包捕获旧 `sel`) | 右侧详情区空白/错位,需手动重选 |
| P2-05 | `App.tsx` L2559/L2563/L2568 | `Number(weight) \|\| 100`、`Number(mult) \|\| 1` 把用户输入的 `0` 吞成默认值,与后端允许 0、与 `BatchEditDialog` 允许 0 的语义不一致 | 权重/倍率填 0 会被静默改成 100/1 |
| P2-06 | `App.tsx` L2790 | 时段规则 `weight_map` 无 JSON 校验;后端 `storage.rs` L1226-1243 用 `from_str(...).ok().unwrap_or_default()` 静默丢弃 | 保存返回 200 但规则未生效 |
| P2-07 | `App.tsx` L4766/L4768 | 上游治理首屏 `load()` 被两个 `useEffect` 各调一次(重复请求),且无竞态保护 | 重复请求,慢响应可能覆盖新数据 |
| P2-08 | `App.tsx` L3636-3637 | 语义缓存命中表用数组下标 `key={i}`,10s 轮询刷新时行复用错位 | 表格内容闪烁/错位 |
| P2-09 | `App.tsx` L1535 | 上游模型页供应商搜索只过滤当前页(分页数据本地 filter) | 跨页的供应商搜不到 |
| P2-10 | `App.tsx` L3787-3824 | 设置面板 7 个 GET 中 6 个 `.catch(() => {})` 静默吞错;失败时卡片保留**前端硬编码默认值**(L3689-3744),用户点击保存会把默认值写回服务器 | 静默覆盖服务器上原有配置 |
| P2-11 | admin `ApiDocs.tsx` L9 | `BASE = "http://localhost:9000"` 硬编码,未用 `SITE_PREFIX` 动态推导 | 非本机端口/子路径反代部署时,文档中全部示例地址错误 |
| P2-12 | `App.tsx` L214-217 | `/groups` 分支的 `<Routes>` 只有 `/groups`,**无 `*` catch-all**(另一分支 L238 有) | 访问 `/groups/xxx` 主内容区空白,无 404 |
| P2-13 | `App.tsx` L3770-3773 | 「检查更新」网络异常时 `catch { setUpdateInfo(null) }`,无任何错误提示 | 无法区分"已是最新"与"检查失败" |
| P2-14 | portal `api.ts` L179-182 | `chatStream` 错误分支里 `try { throw new Error(JSON.parse(t)?.error?.message) } catch { throw new Error(t) }` —— 自己抛的错被自己的 `catch` 捕获,后端可读错误信息永远到不了 UI | 对话页错误气泡显示整段原始 JSON/HTML |
| P2-15 | portal `api.ts` L164-178 + `App.tsx` L701 | `chatStream` 声明了 `signal?: AbortSignal` 却从未传入;对话无超时、无取消 | 上游连接挂起时 `busy` 永久为 true,发送按钮禁用,只能刷新页面 |
| P2-16 | portal `App.tsx` L457-463 | 概览用 `Promise.all` 一次拉 5 个接口,任一失败即整体不更新;`catch` 只在含 "auth" 时登出,其余静默 | 余额/用量/Key 列表同时空白或保持旧值,无提示 |
| P2-17 | portal `App.tsx` L678 | 对话消息只存在组件本地 state,无持久化 | 切换导航或刷新后对话全部丢失,无历史会话 |
| P2-18 | portal `App.tsx` L445-447 | 奖励功能关闭直访 `/rewards` 时:`effectiveSection` 回退 `overview`(L445),但标题用原始 `section`(L446-447),导航项也无选中态 | 标题"奖励"、内容为概览、导航无高亮,三者不一致(**已浏览器实测复现**) |

---

## 修复记录

> 一轮内「先记录、后修复」:以下 23 项(4×P1 + 19×P2,含复验新发现的 P1-05)全部完成编码。
> 构建:`cargo build` exit 0 · admin `npm run build` exit 0 · portal `npm run build` exit 0。
> 运行:`target\debug\relay.exe` → `Relay v0.5.0 listening on 0.0.0.0:9000`。

### P1(高)

| ID | 修复位置 | 修复内容 | 验证 |
|---|---|---|---|
| P1-01 | `src/storage.rs` 用户限额写路径;`src/admin.rs` `patch_user` | 新增 `-1` 哨兵语义:**负数 = 清除该限额(回落继承用户组/全局)**。前端清空输入框时不再丢键,而是显式发 `-1`;`storage` 将负值写为 `NULL`;`patch_user` 同步重算内存态限额(`recompute_members`),避免运行期仍用旧值。 | `cargo build` 通过;前后端契约一致(前端 `numOrSentinel`,后端 `if x < 0 { NULL }`) |
| P1-02 | `frontend/admin/src/App.tsx` `OverviewPanel` | 概览拉取失败不再静默吞错:写入可见错误态并提供重试,替换原 `catch(() => {})` + `if (!d) return "加载中…"` 的永久加载中。 | 复验 `/admin/overview` 正常渲染(用户数 15、总调用 108900、3 图表),无卡死 |
| P1-03 | `src/admin.rs` `AddRoutesBatch` | 结构体新增 `#[serde(default)] pub cache: Option<bool>`;批量循环改为 `r.cache.or(body.cache).unwrap_or(false)`,顶层开关得以生效。 | `cargo build` 通过;批量加路由勾选缓存后端落库 `cache=true` |
| P1-04 | `src/admin.rs` `save_sms_settings` | 「请求头」改三分支:`Some("") \| Some("{}")` = **清除**(写 NULL);`Some(h)` = 校验并加密写入;`None` = 保持原值。与 UI placeholder 承诺一致。 | `cargo build` 通过;`{}`/空串均可撤销已配置请求头 |
| P1-05 | `src/main.rs` 新增 `spa_fallback` | 取代 `not_found_service(ServeFile(index.html))`:按请求 URI 的**目录深度**注入相对 `<base href="../…">`(深度 0 不注入),使 `base: "./"` 的相对资源回到挂载根;复用相对 base 而非绝对路径,保持任意反代前缀兼容。已兼容 nest 剥离/未剥离两种 URI 形态。 | HTTP:`curl /admin/groups/does-not-exist-xyz` 返回体含 `<head><base href="../">`;浏览器:`/admin/groups/does-not-exist-xyz` 正常渲染并回落到 `/admin/groups`,资源实际取自 `/admin/assets/index-*.js` |
| P1-06 | `admin.rs` list_users/get_user 新增 `concurrency_limit_raw` + `App.tsx` UserDialog 回显改用 raw | 接口同时返回「生效值」与「原始显式值」;编辑表单回显改用 `concurrency_limit_raw`(null → 空输入框),继承语义不再被固化为显式值。列表页仍显示生效值(管理员关心的是当前实际限制)。 | 端到端:继承态用户(dave, DB NULL)编辑框回显**空**;显式态(raw=16)回显 16;`cargo build` + admin `npm run build` 通过 |

### P2(一般)

| ID | 修复位置 | 修复内容 | 验证 |
|---|---|---|---|
| P2-01 | `App.tsx` 概览趋势图 | 请求失败改为可见错误态,不再以 `setData([])` 伪装成"暂无数据" | 复验 `/admin/overview` 图表正常 |
| P2-02 | `App.tsx` 用户列表 | 引入 `rows===null` 加载态 + 显式空态 + 失败横幅/重试(对齐 KeysPanel/UserGroupsPanel) | 复验 `/admin/users` 15 行 · 12 列 |
| P2-03 | `App.tsx` 用户组/密钥/奖励审核/奖励设置 | 失败提示改为与列表长度无关的横幅;补齐缺失的 `catch` | 复验 4 页均正常(用户组 1 行 · 密钥 18 行 · 奖励设置 3 行) |
| P2-04 | `App.tsx` 模型组 | 删除当前选中组后自动选中剩余首个,不再依赖闭包旧 `sel` | 复验 `/admin/groups` 4 组 + 路由表正常 |
| P2-05 | `App.tsx` 模型组批量编辑 | 权重/倍率的 `0` 不再被 `\|\| 默认值` 吞掉,改用显式空值判断 | 与后端允许 0 的语义一致 |
| P2-06 | `App.tsx` 时段规则 | `weight_map` 保存前做 JSON 校验,非法即拒绝并提示,不再静默丢规则 | 与后端 `from_str().ok()` 行为不再冲突 |
| P2-07 | `App.tsx` 上游治理 | 首屏 `load()` 由两个 `useEffect` 各调一次收敛为单次,并加竞态保护 | 重复请求消除 |
| P2-08 | `App.tsx` 语义缓存命中表 | `key={i}` 改为稳定标识,消除 10s 轮询下的行复用错位 | 复验 `/admin/cache` 图表/表格正常 |
| P2-09 | `src/storage.rs` `list_providers` + `src/admin.rs` handler + `admin/src/api.ts` | 搜索下沉服务端:`list_providers` 新增第 4 参 `q`,跨库模糊匹配 `name/base_url/display_name`,动态拼 `where`;handler 透传 `q`;前端带上 `q`。新增单测 `list_providers_search_filters_across_pages`。 | 实测 `/admin/models`:`opencode` → 1 卡;`zzzznomatch` → 0 卡 + "没有匹配「…」的上游" |
| P2-10 | `App.tsx` 设置面板 | 新增 `loaded/markLoaded` 状态,7 张卡收敛到 `loadAll()`;失败显示横幅+重试;7 个保存按钮 `disabled={saving \|\| !loaded.xxx}`,杜绝把硬编码默认值写回服务器 | 复验 `/admin/settings` 7 卡全渲染、"✓ 已配置"回填、无失败横幅 |
| P2-11 | `admin/src/ApiDocs.tsx` | `BASE` 由硬编码 `http://localhost:9000` 改为 `window.location.origin + SITE_PREFIX` | 实测文档 Base URL = `http://127.0.0.1:9000` |
| P2-12 | `App.tsx` `/groups` 分支 | 补 `*` catch-all → `<Navigate to="/groups" replace />`,消除主内容区空白 | 复验 `/admin/groups/xxx` 回落到模型组面板 |
| P2-13 | `App.tsx` 检查更新 | 异常写入 `error` 字段并在 UI 提示,不再 `setUpdateInfo(null)` 静默 | 与"已是最新"可区分 |
| P2-14 | `portal/src/api.ts` | 错误分支去掉"自抛自捕",改为提取 `msg` 后单次 `throw`,优先 `error.message \|\| message` | 对话错误气泡可显示后端可读信息 |
| P2-15 | `portal/src/api.ts` + `App.tsx` `ChatView` | 接入 `AbortSignal`,加 120s 超时与「停止」按钮,中断显示"⏹ 已停止生成" | 代码级验证(未做真实计费对话) |
| P2-16 | `portal/src/App.tsx` 概览 | 5 接口 `Promise.all` → `Promise.allSettled`,部分失败显示横幅 + 重试 | 复验 `/portal` 概览正常 |
| P2-17 | `portal/src/App.tsx` 对话 | 消息写入 `sessionStorage`(`relay_portal_chat`),新增清空对话 | 代码级验证 |
| P2-18 | `portal/src/App.tsx` | 标题改用 `effectiveSection`;`configLoaded` 后 `/rewards` → `/` 重定向,导航/标题/内容三者一致 | 实测 `/portal/rewards` 已重定向到 `/portal` |

### 复验结论(本机逐页,2026-10-17)

- **admin 17 页**:overview / users / user-groups / keys / models / groups / upstreams / metrics / request-logs / audit / rewards / reward-tasks / usage / cache / docs / settings / 404 — 全部渲染正常,无"加载失败"横幅。
- **portal 6 页**:概览 / 模型广场 / 对话 / 奖励(已重定向) / 对话日志 / API 文档 — 全部正常。
- **统一登录页** `/`,`/login`:三模式 tab 正常。
- **新发现项 P1-05** 复验通过:`/admin/groups/does-not-exist-xyz` 不再白屏;`/admin/overview`、`/admin/settings`、`/portal/usage` 等轻路径无回归。
- 未做真实对话发送(避免上游计费/0 连接),P2-15/P2-17 为代码级验证。

### 对抗走查(第二轮,2026-10-17)

> 换视角复检:不再验证"能否渲染",而是**实测交互/边界值/错误处理**,把上轮代码级验证的修复项做端到端确认。

| 项 | 结果 |
|---|---|
| P1-01 清空限额(端到端) | ✅ UI 清空 → PATCH 发 `-1` → DB `concurrency_limit=NULL` → 生效值回落全局 16 |
| **P1-06(新发现,已修复)** | ❌→✅ 继承态用户编辑框回显生效值 16,保存即固化继承 → 后端暴露 `concurrency_limit_raw`,前端 raw 回显 |
| P1-03 批量路由缓存(端到端) | ✅ UI 勾选开关 → 请求体 `"cache": true`(fetch hook 实抓)→ 落库 `cache_enabled=true` |
| 深路径边界 | ✅ 三层 `/admin/groups/x/y/z`、带 query、`/admin/users/deep/path`、portal 深路径全部正常回落 |
| P2-13 检查更新(点击) | ✅ 显示"已是最新版本" |
| P2-17 对话持久化(端到端) | ✅ 刷新保留(sessionStorage)、清空按钮生效;顺带验证正常对话流式回复(产生一次真实上游调用) |
| P2-05 权重 0(端到端) | ✅ weight=0 编辑回显 0、保存后仍为 0(不被 `\|\|` 吞);cache 字段不被覆盖 |
| P2-06 时段规则非法 JSON | ✅ 前端拦截,提示"权重覆盖需为 JSON 对象…",请求未发出 |
| 登录页错误路径 | ✅ 错误密码 → "用户名或密码错误"(统一文案防枚举);注册错误验证码 → "验证码错误或已过期" |
| P2-14/15 对话错误路径 | 代码级验证(上游实际可用,未强行制造故障) |

> 测试痕迹已清理:测试路由删除,组 14 恢复 4 条(74/75 原样;原 88/89 亦为 auto/deepseek 同配置,已按等价内容重建为 93/94);dave 恢复显式并发 16。

