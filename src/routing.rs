use std::collections::HashMap;

use chrono::{Datelike, Timelike, Utc};

use crate::config::ProviderKind;
use crate::error::ApiError;

/// 一个上游连接(协议 + 地址 + 密钥)。
#[derive(Clone)]
pub struct ProviderConn {
    pub kind: ProviderKind,
    pub base_url: String,
    pub api_key: Option<String>,
    /// 该上游独立并发上限(None = 用全局 Defaults.upstream_concurrency, 0 = 不限)。
    pub concurrency: Option<u32>,
}

/// 一个「模型」= 某供应商上的真实模型。
#[derive(Clone)]
pub struct ModelDef {
    pub provider: String,
    pub upstream_model: String,
}

/// 组内一条路由目标。
#[derive(Clone)]
pub struct Target {
    pub model_id: i64,
    pub weight: u32,
    pub multiplier: f64,
}

/// 高峰/低谷时段规则:按「星期 + 时间段」驱动计费倍率与路由权重覆盖。
#[derive(Clone)]
pub struct TimeRule {
    #[allow(dead_code)]
    pub id: i64,
    pub name: String,
    /// 生效星期:0-6 或 "1-5" / "0,6" / "0-6"。
    pub weekdays: String,
    /// 开始 HH:MM。
    pub start_time: String,
    /// 结束 HH:MM。
    pub end_time: String,
    /// 该时段计费倍率系数。
    pub multiplier: f64,
    /// 该时段路由权重覆盖 {"public_name":{"model_id":weight}}。
    pub weight_map: HashMap<String, HashMap<i64, u32>>,
    /// 是否启用。
    pub active: bool,
}

/// 组级负载策略算法。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Strategy {
    /// 加权随机(默认):按 weight 概率随机挑一个,失败后按权重序 failover。
    WeightedRandom,
    /// 加权轮询。
    WeightedRoundRobin,
    /// 简单轮询。
    RoundRobin,
    /// 优先级:按 weight 降序(越大越优先),失败自动切下一个。
    Priority,
}

impl Strategy {
    pub fn as_str(&self) -> &'static str {
        match self {
            Strategy::WeightedRandom => "weighted_random",
            Strategy::WeightedRoundRobin => "weighted_round_robin",
            Strategy::RoundRobin => "round_robin",
            Strategy::Priority => "priority",
        }
    }
}

/// 把配置串解析为 Strategy;未知值回退到加权随机。
pub fn strategy_from_str(s: &str) -> Strategy {
    match s {
        "weighted_round_robin" => Strategy::WeightedRoundRobin,
        "round_robin" => Strategy::RoundRobin,
        "priority" => Strategy::Priority,
        _ => Strategy::WeightedRandom,
    }
}

/// 全量内存路由图,放在 ArcSwap 里热替换。
#[derive(Default, Clone)]
pub struct Routing {
    pub providers: HashMap<String, ProviderConn>,
    pub models: HashMap<i64, ModelDef>,
    pub group_names: HashMap<i64, String>,
    /// 每组绑定的负载策略(缺省=加权随机)。
    pub group_strategy: HashMap<i64, Strategy>,
    /// 每组的高峰/低谷时段规则(按当前时间命中后:算倍率、覆盖权重)。
    pub time_rules: HashMap<i64, Vec<TimeRule>>,
    /// group_id -> (对外模型名 -> 目标列表)。按名精确路由,同名多条即加权。
    pub groups: HashMap<i64, HashMap<String, Vec<Target>>>,
}

/// 解析结果:实际要打到哪个上游。
pub struct Resolved {
    pub provider: String,
    pub kind: ProviderKind,
    pub base_url: String,
    pub api_key: Option<String>,
    pub upstream_model: String,
    pub multiplier: f64,
    /// 该候选在路由中的权重(负载均衡权重,供链路展示)。
    pub weight: u32,
    /// 高峰/低谷时段倍率系数(无命中时段=1.0),已计入 multiplier;此处保留原始值供展示。
    #[allow(dead_code)]
    pub time_multiplier: f64,
    /// 命中的时段名(供链路展示),None=当前无生效时段。
    #[allow(dead_code)]
    pub time_slot: Option<String>,
}

impl Routing {
    /// 组内可用的对外模型名(供门户对话页选择器)。
    pub fn group_model_names(&self, group_id: i64) -> Vec<String> {
        self.groups
            .get(&group_id)
            .map(|m| {
                let mut v: Vec<String> = m.keys().cloned().collect();
                v.sort();
                v
            })
            .unwrap_or_default()
    }

