use std::collections::HashMap;

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

/// 全量内存路由图,放在 ArcSwap 里热替换。
#[derive(Default, Clone)]
pub struct Routing {
    pub providers: HashMap<String, ProviderConn>,
    pub models: HashMap<i64, ModelDef>,
    pub group_names: HashMap<i64, String>,
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

    /// 解析该对外模型的全部可用目标(加权随机排序),用于「负载均衡 + 故障转移」。
    /// 1) 先把 targets 按「加权随机不放回」排成一个候选顺序(第一条就是加权随机命中,后面是 failover 次序);
    /// 2) 然后按 (kind + base_url + upstream_model) 去重,避免重复打同一个上游。
    pub fn resolve_all(&self, group_id: i64, requested: &str) -> Result<Vec<Resolved>, ApiError> {
        let group = self
            .groups
            .get(&group_id)
            .ok_or_else(|| ApiError::ModelNotAllowed(requested.to_string()))?;
        let targets = group
            .get(requested)
            .filter(|v| !v.is_empty())
            .ok_or_else(|| ApiError::ModelNotFound(requested.to_string()))?;
        let order = shuffle_weighted(targets);
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
                multiplier: t.multiplier,
                weight: t.weight,
            });
        }
        if out.is_empty() {
            return Err(ApiError::NoTarget(requested.to_string()));
        }
        Ok(out)
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

