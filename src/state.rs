use std::sync::{
    atomic::{AtomicBool, AtomicI64, AtomicU32, AtomicU8, Ordering},
    Arc,
};

use arc_swap::ArcSwap;
use dashmap::DashMap;
use tokio::sync::mpsc;
use uuid::Uuid;

use crate::config::Config;

/// 用户状态:0=active,1=disabled。
pub const STATUS_ACTIVE: u8 = 0;

/// API Key 在内存中的映射条目。校验请求时只读此结构,不查库。
#[derive(Debug, Clone)]
pub struct KeyEntry {
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
}

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
        }
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
    pub model: String,
    pub provider: String,
    pub upstream_model: String,
    pub input_tokens: u32,
    pub output_tokens: u32,
    pub charged_tokens: i64,
    pub status: u16,
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
}

impl AppState {
    pub fn config(&self) -> arc_swap::Guard<Arc<Config>> {
        self.config.load()
    }

    pub fn user(&self, id: &Uuid) -> Option<Arc<UserState>> {
        self.users.get(id).map(|e| Arc::clone(e.value()))
    }
}
