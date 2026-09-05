use std::collections::VecDeque;
use std::hash::{Hash, Hasher};
use std::sync::{
    atomic::{AtomicBool, AtomicI64, AtomicU32, AtomicU8, Ordering},
    Arc, Mutex,
};
use std::time::{Duration, Instant};

use arc_swap::ArcSwap;
use dashmap::DashMap;
use serde::Serialize;
use serde_json::{json, Value};
use tokio::sync::{mpsc, Mutex as AsyncMutex, OwnedSemaphorePermit, Semaphore};
use uuid::Uuid;

use crate::config::{Config, ProviderKind};

/// 用户状态:0=active,1=disabled。
pub const STATUS_ACTIVE: u8 = 0;

/// 周期预算检查结果。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BudgetStatus {
    Ok,
    DailyExhausted,
    MonthlyExhausted,
}

/// API Key 在内存中的映射条目。校验请求时只读此结构,不查库。
#[derive(Debug, Clone)]
pub struct KeyEntry {
    /// api_keys.id,用量明细按密钥维度归因用。
    pub id: Uuid,
    pub user_id: Uuid,
}

/// 单用户运行态。token 余额/并发都是原子量,落库异步进行。
#[derive(Debug)]
pub struct UserState {
    pub id: Uuid,
    pub token_balance: AtomicI64,
    pub in_flight: AtomicU32,
    pub concurrency_limit: AtomicU32,
    pub status: AtomicU8,
    /// 用户级计费倍率 ×1000(如 3000 = ×3.0),与模型倍率相乘。
    pub bill_multiplier_milli: AtomicU32,
    /// 绑定的模型组 id(0 = 未绑定)。
    pub group_id: AtomicI64,
    /// 余额自上次落盘以来是否变更。
    pub dirty: AtomicBool,
    /// 每分钟请求数上限(0 = 不限)。
    pub rpm_limit: AtomicU32,
    /// 每分钟 token 消耗上限(0 = 不限)。
    pub tpm_limit: AtomicU32,
    /// 日预算(charged tokens,0 = 不限)。
    pub budget_daily: AtomicI64,
    /// 月预算(charged tokens,0 = 不限)。
    pub budget_monthly: AtomicI64,
    /// 当前日窗口已用 charged tokens。
    pub used_daily: AtomicI64,
    /// 当前月窗口已用 charged tokens。
    pub used_monthly: AtomicI64,
    /// 日窗口 key(本地日序数),变更时重置 used_daily。
    day_key: AtomicI64,
    /// 月窗口 key(本地 年*12+月),变更时重置 used_monthly。
    month_key: AtomicI64,
    /// 滑动窗口限流状态(RPM 时间戳 / TPM 消费量)。
    rate: Mutex<RateWindow>,
}

/// 滑动窗口限流内部状态。
#[derive(Debug, Default)]
struct RateWindow {
    /// 近 60 秒内(缩放到秒)的请求时间戳,用于 RPM。
    rpm: VecDeque<i64>,
    /// (unix 秒, tokens),用于 TPM。
    tpm: VecDeque<(i64, u32)>,
}

/// 滑动窗口固定时长(秒)。
const RATE_WINDOW_SECS: i64 = 60;

impl UserState {
    pub fn new(
        id: Uuid,
        balance: i64,
        concurrency_limit: u32,
        status: u8,
        bill_multiplier: f64,
        group_id: i64,
    ) -> Self {
        Self {
            id,
            token_balance: AtomicI64::new(balance),
            in_flight: AtomicU32::new(0),
            concurrency_limit: AtomicU32::new(concurrency_limit),
            status: AtomicU8::new(status),
            bill_multiplier_milli: AtomicU32::new((bill_multiplier * 1000.0).round() as u32),
            group_id: AtomicI64::new(group_id),
            dirty: AtomicBool::new(false),
            rpm_limit: AtomicU32::new(0),
            tpm_limit: AtomicU32::new(0),
            budget_daily: AtomicI64::new(0),
            budget_monthly: AtomicI64::new(0),
            used_daily: AtomicI64::new(0),
            used_monthly: AtomicI64::new(0),
            day_key: AtomicI64::new(0),
            month_key: AtomicI64::new(0),
            rate: Mutex::new(RateWindow::default()),
        }
    }

    /// 设置在内存状态。供管理端创建/修改用户后同步。
    pub fn set_limits(&self, rpm: u32, tpm: u32) {
        self.rpm_limit.store(rpm, Ordering::Relaxed);
        self.tpm_limit.store(tpm, Ordering::Relaxed);
    }

    /// 设置日/月预算(charged tokens,0 = 不限)。供管理端创建/修改用户后同步。
    pub fn set_budgets(&self, daily: i64, monthly: i64) {
        self.budget_daily.store(daily.max(0), Ordering::Relaxed);
        self.budget_monthly.store(monthly.max(0), Ordering::Relaxed);
    }

    /// 窗口翻转(key 可注入,供确定性测试)。key 变化即跨窗口,重置对应计数。
    fn roll_windows_keys(&self, day: i64, month: i64) {
        if self.day_key.swap(day, Ordering::Relaxed) != day {
            self.used_daily.store(0, Ordering::Relaxed);
        }
        if self.month_key.swap(month, Ordering::Relaxed) != month {
            self.used_monthly.store(0, Ordering::Relaxed);
        }
    }

    /// 按本地时区推进日/月窗口;跨窗口时重置计数。
    fn roll_windows(&self, tz_offset_hours: i64, now: i64) {
        let day = (now + tz_offset_hours * 3600).div_euclid(86400);
        let (y, m, _) = crate::storage::civil_from_days(day);
        self.roll_windows_keys(day, y * 12 + m as i64);
    }

    /// 记录一笔 charged tokens 到日/月窗口(自动跨窗口翻转)。
    pub fn record_budget(&self, charged: i64, tz_offset_hours: i64) {
        self.roll_windows(tz_offset_hours, Self::now_secs());
        self.used_daily.fetch_add(charged, Ordering::Relaxed);
        self.used_monthly.fetch_add(charged, Ordering::Relaxed);
    }

    /// 当前预算状态;耗尽返回对应窗口变体(调用方转 429 insufficient_quota)。
    pub fn budget_status(&self, tz_offset_hours: i64) -> BudgetStatus {
        self.roll_windows(tz_offset_hours, Self::now_secs());
        let d = self.budget_daily.load(Ordering::Relaxed);
        if d > 0 && self.used_daily.load(Ordering::Relaxed) >= d {
            return BudgetStatus::DailyExhausted;
        }
        let m = self.budget_monthly.load(Ordering::Relaxed);
        if m > 0 && self.used_monthly.load(Ordering::Relaxed) >= m {
            return BudgetStatus::MonthlyExhausted;
        }
        BudgetStatus::Ok
    }

    fn now_secs() -> i64 {
        use std::time::{SystemTime, UNIX_EPOCH};
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64
    }

    /// 尝试通过 RPM 限流并登记一次请求。超限返回 false。
    pub fn try_rpm(&self) -> bool {
        let limit = self.rpm_limit.load(Ordering::Relaxed);
        if limit == 0 {
            return true;
        }
        let now = Self::now_secs();
        let mut g = self.rate.lock().unwrap();
        let win = &mut g.rpm;
        while let Some(&t) = win.front() {
            if t > now - RATE_WINDOW_SECS {
                break;
            }
            win.pop_front();
        }
        if (win.len() as u32) >= limit {
            return false;
        }
        win.push_back(now);
        true
    }

    /// TPM 入站检查:当前窗口是否已超限(不登记,实际消耗由 record_tpm 落账)。
    pub fn tpm_ok(&self) -> bool {
        let limit = self.tpm_limit.load(Ordering::Relaxed);
        if limit == 0 {
            return true;
        }
        let now = Self::now_secs();
        let mut g = self.rate.lock().unwrap();
        let win = &mut g.tpm;
        while let Some(&(t, _)) = win.front() {
            if t > now - RATE_WINDOW_SECS {
                break;
            }
            win.pop_front();
        }
        win.iter().map(|&(_, tok)| tok as u64).sum::<u64>() < limit as u64
    }

