use std::collections::HashMap;

use crate::config::ProviderKind;
use crate::error::ApiError;

/// 一个上游连接(协议 + 地址 + 密钥)。
#[derive(Clone)]
pub struct ProviderConn {
    pub kind: ProviderKind,
    pub base_url: String,
    pub api_key: Option<String>,
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
}

impl Routing {
    /// 按「用户所在组 + 请求的对外模型名」精确路由到指定模型(同名多条则加权)。
    pub fn resolve(&self, group_id: i64, requested: &str) -> Result<Resolved, ApiError> {
        let group = self
            .groups
            .get(&group_id)
            .ok_or_else(|| ApiError::ModelNotAllowed(requested.to_string()))?;
        let targets = group
            .get(requested)
            .filter(|v| !v.is_empty())
            .ok_or_else(|| ApiError::ModelNotFound(requested.to_string()))?;
        let t = pick_weighted(targets);
        let m = self
            .models
            .get(&t.model_id)
            .ok_or_else(|| ApiError::NoTarget(requested.to_string()))?;
        let p = self
            .providers
            .get(&m.provider)
            .ok_or_else(|| ApiError::NoTarget(requested.to_string()))?;
        Ok(Resolved {
            provider: m.provider.clone(),
            kind: p.kind,
            base_url: p.base_url.clone(),
            api_key: p.api_key.clone(),
            upstream_model: m.upstream_model.clone(),
            multiplier: t.multiplier,
        })
    }

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
}

fn pick_weighted(targets: &[Target]) -> &Target {
    let total: u32 = targets.iter().map(|t| t.weight.max(1)).sum();
    let mut pick = rand::random::<u32>() % total.max(1);
    for t in targets {
        let w = t.weight.max(1);
        if pick < w {
            return t;
        }
        pick -= w;
    }
    targets.last().unwrap()
}

