# 邮箱配置页面化设计

- 日期：2026-07-30
- 状态：已与用户对齐，待实现
- 范围：把 `[email]` SMTP 配置从配置文件搬到管理后台 UI，可在页面上配置、保存、测试发信；运行时热更新，无需重启。

## 背景与动机

当前 SMTP 配置写在 `config/default.toml` 的 `[email]` 段，或用环境变量 `RUNAPI_EMAIL__*` 注入，由 `figment` 在启动时一次性加载。注册验证码（`portal::send_email_code`）据此决定走 SMTP 还是 dev 模式（验证码回传 + 打日志）。问题：改 SMTP 要编辑文件并重启，对非开发者运维不友好。

目标：管理员在后台「设置」页直接配置 SMTP，保存即生效；支持「发送测试邮件」即时验证配置正确性；SMTP 授权码可逆加密入库，页面不回显明文。

## 现状关键事实

- `AppState.config` 是 `ArcSwap<Config>`，所有 handler 经 `state.config()` 读取，后台改完 `store()` 即刻生效。
- `EmailConfig` 字段：`smtp_host` / `smtp_port` / `username` / `password` / `from`；`enabled()` = `smtp_host` 非空。
- 邮件使用点唯一：`portal.rs` 的 `send_email_code`。
- 后台 CRUD 模式：`admin_guard` 鉴权 -> `storage::*` 读写 DB -> 必要时重建内存态。
- 迁移兼容：SQLite `ALTER TABLE ADD COLUMN`（忽略已存在），Postgres `IF NOT EXISTS`。
- 敏感字段先例：模型编辑 `api_key` 留空=保持不变，GET 不回显明文。
- 前端：`App.tsx` 的 `NAV` 数组 + 各 Panel 路由；`api.ts` 的 `req()` 封装；`ModelsPanel` 有 `testModel` 校验反馈模式可复用。

## 决策（与用户逐项确认）

1. **SMTP 授权码可逆加密入库**（方案 B）。用 `Config.auth.jwt_secret` 派生 AES-256-GCM 密钥，`encrypt(plain) -> base64(nonce||ciphertext)` / `decrypt(b64) -> plain`。页面不回显、留空=保持不变。
2. **范围：5 个 SMTP 字段**（host / port / username / password / from）。邮件主题/正文模板保持硬编码，本次不动。
3. **配置优先级：DB > 配置文件(toml+env)**。DB 有 `email.*` 则覆盖；DB 无则保持 `Config::load()` 结果（dev 模式兜底）。不实现环境变量锁定字段。
4. **测试发信**：表单配置直接测，不必先保存，复用 `testModel` 前端展示模式。
5. **通用系统配置表 `settings`**（KV，点分命名空间）。邮箱是第一个落项，未来 JWT/admin 密码等系统配置复用此表。

## 架构与数据流

```
                  ┌─ 管理后台「设置」页 ─┐
                  │  SMTP 表单 + 测试发信  │
                  └──────────┬───────────┘
                     POST /admin/api/settings/email      (保存：upsert settings + 刷内存 Config)
                     POST /admin/api/settings/email/test (测试：用表单配置发信，不落库)
                     GET  /admin/api/settings/email      (读取：回显非敏感字段，password 返回 has_password 布尔)
                              │
                              ▼
            settings 表 (key TEXT PK, value TEXT)
            email.smtp_host / smtp_port / username / from / password_enc
                              │
              启动时 load_settings("email.") -> 覆盖到内存 Config.email
                              │
                              ▼
            AppState.config (ArcSwap<Config>)  ←── 保存时 store() 刷新
                              │
                              ▼
            portal::send_email_code 读 state.config().email 发信
```

三个新接口挂在 `/admin/api/settings/*` 下，复用 `admin_guard` 鉴权。`email.rs::send_code` 签名不变，只是其读到的 `EmailConfig` 来源从「纯配置文件」变为「配置文件 + DB 覆盖」。

## 数据模型