    /// 记录本次调用实际消耗的 token(用于 TPM 滑动窗口)。
    pub fn record_tokens(&self, tokens: u32) {
        let limit = self.tpm_limit.load(Ordering::Relaxed);
        if limit == 0 {
            return;
        }
        let now = Self::now_secs();
        let mut g = self.rate.lock().unwrap();
        let win = &mut g.tpm;
        while let Some(&(t, _)) = win.front() {
            if t > now - RATE_WINDOW_SECS {
                break;
            }
            win.pop_front();
        }
        win.push_back((now, tokens));
    }

    pub fn group(&self) -> i64 {
        self.group_id.load(Ordering::Relaxed)
    }

    pub fn bill_multiplier(&self) -> f64 {
        self.bill_multiplier_milli.load(Ordering::Relaxed) as f64 / 1000.0
    }

    pub fn set_bill_multiplier(&self, v: f64) {
        self.bill_multiplier_milli
            .store((v * 1000.0).round().max(0.0) as u32, Ordering::Relaxed);
    }

    pub fn is_active(&self) -> bool {
        self.status.load(Ordering::Relaxed) == STATUS_ACTIVE
    }

    pub fn balance(&self) -> i64 {
        self.token_balance.load(Ordering::Relaxed)
    }

    /// 扣减 token,并标记需落盘。允许扣到负数(让本次请求完成,下次再拒)。
    pub fn deduct(&self, amount: i64) {
        self.token_balance.fetch_sub(amount, Ordering::Relaxed);
        self.dirty.store(true, Ordering::Relaxed);
    }

    /// 尝试占用一个并发名额,成功返回 RAII guard。
    pub fn try_acquire(self: &Arc<Self>) -> Option<ConcurrencyGuard> {
        let limit = self.concurrency_limit.load(Ordering::Relaxed);
        let prev = self.in_flight.fetch_add(1, Ordering::AcqRel);
        if prev >= limit {
            self.in_flight.fetch_sub(1, Ordering::AcqRel);
            None
        } else {
            Some(ConcurrencyGuard {
                user: Arc::clone(self),
            })
        }
    }
}

/// 持有期间占用一个并发名额,Drop 时释放(覆盖正常返回/错误/流中断)。
pub struct ConcurrencyGuard {
    user: Arc<UserState>,
}

impl Drop for ConcurrencyGuard {
    fn drop(&mut self) {
        self.user.in_flight.fetch_sub(1, Ordering::AcqRel);
    }
}

/// 一次调用的用量事件,异步落盘。
#[derive(Debug, Clone)]
pub struct UsageEvent {
    pub user_id: Uuid,
    /// 发起调用的 API Key(api_keys.id);门户侧调用无密钥时为 None。
    pub key_id: Option<Uuid>,
    pub model: String,
    pub provider: String,
    pub upstream_model: String,
    pub input_tokens: u32,
    pub output_tokens: u32,
    pub charged_tokens: i64,
    /// 计费口径成本(USD),由消费端按上游单价×总倍率折算后覆写;发送侧恒填 0.0。
    pub cost_usd: f64,
    pub status: u16,
    /// 关联的请求链路 ID(request_logs.request_id)。
    pub request_id: Option<String>,
}

// ===================== 上游治理:并发槽(P1) + 熔断器(P2) =====================

/// 一个「上游连接」的身份:协议 + base_url + api_key。
/// api_key 太长不直接 Hash,只存它的 u64 指纹 + 空标记。
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct UpstreamKey {
    pub kind: ProviderKind,
    pub base_url: String,
    /// None 代表「该上游无 key」,Some 是 key 的 u64 指纹(足够做身份区分)。
    pub key_fingerprint: Option<u64>,
}

impl UpstreamKey {
    pub fn new(kind: ProviderKind, base_url: &str, api_key: Option<&str>) -> Self {
        let key_fingerprint = api_key.map(|k| {
            let mut h = std::collections::hash_map::DefaultHasher::new();
            k.hash(&mut h);
            h.finish()
        });
        Self { kind, base_url: base_url.to_string(), key_fingerprint }
    }

    /// 从管理后台传回来的「指纹 16 hex 字符串」反构造。
    /// 空串 = 无 key;其它 = 按 u64 解析。
    pub fn from_fingerprint(kind: ProviderKind, base_url: String, fingerprint_hex: &str) -> Self {
        let fp = if fingerprint_hex.is_empty() {
            None
        } else {
            u64::from_str_radix(fingerprint_hex, 16).ok()
        };
        Self { kind, base_url, key_fingerprint: fp }
    }
}

// ======================== 路由失败审计 ========================
/// 最多保存多少条失败审计(内存 ring buffer)。
pub const AUDIT_FAILURE_CAP: usize = 500;

#[derive(Clone, Debug, Serialize)]
pub struct FailureRecord {
    /// 事件发生时间(unix ms,客户端可直接格式化当地时间)。
    pub ts_ms: u64,
    /// 上游协议:openai / anthropic。
    pub kind: &'static str,
    pub base_url: String,
    /// 16 hex 的 Key 指纹;"" = 无 Key。
    pub key_fingerprint: String,
    /// 给管理后台「一键清零/定位」用的稳定 ID 格式: kind|base_url|fp_hex。
    pub upstream_id: String,
    /// 用户请求的「对外模型名」(可能无法解析时为 "")。
    pub requested_model: String,
    /// 来源于哪条链路:chat completions 还是 messages(anthropic)。
    pub path: &'static str,
    /// 错误摘要(直接从 ApiError::Unavailable 的 Display 取,最多 300 chars 避免炸内存)。
    pub error_summary: String,
    /// 这次失败后,失败计数变成了多少。
    pub fail_count_after: u8,
    /// 熔断阈值(一般 3,记录下来前端不用自己硬编码)。
    pub threshold: u8,
    /// true = 这一次失败刚好触发了熔断(= 前端可以标红高亮)。
    pub triggered_break: bool,
}

/// 连续失败 N 次就熔断熔断窗这么久。
const BREAKER_FAILS: u8 = 3;
const BREAKER_WINDOW: Duration = Duration::from_secs(30);

/// 每个上游的熔断器状态(内部可变,AsyncMutex 保护避免竞态)。
#[derive(Debug, Default)]
pub struct Breaker {
    fail_count: u8,
    /// 熔断结束时刻;None = 没在熔断。
    recover_at: Option<Instant>,
}

impl Breaker {
    /// 当前是否可用(没熔断就算可用)。
    pub fn is_available(&self) -> bool {
        match self.recover_at {
            None => true,
            Some(t) => Instant::now() >= t,
        }
    }

    /// 调用成功:清零失败计数。
    pub fn record_success(&mut self) {
        self.fail_count = 0;
        self.recover_at = None;
    }

    /// 遇到一次 Unavailable 级错误:计数+1,达到阈值就进入熔断窗。
    pub fn record_unavailable(&mut self) {
        self.fail_count = self.fail_count.saturating_add(1);
        if self.fail_count >= BREAKER_FAILS {
            self.recover_at = Some(Instant::now() + BREAKER_WINDOW);
        }
    }

    /// 探活:若熔断窗已过,重置计数为 1,允许放行一个请求(=半开)。
    /// 返回 true 表示本次应放行(半开),false 表示仍在熔断中。
    pub fn maybe_half_open(&mut self) -> bool {
        match self.recover_at {
            Some(t) if Instant::now() >= t => {
                self.fail_count = 1; // 保持警戒:再 2 次失败就又熔断
                self.recover_at = None;
                true
            }
            Some(_) => false,
            None => true,
        }
    }

