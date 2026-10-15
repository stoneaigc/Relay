# Relay 生产环境部署前检查清单

> 适用版本:v0.5.0+ · 部署方式:安装脚本(systemd)/Docker/手动 · 每节按顺序执行,全部勾选后方可对外放量。

---

## 一、环境与安装包

- [ ] 从 GitHub Releases 下载与本机架构匹配的包(x86_64 / aarch64,musl 静态链接,无依赖)
- [ ] 校验产物完整:解压后应含 `./relay`、`./config/default.toml`、`./frontend/{admin,portal}/dist`、`install.sh`、`upgrade.sh`、`relay-cli.sh`
- [ ] 确认目标端口未被占用(默认 **9000**):`ss -tlnp | grep 9000`
- [ ] 服务器时间同步已开启(验证码/预算窗口/审计时间戳依赖):`timedatectl | grep synchronized`
- [ ] (可选)PostgreSQL 13+ 就绪:连接串、库名、账号权限;SQLite 适合单机小规模,多实例/高并发选 PG

## 二、安装与配置(安装前)

- [ ] **管理员凭据**:通过环境变量覆盖默认值(默认 `admin/Relay@123` 仅限首启,生产严禁沿用)
  - `RELAY_ADMIN__USERNAME=你的管理员名`
  - `RELAY_ADMIN__PASSWORD=强密码(≥12位,含大小写数字符号)`
- [ ] **JWT 密钥策略**:`RELAY_AUTH__JWT_SECRET` 留空则首启自动生成 64 位强随机密钥并持久化入库
  - ⚠️ 该密钥兼任供应商密钥/初始密码等敏感字段的加密主密钥——**定稿后严禁更换**,否则已加密数据不可解
- [ ] **监听地址**:`server.bind` 生产建议 `0.0.0.0:9000` + 防火墙限源;仅本机反代可改 `127.0.0.1:9000`
- [ ] **全局默认限额**按运营策略设置(可选,不设用内置默认:并发 16 / RPM 0=不限 / TPM 0=不限 / 上游并发 32):
  - `RELAY_DEFAULTS__CONCURRENCY_LIMIT` / `RELAY_DEFAULTS__RPM_LIMIT` / `RELAY_DEFAULTS__TPM_LIMIT`
  - 新用户赠送额度:`RELAY_DEFAULTS__SIGNUP_GRANT_TOKENS`(默认 100 万,售卖场景按需调整)
- [ ] **邮箱 SMTP**(邮箱注册/找回密码必需):`RELAY_EMAIL__SMTP_HOST/PORT/USERNAME/PASSWORD`(465=SSL,587=STARTTLS;password 填客户端授权码)
- [ ] 环境变量落盘位置确认:systemd 部署写在 `relay.env`(安装目录),改后 `relay restart` 生效

## 三、安装

- [ ] 执行安装:`sudo bash install.sh`(自动创建 systemd 服务、写入 relay.env、安装 `relay` 运维命令)
- [ ] `relay status` 服务 active(running);`relay health` 返回 `{"status":"ok","version":"0.5.0"}`
- [ ] `relay logs -f` 首启日志检查:无 `ERROR`;出现 `seeded default ...` 属正常(default 组播种)

## 四、首启必做(安全加固,登录后台 http://<host>:9000/admin/)

- [ ] 修改管理员密码(设置 → 管理员账号;若已用 env 覆盖可跳过但建议后台再改一次验证链路)
- [ ] **供应商密钥录入**:上游模型页添加供应商(api_key 入库即 AES-256-GCM 加密);添加模型并至少配置一个激活模型组
- [ ] **default 用户组检查**:运营 → 用户组,确认存在 `default`(首启自动播种);按售卖策略调整其限额或新建自定义组
- [ ] **注册开关**:设置 → 门户——`手机号注册`、`新建 Key 需审批` 按运营策略决定(手机号注册默认开,审批默认关)
- [ ] **短信网关**(启用手机号注册的前置条件):设置 → 短信网关,配置网关 URL/请求体模板(含 `{phone}` `{code}`)/请求头 → 点「发送测试短信」用真实手机号验证收到;未配置网关时验证码会回显在注册响应里,**生产严禁带开发模式放量**
- [ ] 邮箱验证码链路:登录页注册 Tab 发一封测试邮件,确认能收到(SMTP 未配置时同样是开发模式回显)
- [ ] 管理员专属 Key(可选):设置 → 管理员密钥,为脚本/自动化创建;**明文仅此一次**,立即复制入密码库

