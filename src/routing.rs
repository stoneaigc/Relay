use std::collections::HashMap;

use chrono::{Datelike, Timelike, Utc};

use crate::config::ProviderKind;
use crate::error::ApiError;
use crate::pricing;

/// 一个上游连接(协议 + 地址 + 密钥)。
#[derive(Clone)]
pub struct ProviderConn {
    pub kind: ProviderKind,
    pub base_url: String,
    pub api_key: Option<String>,
    /// 该上游独立并发上限(None = 用全局 Defaults.upstream_concurrency, 0 = 不限)。
    pub concurrency: Option<u32>,
    /// 供应商自定义请求头(如 opencode 要求的 x-opencode-session),随每次上游请求附带。
    pub headers: Vec<(String, String)>,
}

/// 一个「模型」= 某供应商上的真实模型。
#[derive(Clone)]
pub struct ModelDef {
    pub provider: String,
    pub upstream_model: String,
    /// 备注/展示名。
    pub label: Option<String>,
    /// 输入单价 $/1M tokens(手动定价;None=回退内置默认价表)。
    pub input_price: Option<f64>,
    /// 输出单价 $/1M tokens(手动定价;None=回退内置默认价表)。
    pub output_price: Option<f64>,
    /// 上下文长度(tokens,管理员配置;None=未标注)。
    pub context_length: Option<i64>,
    /// 能力标签(逗号分隔;None=按名称自动推断)。
    pub tags: Option<String>,
}

/// 组内一条路由目标。
#[derive(Clone)]
pub struct Target {
    pub model_id: i64,
    /// 优先级档位:数值越大越先被使用;同档内才按策略分流。
    /// 只有更高档全部不可用时才降到下一档(故障转移)。
    pub priority: u32,
    /// 档内分流权重(策略为加权随机/加权轮询时生效)。
    pub weight: u32,
    pub multiplier: f64,
    /// 路由级语义缓存 opt-in(与全局开关/确定性判定 AND)。
    pub cache_enabled: bool,
}

/// 门户「模型广场」单张卡片的数据。
pub struct ModelCard {
    /// 对外模型名(请求里填的名字)。
    pub name: String,
    /// 各部署的备注/展示名(去重)。
    pub labels: Vec<String>,
    /// 能力标签(管理员配置优先,否则按名称推断)。
    pub tags: Vec<String>,
    /// 上下文长度(tokens;多部署取最大;None=未标注)。
    pub context_length: Option<i64>,
    /// 目标中的最小计费倍率。
    pub multiplier: f64,
    /// 任一目标启用了语义缓存。
    pub cache: bool,
    /// 上游部署数(负载均衡/故障转移候选数)。
    pub upstreams: usize,
    /// 最低输入单价 $/1M(NaN=存在未定价部署,按默认价表)。
    pub input_price: f64,
    /// 最低输出单价 $/1M(NaN=存在未定价部署)。
    pub output_price: f64,
}

/// 按模型名自动推断能力标签(未配置 tags 时的兜底)。
fn infer_tags(upstream_model: &str) -> Vec<String> {
    let m = upstream_model.to_ascii_lowercase();
    let mut tags: Vec<String> = Vec::new();
    for t in ["视觉", "推理", "向量", "语音", "搜索"] {
        tags.push(t.to_string());
    }
    tags.retain(|t| match t.as_str() {
        "视觉" => m.contains("vision") || m.contains("vl") || m.contains("omni"),
        "推理" => m.contains("reasoner") || m.contains("reasoning") || m.contains("think") || m.contains("o1") || m.contains("o3") || m.ends_with("-r") || m.contains("-r1"),
        "向量" => m.contains("embedding") || m.contains("embed"),
        "语音" => m.contains("audio") || m.contains("tts") || m.contains("asr") || m.contains("whisper") || m.contains("speech") || m.contains("transcri"),
        "搜索" => m.contains("search"),
        _ => false,
    });
    if tags.is_empty() {
        tags.push("对话".to_string());
    }
    tags
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

/// 组级负载策略算法(仅决定「同一优先级档内」的分流方式)。
/// 优先级档位是路由属性(见 Target.priority),不占策略名额。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Strategy {
    /// 加权随机(默认):档内按 weight 概率随机挑一个,失败后按权重序 failover。
    WeightedRandom,
    /// 加权轮询:档内按权重轮盘轮转(权重全 1 即简单轮询)。
    WeightedRoundRobin,
    /// 智能自适应:档内按「延迟 + 成本」综合得分升序(得分越低越优),
    /// 无数据候选取有数据候选的中位分,并以 10% 概率探索(冷启动也能拿到流量)。
    Auto,
}