    /// 给管理后台看的状态快照(纯数据)。
    pub fn snapshot(&self) -> Value {
        let now = Instant::now();
        let (broken, remaining_ms) = match self.recover_at {
            Some(t) if t > now => (true, t.saturating_duration_since(now).as_millis() as u64),
            _ => (false, 0),
        };
        json!({
            "fail_count": self.fail_count,
            "threshold": BREAKER_FAILS,
            "is_broken": broken,
            "recover_remaining_ms": remaining_ms,
            "window_secs": BREAKER_WINDOW.as_secs(),
        })
    }
}

/// 全局共享状态。`Arc<AppState>` 注入所有 handler。
pub struct AppState {
    pub config: ArcSwap<Config>,
    pub routing: ArcSwap<crate::routing::Routing>,
    pub keys: DashMap<String, KeyEntry>,
    pub users: DashMap<Uuid, Arc<UserState>>,
    pub http: reqwest::Client,
    pub db: crate::storage::Db,
    pub usage_tx: mpsc::Sender<UsageEvent>,
    /// 带 TTL 的缓存(内存或 Redis):OAuth CSRF state、邮箱验证码。
    pub cache: crate::cache::Cache,
    /// 语义缓存 L1(精确哈希 + TTL,进程内):响应缓存与命中统计。
    pub semantic_cache: crate::semantic_cache::SemanticCache,
    /// ---------- 上游治理 ----------
    /// 每个上游连接的并发槽(排队信号量)。0 容量 = 不限(跳过获取)。
    pub upstream_slots: DashMap<UpstreamKey, Arc<Semaphore>>,
    /// 每个上游的熔断器状态。
    pub upstream_breakers: DashMap<UpstreamKey, Arc<AsyncMutex<Breaker>>>,
    /// ---------- 组级负载策略的运行态 ----------
    /// 轮询游标(加权轮询 / 简单轮询用):group_id -> 自增计数。跨路由热替换存活。
    pub round_robin: DashMap<i64, std::sync::atomic::AtomicU32>,
    /// 高峰/低谷时段判断用的时区偏移秒数(Asia/Shanghai → 28800;DST 感知)。
    pub tz_offset_secs: i32,
    /// ---------- 路由失败审计(ring buffer,最新 push_back) ----------
    pub audit_failures: AsyncMutex<VecDeque<FailureRecord>>,
    /// ---------- 接口指标(内存滚动窗口) ----------
    pub metrics: MetricsStore,
    /// ---------- 请求链路日志(存储后端抽象:默认 SQLite,预留 ES) ----------
    pub request_log: Arc<dyn crate::reqlog::RequestLogStore>,
    /// 请求链路异步落库 channel(避免热路径阻塞)。
    pub request_log_tx: mpsc::Sender<crate::reqlog::RequestLog>,
}

impl AppState {
    pub fn config(&self) -> arc_swap::Guard<Arc<Config>> {
        self.config.load()
    }

    pub fn user(&self, id: &Uuid) -> Option<Arc<UserState>> {
        self.users.get(id).map(|e| Arc::clone(e.value()))
    }

    // ======================== P1:上游并发槽 ========================

    /// 按「provider 名」查找并发上限:
    /// ProviderConn.concurrency → Defaults.upstream_concurrency → 32。
    fn resolve_permits(&self, provider_name: &str) -> u32 {
        let default_limit = self.config.load().defaults.upstream_concurrency;
        let from_provider = self
            .routing
            .load()
            .providers
            .get(provider_name)
            .and_then(|p| p.concurrency);
        from_provider.unwrap_or(default_limit)
    }

    /// 获取一个上游并发槽(返回 OwnedPermit,自动释放)。
    /// permits==0 时直接返回 None(不排队,相当于不限)。
    pub async fn acquire_upstream(
        &self,
        key: UpstreamKey,
        provider_name: &str,
    ) -> Option<OwnedSemaphorePermit> {
        let permits = self.resolve_permits(provider_name);
        if permits == 0 {
            return None;
        }
        let sem = self
            .upstream_slots
            .entry(key)
            .or_insert_with(|| Arc::new(Semaphore::new(permits as usize)))
            .clone();
        // 若 permits 后来调大,简单方案:重新包装成新容量。这里暂不支持热调(和用户并发一致)。
        // 拿 OwnedSemaphorePermit:permit 被 drop 时自动归还槽位。
        match sem.acquire_owned().await {
            Ok(p) => Some(p),
            Err(_) => None, // Semaphore 不会 close,理论上不会到这里
        }
    }

    // ======================== P2:上游熔断器 ========================

    /// 检查一个上游是否被熔断(调用者在选候选前先问,熔断就跳过)。
    /// 若刚到熔断窗末尾,会自动进入「半开」状态(放一个请求探活)。
    pub async fn breaker_should_skip(&self, key: &UpstreamKey) -> bool {
        let breaker = self
            .upstream_breakers
            .entry(key.clone())
            .or_insert_with(|| Arc::new(AsyncMutex::new(Breaker::default())))
            .clone();
        let mut b = breaker.lock().await;
        if b.is_available() {
            return false;
        }
        // 熔断窗中?但可能时间到了,试试半开
        !b.maybe_half_open()
    }

    /// 该上游调用成功:熔断器清零。
    pub async fn breaker_success(&self, key: &UpstreamKey) {
        let breaker = self
            .upstream_breakers
            .entry(key.clone())
            .or_insert_with(|| Arc::new(AsyncMutex::new(Breaker::default())))
            .clone();
        breaker.lock().await.record_success();
    }

    /// 该上游返回 Unavailable:累加熔断计数,返回「累加后的 fail_count」,供审计链路判断是否触发熔断。
    pub async fn breaker_unavailable(&self, key: &UpstreamKey) -> u8 {
        let breaker = self
            .upstream_breakers
            .entry(key.clone())
            .or_insert_with(|| Arc::new(AsyncMutex::new(Breaker::default())))
            .clone();
        let mut g = breaker.lock().await;
        g.record_unavailable();
        g.fail_count
    }

    // ======================== 管理:上游治理快照 ========================

    /// 返回所有已被访问过的上游连接的运行时状态(给管理后台仪表盘用)。
    /// 包含:连接标识(kind/base_url/key 指纹)、熔断器状态、并发槽占用率。
    pub async fn list_upstream_status(&self) -> Value {
        use std::collections::HashMap;
        // 1) 先把所有见过的 key 收集到一个集合(并发槽 + 熔断器的并集)
        let mut keys: HashMap<UpstreamKey, ()> = HashMap::new();
        for e in self.upstream_slots.iter() {
            keys.insert(e.key().clone(), ());
        }
        for e in self.upstream_breakers.iter() {
            keys.insert(e.key().clone(), ());
        }
        // 2) 并发上限配置(按 UpstreamKey 查)
        let routing = self.routing.load();
        let default_limit = self.config.load().defaults.upstream_concurrency;
        let mut limit_by_conn: HashMap<UpstreamKey, u32> = HashMap::new();
        for (_, conn) in routing.providers.iter() {
            let k = UpstreamKey::new(conn.kind, &conn.base_url, conn.api_key.as_deref());
            limit_by_conn
                .entry(k)
                .or_insert(conn.concurrency.unwrap_or(default_limit));
        }

        let mut items: Vec<Value> = Vec::new();
        for key in keys.keys() {
            let limit = limit_by_conn.get(key).copied().unwrap_or(default_limit);
            // 熔断器状态
            let breaker = self
                .upstream_breakers
                .entry(key.clone())
                .or_insert_with(|| Arc::new(AsyncMutex::new(Breaker::default())))
                .clone();
            let breaker_snap = breaker.lock().await.snapshot();

            // 并发槽状态
            let (permits_total, in_flight) = match self.upstream_slots.get(key) {
                Some(s) => {
                    let avail = s.value().available_permits() as u32;
                    (limit, limit.saturating_sub(avail))
                }
                None => (limit, 0),
            };

            items.push(json!({
                "kind": match key.kind {
                    ProviderKind::Openai => "openai",
                    ProviderKind::Anthropic => "anthropic",
                },
                "base_url": key.base_url,
                "has_key": key.key_fingerprint.is_some(),
                "key_fingerprint": key.key_fingerprint.map(|f| format!("{:016x}", f)).unwrap_or_default(),
                "breaker": breaker_snap,
                "concurrency": {
                    "limit": permits_total,
                    "in_flight": in_flight,
                    "utilization_pct": if permits_total == 0 { 0 } else { (in_flight as u64 * 100) / permits_total as u64 },
                },
            }));
        }
        // 稳定输出:先按 is_broken 降序(熔断的放最上面),再按 base_url 排序
        items.sort_by(|a, b| {
            let broken_a = a.pointer("/breaker/is_broken").and_then(|v| v.as_bool()).unwrap_or(false);
            let broken_b = b.pointer("/breaker/is_broken").and_then(|v| v.as_bool()).unwrap_or(false);
            broken_b.cmp(&broken_a).then_with(|| {
                let ua = a.get("base_url").and_then(|v| v.as_str()).unwrap_or("");
                let ub = b.get("base_url").and_then(|v| v.as_str()).unwrap_or("");
                ua.cmp(ub)
            })
        });
        json!({
            "total": items.len(),
            "broken_count": items.iter().filter(|x| x.pointer("/breaker/is_broken").and_then(|v| v.as_bool()).unwrap_or(false)).count(),
            "items": items,
        })
    }

