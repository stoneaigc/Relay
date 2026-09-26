# Daily Log - 2026-09-22

## [00:04] - [文档审查/生成]: README 按开源项目专家审查结论重写

- **文件**: README.md（258 行 → 323 行全文重写）
- **决策**:
  - 采纳专家推荐目录结构：badges → 核心能力（3 组 bullets）→ 快速开始（前置）→ 数据面接口 → 为什么选 Relay（含竞品对比）→ 可观测 → 性能基准 → 配置 → 部署形态 → 开发与 CI → 路线图 → FAQ 与安全 → License 与致谢
  - 修复 P0×2：负载策略「五种」→「六种」（依据 src/routing.rs:62-75 六个枚举变体）；数据面接口表补 `POST /v1/responses`（依据 src/main.rs:340）
  - 补齐 OSS 门面标准件：环境要求（Rust stable / Node≥20 / bench Node≥18）、贡献指引、安全披露（GitHub Security Advisories）、FAQ、compose 默认口令告警、relay.env 检查清单项、性能数字免责声明（mock 零延迟上游）、arm64 包名说明、本地打包非 musl 说明
  - 配置表补 `[defaults]` 段（default.toml L57-59 实测存在，旧版漏列）；`[logging]` 保留天数标注默认 30 天（config.rs:100-102）；注明 `[proxy]/[metrics]` 为代码内置默认值
- **验证**: grep 校验「五种/aarch64」零残留；24 个代码围栏成对闭合；TOC 11 个锚点与 H2 标题一一对应；新增事实逐条溯源（tests/ 描述按 routing_integration.rs 实测收敛为「路由与负载策略」）
- **遗留**: ① 外链 stoneaigc/Relay 与 runify-dev/runapi 无法本地验证，待人工点验；② 截图待补（README 已留 TODO 注释）；③ DESIGN.md 为过时草案与实现冲突（专家 P1-9，未动，建议更新或加声明）；④ config/default.toml L68 注释「0=永久保留(默认)」与代码默认 30 天矛盾，待修；⑤ LICENSE 是否需保留 RunAPI 上游版权行待用户确认；⑥ 根目录内部过程文件（build_err.txt 等）建议清理或 gitignore