新表（`migrations/0001_init.sql` 与 `0001_init.postgres.sql` 各加一段；`storage::init_schema` 用 `CREATE TABLE IF NOT EXISTS`，无需 ALTER，兼容旧库）：

```sql
CREATE TABLE IF NOT EXISTS settings (
  key        TEXT PRIMARY KEY,
  value      TEXT NOT NULL,
  updated_at TEXT NOT NULL
);
```

key 用点分命名空间：`email.smtp_host` / `email.smtp_port` / `email.username` / `email.from` / `email.password_enc`（加密后）。value 统一 TEXT，标量直接存字符串。

## 后端改动

### `storage.rs`：通用 KV 读写层

```rust
/// 读取某命名空间下所有配置（prefix 如 "email."）。
pub async fn load_settings(pool: &Db, prefix: &str) -> anyhow::Result<HashMap<String, String>>
/// upsert 单个配置项。
pub async fn set_setting(pool: &Db, key: &str, value: &str) -> anyhow::Result<()>
/// 批量 upsert（一次写多个 key，保存邮箱用）。
pub async fn set_settings(pool: &Db, items: &[(String, String)]) -> anyhow::Result<()>
```

### 新模块 `src/settings.rs`：配置应用逻辑 + 加解密

- `apply_email_settings(cfg: &mut Config, kv: &HashMap<String,String>)`：把 DB 的 `email.*` 覆盖到 `Config.email`；`password_enc` 解密后填入 `password`。
- 加解密：HKDF-SHA256 从 `jwt_secret` 派生 AES-256-GCM 密钥；`encrypt(plain) -> base64(nonce||ciphertext)` / `decrypt(b64) -> plain`。新增依赖 `aes-gcm`。

### `admin.rs`：三个 handler（仿 `test_model` / `update_model` 模式）

- `GET /admin/api/settings/email`：读 DB，回显 host/port/username/from；密码不回显，返回 `has_password: bool`。
- `POST /admin/api/settings/email`：校验 -> 加密密码 -> `set_settings` 批量写 -> 重载 `Config.email` 并 `state.config.store()` 刷新内存。
- `POST /admin/api/settings/email/test`：用请求体里的表单配置（非库里）临时构造 `EmailConfig`，调 `email::send_code` 发测试信到指定收件人，返回 `{ok, error?}`。密码字段：请求里为空且库里已有则用库里的（支持「不改密码只测当前配置」）。

密码留空语义：保存时 `password` 为空 -> 不写 `email.password_enc`（保持原值），其余字段正常更新。

### `main.rs`

- 启动时：`Config::load()` 之后、构造 `AppState` 之前，读 `load_settings(&db, "email.")`，`apply_email_settings(&mut cfg, &kv)`，再放进 `ArcSwap`。
- 路由：admin_api 加 `.route("/settings/email", get(...).post(...))` 和 `.route("/settings/email/test", post(...))`。
- 模块声明加 `mod settings;`。

## 前端改动

### 导航（`App.tsx`）

`NAV` 数组加 `{ path: "/settings", label: "设置", icon: Settings }`（`Settings` 从 `lucide-react` 导入）；`Routes` 加 `<Route path="/settings" element={<SettingsPanel />} />`。

### `api.ts`：三个方法

```ts
emailSettings: (): Promise<EmailSettingsResp> => req("/admin/api/settings/email"),
saveEmailSettings: (body: EmailSettingsBody) =>
  req("/admin/api/settings/email", { method: "POST", body: JSON.stringify(body) }),
testEmail: (body: EmailSettingsBody & { test_to: string }): Promise<{ ok: boolean; error?: string }> =>
  req("/admin/api/settings/email/test", { method: "POST", body: JSON.stringify(body) }),
```

类型：`EmailSettingsBody` = `{ smtp_host, smtp_port, username, password?, from }`（password 可空=保持不变）；`EmailSettingsResp` = body + `has_password: boolean`。