    // ======================== 失败审计 ========================

    /// 记录一次上游 Unavailable 级失败到审计 ring buffer(最新在尾部)。
    /// **该方法内部绝对不 panic / return Err**——审计失败绝不影响主链路。
    ///
    /// - `key`: 上游 Key(构造 kind/base_url/fp 用)
    /// - `requested`: 用户请求的对外模型名(若已知)
    /// - `path`: 哪条链路,建议传 "chat" 或 "messages"
    /// - `error_summary`: 错误摘要(自动截断到 300 字)
    /// - `fail_count_after`: 更新后的失败计数(从 Breaker 读完后传进来,避免再锁一次)
    pub async fn record_failure(
        &self,
        key: &UpstreamKey,
        requested: &str,
        path: &'static str,
        error_summary: &str,
        fail_count_after: u8,
    ) {
        use std::time::{SystemTime, UNIX_EPOCH};
        // 构造记录:任何一步出错都要吞掉,安全返回
        let kind_s = match key.kind {
            ProviderKind::Openai => "openai",
            ProviderKind::Anthropic => "anthropic",
        };
        let fp_hex = key
            .key_fingerprint
            .map(|f| format!("{:016x}", f))
            .unwrap_or_default();
        let ts_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);
        // 错误摘要最多 300 个 UTF-16 code unit,避免单个记录过长
        let summary_truncated: String = error_summary.chars().take(300).collect();
        let upstream_id = format!("{}|{}|{}", kind_s, key.base_url, fp_hex);
        let triggered = fail_count_after >= BREAKER_FAILS;

        let rec = FailureRecord {
            ts_ms,
            kind: kind_s,
            base_url: key.base_url.clone(),
            key_fingerprint: fp_hex,
            upstream_id,
            requested_model: requested.to_string(),
            path,
            error_summary: summary_truncated,
            fail_count_after,
            threshold: BREAKER_FAILS,
            triggered_break: triggered,
        };

        let mut guard = self.audit_failures.lock().await;
        if guard.len() >= AUDIT_FAILURE_CAP {
            guard.pop_front();
        }
        guard.push_back(rec);
    }

    /// 取最近 `limit` 条失败审计(按时间**倒序**返回——最新在最前,默认 50,最大 200)。
    pub async fn list_failures(&self, limit: Option<usize>) -> Value {
        let cap = limit.unwrap_or(50).clamp(1, 200);
        let guard = self.audit_failures.lock().await;
        // 尾部 = 最新,倒序取 cap 个
        let items: Vec<&FailureRecord> = guard.iter().rev().take(cap).collect();
        json!({
            "stored": guard.len(),
            "capacity": AUDIT_FAILURE_CAP,
            "items": items
        })
    }
}

// ====================================================================
// · 接口指标(内存滚动窗口) · 分层桶:秒级(120s) + 分桶(48h)
// ====================================================================

/// 请求成功:任何 2xx。
pub const METRIC_STATUS_OK: u16 = 200;
/// 路由转移/上游不可用:429/5xx/断链。
pub const METRIC_STATUS_UNAVAILABLE: u16 = 503;

/// 单桶单组聚合。所有字段都按「追加」语义更新。
#[derive(Clone, Debug, Default)]
pub struct MetricsBucket {
    pub req: u64,
    pub success: u64,
    pub fail_unavail: u64,
    pub fail_other: u64,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub latency_sum_ms: u64,
    pub latency_max_ms: u64,
    /// 近似分位样本:200 条滑窗内"均匀覆盖"
    pub samples: Vec<u32>,
}

const METRIC_SAMPLE_PER_BUCKET: usize = 200;
pub const METRIC_SEC_WINDOW: usize = 120;
pub const METRIC_MIN_WINDOW: usize = 2880; // 48 * 60

impl MetricsBucket {
    fn push(&mut self, latency_ms: u32, is_ok: bool, itk: u64, otk: u64) {
        self.req += 1;
        if is_ok {
            self.success += 1;
            self.latency_sum_ms += latency_ms as u64;
            if (latency_ms as u64) > self.latency_max_ms { self.latency_max_ms = latency_ms as u64; }
            // 延迟样本只记录成功请求,失败(0/无效)不参与分位和均值计算
            if self.samples.len() < METRIC_SAMPLE_PER_BUCKET {
                self.samples.push(latency_ms);
            } else {
                let i = (self.success as usize) % METRIC_SAMPLE_PER_BUCKET;
                self.samples[i] = latency_ms;
            }
        } else {
            // 默认按"其它失败"记(一般是 4xx/5xx/超时)。
            // 如果需要标记为 fail_unavail(上游不可用/熔断转移),调用方用 push_unavail。
            self.fail_other += 1;
        }
        self.input_tokens += itk;
        self.output_tokens += otk;
    }

    /// push,但把这次失败(is_ok=false)记为 fail_unavail(上游不可用/熔断转移)。
    fn push_unavail(&mut self, latency_ms: u32, itk: u64, otk: u64) {
        self.push(latency_ms, false, itk, otk);
        // push 里 is_ok=false 时 fail_other+=1,我们把它转移到 fail_unavail
        self.fail_other -= 1;
        self.fail_unavail += 1;
    }

    pub fn avg_ms(&self) -> f64 {
        if self.success == 0 { return 0.0; }
        (self.latency_sum_ms as f64) / (self.success as f64)
    }

    pub fn p(&self, pct: f64) -> u32 {
        if self.samples.is_empty() { return 0; }
        let mut s: Vec<u32> = self.samples.clone();
        s.sort_unstable();
        let n = s.len();
        let idx = ((n as f64 - 1.0) * pct.clamp(0.0, 1.0)).round() as usize;
        s[idx.min(n - 1)]
    }

    fn merge(&mut self, other: &MetricsBucket) {
        self.req += other.req;
        self.success += other.success;
        self.fail_unavail += other.fail_unavail;
        self.fail_other += other.fail_other;
        self.input_tokens += other.input_tokens;
        self.output_tokens += other.output_tokens;
        self.latency_sum_ms += other.latency_sum_ms;
        if other.latency_max_ms > self.latency_max_ms { self.latency_max_ms = other.latency_max_ms; }
        let mut merged: Vec<u32> = Vec::with_capacity(self.samples.len() + other.samples.len());
        merged.extend_from_slice(&self.samples);
        merged.extend_from_slice(&other.samples);
        if merged.len() > METRIC_SAMPLE_PER_BUCKET {
            let drop = merged.len() - METRIC_SAMPLE_PER_BUCKET;
            merged = merged.split_off(drop);
        }
        self.samples = merged;
    }
}