impl Strategy {
    pub fn as_str(&self) -> &'static str {
        match self {
            Strategy::WeightedRandom => "weighted_random",
            Strategy::WeightedRoundRobin => "weighted_round_robin",
            Strategy::Auto => "auto",
        }
    }
}

/// 把配置串解析为 Strategy;旧策略名做一次语义等价映射,未知值回退到加权随机。
/// - priority → weighted_random(优先级已改为路由属性)
/// - round_robin → weighted_round_robin(权重全 1 即简单轮询)
/// - cost_aware / latency_aware → auto(两者已合并)
pub fn strategy_from_str(s: &str) -> Strategy {
    match s {
        "weighted_round_robin" | "round_robin" => Strategy::WeightedRoundRobin,
        "auto" | "cost_aware" | "latency_aware" => Strategy::Auto,
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
    /// 每组总并发上限(缺省/0=不限)。
    pub group_concurrency: HashMap<i64, u32>,
    /// 每组的高峰/低谷时段规则(按当前时间命中后:算倍率、覆盖权重)。
    pub time_rules: HashMap<i64, Vec<TimeRule>>,
    /// group_id -> (对外模型名 -> 目标列表)。按名精确路由,同名多条即加权。
    pub groups: HashMap<i64, HashMap<String, Vec<Target>>>,
    /// 用户组 -> 有序绑定的模型组 id(下标即优先级,0=最高)。两级寻址第一级。
    pub user_group_models: HashMap<i64, Vec<i64>>,
}

/// 解析结果:实际要打到哪个上游。
pub struct Resolved {
    /// 候选所属模型组;跨组 failover 时逐组获取并发槽。
    pub group_id: i64,
    pub provider: String,
    pub kind: ProviderKind,
    pub base_url: String,
    pub api_key: Option<String>,
    /// 供应商自定义请求头(随上游请求附带)。
    pub headers: Vec<(String, String)>,
    pub upstream_model: String,
    pub multiplier: f64,
    /// 路由级语义缓存 opt-in(与全局开关/确定性判定 AND)。
    pub cache_enabled: bool,
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

    /// 门户「模型广场」卡片:每个对外模型名一条,聚合展示名/标签/上下文/倍率/缓存/上游数。
    pub fn group_model_cards(&self, group_id: i64) -> Vec<ModelCard> {
        let Some(map) = self.groups.get(&group_id) else { return Vec::new() };
        let mut cards: Vec<ModelCard> = map
            .iter()
            .map(|(name, targets)| {
                let defs: Vec<&ModelDef> = targets
                    .iter()
                    .filter_map(|t| self.models.get(&t.model_id))
                    .collect();
                // 展示名:优先各部署的 label,去重;与对外模型名完全相同的 label 不携带额外信息,丢弃避免卡片重复渲染。
                let mut labels: Vec<String> = Vec::new();
                for d in &defs {
                    if let Some(l) = d.label.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
                        if l.eq_ignore_ascii_case(name) {
                            continue;
                        }
                        if !labels.iter().any(|x| x.eq_ignore_ascii_case(l)) {
                            labels.push(l.to_string());
                        }
                    }
                }
                // 标签:管理员配置优先,否则按模型名自动推断;多部署合并去重。
                let mut tags: Vec<String> = Vec::new();
                let configured: Vec<&str> = defs
                    .iter()
                    .filter_map(|d| d.tags.as_deref().map(str::trim).filter(|s| !s.is_empty()))
                    .flat_map(|s| s.split(','))
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .collect();
                if !configured.is_empty() {
                    for t in configured {
                        if !tags.iter().any(|x| x.eq_ignore_ascii_case(t)) {
                            tags.push(t.to_string());
                        }
                    }
                } else {
                    for d in &defs {
                        for t in infer_tags(&d.upstream_model) {
                            if !tags.contains(&t) {
                                tags.push(t);
                            }
                        }
                    }
                }
                let context_length = defs.iter().filter_map(|d| d.context_length).max();
                let multiplier = targets
                    .iter()
                    .map(|t| t.multiplier)
                    .fold(f64::INFINITY, f64::min);
                let cache = targets.iter().any(|t| t.cache_enabled);
                // 价格:任一部署未定价则视为"按默认价表",否则给出区间。
                let inputs: Vec<f64> = defs.iter().filter_map(|d| d.input_price).collect();
                let outputs: Vec<f64> = defs.iter().filter_map(|d| d.output_price).collect();
                let (input_price, output_price) = if inputs.len() == defs.len() && outputs.len() == defs.len() {
                    (inputs.iter().cloned().fold(f64::INFINITY, f64::min), outputs.iter().cloned().fold(f64::INFINITY, f64::min))
                } else {
                    (f64::NAN, f64::NAN) // NaN = 存在未定价部署
                };
                ModelCard {
                    name: name.clone(),
                    labels,
                    tags,
                    context_length,
                    multiplier: if multiplier.is_finite() { multiplier } else { 1.0 },
                    cache,
                    upstreams: targets.len(),
                    input_price,
                    output_price,
                }
            })
            .collect();
        cards.sort_by(|a, b| a.name.cmp(&b.name));
        cards
    }

    /// 解析该对外模型的全部可用目标,用于「负载均衡 + 故障转移」。
    /// 1) 先按该组绑定的负载策略算出候选顺序(第一条就是本次命中的目标,后面是 failover 次序);
    /// 2) 然后按 (kind + base_url + upstream_model) 去重,避免重复打同一个上游。
    /// `rr_counter`: 组级轮询游标(加权轮询 / 简单轮询需要跨请求记住轮到哪),由 AppState 持有。
    /// `lat_p50`: 供应商名 -> P50 延迟(ms)的快照(延迟优先策略用),由 AppState 后台周期刷新。
    /// `now`: 可选的当前时间(测试注入用);None 则用 Utc::now()。
    pub fn resolve_all(
        &self,
        group_id: i64,
        requested: &str,
        rr_counter: &dashmap::DashMap<i64, std::sync::atomic::AtomicU32>,
        lat_p50: &dashmap::DashMap<String, u64>,
        tz_offset_secs: i32,
        now: Option<chrono::DateTime<Utc>>,
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
        let (now_dow, now_min) = now_dow_min(now.unwrap_or_else(Utc::now), tz_offset_secs);
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
            targets.iter().map(|t| Target { model_id: t.model_id, priority: t.priority, weight: eff_weight(t), multiplier: t.multiplier, cache_enabled: t.cache_enabled }).collect()
        });
        let sort_targets: &[Target] = weighted_targets.as_deref().unwrap_or(targets);
        // 先按优先级分档(高→低,只有高档全不可用才降档),档内再按策略排序。
        let mut order: Vec<usize> = Vec::with_capacity(sort_targets.len());
        for (tier_rank, idxs) in priority_tiers(sort_targets).into_iter().enumerate() {
            let tier: Vec<Target> = idxs.iter().map(|&i| sort_targets[i].clone()).collect();
            let in_tier = match strategy {
                // 加权轮询只对最高档做严格轮转;更低档只在容灾时用,保持随机次序即可。
                Strategy::WeightedRoundRobin if tier_rank == 0 => {
                    order_round_robin(group_id, &tier, rr_counter)
                }
                Strategy::WeightedRoundRobin => shuffle_weighted(&tier),
                Strategy::WeightedRandom => shuffle_weighted(&tier),
                Strategy::Auto => order_by_auto(&tier, &self.models, lat_p50),
            };
            order.extend(in_tier.into_iter().map(|j| idxs[j]));
        }
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
                group_id,
                provider: m.provider.clone(),
                kind: p.kind,
                base_url: p.base_url.clone(),
                api_key: p.api_key.clone(),
                headers: p.headers.clone(),
                upstream_model: m.upstream_model.clone(),
                // 计费倍率:模型倍率 × 时段倍率(无时段=×1.0)。handlers 直接用它计费,无需改计费链。
                multiplier: t.multiplier * time_multiplier,
                weight: eff_weight(t),
                time_multiplier,
                time_slot: time_slot.clone(),
                cache_enabled: t.cache_enabled,
            });
        }
        if out.is_empty() {
            return Err(ApiError::NoTarget(requested.to_string()));
        }
        Ok(out)
    }

    /// 两级寻址:按用户组绑定的有序模型组列表解析对外模型。
    /// 语义:先按组优先级,首个含该模型的组用它自己的负载策略解析;候选链按组优先级
    /// 跨组拼接(高优先级组在前),同名上游跨组去重,形成跨组 failover。
    /// group_ids 为空 → ModelNotAllowed;所有组都无此模型 → ModelNotFound;有路由但无有效上游 → NoTarget。
    pub fn resolve_priority(
        &self,
        group_ids: &[i64],
        requested: &str,
        rr_counter: &dashmap::DashMap<i64, std::sync::atomic::AtomicU32>,
        lat_p50: &dashmap::DashMap<String, u64>,
        tz_offset_secs: i32,
        now: Option<chrono::DateTime<Utc>>,
    ) -> Result<Vec<Resolved>, ApiError> {
        if group_ids.is_empty() {
            return Err(ApiError::ModelNotAllowed(requested.to_string()));
        }
        let mut had_name = false;
        let mut seen: std::collections::HashSet<(ProviderKind, String, String)> = std::collections::HashSet::new();
        let mut out: Vec<Resolved> = Vec::new();
        for gid in group_ids {
            // 预检:该组确实含此对外模型名才进入解析(否则尝试下一优先级的组)。
            let contains = self
                .groups
                .get(gid)
                .and_then(|g| g.get(requested))
                .map(|v| !v.is_empty())
                .unwrap_or(false);
            if !contains {
                continue;
            }
            had_name = true;
            let mut cands = self.resolve_all(*gid, requested, rr_counter, lat_p50, tz_offset_secs, now)?;
            for r in cands.drain(..) {
                if seen.insert((r.kind, r.base_url.clone(), r.upstream_model.clone())) {
                    out.push(r);
                }
            }
        }
        if out.is_empty() {
            return Err(if had_name {
                ApiError::NoTarget(requested.to_string())
            } else {
                ApiError::ModelNotFound(requested.to_string())
            });
        }
        Ok(out)
    }

    /// 多个模型组的对外模型名并集(排序去重),供 /v1/models 与门户选择器。
    pub fn model_names_union(&self, group_ids: &[i64]) -> Vec<String> {
        let mut set: std::collections::HashSet<String> = std::collections::HashSet::new();
        for gid in group_ids {
            if let Some(m) = self.groups.get(gid) {
                set.extend(m.keys().cloned());
            }
        }
        let mut v: Vec<String> = set.into_iter().collect();
        v.sort();
        v
    }

    /// 多个模型组的模型卡片并集:同名合并(上游数累加,倍率/价格取最优,标签/展示名取并集,
    /// 高优先级组的展示名与标签排前),供门户「模型广场」。
    pub fn model_cards_union(&self, group_ids: &[i64]) -> Vec<ModelCard> {
        let mut merged: HashMap<String, ModelCard> = HashMap::new();
        for gid in group_ids {
            for c in self.group_model_cards(*gid) {
                merged
                    .entry(c.name.clone())
                    .and_modify(|m| {
                        m.upstreams += c.upstreams;
                        m.multiplier = m.multiplier.min(c.multiplier);
                        m.cache = m.cache || c.cache;
                        m.context_length = match (m.context_length, c.context_length) {
                            (Some(a), Some(b)) => Some(a.max(b)),
                            (None, b) => b,
                            (a, None) => a,
                        };
                        for l in &c.labels {
                            if !m.labels.iter().any(|x| x.eq_ignore_ascii_case(l)) {
                                m.labels.push(l.clone());
                            }
                        }
                        for t in &c.tags {
                            if !m.tags.contains(t) {
                                m.tags.push(t.clone());
                            }
                        }
                        // 价格 NaN = 存在未定价部署;任一合并来源 NaN 则结果 NaN。
                        m.input_price = if m.input_price.is_nan() || c.input_price.is_nan() {
                            f64::NAN
                        } else {
                            m.input_price.min(c.input_price)
                        };
                        m.output_price = if m.output_price.is_nan() || c.output_price.is_nan() {
                            f64::NAN
                        } else {
                            m.output_price.min(c.output_price)
                        };
                    })
                    .or_insert(c);
            }
        }
        let mut cards: Vec<ModelCard> = merged.into_values().collect();
        cards.sort_by(|a, b| a.name.cmp(&b.name));
        cards
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

/// 按 priority 分档:返回「档位从高到低」的索引分组,同档内保持原序。
/// 档位语义:只有更高档的候选全部失败时才降档,因此各档的权重互不影响。
fn priority_tiers(targets: &[Target]) -> Vec<Vec<usize>> {
    let mut prios: Vec<u32> = targets.iter().map(|t| t.priority).collect();
    prios.sort_unstable_by(|a, b| b.cmp(a));
    prios.dedup();
    prios
        .into_iter()
        .map(|p| {
            targets
                .iter()
                .enumerate()
                .filter(|(_, t)| t.priority == p)
                .map(|(i, _)| i)
                .collect()
        })
        .collect()
}

/// 原地 Fisher-Yates 洗牌(用 rand::random,不引入额外 trait 依赖)。
fn fisher_yates_shuffle<T>(v: &mut [T]) {
    for i in (1..v.len()).rev() {
        let j = rand::random::<usize>() % (i + 1);
        v.swap(i, j);
    }
}

/// 单个目标的成本基数:两侧单价之和 × 计费倍率(负倍率按 0 计)。
/// 价格经 pricing::resolve_prices(手动定价优先,回退内置价表);任一侧未定价 = None。
fn target_cost(t: &Target, models: &HashMap<i64, ModelDef>) -> Option<f64> {
    let m = models.get(&t.model_id)?;
    let (i, o) = pricing::resolve_prices(&m.upstream_model, m.input_price, m.output_price);
    match (i, o) {
        (Some(i), Some(o)) => Some((i + o) * t.multiplier.max(0.0)),
        _ => None,
    }
}

/// 单个目标的 P50 延迟基数:经模型查供应商名,再查快照;无数据 = None。
fn target_latency(t: &Target, models: &HashMap<i64, ModelDef>, lat_p50: &dashmap::DashMap<String, u64>) -> Option<u64> {
    models
        .get(&t.model_id)
        .and_then(|m| lat_p50.get(&m.provider).map(|e| *e.value()))
}

/// 取中位数(None 不参与);样本为空返回 None。
fn median(vals: &mut Vec<f64>) -> Option<f64> {
    if vals.is_empty() {
        return None;
    }
    vals.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    Some(vals[vals.len() / 2])
}

/// 智能自适应排序:综合「延迟 + 成本」打分,得分越低越优。
/// - 延迟取窗口内 P50(ms),成本取 (输入+输出) 单价 × 计费倍率。
/// - 两项各自按候选内的最大值归一化后各占 50%。
/// - **无数据的候选取「有数据候选的中位数」作为占位分**,而不是排到最后:
///   否则新供应商/新模型永远拿不到流量,也就永远产生不了数据(冷启动死锁)。
/// - **冷启动探索**:无延迟数据的候选以 10% 概率被直接提到队首,保证能产生延迟样本
///   (仅靠中位数占位,在成本不同时仍会被永久压住)。
/// - 同分随机(先洗牌再稳定排序)。
fn order_by_auto(
    targets: &[Target],
    models: &HashMap<i64, ModelDef>,
    lat_p50: &dashmap::DashMap<String, u64>,
) -> Vec<usize> {
    let n = targets.len();
    if n <= 1 {
        return (0..n).collect();
    }
    let lats: Vec<Option<f64>> = targets.iter().map(|t| target_latency(t, models, lat_p50).map(|v| v as f64)).collect();
    let costs: Vec<Option<f64>> = targets.iter().map(|t| target_cost(t, models)).collect();
    let med_lat = median(&mut lats.iter().flatten().copied().collect());
    let med_cost = median(&mut costs.iter().flatten().copied().collect());
    // 归一化基准取「填充后」的最大值,保证 0..1 区间。
    let max_lat = lats.iter().map(|v| v.or(med_lat)).flatten().fold(0.0f64, f64::max);
    let max_cost = costs.iter().map(|v| v.or(med_cost)).flatten().fold(0.0f64, f64::max);
    let norm = |v: Option<f64>, med: Option<f64>, max: f64| -> f64 {
        match v.or(med) {
            Some(x) if max > 0.0 => (x / max).clamp(0.0, 1.0),
            _ => 0.5, // 全无数据:中性分,交由洗牌随机分摊
        }
    };
    let mut keyed: Vec<(usize, f64)> = (0..n)
        .map(|i| {
            let score = 0.5 * norm(lats[i], med_lat, max_lat) + 0.5 * norm(costs[i], med_cost, max_cost);
            (i, score)
        })
        .collect();
    fisher_yates_shuffle(&mut keyed);
    keyed.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));
    let mut order: Vec<usize> = keyed.into_iter().map(|(i, _)| i).collect();
    // 冷启动探索:无延迟数据的候选按 10% 概率置顶,避免"无数据 → 无流量 → 永远无数据"。
    let unknown: Vec<usize> = (0..n).filter(|&i| lats[i].is_none()).collect();
    if !unknown.is_empty() && rand::random::<f64>() < 0.1 {
        let pick = unknown[rand::random::<usize>() % unknown.len()];
        order.retain(|&i| i != pick);
        order.insert(0, pick);
    }
    order
}