### `SettingsPanel` 组件（新页面，仿 `AddModelDialog` 表单 + `testModel` 校验反馈）

- 一个 `Card`，标题「邮箱配置」。字段：SMTP 主机 / 端口（select：465 SSL / 587 STARTTLS / 25 STARTTLS / 自定义）/ 用户名 / 授权码（password 输入框，placeholder「已设置，留空保持不变」或空）/ 发件人（可选）。
- 顶部状态条：读取后若 `has_password=false` 或 `smtp_host` 空 ->「⚠ 未配置，注册走开发模式（验证码回传）」；否则「✓ 已配置」。
- 底部两按钮：**保存配置**（密码留空不传）、**发送测试邮件**（先弹小输入框收测试收件邮箱，调 `testEmail`，按钮旁显示 `✓ 成功` / `✗ 失败信息`，复用 `ModelsPanel` 的 `test` 状态 UI）。
- 顶部说明：端口 465=SSL、587/25=STARTTLS，授权码填客户端授权码而非登录密码（与 `config/default.toml` 注释一致）。

### 门户不改

`portal::send_email_code` 零改动，读到的 `Config.email` 已是 DB 覆盖后的值。

## 测试与验证

项目无现成测试框架，按「手动验证 + `cargo build`」惯例：

1. **编译**：`cargo build` 通过（含新依赖 `aes-gcm`）。
2. **冷启动迁移**：删除 `runapi.db` 重启 -> `settings` 表随 `init_schema` 建出 -> 邮箱走 dev 模式（库内无 `email.*`）。
3. **保存生效**：后台填入真实 SMTP（如 163 + 授权码）-> 保存 -> 门户注册 -> 邮箱真实收到验证码。
4. **密码不回显**：刷新「设置」页 -> 授权码框为空、状态「已配置」-> 留空保存其它字段 -> 密码不被清空（再测发信仍成功）。
5. **测试发信**：填错端口 -> 点测试 -> 返回失败原因；改对 -> 成功。
6. **加密落库**：`SELECT key,value FROM settings WHERE key='email.password_enc'` -> value 是 base64 密文，非明文。
7. **热更新**：保存后不重启，立即发信即用新配置。

## 边界与错误处理

- **解密失败**（如 `jwt_secret` 变了导致旧密文解不开）：`apply_email_settings` 解密失败时记 `warn` 日志、该字段按空处理（等价未配密码），不阻断启动；后台 GET 时 `has_password=true` 但发信会失败并返回明确错误。
- **DB 写失败**：handler 返回 `ApiError::Internal`，前端显示错误，内存 Config 不动（保持旧值）。
- **测试发信不落库**：纯用请求体配置发一封，失败也不影响已保存配置。
- **并发保存**：`set_settings` 逐 key upsert，最后 `store()` 一次性刷新内存；读最终一致快照，无锁问题。
- **配置文件优先级**：DB 有 `email.*` 覆盖 toml/env；DB 无则保持 `Config::load()` 结果（dev 模式兜底）。

## 不做的事（YAGNI）

- 邮件主题/正文模板页面化（保持硬编码）。
- JWT secret、admin 密码等其它系统配置的页面化（表已支持，本次不实现 UI）。
- 环境变量锁定字段（`RUNAPI_EMAIL__LOCK` 之类）。

## 影响文件清单

- 新增：`src/settings.rs`
- 新增迁移段：`migrations/0001_init.sql`、`migrations/0001_init.postgres.sql`（各加 `settings` 表）
- 改：`Cargo.toml`（加 `aes-gcm`）、`src/main.rs`（启动加载 + 路由 + `mod settings`）、`src/storage.rs`（KV 读写层）、`src/admin.rs`（三个 handler）
- 改前端：`frontend/admin/src/App.tsx`（导航 + 路由 + `SettingsPanel`）、`frontend/admin/src/api.ts`（三个方法 + 类型）
- 不动：`src/email.rs`、`src/portal.rs`、`frontend/portal/**`