#[derive(Clone, Debug, Default)]
struct TimeBuckets {
    secs: std::collections::VecDeque<(u64, MetricsBucket)>,
    mins: std::collections::VecDeque<(u64, MetricsBucket)>,
}

impl TimeBuckets {
    fn push(&mut self, latency_ms: u32, is_ok: bool, unavailable: bool, itk: u64, otk: u64) {
        use std::time::{SystemTime, UNIX_EPOCH};
        let now_secs = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
        let sec_bucket_key = now_secs;
        let min_bucket_key = now_secs / 60 * 60;

        if self.secs.back().map(|(k, _)| *k != sec_bucket_key).unwrap_or(true) {
            self.secs.push_back((sec_bucket_key, MetricsBucket::default()));
            while self.secs.len() > METRIC_SEC_WINDOW { self.secs.pop_front(); }
        }
        if let Some((_, b)) = self.secs.back_mut() {
            if !is_ok && unavailable {
                b.push_unavail(latency_ms, itk, otk);
            } else {
                b.push(latency_ms, is_ok, itk, otk);
            }
        }

        if self.mins.back().map(|(k, _)| *k != min_bucket_key).unwrap_or(true) {
            self.mins.push_back((min_bucket_key, MetricsBucket::default()));
            while self.mins.len() > METRIC_MIN_WINDOW { self.mins.pop_front(); }
        }
        if let Some((_, b)) = self.mins.back_mut() {
            if !is_ok && unavailable {
                b.push_unavail(latency_ms, itk, otk);
            } else {
                b.push(latency_ms, is_ok, itk, otk);
            }
        }
    }

    /// 只补记 tokens(不影响 req / success / samples —— 对成功率和延迟指标零影响)。
    fn push_tokens_only(&mut self, itk: u64, otk: u64) {
        use std::time::{SystemTime, UNIX_EPOCH};
        let now_secs = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
        let sec_bucket_key = now_secs;
        let min_bucket_key = now_secs / 60 * 60;

        if self.secs.back().map(|(k, _)| *k != sec_bucket_key).unwrap_or(true) {
            self.secs.push_back((sec_bucket_key, MetricsBucket::default()));
            while self.secs.len() > METRIC_SEC_WINDOW { self.secs.pop_front(); }
        }
        if let Some((_, b)) = self.secs.back_mut() {
            b.input_tokens += itk;
            b.output_tokens += otk;
        }

        if self.mins.back().map(|(k, _)| *k != min_bucket_key).unwrap_or(true) {
            self.mins.push_back((min_bucket_key, MetricsBucket::default()));
            while self.mins.len() > METRIC_MIN_WINDOW { self.mins.pop_front(); }
        }
        if let Some((_, b)) = self.mins.back_mut() {
            b.input_tokens += itk;
            b.output_tokens += otk;
        }
    }
}

#[derive(Default)]
pub struct MetricsStore {
    inner: DashMap<Option<UpstreamKey>, Arc<AsyncMutex<TimeBuckets>>>,
}

impl MetricsStore {
    fn get_bucket(&self, key: Option<UpstreamKey>) -> Arc<AsyncMutex<TimeBuckets>> {
        self.inner
            .entry(key)
            .or_insert_with(|| Arc::new(AsyncMutex::new(TimeBuckets::default())))
            .clone()
    }

    async fn push_tokens_only(&self, key: Option<UpstreamKey>, itk: u64, otk: u64) {
        let b = self.get_bucket(key);
        b.lock().await.push_tokens_only(itk, otk);
    }
}

/// 从 TimeBuckets 聚合指定窗口:返回 (合并总量, 原始 series)。use_sec=true 走秒桶,否则走分钟桶。
async fn aggregate_timebuckets(
    buckets: Arc<AsyncMutex<TimeBuckets>>,
    since_min: u64,
    since_sec: u64,
    use_sec: bool,
) -> (MetricsBucket, Vec<(u64, MetricsBucket)>) {
    let g = buckets.lock().await;
    let mut total = MetricsBucket::default();
    let series: Vec<(u64, MetricsBucket)> = if use_sec {
        g.secs
            .iter()
            .filter(|(ts, _)| *ts >= since_sec)
            .map(|(ts, b)| {
                total.merge(b);
                (*ts, b.clone())
            })
            .collect()
    } else {
        g.mins
            .iter()
            .filter(|(ts, _)| *ts >= since_min)
            .map(|(ts, b)| {
                total.merge(b);
                (*ts, b.clone())
            })
            .collect()
    };
    (total, series)
}

// 给 AppState 加 record_metrics / query_metrics
impl AppState {
    /// 观测一次请求结果。任何情况绝不抛异常。
    /// - `status`:建议传 METRIC_STATUS_OK(200) 或 METRIC_STATUS_UNAVAILABLE(503)。
    /// - `latency`:请求耗时。
    /// - `input/output_tokens`:成功时传真实值,失败时传 0。
    pub async fn record_metrics(
        &self,
        key: Option<&UpstreamKey>,
        status: u16,
        latency: Duration,
        input_tokens: u64,
        output_tokens: u64,
    ) {
        let ms = latency.as_millis().min(u32::MAX as u128) as u32;
        let is_ok = (200..300).contains(&status);
        // 503 = 上游不可用(熔断转移);其它非 2xx = 其它失败(4xx/5xx/超时)
        let unavailable = status == METRIC_STATUS_UNAVAILABLE;
        // 1) 写全局桶
        {
            let gb = self.metrics.get_bucket(None);
            gb.lock().await.push(ms, is_ok, unavailable, input_tokens, output_tokens);
        }
        // 2) 写上游分桶
        if let Some(k) = key {
            let ub = self.metrics.get_bucket(Some(k.clone()));
            ub.lock().await.push(ms, is_ok, unavailable, input_tokens, output_tokens);
        }
    }

    /// 仅记录一次请求计数+延迟(不含 tokens,用于外层 catch-all 成功/失败)。
    pub async fn record_metrics_request(
        &self,
        key: Option<&UpstreamKey>,
        status: u16,
        latency: Duration,
    ) {
        self.record_metrics(key, status, latency, 0, 0).await;
    }

    /// 仅累加 tokens(不增加 req 计数/不写入延迟样本,用于 charge 函数结算处补记 TPM 数据)。
    pub async fn record_metrics_tokens(
        &self,
        key: Option<&UpstreamKey>,
        input_tokens: u64,
        output_tokens: u64,
    ) {
        // 全局桶
        self.metrics
            .push_tokens_only(None, input_tokens, output_tokens)
            .await;
        // 分桶
        if let Some(k) = key {
            self.metrics
                .push_tokens_only(Some(k.clone()), input_tokens, output_tokens)
                .await;
        }
    }