/// 加权轮询:构造「权重轮盘」[i0×w0, i1×w1, ...],游标取模选中命中项,
/// 命中项排第一,其余按原始下标排后作为 failover 次序。
/// `group_id` 用于在 `rr_counter` 中取/建该组的游标(档内轮转互不干扰)。
fn order_round_robin(
    group_id: i64,
    targets: &[Target],
    rr_counter: &dashmap::DashMap<i64, std::sync::atomic::AtomicU32>,
) -> Vec<usize> {
    let n = targets.len();
    if n <= 1 {
        return (0..n).collect();
    }
    use std::sync::atomic::Ordering;
    let wheel: Vec<usize> = targets
        .iter()
        .enumerate()
        .flat_map(|(i, t)| std::iter::repeat(i).take(t.weight.max(1) as usize))
        .collect();
    if wheel.is_empty() {
        return (0..n).collect();
    }
    let counter = rr_counter.entry(group_id).or_insert_with(|| std::sync::atomic::AtomicU32::new(0));
    let sum = wheel.len() as u64;
    let pick = (counter.fetch_add(1, Ordering::Relaxed) as u64) % sum;
    let first = wheel[pick as usize];
    let mut order: Vec<usize> = vec![first];
    for i in 0..n {
        if i != first {
            order.push(i);
        }
    }
    order
}

