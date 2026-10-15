use std::collections::VecDeque;

use dashmap::DashMap;

/// 登录防爆破:滑动窗口失败计数 + 固定锁定时长。
/// 窗口内失败达到上限后,从**最近一次失败**起固定锁定 lockout_secs;
/// 锁定期满自动清零计数,给一次全新开始(连续失败不会无限累积)。
/// 仅内存态,重启即清零;不落库是为避免攻击流量写穿存储。
pub struct LoginGuard {
    window_secs: i64,
    max_failures: usize,
    lockout_secs: i64,
    attempts: DashMap<String, VecDeque<i64>>,
}

impl Default for LoginGuard {
    fn default() -> Self {
        // 15 分钟窗口内 5 次失败 → 锁 5 分钟(从最近一次失败起算)。
        Self::new(15 * 60, 5, 5 * 60)
    }
}

impl LoginGuard {
    pub fn new(window_secs: i64, max_failures: usize, lockout_secs: i64) -> Self {
        Self {
            window_secs,
            max_failures,
            lockout_secs,
            attempts: DashMap::new(),
        }
    }

    fn now() -> i64 {
        use std::time::{SystemTime, UNIX_EPOCH};
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64
    }

    /// 被锁则返回剩余秒数,否则 None。锁定期满时清空记录(重新开始计数)。
    pub fn locked_for(&self, key: &str) -> Option<i64> {
        let now = Self::now();
        let mut w = self.attempts.get_mut(key)?;
        while w.front().is_some_and(|&t| now - t >= self.window_secs) {
            w.pop_front();
        }
        if w.len() >= self.max_failures {
            // 从最近一次失败起算固定锁定期。
            let last = *w.back().unwrap();
            let remaining = last + self.lockout_secs - now;
            if remaining > 0 {
                return Some(remaining);
            }
            // 锁定期已过:清零,给一次全新开始。
            drop(w);
            self.attempts.remove(key);
            return None;
        }
        if w.is_empty() {
            drop(w);
            self.attempts.remove(key);
        }
        None
    }

    /// 记一次失败(或一次调用,视入口语义而定)。
    pub fn record_failure(&self, key: &str) {
        let now = Self::now();
        let mut w = self.attempts.entry(key.to_owned()).or_default();
        w.push_back(now);
        while w.front().is_some_and(|&t| now - t >= self.window_secs) {
            w.pop_front();
        }
    }

    /// 验证成功,清零该标识。
    pub fn clear(&self, key: &str) {
        self.attempts.remove(key);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn locks_after_max_failures() {
        let g = LoginGuard::new(60, 3, 60);
        for i in 0..2 {
            g.record_failure("k");
            assert!(g.locked_for("k").is_none(), "第 {} 次失败不应触发锁定", i + 1);
        }
        g.record_failure("k");
        let secs = g.locked_for("k").expect("第 3 次失败后应锁定");
        assert!(secs > 0 && secs <= 60);
    }

    #[test]
    fn lockout_anchored_to_last_failure() {
        let g = LoginGuard::new(60, 3, 30);
        g.record_failure("k");
        std::thread::sleep(std::time::Duration::from_millis(1100));
        g.record_failure("k");
        g.record_failure("k");
        // 锁定从最近一次失败起算:剩余应接近 30 而非 30-1。
        let secs = g.locked_for("k").expect("3 次失败后应锁定");
        assert!(secs > 28, "锁定应锚定最近一次失败,实际剩余 {}s", secs);
    }

    #[test]
    fn lockout_expiry_resets_counter() {
        let g = LoginGuard::new(60, 2, 1);
        g.record_failure("k");
        g.record_failure("k");
        assert!(g.locked_for("k").is_some());
        std::thread::sleep(std::time::Duration::from_millis(1100));
        // 锁定期(1s)已过 → 清零,重新开始计数。
        assert!(g.locked_for("k").is_none(), "锁定期满应解锁并清零");
        g.record_failure("k");
        assert!(g.locked_for("k").is_none(), "清零后一次失败不应锁定");
    }

    #[test]
    fn clear_resets_state() {
        let g = LoginGuard::new(60, 2, 60);
        g.record_failure("k");
        g.record_failure("k");
        assert!(g.locked_for("k").is_some());
        g.clear("k");
        assert!(g.locked_for("k").is_none(), "成功登录后应解锁");
    }

    #[test]
    fn keys_are_isolated() {
        let g = LoginGuard::new(60, 2, 60);
        g.record_failure("a");
        g.record_failure("a");
        assert!(g.locked_for("a").is_some());
        assert!(g.locked_for("b").is_none(), "不同标识互不影响");
    }
}
