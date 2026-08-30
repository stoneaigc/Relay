//! 优先级路由 + 故障转移链 集成测试。
//! 测试 Routing::resolve_all 在不同策略和场景下的完整行为。

use std::collections::HashMap;
use relay::routing::{
    ModelDef, ProviderConn, Routing, Strategy, Target, TimeRule,
};
use relay::config::ProviderKind;

/// 构造一个包含多个 provider + model + group 的测试路由图。
fn make_test_routing() -> Routing {
    let mut providers = HashMap::new();
    providers.insert("p-openai".into(), ProviderConn {
        kind: ProviderKind::Openai,
        base_url: "https://api.openai.com/v1".into(),
        api_key: Some("sk-openai".into()),
        concurrency: None,
    });
    providers.insert("p-deepseek".into(), ProviderConn {
        kind: ProviderKind::Openai,
        base_url: "https://api.deepseek.com/v1".into(),
        api_key: Some("sk-deepseek".into()),
        concurrency: None,
    });
    providers.insert("p-anthropic".into(), ProviderConn {
        kind: ProviderKind::Openai,
        base_url: "https://api.anthropic.com/v1".into(),
        api_key: Some("sk-anthropic".into()),
        concurrency: None,
    });

    let mut models = HashMap::new();
    // OpenAI models
    models.insert(1, ModelDef { provider: "p-openai".into(), upstream_model: "gpt-4o".into() });
    models.insert(2, ModelDef { provider: "p-openai".into(), upstream_model: "gpt-4o-mini".into() });
    // DeepSeek models
    models.insert(3, ModelDef { provider: "p-deepseek".into(), upstream_model: "deepseek-chat".into() });
    models.insert(4, ModelDef { provider: "p-deepseek".into(), upstream_model: "deepseek-reasoner".into() });
    // Anthropic model
    models.insert(5, ModelDef { provider: "p-anthropic".into(), upstream_model: "claude-sonnet-4-20250514".into() });

    let mut group_names = HashMap::new();
    group_names.insert(1, "Production".into());

    let mut group_strategy = HashMap::new();
    group_strategy.insert(1, Strategy::WeightedRandom);

    let mut groups = HashMap::new();
    let mut chat_routes = HashMap::new();
    // "chat" 对外模型名 → 3 条路由
    chat_routes.insert("chat".into(), vec![
        Target { model_id: 1, weight: 50, multiplier: 1.5 },  // OpenAI GPT-4o
        Target { model_id: 3, weight: 30, multiplier: 1.0 },  // DeepSeek V3
        Target { model_id: 5, weight: 20, multiplier: 2.0 },  // Claude Sonnet 4
    ]);
    groups.insert(1, chat_routes);

    Routing {
        providers,
        models,
        group_names,
        group_strategy,
        time_rules: HashMap::new(),
        groups,
    }
}

/// 辅助:调用 resolve_all 并提取候选列表(用共享 counter)。
fn resolve_with_counter(routing: &Routing, strategy: Strategy, requested: &str, rr: &dashmap::DashMap<i64, std::sync::atomic::AtomicU32>) -> Vec<(String, String, u32)> {
    let mut r = routing.clone();
    r.group_strategy.insert(1, strategy);
    let result = r.resolve_all(1, requested, rr, 28800).unwrap();
    result.into_iter().map(|r| (r.upstream_model, r.base_url, r.weight)).collect()
}

// ==================== 优先级路由 ====================

#[test]
fn priority_routes_in_descending_weight_order() {
    let rt = make_test_routing();
    let rr = dashmap::DashMap::new();
    let candidates = resolve_with_counter(&rt, Strategy::Priority, "chat", &rr);
    // 权重: 50(OpenAI) > 30(DeepSeek) > 20(Anthropic)
    assert_eq!(candidates[0].0, "gpt-4o");
    assert_eq!(candidates[0].2, 50);
    assert_eq!(candidates[1].0, "deepseek-chat");
    assert_eq!(candidates[1].2, 30);
    assert_eq!(candidates[2].0, "claude-sonnet-4-20250514");
    assert_eq!(candidates[2].2, 20);
}

#[test]
fn priority_all_three_candidates_present() {
    let rt = make_test_routing();
    let rr = dashmap::DashMap::new();
    let candidates = resolve_with_counter(&rt, Strategy::Priority, "chat", &rr);
    assert_eq!(candidates.len(), 3);
}