#[cfg(test)]
mod tests {
    use super::*;

    // ---- strategy_from_str / as_str ----

    #[test]
    fn strategy_from_str_known() {
        assert_eq!(strategy_from_str("weighted_random"), Strategy::WeightedRandom);
        assert_eq!(strategy_from_str("weighted_round_robin"), Strategy::WeightedRoundRobin);
        assert_eq!(strategy_from_str("auto"), Strategy::Auto);
    }

    /// 旧策略名必须映射到语义等价的新策略,保证存量配置不会突变。
    #[test]
    fn strategy_from_str_legacy_mapping() {
        assert_eq!(strategy_from_str("priority"), Strategy::WeightedRandom);
        assert_eq!(strategy_from_str("round_robin"), Strategy::WeightedRoundRobin);
        assert_eq!(strategy_from_str("cost_aware"), Strategy::Auto);
        assert_eq!(strategy_from_str("latency_aware"), Strategy::Auto);
    }

    #[test]
    fn strategy_from_str_unknown_falls_back() {
        assert_eq!(strategy_from_str("anything"), Strategy::WeightedRandom);
        assert_eq!(strategy_from_str(""), Strategy::WeightedRandom);
    }

    #[test]
    fn strategy_as_str_roundtrip() {
        let strategies = [Strategy::WeightedRandom, Strategy::WeightedRoundRobin, Strategy::Auto];
        for s in &strategies {
            assert_eq!(strategy_from_str(s.as_str()), *s);
        }
    }