    /// 解析该对外模型的全部可用目标,用于「负载均衡 + 故障转移」。
    /// 1) 先按该组绑定的负载策略算出候选顺序(第一条就是本次命中的目标,后面是 failover 次序);
    /// 2) 然后按 (kind + base_url + upstream_model) 去重,避免重复打同一个上游。
    /// `rr_counter`: 组级轮询游标(加权轮询 / 简单轮询需要跨请求记住轮到哪),由 AppState 持有。
    pub fn resolve_all(
        &self,
        group_id: i64,
        requested: &str,
        rr_counter: &dashmap::DashMap<i64, std::sync::atomic::AtomicU32>,
        tz_offset_secs: i32,
    ) -> Result<Vec<Resolved>, ApiError> {
        let group = self
            .groups
            .get(&group_id)
            .ok_or_else(|| ApiError::ModelNotAllowed(requested.to_string()))?;
        let targets = group
            .get(requested)
            .filter(|v| !v.is_empty())
            .ok_or_else(|| ApiError::ModelNotFound(requested.to_string()))?;
        // 高峰/低谷时段解析:命中则取该时段的倍率系数,并可能覆盖路由权重(B 部分)。
        let (now_dow, now_min) = now_dow_min(Utc::now(), tz_offset_secs);
        let rule = self.current_rule(group_id, now_dow, now_min);
        let time_multiplier = rule.map(|r| r.multiplier.max(0.05)).unwrap_or(1.0);
        let time_slot = rule.map(|r| r.name.clone());
        // 当前时段的路由权重覆盖:public_name -> {model_id: weight}
        let weight_over: Option<&HashMap<i64, u32>> = rule.and_then(|r| r.weight_map.get(requested));
        let eff_weight = |t: &Target| -> u32 {
            weight_over.and_then(|m| m.get(&t.model_id)).copied().unwrap_or(t.weight).max(1)
        };
        // 按该组绑定的负载策略生成候选顺序(命中的排前,其后是 failover 次序)。
        let strategy = self.group_strategy.get(&group_id).copied().unwrap_or(Strategy::WeightedRandom);
        // 时段覆盖权重时,不透传原权重给排序,单独构造一份带覆盖权重的 targets 快照。
        let weighted_targets: Option<Vec<Target>> = rule.as_ref().map(|_| {
            targets.iter().map(|t| Target { model_id: t.model_id, weight: eff_weight(t), multiplier: t.multiplier }).collect()
        });
        let sort_targets: &[Target] = weighted_targets.as_deref().unwrap_or(targets);
        let order = match strategy {
            Strategy::WeightedRandom => shuffle_weighted(sort_targets),
            Strategy::RoundRobin => order_round_robin(group_id, sort_targets, rr_counter, true),
            Strategy::WeightedRoundRobin => order_round_robin(group_id, sort_targets, rr_counter, false),
            Strategy::Priority => order_by_priority(sort_targets),
        };
        let mut seen: std::collections::HashSet<(ProviderKind, String, String)> = std::collections::HashSet::new();
        let mut out: Vec<Resolved> = Vec::new();
        for idx in order {
            let t = &targets[idx];
            let Some(m) = self.models.get(&t.model_id) else { continue };
            let Some(p) = self.providers.get(&m.provider) else { continue };
            let key = (p.kind, p.base_url.clone(), m.upstream_model.clone());
            if !seen.insert(key) {
                continue;
            }
            out.push(Resolved {
                provider: m.provider.clone(),
                kind: p.kind,
                base_url: p.base_url.clone(),
                api_key: p.api_key.clone(),
                upstream_model: m.upstream_model.clone(),
                // 计费倍率:模型倍率 × 时段倍率(无时段=×1.0)。handlers 直接用它计费,无需改计费链。
                multiplier: t.multiplier * time_multiplier,
                weight: eff_weight(t),
                time_multiplier,
                time_slot: time_slot.clone(),
            });
        }
        if out.is_empty() {
            return Err(ApiError::NoTarget(requested.to_string()));
        }
        Ok(out)
    }

    /// 返回该组当前时间命中的时段规则(第一个命中),并给出「星期几 + 时分」的判断参数。
    /// 无命中或未启用则返回 None。
    fn current_rule(&self, group_id: i64, now_dow: u32, now_min: u32) -> Option<&TimeRule> {
        self.time_rules.get(&group_id).and_then(|rules| {
            rules.iter().find(|r| {
                r.active && rules_contains_weekday(&r.weekdays, now_dow) && time_in_range(&r.start_time, &r.end_time, now_min)
            })
        })
    }
}

/// 根据传入的 UTC 时间和时区偏移秒数,计算「周几(0=周一..6=周日)」与「当天分钟数」。
/// 使用 chrono Datelike/Timelike trait 做日历换算(自动处理日期边界,如 UTC 23:xx + 8h → 次日 07:xx)。
pub fn now_dow_min(utc_now: chrono::DateTime<Utc>, offset_secs: i32) -> (u32, u32) {
    // 将 UTC 时间加上时区偏移得到本地时间(注意:日期可能跨天,如 UTC 23:00 + 8h → 次日 07:00)。
    let local_secs = utc_now.timestamp() + offset_secs as i64;
    // 构造本地日期时间用于日历计算(不受夏令时影响,用固定偏移)。
    let local = chrono::DateTime::from_timestamp(local_secs, 0)
        .unwrap_or_else(|| chrono::DateTime::from_timestamp(0, 0).unwrap());
    let dow = (local.weekday().number_from_monday() - 1) as u32; // Monday=1..Sunday=7 -> 0..6
    let min = (local.hour() as u32) * 60 + local.minute() as u32;
    (dow, min)
}