#[test]
fn priority_deduplicates_same_upstream() {
    let mut rt = make_test_routing();
    // 给 chat 加一条重复的 OpenAI GPT-4o(不同 model_id 但相同上游)
    rt.groups.get_mut(&1).unwrap().get_mut("chat").unwrap()
        .push(Target { model_id: 100, weight: 10, multiplier: 1.0 });
    // model_id=100 没有对应的 ModelDef,会被跳过;但如果添加了就要去重
    // 用另一个 model_id 指向同一个 provider+model 来测试去重
    rt.models.insert(100, ModelDef { provider: "p-openai".into(), upstream_model: "gpt-4o".into() });
    let rr = dashmap::DashMap::new();
    let candidates = resolve_with_counter(&rt, Strategy::Priority, "chat", &rr);
    // 去重后应该只有 3 个(OpenAI 只出现一次)
    let openai_count = candidates.iter().filter(|(m, _, _)| m == "gpt-4o").count();
    assert_eq!(openai_count, 1);
    assert_eq!(candidates.len(), 3);
}

// ==================== 加权随机 ====================

#[test]
fn weighted_random_covers_all_models() {
    let rt = make_test_routing();
    let rr = dashmap::DashMap::new();
    let mut seen = std::collections::HashSet::new();
    for _ in 0..100 {
        let candidates = resolve_with_counter(&rt, Strategy::WeightedRandom, "chat", &rr);
        for (model, _, _) in &candidates {
            seen.insert(model.clone());
        }
    }
    assert!(seen.contains("gpt-4o"));
    assert!(seen.contains("deepseek-chat"));
    assert!(seen.contains("claude-sonnet-4-20250514"));
}

#[test]
fn weighted_random_first_is_not_always_same() {
    let rt = make_test_routing();
    let rr = dashmap::DashMap::new();
    let mut first_models = std::collections::HashSet::new();
    for _ in 0..50 {
        let candidates = resolve_with_counter(&rt, Strategy::WeightedRandom, "chat", &rr);
        first_models.insert(candidates[0].0.clone());
    }
    // 加权随机下,第一个候选不应该总是同一个
    assert!(first_models.len() > 1);
}

// ==================== 简单轮询 ====================

#[test]
fn round_robin_cycles_through_models() {
    let rt = make_test_routing();
    let rr = dashmap::DashMap::new();
    let mut first_models = Vec::new();
    for _ in 0..6 {
        let candidates = resolve_with_counter(&rt, Strategy::RoundRobin, "chat", &rr);
        first_models.push(candidates[0].0.clone());
    }
    // 简单轮询应该循环: A, B, C, A, B, C
    assert_eq!(first_models[0], first_models[3]);
    assert_eq!(first_models[1], first_models[4]);
    assert_eq!(first_models[2], first_models[5]);
}

// ==================== 加权轮询 ====================

#[test]
fn weighted_round_robin_uses_weights() {
    let rt = make_test_routing();
    let rr = dashmap::DashMap::new();
    let mut counts = HashMap::new();
    for _ in 0..300 {
        let candidates = resolve_with_counter(&rt, Strategy::WeightedRoundRobin, "chat", &rr);
        *counts.entry(candidates[0].0.clone()).or_insert(0) += 1;
    }
    // 权重 50:30:20 → 大约 50%:30%:20%
    let total: u32 = counts.values().sum();
    let openai_pct = *counts.get("gpt-4o").unwrap_or(&0) as f64 / total as f64;
    let deepseek_pct = *counts.get("deepseek-chat").unwrap_or(&0) as f64 / total as f64;
    assert!(openai_pct > 0.35, "OpenAI should get ~50%, got {openai_pct:.2}");
    assert!(deepseek_pct > 0.20, "DeepSeek should get ~30%, got {deepseek_pct:.2}");
}

// ==================== 时段规则 ====================