    // ---- priority_tiers(优先级分档) ----

    #[test]
    fn tiers_group_by_priority_desc() {
        let targets = vec![
            Target { model_id: 1, weight: 1, priority: 0, multiplier: 1.0, cache_enabled: false },
            Target { model_id: 2, weight: 1, priority: 10, multiplier: 1.0, cache_enabled: false },
            Target { model_id: 3, weight: 1, priority: 0, multiplier: 1.0, cache_enabled: false },
            Target { model_id: 4, weight: 1, priority: 5, multiplier: 1.0, cache_enabled: false },
        ];
        let tiers = priority_tiers(&targets);
        // 档位 10 > 5 > 0,档内保持原序
        assert_eq!(tiers, vec![vec![1], vec![3], vec![0, 2]]);
    }

    #[test]
    fn tiers_single_tier_when_all_equal() {
        let targets = vec![
            Target { model_id: 1, weight: 1, priority: 3, multiplier: 1.0, cache_enabled: false },
            Target { model_id: 2, weight: 1, priority: 3, multiplier: 1.0, cache_enabled: false },
        ];
        assert_eq!(priority_tiers(&targets), vec![vec![0, 1]]);
    }

    #[test]
    fn tiers_empty_targets() {
        let targets: Vec<Target> = vec![];
        assert!(priority_tiers(&targets).is_empty());
    }