    /// 按时间范围合并桶,返回仪表盘 JSON。
    /// - `range_secs`:0 = 取全部(mins 里的,最多 48h)。否则取最近 N 秒(最小 60s)。
    /// - `top_n_upstream`:按总请求数取前 N 个上游(默认 20)。
    pub async fn query_metrics(&self, range_secs: Option<u64>, top_n_upstream: Option<usize>) -> Value {
        use std::time::{SystemTime, UNIX_EPOCH};
        let now = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
        // None / Some(0) 语义一致:窗口 = "all"(即分钟桶全量,最大 48h)
        let is_all_range = range_secs.is_none() || matches!(range_secs, Some(0));
        let since_minute_bucket = if is_all_range {
            0
        } else {
            let s = range_secs.unwrap();
            (now - s.max(60)) / 60 * 60
        };
        // 秒桶仅用于"明确短窗口"(>0 且 ≤ 240s)。None/0 都走分钟桶全量。
        let use_secs = !is_all_range
            && range_secs.map(|s| s <= METRIC_SEC_WINDOW as u64 * 2).unwrap_or(false);
        let since_sec_bucket = now
            - if is_all_range {
                0
            } else {
                range_secs.unwrap().min(METRIC_SEC_WINDOW as u64)
            };

        // 实际窗口(秒):用于 RPS / TPM 归一化
        let effective_range_secs: u64 = if use_secs {
            range_secs.unwrap_or(METRIC_SEC_WINDOW as u64).min(METRIC_SEC_WINDOW as u64).max(1)
        } else {
            if is_all_range {
                (METRIC_MIN_WINDOW as u64 * 60).max(60)
            } else {
                range_secs.unwrap().max(60)
            }
        };

        let top_n = top_n_upstream.unwrap_or(20).clamp(1, 100);

        // 1) 全局汇总+时序
        let (global_total, global_series) = aggregate_timebuckets(
            self.metrics.get_bucket(None),
            since_minute_bucket,
            since_sec_bucket,
            use_secs,
        )
        .await;

        // 2) 上游 Top N
        // 注意:iter() 时持有 per-shard RLock;get_bucket() 内部用 entry()=WLock,同 shard 会死锁。
        // 所以先把所有有数据的 UpstreamKey 收集出来(释放 iter),再单独聚合。
        let mut candidate_keys: Vec<UpstreamKey> = Vec::new();
        for entry in self.metrics.inner.iter() {
            let (k, _) = entry.pair();
            let Some(up_key) = k else { continue };
            candidate_keys.push(up_key.clone());
        }
        // (退出 for 后 entry 已销毁,DashMap 各 shard RLock 已释放)

        let mut per_up = Vec::new();
        for up_key in candidate_keys {
            let (total, _) = aggregate_timebuckets(
                self.metrics.get_bucket(Some(up_key.clone())),
                since_minute_bucket,
                since_sec_bucket,
                use_secs,
            )
            .await;
            if total.req == 0 { continue; }
            let fp_hex = up_key
                .key_fingerprint
                .map(|f| format!("{:016x}", f))
                .unwrap_or_default();
            let id = format!(
                "{}|{}|{}",
                match up_key.kind {
                    ProviderKind::Openai => "openai",
                    ProviderKind::Anthropic => "anthropic",
                },
                up_key.base_url,
                fp_hex
            );
            let ok_rate = if total.req == 0 { 0.0 } else { (total.success as f64) / (total.req as f64) };
            let tpm_total = total.input_tokens.saturating_add(total.output_tokens);
            per_up.push(json!({
                "upstream_id": id,
                "kind": match up_key.kind {
                    ProviderKind::Openai => "openai",
                    ProviderKind::Anthropic => "anthropic",
                },
                "base_url": up_key.base_url,
                "key_fingerprint": fp_hex,
                "requests": total.req,
                "success": total.success,
                "fail_unavailable": total.fail_unavail,
                "success_rate": ok_rate,
                "tpm": tpm_total,
                "input_tokens": total.input_tokens,
                "output_tokens": total.output_tokens,
                "avg_ms": total.avg_ms(),
                "p50_ms": total.p(0.50),
                "p95_ms": total.p(0.95),
                "p99_ms": total.p(0.99),
                "max_ms": total.latency_max_ms,
            }));
        }
        per_up.sort_by(|a, b| {
            let ra = a.get("requests").and_then(|v| v.as_u64()).unwrap_or(0);
            let rb = b.get("requests").and_then(|v| v.as_u64()).unwrap_or(0);
            rb.cmp(&ra)
        });
        let per_up_truncated: Vec<Value> = per_up.into_iter().take(top_n).collect();

        // 3) series -> 数组 JSON(字段与前端 MetricsSeriesPoint 一一对应)
        let series_values: Vec<Value> = global_series
            .into_iter()
            .map(|(ts, b)| {
                let fail_other = b.req.saturating_sub(b.success).saturating_sub(b.fail_unavail);
                json!({
                    "ts": ts,
                    "requests": b.req,
                    "success": b.success,
                    "fail_unavailable": b.fail_unavail,
                    "fail_other": fail_other,
                    "input_tokens": b.input_tokens,
                    "output_tokens": b.output_tokens,
                    "tpm": b.input_tokens.saturating_add(b.output_tokens),
                    "avg_ms": b.avg_ms(),
                    "p50_ms": b.p(0.50),
                    "p95_ms": b.p(0.95),
                    "p99_ms": b.p(0.99),
                })
            })
            .collect();

        // 4) 汇总 summary(字段与前端 MetricsSummary 一一对应)
        let total_req = global_total.req;
        let success_rate = if total_req == 0 { 0.0 } else { (global_total.success as f64) / (total_req as f64) };
        let rps = (total_req as f64) / (effective_range_secs as f64);
        let tokens_total = global_total.input_tokens.saturating_add(global_total.output_tokens);
        // TPM = tokens_total / range_secs * 60
        let tpm = (tokens_total as f64) / (effective_range_secs as f64) * 60.0;
        let fail_other = total_req.saturating_sub(global_total.success).saturating_sub(global_total.fail_unavail);

        json!({
            "range_secs": effective_range_secs,
            "sampled_at": now,
            "granularity": if use_secs { "second" } else { "minute" },
            "summary": {
                "range_secs": effective_range_secs,
                "sampled_at": now,
                "requests": total_req,
                "success_rate": success_rate,
                "rps": rps,
                "tpm": tpm,
                "input_tokens": global_total.input_tokens,
                "output_tokens": global_total.output_tokens,
                "fail_unavailable": global_total.fail_unavail,
                "fail_other": fail_other,
                "avg_ms": global_total.avg_ms(),
                "p50_ms": global_total.p(0.50),
                "p95_ms": global_total.p(0.95),
                "p99_ms": global_total.p(0.99),
            },
            "series": series_values,
            "upstreams": per_up_truncated,
        })
    }

