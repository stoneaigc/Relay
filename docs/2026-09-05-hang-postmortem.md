# Relay 数据面挂死排查与修复报告

| 项 | 值 |
|---|---|
| 日期 | 2026-09-05 |
| 批次 | 批 I（基准） |
| 对应提交 | `3763eb1`（8 文件，+367/−74） |
| 影响组件 | 语义缓存（L1）、用量落盘、请求链路落库 |

---

## 一、问题现象

压测（16 并发 chat 全 miss 场景）触发 relay 挂死：进程存活、CPU 0%、接口陆续无响应，仅重启恢复。诊断插桩显示僵尸 handler 常驻（alive_tasks 恒定 34-38）、通道零积压（up/log 通道 4096 满容量）、卡点集中在缓存查找段（PhaseGuard 覆盖段之前）。

## 二、排查过程

### 1. 判别实验（关键证据）

用 curl 逐个探测发现 relay 从「部分死亡」（healthz 活、chat/stats 死）随时间**恶化为全死**（healthz 也 5s 超时）。渐进恶化实锤了传染模型：

> 同步锁等待占住 tokio worker → 新请求持续堆进同一把锁 → worker 耗尽 → I/O driver 无人 poll → 自维持死锁

### 2. 已知案例检索

DashMap 社区三例与本症状同构：

- `notify_push#739`：DashMap guard 跨 await → worker 全阻塞 → I/O driver 无人 poll → 自维持死锁（症状完全一致）
- `dashmap#369`：持 Ref 跨 key 操作交叉死锁（parking_lot RwLock 非重入）
- `dashmap#304`：6.0.0 shrink_to_fit 死锁（6.0.1 修复）

### 3. 应用代码静态穷举

handlers 全部缓存调用点（chat/messages/stream wrap）+ semantic_cache 内部：无 Ref 跨 await、无 AB-BA 死锁环。嫌疑收敛到 **DashMap 分片锁内部行为 + 热路径阻塞式统计锁**。

## 三、修复方案（结构性防御，三支柱）

| # | 措施 | 消灭的嫌疑面 |
|---|---|---|
| 1 | `DashMap` → `std::sync::RwLock<HashMap>`（src/semantic_cache.rs 全量重写） | 分片锁交叉死锁 |
| 2 | 全表容量操作移出热路径，后台 5s sweep 承担（软上限 2560 → 逐出至 1536） | 请求路径全表扫描 |
| 3 | 统计锁全部 `try_lock` 降级：竞争即丢弃统计，绝不阻塞请求 | 阻塞式 Mutex 传染 |

关键实现要点：

- `get`：读锁短临界区返回 clone；过期清理用 `try_write`，拿不到就交给后台 sweep，热路径绝不久等
- `store`：热路径仅单条 insert，不判满不逐出
- `sweep`：后台独占 `write`，先 retain 清过期，再按 expires_at 排序逐出至容量下限
- `record_hit` / `push_event` / `stats_snapshot`：统计一律 `try_lock`，竞争即丢弃

### 同批配套修复

- `flush_dirty` 快照化：先 clone dirty 列表再落库，消除锁跨 await
- 请求链路落库批量化：background_task 批量 drain + 批量 insert
- `activate_group` 切组后同步内存路由（此前切组后内存路由不刷新）

## 四、验证结果

- **行为**：MISS→HIT 链路正常（同一 body 二次请求返回 `x-relay-cache: HIT`）
- **压测**：探针 ×3 + 30s 加重，共 **25974 个全 miss 请求零错误**，每轮后 healthz 200（修复前同场景渐进挂死）
- **机制生效**：`entries=1536` 恰为兜底下限——后台逐出实际工作；`misses` 计数与探针总数精确吻合（4408+2048+3629+15888）
- **测试**：全项目 89 测试通过（含 semantic_cache 5 个测试）；诊断插桩全部移除

## 五、性能基准（已录入 README「性能基准」小节）

| 场景 | RPS | avg | P50 | P95 | P99 | 错误 |
|---|---|---|---|---|---|---|
| GET /healthz（纯 HTTP 栈） | ~9900 | 1.6ms | 1.3ms | 3.1ms | 4.3ms | 0 |
| GET /v1/models（内存鉴权） | ~9600 | 1.7ms | 1.5ms | 3.0ms | 3.8ms | 0 |
| POST /v1/chat/completions（全 miss 重路径） | ~720 | 22ms | 4.4ms | 7.7ms | 300–390ms | 0 |

- 环境：release build（lto=thin）、`--duration 10 --concurrency 16`、Windows 11 桌面机、本机回环 mock 上游、两轮取稳定值
- 内存：压测 4 万+ 请求（缓存 1536 条驻留）后 WorkingSet ≈ **29MB**（Private ≈ 17MB）

## 六、过程教训

1. **编辑工具假落盘**：SearchReplace 批量并行编辑 12 个仅 2 个真实落盘（返回成功 diff 但文件未变）→ 多编辑场景改用全量 Write + 每步 grep/Read 验证。此前窗口亦有 1/2 假落盘记录，此问题已两度实证。
2. **PowerShell 传参陷阱**：向 curl.exe 传 JSON（字符串或变量）会吞双引号，导致上游报 `key must be a string at column 2` → 用 `--data "@file"` 从文件读取绕开引号处理。

## 七、遗留事项

1. 挂死为**结构性修复**而非精确定位到某一把锁（未能在全死现场抓到具体持锁者）——修复覆盖了全部嫌疑面，风险收益比更高；若未来再现同症状，可按「读锁短临界区 + try_lock 统计」的新结构直接排除缓存面。
2. chat P99 300–390ms 毛刺（P50 仅 4.4ms）：来源为落库批刷与回环 mock 抖动，可作为后续优化项。
3. 语义 L2（embedding 相似度缓存）历史上曾有开启记录，当前配置为 `enabled=false` 保持关闭，代码路径保留。

## 八、现场状态（报告时点）

- 测试数据已清理：组 19 / 模型 100 / route 72 已删（路由计数回落 8 providers / 30 models / 4 groups）
- 临时文件已删：probe / mock / body / token / cookies 共 7 个
- 运行态：debug relay（`cargo run`）监听 8080，healthz 200