## 五、反向代理(子路径部署必读)

- [ ] 根路径部署:直接反代 `http://127.0.0.1:9000/`,无需额外配置
- [ ] 子路径部署(如 `https://host/relay/` → `127.0.0.1:9000/`):`proxy_pass http://127.0.0.1:9000/;`(带尾斜杠剥前缀)或整段透传(不剥前缀)——**两种都被支持**:网关内的前缀剥离中间件按「最早段匹配」自动识别 `/relay/admin`、`/relay/portal`、`/relay/v1`、`/relay/login`
- [ ] 反代需放通 WebSocket/SSE 所需配置:`proxy_buffering off;`(流式对话)、`proxy_read_timeout 300s;`
- [ ] 冒烟:浏览器访问 `https://host/relay/login` 登录 → 自动进入对应工作台;`https://host/relay/v1/models`(带数据面 Key)返回模型列表

## 六、功能冒烟(10 分钟,全部通过才算部署完成)

- [ ] 统一登录:管理员账号登录 → 直达管理后台;普通用户登录 → 直达门户
- [ ] 创建用户(密码留空)→ 弹出 16 位初始密码并可复制 → 用该密码登录门户成功
- [ ] 用户组:新建一个限额组 → 用户管理里挂组 → 该用户门户「我的套餐」显示组名与限额
- [ ] 门户建 Key(openai)→ 复制 → `curl http://<host>:9000/v1/models -H "Authorization: Bearer sk-..."` 返回模型列表
- [ ] 真实对话一次(门户对话页或 curl chat)→ 对话日志出现记录、余额扣减、上游健康页无异常
- [ ] 手机号注册全流程(若开放):发码 → 收到短信 → 注册 → 登录
- [ ] 预算/限流抽查:给测试用户设日预算 1000 → 打满后请求返回 429 insufficient_quota
- [ ] 审计:操作审计页能看到上述变更操作记录;管理员 Key 的操作归因为 `admin-key:{name}`

## 七、上线后运维基线

- [ ] **备份**:`relay backup`(SQLite 在线热备,默认落 /var/backups/relay);建议 cron 每日一次 + 备份异地拷贝
- [ ] **升级**:`relay upgrade`(在线);离线环境手动替换二进制后 `relay restart`——升级不覆盖 config 与数据库,迁移自动幂等执行
- [ ] **回滚预案**:保留上一版本二进制;数据库迁移为增量补列(向前兼容),回滚旧版本可直接运行
- [ ] 监控:Prometheus 抓 `GET /metrics`(需 admin JWT 或 metrics.export_token);`/healthz` 接入探活
- [ ] 日志:`journalctl -u relay -f` 或 `relay logs -f`;链路日志在后台「请求链路」页,按 `logging.retention_days` 自动清理
- [ ] 防火墙:仅放行 9000(或反代端口)与 SSH;数据面与页面同端口,无需额外开洞

## 八、已知边界(对客说明口径)

- 手机号注册用户**忘记密码只能由管理员重置**(无短信找回通道);邮箱用户走验证码自助找回
- 本版本不含任何第三方登录(微信/支付宝/OAuth)与短信营销能力
- 单管理员设计;多管理员/RBAC 属后续多租户范畴
- 语义缓存、Redis 缓存、Postgres 为可选能力,默认关闭/SQLite,按需在配置中启用
