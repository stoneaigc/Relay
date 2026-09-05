//! 模型价格表:单价单位统一为 $/1M tokens。
//!
//! 价格来源优先级:模型手动定价(models.input_price/output_price)
//! > 内置默认价表(按模型名前缀匹配)> 未定价(成本按无法估算处理)。

/// 内置默认价表(主流模型,前缀匹配,顺序敏感:长/特异前缀在前)。格式:(前缀, 输入单价, 输出单价)。
const DEFAULT_PRICES: &[(&str, f64, f64)] = &[
    // OpenAI
    ("gpt-4o-mini", 0.15, 0.6),
    ("gpt-4o", 2.5, 10.0),
    ("gpt-4.1-mini", 0.4, 1.6),
    ("gpt-4.1-nano", 0.1, 0.4),
    ("gpt-4.1", 2.0, 8.0),
    ("o4-mini", 1.1, 4.4),
    ("o3-mini", 1.1, 4.4),
    ("o3", 2.0, 8.0),
    ("gpt-3.5-turbo", 0.5, 1.5),
    // Anthropic
    ("claude-opus-4", 15.0, 75.0),
    ("claude-sonnet-4", 3.0, 15.0),
    ("claude-3-5-sonnet", 3.0, 15.0),
    ("claude-3-5-haiku", 0.8, 4.0),
    ("claude-3-haiku", 0.25, 1.25),
    // DeepSeek
    ("deepseek-reasoner", 0.55, 2.19),
    ("deepseek-chat", 0.27, 1.1),
    // 通义千问
    ("qwen-max", 1.6, 6.4),
    ("qwen-plus", 0.4, 1.2),
    ("qwen-turbo", 0.05, 0.2),
    // 智谱 / Kimi / Gemini
    ("glm-4.5", 0.6, 2.2),
    ("glm-4-flash", 0.0, 0.0),
    ("glm-4", 0.1, 0.1),
    ("kimi-k2", 0.6, 2.5),
    ("gemini-2.5-pro", 1.25, 10.0),
    ("gemini-2.5-flash", 0.3, 2.5),
    ("gemini-2.0-flash", 0.1, 0.4),
];

/// 按模型名前缀查内置默认价表(大小写不敏感)。
pub fn default_price(model: &str) -> Option<(f64, f64)> {
    let m = model.to_ascii_lowercase();
    DEFAULT_PRICES
        .iter()
        .find(|(prefix, _, _)| m.starts_with(prefix))
        .map(|(_, i, o)| (*i, *o))
}

/// 逐侧解析生效单价:手动定价优先,未手动定价的一侧回退内置默认表。
pub fn resolve_prices(
    model: &str,
    input_price: Option<f64>,
    output_price: Option<f64>,
) -> (Option<f64>, Option<f64>) {
    let d = default_price(model);
    (
        input_price.or_else(|| d.map(|(i, _)| i)),
        output_price.or_else(|| d.map(|(_, o)| o)),
    )
}

/// 按单价计算成本(USD):(tokens / 1e6) × 单价 后逐侧相加。
pub fn cost_usd(input_tokens: u64, output_tokens: u64, input_price: f64, output_price: f64) -> f64 {
    input_tokens as f64 / 1e6 * input_price + output_tokens as f64 / 1e6 * output_price
}

/// 估算一次调用的成本;任一侧价格无法确定时返回 None(由调用方决定按 0 计或拒绝)。
/// 预留:阶段二批E CostAware 策略与费用维度接线。
#[allow(dead_code)]
pub fn estimate_cost(
    model: &str,
    input_tokens: u64,
    output_tokens: u64,
    manual_input: Option<f64>,
    manual_output: Option<f64>,
) -> Option<f64> {
    let (i, o) = resolve_prices(model, manual_input, manual_output);
    Some(cost_usd(input_tokens, output_tokens, i?, o?))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_price_matches_known_models() {
        assert_eq!(default_price("gpt-4o-mini"), Some((0.15, 0.6)));
        assert_eq!(default_price("gpt-4o-2024-08-06"), Some((2.5, 10.0)));
        assert_eq!(default_price("claude-sonnet-4-20250514"), Some((3.0, 15.0)));
        assert_eq!(default_price("deepseek-chat"), Some((0.27, 1.1)));
    }

    #[test]
    fn default_price_prefix_order_specific_first() {
        // 带日期后缀的变体仍命中对应前缀
        assert_eq!(default_price("claude-3-5-sonnet-20241022"), Some((3.0, 15.0)));
        assert_eq!(default_price("claude-3-haiku-20240307"), Some((0.25, 1.25)));
    }

    #[test]
    fn default_price_case_insensitive() {
        assert_eq!(default_price("GPT-4O"), Some((2.5, 10.0)));
    }

    #[test]
    fn default_price_unknown_model() {
        assert_eq!(default_price("totally-custom-model"), None);
    }

    #[test]
    fn resolve_prices_manual_overrides_default() {
        let (i, o) = resolve_prices("gpt-4o", Some(1.0), None);
        assert_eq!(i, Some(1.0));
        assert_eq!(o, Some(10.0)); // 输出侧回退内置表
    }

    #[test]
    fn resolve_prices_unknown_falls_back_to_none() {
        let (i, o) = resolve_prices("custom-x", None, None);
        assert_eq!(i, None);
        assert_eq!(o, None);
    }

    #[test]
    fn cost_usd_math() {
        // 1M 输入 @2.5 + 0.5M 输出 @10 = 7.5
        let c = cost_usd(1_000_000, 500_000, 2.5, 10.0);
        assert!((c - 7.5).abs() < 1e-9);
    }

    #[test]
    fn estimate_cost_resolves_via_default_table() {
        let c = estimate_cost("gpt-4o", 1_000_000, 500_000, None, None).unwrap();
        assert!((c - 7.5).abs() < 1e-9);
    }

    #[test]
    fn estimate_cost_unpriced_returns_none() {
        assert!(estimate_cost("custom-x", 1000, 1000, None, None).is_none());
    }
}