/// weekdays 串形如 "0-6" / "1-5" / "0,6" / "5";判断给定位索引是否命中(0=周一..6=周日)。
fn rules_contains_weekday(spec: &str, dow: u32) -> bool {
    for part in spec.split(',') {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        if let Some((a, b)) = part.split_once('-') {
            let a: u32 = a.trim().parse().unwrap_or(0);
            let b: u32 = b.trim().parse().unwrap_or(0);
            if dow >= a.min(b) && dow <= a.max(b) {
                return true;
            }
        } else if let Ok(d) = part.parse::<u32>() {
            if d == dow {
                return true;
            }
        }
    }
    false
}

/// 判断当前分钟数是否落在 [start,end) HH:MM 区间内;支持跨午夜(如 23:00-05:00)。
fn time_in_range(start: &str, end: &str, now_min: u32) -> bool {
    let parse = |s: &str| -> Option<u32> {
        let (h, m) = s.trim().split_once(':')?;
        Some(h.trim().parse::<u32>().ok()? * 60 + m.trim().parse::<u32>().ok()?)
    };
    let (Some(s), Some(e)) = (parse(start), parse(end)) else {
        return false;
    };
    if s <= e {
        now_min >= s && now_min < e
    } else {
        // 跨午夜:当前在 [s,1440) 或 [0,e) 都算。
        now_min >= s || now_min < e
    }
}

/// 加权随机「不放回」:返回 0..targets.len() 的一个排列,
/// 靠前的索引被抽中的概率与其 weight 正相关。
/// 等价于连续多次加权随机抽取,每抽中一个就从池子移除。
fn shuffle_weighted(targets: &[Target]) -> Vec<usize> {
    let n = targets.len();
    let weights: Vec<u32> = targets.iter().map(|t| t.weight.max(1)).collect();
    let mut remaining: u64 = weights.iter().map(|&w| w as u64).sum();
    let mut picked: Vec<bool> = vec![false; n];
    let mut order = Vec::with_capacity(n);
    for _ in 0..n {
        let mut r = (rand::random::<u64>() % remaining.max(1)) as u32;
        for i in 0..n {
            if picked[i] { continue; }
            let w = weights[i];
            if r < w {
                picked[i] = true;
                order.push(i);
                remaining -= w as u64;
                break;
            }
            r -= w;
        }
    }
    order
}

/// 优先级排序:按 weight 降序(越大越优先),返回索引排列。
fn order_by_priority(targets: &[Target]) -> Vec<usize> {
    let mut idx: Vec<usize> = (0..targets.len()).collect();
    idx.sort_by(|&a, &b| targets[b].weight.cmp(&targets[a].weight));
    idx
}

/// 轮询类负载策略:生成候选顺序,第一条为「本次命中的目标」,其后为 failover 次序。
/// - `simple_mode=true`(对应简单轮询):不看权重,每次按游标逐条轮流。
/// - `simple_mode=false`(对应加权轮询):按命中顺序排列,再由 resolve_all 按权重轮盘驱动游标。
/// `group_id` 用于在 `rr_counter` 中取/建该组的游标。
fn order_round_robin(
    group_id: i64,
    targets: &[Target],
    rr_counter: &dashmap::DashMap<i64, std::sync::atomic::AtomicU32>,
    simple_mode: bool,
) -> Vec<usize> {
    let n = targets.len();
    if n <= 1 {
        return (0..n).collect();
    }
    use std::sync::atomic::Ordering;
    let counter = rr_counter.entry(group_id).or_insert_with(|| std::sync::atomic::AtomicU32::new(0));
    if simple_mode {
        // 简单轮询:按裸下标轮流。
        let idx = (counter.fetch_add(1, Ordering::Relaxed) as usize) % n;
        let mut order: Vec<usize> = (0..n).collect();
        order.rotate_left(idx);
        order
    } else {
        // 加权轮询:构造「权重轮盘」[i0×w0, i1×w1, ...],游标对该组权重和取模选中命中项。
        let wheel: Vec<usize> = targets
            .iter()
            .enumerate()
            .flat_map(|(i, t)| std::iter::repeat(i).take(t.weight.max(1) as usize))
            .collect();
        if wheel.is_empty() {
            return (0..n).collect();
        }
        let sum = wheel.len() as u64;
        let pick = (counter.fetch_add(1, Ordering::Relaxed) as u64) % sum;
        let first = wheel[pick as usize];
        // 命中项放第一,其余按原始下标靠后(failover 次序)。
        let mut order: Vec<usize> = vec![first];
        for i in 0..n {
            if i != first {
                order.push(i);
            }
        }
        order
    }
}

