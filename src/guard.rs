use std::collections::VecDeque;

use dashmap::DashMap;

/// 登录防爆破:内存滑动窗口失败计数。
/// 窗口内失败达到上限后,该标识被锁至最早一次失败滑出窗口为止。
/// 仅内存态,重启即清零;不落库是为避免攻击流量写穿存储。
pub struct LoginGuard {
    window_secs: i64,
    max_failures: usize,
    attempts: DashMap<String, VecDeque<i64>>,
}

impl Default for LoginGuard {
    fn default() -> Self {
        // 15 分钟窗口内 5 次失败即锁。
        Self::new(15 * 60, 5)
    }
}

impl LoginGuard {
    pub fn new(window_secs: i64, max_failures: usize) -> Self {
        Self {
            window_secs,
            max_failures,
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

    /// 被锁则返回剩余秒数,否则 None。顺带清理已滑出窗口的旧记录。
    pub fn locked_for(&self, key: &str) -> Option<i64> {
        let now = Self::now();
        let mut w = self.attempts.get_mut(key)?;
        while w.front().is_some_and(|&t| now - t >= self.window_secs) {
            w.pop_front();
        }
        let locked = if w.len() >= self.max_failures {
            w.front().map(|&first| self.window_secs - (now - first))
        } else {
            None
        };
        if w.is_empty() {
            drop(w);
            self.attempts.remove(key);
        }
        locked
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
        let g = LoginGuard::new(60, 3);
        for i in 0..2 {
            g.record_failure("k");
            assert!(g.locked_for("k").is_none(), "第 {} 次失败不应触发锁定", i + 1);
        }
        g.record_failure("k");
        let secs = g.locked_for("k").expect("第 3 次失败后应锁定");
        assert!(secs > 0 && secs <= 60);
    }

    #[test]
    fn clear_resets_state() {
        let g = LoginGuard::new(60, 2);
        g.record_failure("k");
        g.record_failure("k");
        assert!(g.locked_for("k").is_some());
        g.clear("k");
        assert!(g.locked_for("k").is_none(), "成功登录后应解锁");
    }

    #[test]
    fn keys_are_isolated() {
        let g = LoginGuard::new(60, 2);
        g.record_failure("a");
        g.record_failure("a");
        assert!(g.locked_for("a").is_some());
        assert!(g.locked_for("b").is_none(), "不同标识互不影响");
    }

    #[test]
    fn window_slide_unlocks() {
        let g = LoginGuard::new(1, 1);
        g.record_failure("k");
        assert!(g.locked_for("k").is_some());
        std::thread::sleep(std::time::Duration::from_millis(1100));
        assert!(g.locked_for("k").is_none(), "滑出窗口后应自动解锁");
    }
}