    // ---- shuffle_weighted 基本性质 ----

    #[test]
    fn shuffle_weighted_covers_all() {
        let targets = vec![
            Target { model_id: 1, weight: 10, priority: 0, multiplier: 1.0, cache_enabled: false },
            Target { model_id: 2, weight: 20, priority: 0, multiplier: 1.0, cache_enabled: false },
            Target { model_id: 3, weight: 30, priority: 0, multiplier: 1.0, cache_enabled: false },
        ];
        let order = shuffle_weighted(&targets);
        assert_eq!(order.len(), 3);
        let mut sorted = order.clone();
        sorted.sort();
        assert_eq!(sorted, vec![0, 1, 2]);
    }

    #[test]
    fn shuffle_weighted_single() {
        let targets = vec![Target { model_id: 1, weight: 100, priority: 0, multiplier: 1.0, cache_enabled: false }];
        let order = shuffle_weighted(&targets);
        assert_eq!(order, vec![0]);
    }

    // ---- order_by_auto(智能自适应) ----

    fn mdl(upstream: &str, input_price: Option<f64>, output_price: Option<f64>) -> ModelDef {
        ModelDef { provider: "p".into(), upstream_model: upstream.into(), label: None, input_price, output_price, context_length: None, tags: None }
    }

    fn lat_map(pairs: &[(&str, u64)]) -> dashmap::DashMap<String, u64> {
        let m = dashmap::DashMap::new();
        for (k, v) in pairs {
            m.insert((*k).to_string(), *v);
        }
        m
    }

    fn prov_model(prov: &str, upstream: &str) -> ModelDef {
        let mut m = mdl(upstream, None, None);
        m.provider = prov.into();
        m
    }

    #[test]
    fn auto_prefers_fastest_when_cost_equal() {
        let mut models = HashMap::new();
        models.insert(1, prov_model("p-slow", "gpt-4o-mini"));
        models.insert(2, prov_model("p-fast", "gpt-4o-mini"));
        let lat = lat_map(&[("p-slow", 800), ("p-fast", 200)]);
        let targets = vec![
            Target { model_id: 1, weight: 1, priority: 0, multiplier: 1.0, cache_enabled: false },
            Target { model_id: 2, weight: 1, priority: 0, multiplier: 1.0, cache_enabled: false },
        ];
        let order = order_by_auto(&targets, &models, &lat);
        assert_eq!(order[0], 1);
    }