#[test]
fn time_rule_overrides_weight() {
    let mut rt = make_test_routing();
    // 添加时段规则:工作日白天,覆盖 chat 的权重
    let mut weight_map = HashMap::new();
    let mut chat_override = HashMap::new();
    chat_override.insert(1i64, 100u32); // OpenAI 权重设为 100(最高)
    chat_override.insert(3i64, 1u32);   // DeepSeek 权重设为 1(最低)
    weight_map.insert("chat".into(), chat_override);
    rt.time_rules.insert(1, vec![TimeRule {
        id: 1,
        name: "全天生效".into(),
        weekdays: "0-6".into(),
        start_time: "12:00".into(),
        end_time: "11:59".into(), // 跨午夜:12:00→次日11:59,覆盖全天
        multiplier: 1.5,
        weight_map,
        active: true,
    }]);

    // 使用 UTC+8 偏移,周一 10:00 = UTC 02:00
    let rr = dashmap::DashMap::new();
    let result = rt.resolve_all(1, "chat", &rr, 28800).unwrap();
    // 在时段覆盖下,OpenAI 权重变为 100,DeepSeek 变为 1
    // Priority 策略下 OpenAI 应该排第一
    let first = &result[0];
    assert_eq!(first.upstream_model, "gpt-4o");
    assert_eq!(first.weight, 100);
    // 倍率: model(1.5) × time(1.5) = 2.25
    assert!((first.multiplier - 2.25).abs() < 0.01);
}

#[test]
fn time_rule_disabled_ignored() {
    let mut rt = make_test_routing();
    let mut weight_map = HashMap::new();
    let mut chat_override = HashMap::new();
    chat_override.insert(1i64, 100u32);
    weight_map.insert("chat".into(), chat_override);
    rt.time_rules.insert(1, vec![TimeRule {
        id: 1,
        name: "已停用".into(),
        weekdays: "0-6".into(),
        start_time: "00:00".into(),
        end_time: "23:59".into(),
        multiplier: 2.0,
        weight_map,
        active: false, // 停用
    }]);

    let rr = dashmap::DashMap::new();
    let result = rt.resolve_all(1, "chat", &rr, 28800).unwrap();
    // 停用的规则不应生效,倍率应该是 model 原始值(无时段倍率)
    let first = result.iter().find(|r| r.upstream_model == "gpt-4o").unwrap();
    assert!((first.multiplier - 1.5).abs() < 0.01, "disabled rule should not affect multiplier, got {}", first.multiplier);
}

// ==================== 错误场景 ====================

#[test]
fn resolve_unknown_model_returns_error() {
    let rt = make_test_routing();
    let rr = dashmap::DashMap::new();
    let result = rt.resolve_all(1, "nonexistent", &rr, 28800);
    assert!(result.is_err());
}

#[test]
fn resolve_unknown_group_returns_error() {
    let rt = make_test_routing();
    let rr = dashmap::DashMap::new();
    let result = rt.resolve_all(999, "chat", &rr, 28800);
    assert!(result.is_err());
}

// ==================== 倍率计算 ====================

#[test]
fn multiplier_includes_time_multiplier() {
    let mut rt = make_test_routing();
    rt.time_rules.insert(1, vec![TimeRule {
        id: 1,
        name: "高峰".into(),
        weekdays: "0-6".into(),
        start_time: "00:00".into(),
        end_time: "23:59".into(),
        multiplier: 2.0,
        weight_map: HashMap::new(),
        active: true,
    }]);

    let rr = dashmap::DashMap::new();
    let result = rt.resolve_all(1, "chat", &rr, 28800).unwrap();
    // OpenAI: multiplier=1.5 * time_multiplier=2.0 = 3.0
    let openai = result.iter().find(|r| r.upstream_model == "gpt-4o").unwrap();
    assert!((openai.multiplier - 3.0).abs() < 0.01);
}

// ==================== 多对外模型名 ====================

#[test]
fn different_public_names_independent_routing() {
    let mut rt = make_test_routing();
    // 添加 "fast" 对外模型名,只绑一个模型
    rt.groups.get_mut(&1).unwrap().insert("fast".into(), vec![
        Target { model_id: 2, weight: 100, multiplier: 0.8 }, // GPT-4o-mini
    ]);

    let rr1 = dashmap::DashMap::new();
    let chat = rt.resolve_all(1, "chat", &rr1, 28800).unwrap();
    let rr2 = dashmap::DashMap::new();
    let fast = rt.resolve_all(1, "fast", &rr2, 28800).unwrap();

    assert_eq!(chat.len(), 3); // chat 有 3 个候选
    assert_eq!(fast.len(), 1); // fast 只有 1 个
    assert_eq!(fast[0].upstream_model, "gpt-4o-mini");
}
