# Relay 上线前最终检查清单

> 用法：逐项勾选，全部 ✅ 后方可上线。功能验收剧本见 [PRE_LAUNCH_REPORT.md](PRE_LAUNCH_REPORT.md)（19 项全过）；本清单聚焦**配置决策、安全基线与近期改动回归**。
> 最近更新：2026-10-10（P0/P1/P2 加固批次后）

---

## 一、构建与质量门禁

- [ ] `cargo test` 本地全绿（CI 同款命令，199+ 项）
- [ ] `frontend/admin` 与 `frontend/portal` `npm run build` 均成功
- [ ] CI 流水线（push main / PR）绿色：[ci.yml](.github/workflows/ci.yml)
- [ ] 确认发布版本号与 [CHANGELOG.md](CHANGELOG.md) Unreleased 段已收口（发布时改为版本号 + 日期）

## 二、安全基线（上线阻断级）

- [x] **JWT 密钥**：v0.3.1 起零配置——缺失/默认/过短时首启自动生成 64 位强随机密钥并持久化到数据库，跨重启稳定；如显式设置 `RELAY_AUTH__JWT_SECRET` 则优先。**定稿后勿更换**（兼任供应商密钥加密主密钥）
- [ ] **管理员强密码**：`RELAY_ADMIN__PASSWORD` 非 `Relay@123`；上线后可在后台「设置 → 管理员账号」修改（argon2 入库即时生效，环境变量口令转为兜底）
- [ ] **一旦定稿不再更换 `jwt_secret`**：它兼任供应商密钥加密主密钥（AES-256-GCM）；更换后需在后台逐个重存供应商密钥
- [ ] SMTP 授权码配置（`RELAY_EMAIL__*` 或后台「设置 → 邮箱」），真实发信测试通过；**确认已退出开发模式**（smtp_host 非空，验证码不再随响应返回）
- [ ] 敏感入口仅经 HTTPS 暴露（nginx TLS），管理后台不裸奔公网（可选：IP 白名单 / VPN）
- [ ] 确认数据库文件与 `relay.env` 权限收紧（`chmod 600 relay.env`，数据目录仅服务账号可读）

## 三、本批次改动回归（P0/P1/P2）

### 密码与登录（argon2）
- [ ] 存量用户（sha256 旧哈希）登录成功，且库中 `password_hash` 已变为 `$argon2id$...` 开头（透明升级生效）
- [ ] 新注册用户密码哈希直接为 argon2
- [ ] 统一登录 `/login`、门户密码登录、重置密码三条路径各验一次

### API Key 生命周期（掩码入库）
- [ ] 创建 Key：弹窗一次性展示完整 Key + 复制成功；**关闭后无法再查看**
- [ ] 库内核对：`api_keys.key_plain` 全部为 NULL（启动自动清空历史明文）
- [ ] 门户 Key 列表仅显示掩码 `rk_live_xxxx****`；「刷新 Key」生成新 Key 可用、旧 Key 立即失效
- [ ] **用户公告**：存量 Key 明文已清空、无法再复制，需「刷新 Key」重新生成（发版说明/群公告）

### 供应商密钥加密
- [ ] 启动日志出现 `provider api_key 静态加密迁移: N 条`（存量明文回写）
- [ ] 库内核对：`providers.api_key` 均为 `enc:v1:` 前缀密文
- [ ] 迁移后模型测试连接 / 真实调用正常（解密链路通）
- [ ] 导出功能仍不含密钥（`groups/export` 设计如此）

### 模型组删除级联
- [ ] 删除测试组：组内路由 + 时段规则一并消失，绑定用户 `group_id=0`，响应含 `unbound_users`
- [ ] 被解绑用户在「用户管理」可见未绑定状态，可重新绑定

### 计费倍率透出
- [ ] 发起一次带倍率的调用：`usage_logs.bill_multiplier` 落值正确（路由×时段×用户）
- [ ] 门户「对话日志」与管理端「全局用量」明细出现「倍率」列，值与库一致

## 四、数据与备份

- [ ] 数据库选型决策：低流量 SQLite 起步 / 并发或共用场景直接 PostgreSQL
- [ ] **备份 cron 已挂**（每日，见 Wiki「安装与升级 → 备份与恢复」）：SQLite 用 `.backup` 热备，PG 用 `pg_dump`
- [ ] **恢复演练一次**：备份导入临时库 → Relay 指向它 → `/healthz` 通过
- [ ] `relay.env` + `config/default.toml` 已纳入备份（与数据库同批）
- [ ] 链路日志保留天数已决策（默认 30 天；`logging.retention_days=0` 为永久，注意库体量）
- [ ] **隐私决策**：请求/响应体预览采集（`body_preview_max_bytes`，默认 8KB）——面向真实用户建议下调或置 0，或至少在用户协议中声明

## 五、计费与商业参数

- [ ] 注册赠送额度确认（现默认 100 万 token；`RELAY_DEFAULTS__SIGNUP_GRANT_TOKENS` 可调）——这是真金白银，按商业策略定
- [ ] 全部上架模型已定价（`input_price`/`output_price`），未定价模型回退内置价表是否符合预期
- [ ] 三层倍率抽查一次：路由倍率 × 时段倍率 × 用户倍率 = 明细「倍率」列（见第三节）
- [ ] 全局默认限额复核：`concurrency_limit`（默认 16）/ RPM / TPM / 日·月预算——新用户未单独设置时全走这套
- [ ] 若开启时段规则：核对时区（`defaults.timezone`）与高峰/低谷权重覆盖

## 六、路由与上游

- [ ] 每个供应商「测试连接」通过（探测模型列表可用）
- [ ] 默认模型组（`is_active`）已设置——邮箱注册用户自动绑定它
- [ ] failover 演练：主目标手动停掉 → 调用自动切候选 → 恢复后半开自愈
- [ ] 语义缓存开关与折扣率已决策（路由级 opt-in + 全局开关）
- [ ] 多 Key 用户确认门户仍为「每协议一把活跃 Key」语义（`interface_kind` 维度后续如简化另行发版）

## 七、部署形态

- [ ] 部署方式二选一并演练：`install-online.sh`（systemd）或 Docker Compose
- [ ] 健康检查通：`curl http://127.0.0.1:9000/healthz` 返回版本号；Docker 镜像内置 HEALTHCHECK
- [ ] nginx：SSE 透传（`proxy_buffering off`）+ SPA `index.html` no-cache 已按 Wiki 配置
- [ ] 升级路径确认：`/opt/relay/upgrade.sh` 或后台「软件更新」可检查到 Release
- [ ] Prometheus 已抓 `/metrics`（带 token 或 admin JWT），核心指标入监控

## 八、上线后首日观察

- [ ] 请求链路失败率与 failover 触发频次（管理端「上游治理」）
- [ ] 注册量 vs 赠送消耗（防套利）
- [ ] 审计日志（`admin_audit_logs`）抽查变更操作留痕
- [ ] 已知观察项跟踪（来自 [PRE_LAUNCH_REPORT.md](PRE_LAUNCH_REPORT.md)）：O1 无 usage 流式零计费（建议首迭代）、O2 拒绝请求不落链路日志