    #[test]
    fn auto_prefers_cheaper_when_latency_equal() {
        let mut models = HashMap::new();
        models.insert(1, prov_model("p1", "gpt-4o"));
        models.insert(2, prov_model("p2", "gpt-4o-mini"));
        let lat = lat_map(&[("p1", 300), ("p2", 300)]);
        let targets = vec![
            Target { model_id: 1, weight: 1, priority: 0, multiplier: 1.0, cache_enabled: false },
            Target { model_id: 2, weight: 1, priority: 0, multiplier: 1.0, cache_enabled: false },
        ];
        let order = order_by_auto(&targets, &models, &lat);
        assert_eq!(order[0], 1);
    }

    /// 回归:无数据候选不能被永久排在最后(否则新上游永远拿不到流量 → 冷启动死锁)。
    #[test]
    fn auto_cold_start_candidate_can_be_selected() {
        let mut models = HashMap::new();
        models.insert(1, prov_model("p-known", "gpt-4o-mini"));
        models.insert(2, prov_model("p-new", "gpt-4o-mini"));
        let lat = lat_map(&[("p-known", 800)]); // p-new 无数据
        let targets = vec![
            Target { model_id: 1, weight: 1, priority: 0, multiplier: 1.0, cache_enabled: false },
            Target { model_id: 2, weight: 1, priority: 0, multiplier: 1.0, cache_enabled: false },
        ];
        let mut new_first = 0;
        for _ in 0..200 {
            if order_by_auto(&targets, &models, &lat)[0] == 1 {
                new_first += 1;
            }
        }
        assert!(new_first > 0, "无数据候选 200 次里一次都没被选中,冷启动死锁复现");
    }

    #[test]
    fn auto_all_unknown_covers_all() {
        let mut models = HashMap::new();
        models.insert(1, prov_model("p1", "gpt-4o-mini"));
        models.insert(2, prov_model("p2", "gpt-4o-mini"));
        let lat = dashmap::DashMap::new();
        let targets = vec![
            Target { model_id: 1, weight: 1, priority: 0, multiplier: 1.0, cache_enabled: false },
            Target { model_id: 2, weight: 1, priority: 0, multiplier: 1.0, cache_enabled: false },
        ];
        let mut sorted = order_by_auto(&targets, &models, &lat);
        sorted.sort();
        assert_eq!(sorted, vec![0, 1]);
    }

    #[test]
    fn auto_multiplier_affects_cost() {
        let mut models = HashMap::new();
        models.insert(1, prov_model("p1", "gpt-4o"));
        models.insert(2, prov_model("p2", "gpt-4o"));
        let lat = lat_map(&[("p1", 300), ("p2", 300)]);
        let targets = vec![
            Target { model_id: 1, weight: 1, priority: 0, multiplier: 1.0, cache_enabled: false },
            Target { model_id: 2, weight: 1, priority: 0, multiplier: 0.1, cache_enabled: false },
        ];
        assert_eq!(order_by_auto(&targets, &models, &lat)[0], 1);
    }

    #[test]
    fn auto_empty_and_single() {
        let models: HashMap<i64, ModelDef> = HashMap::new();
        let lat = dashmap::DashMap::new();
        assert!(order_by_auto(&[], &models, &lat).is_empty());
        let one = vec![Target { model_id: 1, weight: 1, priority: 0, multiplier: 1.0, cache_enabled: false }];
        assert_eq!(order_by_auto(&one, &models, &lat), vec![0]);
    }

    // ---- resolve_priority(两级寻址) ----