    /// 供应商健康总览(模型页徽标数据源):按内存路由表逐个供应商聚合窗口内指标 + 熔断快照。
    /// 全部读内存(MetricsStore 分钟桶 + Breaker),无 SQL;进程重启后统计从零累积。
    /// - `range_secs`:统计窗口,默认 1h,范围 [60s, 48h]。
    pub async fn provider_health(&self, range_secs: Option<u64>) -> Value {
        use std::time::{SystemTime, UNIX_EPOCH};
        let now = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
        let range = range_secs.unwrap_or(3600).clamp(60, METRIC_MIN_WINDOW as u64 * 60);
        let since_min = now.saturating_sub(range) / 60 * 60;

        let routing = self.routing.load();
        let mut items: Vec<(u8, Value)> = Vec::new(); // (状态优先级, item),数值小者排前
        for (name, conn) in routing.providers.iter() {
            let key = UpstreamKey::new(conn.kind, &conn.base_url, conn.api_key.as_deref());
            let (total, _) = aggregate_timebuckets(
                self.metrics.get_bucket(Some(key.clone())),
                since_min,
                0,
                false,
            )
            .await;

            // 熔断快照:先 clone Arc 并释放 DashMap 读守卫,再 await 内部锁,避免锁交叉
            let breaker_snap = match self.upstream_breakers.get(&key) {
                Some(e) => {
                    let b = e.value().clone();
                    drop(e);
                    let snap = b.lock().await.snapshot();
                    snap
                }
                None => Breaker::default().snapshot(),
            };
            let is_broken = breaker_snap.pointer("/is_broken").and_then(|v| v.as_bool()).unwrap_or(false);

            let ok_rate = if total.req == 0 { 0.0 } else { (total.success as f64) / (total.req as f64) };
            // 状态推导:broken(熔断中) > down(窗口内全失败) > degraded(有失败) > ok(全成功) > idle(无请求)
            let status = if is_broken {
                "broken"
            } else if total.req == 0 {
                "idle"
            } else if total.success == 0 {
                "down"
            } else if total.success < total.req {
                "degraded"
            } else {
                "ok"
            };
            let prio: u8 = match status {
                "broken" => 0,
                "down" => 1,
                "degraded" => 2,
                "ok" => 3,
                _ => 4,
            };

            let fp_hex = key
                .key_fingerprint
                .map(|f| format!("{:016x}", f))
                .unwrap_or_default();
            let item = json!({
                "provider": name,
                "kind": match key.kind {
                    ProviderKind::Openai => "openai",
                    ProviderKind::Anthropic => "anthropic",
                },
                "base_url": key.base_url,
                "key_fingerprint": fp_hex,
                "requests": total.req,
                "success": total.success,
                "fail_unavailable": total.fail_unavail,
                "fail_other": total.req.saturating_sub(total.success).saturating_sub(total.fail_unavail),
                "success_rate": ok_rate,
                "avg_ms": total.avg_ms(),
                "p95_ms": total.p(0.95),
                "p99_ms": total.p(0.99),
                "status": status,
                "breaker": breaker_snap,
            });
            items.push((prio, item));
        }
        // 排序:状态差者靠前(熔断/异常先看到),同级按供应商名
        items.sort_by(|a, b| {
            a.0.cmp(&b.0).then_with(|| {
                let na = a.1.get("provider").and_then(|v| v.as_str()).unwrap_or("");
                let nb = b.1.get("provider").and_then(|v| v.as_str()).unwrap_or("");
                na.cmp(nb)
            })
        });
        let items: Vec<Value> = items.into_iter().map(|(_, v)| v).collect();

        json!({
            "range_secs": range,
            "sampled_at": now,
            "items": items,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::ProviderKind;

    // --- 白盒 1:MetricsBucket 计数与分位 ---
    #[test]
    fn bucket_push_and_percentiles() {
        let mut b = MetricsBucket::default();
        // 10 次成功,延迟:10,20,30,40,50,60,70,80,90,100 ms
        for i in 1..=10 {
            b.push(i * 10, true, (i * 100) as u64, (i * 50) as u64);
        }
        // 3 次失败(1 次不可用 + 2 次其它)
        // 注意 push(is_ok=false) 默认把 fail_other+=1;我们手动把其中 1 次挪到 fail_unavail
        b.push(0, false, 0, 0);
        b.push(0, false, 0, 0);
        b.push(0, false, 0, 0);
        // 把 1 个失败从 fail_other 转到 fail_unavail
        b.fail_other -= 1;
        b.fail_unavail += 1;

        assert_eq!(b.req, 13);
        assert_eq!(b.success, 10);
        assert_eq!(b.fail_unavail + b.fail_other, 3);
        assert_eq!(b.fail_unavail, 1);
        assert_eq!(b.fail_other, 2);

        // tokens: Σ i*100 (1..=10) = 5500 in, Σ i*50 = 2750 out
        assert_eq!(b.input_tokens, 5500);
        assert_eq!(b.output_tokens, 2750);

        // 样本数=成功数
        assert_eq!(b.samples.len(), 10);
        assert_eq!(b.avg_ms(), 55.0);
        // 实现采用「最近邻舍入」,非线性插值:
        // sorted samples [10,20,30,40,50,60,70,80,90,100]  n=10
        // P50: idx = ((10-1)*0.5).round() = 5  → s[5]=60
        assert_eq!(b.p(0.5), 60);
        // P95: idx = (9*0.95).round() = 9  → s[9]=100
        assert_eq!(b.p(0.95), 100);
        // P99: idx = (9*0.99).round() = 9  → s[9]=100
        assert_eq!(b.p(0.99), 100);
        assert_eq!(b.latency_max_ms, 100);
    }

    // --- 白盒 2:merge 汇总 ---
    #[test]
    fn bucket_merge() {
        let mut a = MetricsBucket::default();
        a.push(10, true, 100, 200);
        a.push(20, false, 0, 0);
        // 把这次失败记为 unavailable
        a.fail_other -= 1;
        a.fail_unavail += 1;
        let mut b = MetricsBucket::default();
        b.push(30, true, 400, 500);
        b.push(5, false, 0, 0); // 默认 fail_other
        a.merge(&b);
        assert_eq!(a.req, 4);
        assert_eq!(a.success, 2);
        assert_eq!(a.fail_unavail, 1);
        assert_eq!(a.fail_other, 1);
        assert_eq!(a.input_tokens, 500);
        assert_eq!(a.output_tokens, 700);
        // sorted samples: [10, 30], n=2
        // P50: idx = (1*0.5).round() = 1 → s[1]=30
        assert_eq!(a.p(0.5), 30);
        assert_eq!(a.latency_max_ms, 30);
    }

    // --- 周期预算:累计 / 耗尽 / 滚动重置 ---
    #[test]
    fn budget_daily_exhausted() {
        let u = UserState::new(Uuid::new_v4(), 1_000_000, 0, 0, 1.0, 0);
        u.set_budgets(1000, 0);
        assert_eq!(u.budget_status(8), BudgetStatus::Ok);
        u.record_budget(600, 8);
        assert_eq!(u.budget_status(8), BudgetStatus::Ok);
        u.record_budget(400, 8);
        assert_eq!(u.budget_status(8), BudgetStatus::DailyExhausted);
    }

    #[test]
    fn budget_monthly_exhausted() {
        let u = UserState::new(Uuid::new_v4(), 1_000_000, 0, 0, 1.0, 0);
        u.set_budgets(0, 1500);
        u.record_budget(1500, 8);
        assert_eq!(u.budget_status(8), BudgetStatus::MonthlyExhausted);
    }

    #[test]
    fn budget_window_rollover_resets_counters() {
        let u = UserState::new(Uuid::new_v4(), 1_000_000, 0, 0, 1.0, 0);
        u.set_budgets(1000, 2000);
        u.record_budget(1000, 8);
        assert_eq!(u.budget_status(8), BudgetStatus::DailyExhausted);

        // 只换日(day_key+1,月键不变) → used_daily 归 0,used_monthly 保留
        let day0 = u.day_key.load(Ordering::Relaxed);
        let month0 = u.month_key.load(Ordering::Relaxed);
        u.roll_windows_keys(day0 + 1, month0);
        assert_eq!(u.used_daily.load(Ordering::Relaxed), 0);
        assert_eq!(u.used_monthly.load(Ordering::Relaxed), 1000);
        assert_eq!(u.budget_status(8), BudgetStatus::Ok);

        // 再换月(month_key+1) → used_monthly 也归 0
        u.roll_windows_keys(day0 + 1, month0 + 1);
        assert_eq!(u.used_monthly.load(Ordering::Relaxed), 0);

        // 滚动后重新累计正常
        u.record_budget(300, 8);
        assert_eq!(u.used_monthly.load(Ordering::Relaxed), 300);
        assert_eq!(u.budget_status(8), BudgetStatus::Ok);
    }

    // --- 白盒 3:MetricsStore push_tokens_only 不污染 req/samples ---
    #[tokio::test]
    async fn metrics_store_push_tokens_only_is_side_effect_free() {
        let store = MetricsStore::default();
        let key = UpstreamKey::new(ProviderKind::Openai, "http://x", Some("k"));
        // 先一条请求:200 ms 成功(直接通过 TimeBuckets.push,因为 MetricsStore 对外不暴露 push)
        {
            let b = store.get_bucket(Some(key.clone()));
            b.lock().await.push(200, true, false, 0, 0);
        }
        {
            let b = store.get_bucket(None);
            b.lock().await.push(200, true, false, 0, 0);
        }
        // 再补记 tokens
        store.push_tokens_only(Some(key.clone()), 1234, 5678).await;
        store.push_tokens_only(None, 1234, 5678).await;
        // 全局桶(只用 mins,避免 secs+mins 双写重复 —— query_metrics 也是单取 secs 或 mins)
        let g = store.get_bucket(None);
        let gl = g.lock().await;
        let mut total = MetricsBucket::default();
        for (_, b) in gl.mins.iter() { total.merge(b); }
        drop(gl);
        assert_eq!(total.req, 1);
        assert_eq!(total.success, 1);
        assert_eq!(total.samples.len(), 1);
        assert_eq!(total.samples[0], 200);
        assert_eq!(total.input_tokens, 1234);
        assert_eq!(total.output_tokens, 5678);

        // 上游分桶同样
        let u = store.get_bucket(Some(key));
        let ul = u.lock().await;
        let mut ut = MetricsBucket::default();
        for (_, b) in ul.mins.iter() { ut.merge(b); }
        drop(ul);
        assert_eq!(ut.req, 1);
        assert_eq!(ut.input_tokens, 1234);
        assert_eq!(ut.output_tokens, 5678);
        assert_eq!(ut.p(0.5), 200);
    }

    // ---- Breaker 熔断器 ----

    #[test]
    fn breaker_available_by_default() {
        let b = Breaker::default();
        assert!(b.is_available());
    }

    #[test]
    fn breaker_trips_after_threshold() {
        let mut b = Breaker::default();
        for _ in 0..BREAKER_FAILS {
            b.record_unavailable();
        }
        assert!(!b.is_available());
    }

    #[test]
    fn breaker_resets_on_success() {
        let mut b = Breaker::default();
        for _ in 0..BREAKER_FAILS {
            b.record_unavailable();
        }
        assert!(!b.is_available());
        b.record_success();
        assert!(b.is_available());
    }

    #[test]
    fn breaker_half_open_after_window() {
        let mut b = Breaker::default();
        for _ in 0..BREAKER_FAILS {
            b.record_unavailable();
        }
        b.recover_at = Some(Instant::now() - std::time::Duration::from_secs(1));
        assert!(b.maybe_half_open());
        assert!(b.is_available());
    }

    // ---- provider_health 状态机推导 + 排序 ----

    #[tokio::test]
    async fn provider_health_status_machine() {
        use std::collections::HashMap;
        use std::sync::Mutex as StdMutex;

        use crate::cache::Cache;
        use crate::reqlog::SqliteRequestLogStore;
        use crate::routing::{ProviderConn, Routing};

        sqlx::any::install_default_drivers();
        let db = crate::storage::Db::connect("sqlite::memory:").await.unwrap();
        let db2 = db.clone();
        // Config 只有 server.bind / database 必填,其余全有默认,直接反序列化最小配置
        let cfg: crate::config::Config = serde_json::from_str(
            r#"{"server":{"bind":"127.0.0.1:0"},"database":{}}"#,
        )
        .unwrap();

        let conn = |kind, base_url: &str, api_key: Option<&str>| ProviderConn {
            kind,
            base_url: base_url.to_string(),
            api_key: api_key.map(|s| s.to_string()),
            concurrency: None,
        };
        let routing = Routing {
            providers: HashMap::from([
                ("prov-a".to_string(), conn(ProviderKind::Openai, "http://a", Some("ka"))),
                ("prov-b".to_string(), conn(ProviderKind::Anthropic, "http://b", None)),
            ]),
            models: HashMap::new(),
            group_names: HashMap::new(),
            group_strategy: HashMap::new(),
            time_rules: HashMap::new(),
            groups: HashMap::new(),
        };

        let (usage_tx, _usage_rx) = mpsc::channel(16);
        let (log_tx, _log_rx) = mpsc::channel(16);
        let state = Arc::new(AppState {
            config: ArcSwap::from_pointee(cfg),
            routing: ArcSwap::from_pointee(routing),
            keys: DashMap::new(),
            users: DashMap::new(),
            http: reqwest::Client::new(),
            db,
            usage_tx,
            cache: Cache::Memory(StdMutex::new(HashMap::new())),
            semantic_cache: crate::semantic_cache::SemanticCache::new(),
            upstream_slots: DashMap::new(),
            upstream_breakers: DashMap::new(),
            round_robin: DashMap::new(),
            tz_offset_secs: 28800,
            audit_failures: AsyncMutex::new(VecDeque::new()),
            metrics: MetricsStore::default(),
            request_log: Arc::new(SqliteRequestLogStore::new(db2)),
            request_log_tx: log_tx,
        });

        let key_a = UpstreamKey::new(ProviderKind::Openai, "http://a", Some("ka"));
        let key_b = UpstreamKey::new(ProviderKind::Anthropic, "http://b", None);

        // 1) 无请求 → 两个供应商都 idle;同级按名称排序
        let out = state.provider_health(Some(3600)).await;
        let items = out["items"].as_array().unwrap();
        assert_eq!(items.len(), 2);
        assert_eq!(items[0]["provider"], "prov-a");
        assert_eq!(items[0]["status"], "idle");
        assert_eq!(items[1]["provider"], "prov-b");
        assert_eq!(items[1]["status"], "idle");

        // 2) prov-a: 2 成功 + 1 失败 → degraded;prov-b 仍 idle
        state.record_metrics(Some(&key_a), 200, Duration::from_millis(100), 0, 0).await;
        state.record_metrics(Some(&key_a), 200, Duration::from_millis(200), 0, 0).await;
        state.record_metrics(Some(&key_a), 500, Duration::from_millis(50), 0, 0).await;
        let out = state.provider_health(Some(3600)).await;
        let items = out["items"].as_array().unwrap();
        let a = items.iter().find(|it| it["provider"] == "prov-a").unwrap();
        let b = items.iter().find(|it| it["provider"] == "prov-b").unwrap();
        assert_eq!(a["status"], "degraded");
        assert_eq!(a["requests"], 3);
        assert_eq!(a["success"], 2);
        assert_eq!(a["fail_other"], 1);
        assert_eq!(b["status"], "idle");

        // 3) prov-b: 全部失败 → down;且 down(degraded 之前的状态)排前面
        state.record_metrics(Some(&key_b), 502, Duration::ZERO, 0, 0).await;
        state.record_metrics(Some(&key_b), METRIC_STATUS_UNAVAILABLE, Duration::ZERO, 0, 0).await;
        let out = state.provider_health(Some(3600)).await;
        let items = out["items"].as_array().unwrap();
        assert_eq!(items[0]["provider"], "prov-b");
        assert_eq!(items[0]["status"], "down");
        assert_eq!(items[0]["requests"], 2);
        assert_eq!(items[0]["success"], 0);
        assert_eq!(items[0]["fail_unavailable"], 1);
        assert_eq!(items[0]["fail_other"], 1);
        assert_eq!(items[1]["provider"], "prov-a");
        assert_eq!(items[1]["status"], "degraded");

        // 4) prov-b 熔断器连续 3 次不可用 → broken(置顶),带恢复倒计时
        state
            .upstream_breakers
            .insert(key_b.clone(), Arc::new(AsyncMutex::new(Breaker::default())));
        {
            let e = state.upstream_breakers.get(&key_b).unwrap();
            let breaker = e.value().clone();
            drop(e);
            let mut g = breaker.lock().await;
            g.record_unavailable();
            g.record_unavailable();
            g.record_unavailable();
        }
        let out = state.provider_health(Some(3600)).await;
        let items = out["items"].as_array().unwrap();
        assert_eq!(items[0]["provider"], "prov-b");
        assert_eq!(items[0]["status"], "broken");
        assert_eq!(items[0]["breaker"]["is_broken"], true);
        assert_eq!(items[0]["breaker"]["fail_count"], BREAKER_FAILS);
        assert!(items[0]["breaker"]["recover_remaining_ms"].as_u64().unwrap() > 0);
        assert_eq!(items[1]["provider"], "prov-a");
        assert_eq!(items[1]["status"], "degraded");
    }
}