    /// 构造两组路由的测试路由图:
    /// 组 1:同名 "pub" → 上游 A(openai/http://a/ma);组 2:同名 "pub" → 上游 B(anthropic/http://b/mb);
    /// 组 2 另有 "only-b" → 上游 B。组 1 策略 priority,组 2 策略 round_robin(验证组级策略各自生效)。
    fn two_level_routing() -> Routing {
        let conn = |kind: ProviderKind, base: &str| ProviderConn {
            kind,
            base_url: base.to_string(),
            api_key: None,
            concurrency: None,
            headers: Vec::new(),
        };
        let mut providers = HashMap::new();
        providers.insert("pa".to_string(), conn(ProviderKind::Openai, "http://a"));
        providers.insert("pb".to_string(), conn(ProviderKind::Anthropic, "http://b"));
        let mut models = HashMap::new();
        let mut ma = mdl("ma", None, None);
        ma.provider = "pa".into();
        models.insert(1i64, ma);
        let mut mb = mdl("mb", None, None);
        mb.provider = "pb".into();
        models.insert(2i64, mb);
        let mut g1: HashMap<String, Vec<Target>> = HashMap::new();
        g1.insert("pub".to_string(), vec![Target { model_id: 1, weight: 100, priority: 0, multiplier: 1.0, cache_enabled: false }]);
        let mut g2: HashMap<String, Vec<Target>> = HashMap::new();
        g2.insert("pub".to_string(), vec![Target { model_id: 2, weight: 100, priority: 0, multiplier: 1.0, cache_enabled: false }]);
        g2.insert("only-b".to_string(), vec![Target { model_id: 2, weight: 100, priority: 0, multiplier: 1.0, cache_enabled: false }]);
        let mut groups = HashMap::new();
        groups.insert(1i64, g1);
        groups.insert(2i64, g2);
        let mut group_strategy = HashMap::new();
        group_strategy.insert(1i64, Strategy::WeightedRandom);
        group_strategy.insert(2i64, Strategy::WeightedRoundRobin);
        Routing {
            providers,
            models,
            group_names: HashMap::new(),
            group_strategy,
            group_concurrency: HashMap::new(),
            time_rules: HashMap::new(),
            groups,
            user_group_models: HashMap::new(),
        }
    }

    fn rr() -> dashmap::DashMap<i64, std::sync::atomic::AtomicU32> {
        dashmap::DashMap::new()
    }

    #[test]
    fn priority_first_group_wins_then_failover_chain() {
        let r = two_level_routing();
        // 用户组绑定 [组1, 组2]:首候选必须来自高优先级的组 1,组 2 拼接为容灾链。
        let out = r.resolve_priority(&[1, 2], "pub", &rr(), &dashmap::DashMap::new(), 0, None).unwrap();
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].upstream_model, "ma");
        assert_eq!(out[1].upstream_model, "mb");
        assert_eq!(out[0].group_id, 1);
        assert_eq!(out[1].group_id, 2);
    }

    #[test]
    fn priority_order_flips_with_binding_order() {
        let r = two_level_routing();
        // 绑定顺序反过来 [组2, 组1]:首候选来自组 2。
        let out = r.resolve_priority(&[2, 1], "pub", &rr(), &dashmap::DashMap::new(), 0, None).unwrap();
        assert_eq!(out[0].upstream_model, "mb");
        assert_eq!(out[1].upstream_model, "ma");
    }

    #[test]
    fn priority_skips_group_without_model() {
        let r = two_level_routing();
        // "only-b" 只在组 2:即使组 1 优先级更高,也直接落到组 2。
        let out = r.resolve_priority(&[1, 2], "only-b", &rr(), &dashmap::DashMap::new(), 0, None).unwrap();
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].upstream_model, "mb");
    }

    #[test]
    fn priority_empty_groups_not_allowed() {
        let r = two_level_routing();
        match r.resolve_priority(&[], "pub", &rr(), &dashmap::DashMap::new(), 0, None) {
            Err(e) => assert!(matches!(e, ApiError::ModelNotAllowed(_))),
            Ok(_) => panic!("expected ModelNotAllowed"),
        }
    }

    #[test]
    fn priority_unknown_model_not_found() {
        let r = two_level_routing();
        match r.resolve_priority(&[1, 2], "nope", &rr(), &dashmap::DashMap::new(), 0, None) {
            Err(e) => assert!(matches!(e, ApiError::ModelNotFound(_))),
            Ok(_) => panic!("expected ModelNotFound"),
        }
    }

    #[test]
    fn priority_cross_group_dedup_same_upstream() {
        // 两组指向同一 (kind, base_url, upstream_model):只保留高优先级组的那份。
        let mut r = two_level_routing();
        // 把组 2 的 "pub" 也指到模型 1(供应商 pa/http://a/ma)。
        r.groups.get_mut(&2).unwrap().insert(
            "pub".to_string(),
            vec![Target { model_id: 1, weight: 100, priority: 0, multiplier: 1.0, cache_enabled: false }],
        );
        let out = r.resolve_priority(&[1, 2], "pub", &rr(), &dashmap::DashMap::new(), 0, None).unwrap();
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].upstream_model, "ma");
    }

    #[test]
    fn union_names_and_cards_merge() {
        let r = two_level_routing();
        let names = r.model_names_union(&[1, 2]);
        assert_eq!(names, vec!["only-b".to_string(), "pub".to_string()]);
        let cards = r.model_cards_union(&[1, 2]);
        let pub_card = cards.iter().find(|c| c.name == "pub").unwrap();
        // 同名合并:上游数 = 组1(1) + 组2(1)。
        assert_eq!(pub_card.upstreams, 2);
    }
}

