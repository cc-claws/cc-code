use parking_lot::RwLock;
use std::collections::HashSet;
use std::sync::OnceLock;

/// Anthropic Extended Thinking 协议模式
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnthropicThinkingMode {
    /// 传统 Extended Thinking：type: "enabled", budget_tokens: u32
    Budget,
    /// 新一代自适应思考：type: "adaptive", 无 budget_tokens，由 output_config.effort 控制
    Adaptive,
}

/// 运行时自学习的模式纠正指示
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThinkingModeCorrection {
    /// 需自愈升级为 Adaptive 协议
    SwitchToAdaptive,
    /// 需降级回退为 Budget 协议
    SwitchToBudget,
}

/// 运行时动态学习的模式覆盖集合（进程生命周期内存有效）
static ADAPTIVE_LEARNED_MODELS: OnceLock<RwLock<HashSet<String>>> = OnceLock::new();
static BUDGET_LEARNED_MODELS: OnceLock<RwLock<HashSet<String>>> = OnceLock::new();

fn adaptive_cache() -> &'static RwLock<HashSet<String>> {
    ADAPTIVE_LEARNED_MODELS.get_or_init(|| RwLock::new(HashSet::new()))
}

fn budget_cache() -> &'static RwLock<HashSet<String>> {
    BUDGET_LEARNED_MODELS.get_or_init(|| RwLock::new(HashSet::new()))
}

fn normalize_model_key(model: &str) -> String {
    model.trim().to_ascii_lowercase()
}

/// 静态规则预判：判断模型名称是否已知原生采用 adaptive thinking
pub fn is_statically_adaptive_model(model: &str) -> bool {
    let m = normalize_model_key(model);

    // 显式排除 Claude 3.x 及 4.5 系列（传统 budget_tokens 思考模型）
    if m.contains("claude-3-")
        || m.contains("-3-5")
        || m.contains("-3.5")
        || m.contains("-3-7")
        || m.contains("-3.7")
        || m.contains("-4-5")
        || m.contains("-4.5")
    {
        return false;
    }

    // Claude 4.6+, 4.7+, 4.8 及 5.x 全系新模型
    m.contains("opus-4-6")
        || m.contains("opus-4-7")
        || m.contains("opus-4-8")
        || m.contains("sonnet-4-6")
        || m.contains("haiku-5")
        || m.contains("sonnet-5")
        || m.contains("opus-5")
        || m.contains("claude-5")
        || m.contains("fable-5")
        || m.contains("mythos-5")
        || m.contains("-5-5")
        || m.contains("-5.5")
}

/// 决定当前请求对该模型应使用的 thinking 模式
///
/// 优先级：动态降级缓存 > 动态自适应缓存 > 静态预判规则
pub fn resolve_thinking_mode(model: &str) -> AnthropicThinkingMode {
    let key = normalize_model_key(model);

    // 1. 检查运行时学习的强制覆盖
    if budget_cache().read().contains(&key) {
        return AnthropicThinkingMode::Budget;
    }
    if adaptive_cache().read().contains(&key) {
        return AnthropicThinkingMode::Adaptive;
    }

    // 2. 静态规则预判
    if is_statically_adaptive_model(&key) {
        AnthropicThinkingMode::Adaptive
    } else {
        AnthropicThinkingMode::Budget
    }
}

/// 运行时学习：标记该模型必须使用 Adaptive 协议
pub fn mark_model_as_adaptive(model: &str) {
    let key = normalize_model_key(model);
    budget_cache().write().remove(&key);
    adaptive_cache().write().insert(key);
}

/// 运行时学习：标记该模型降级回退到 Budget 协议
pub fn mark_model_as_budget(model: &str) {
    let key = normalize_model_key(model);
    adaptive_cache().write().remove(&key);
    budget_cache().write().insert(key);
}

/// 针对单测或重置场景清空内存自学习缓存
#[cfg(test)]
pub fn clear_learned_cache() {
    budget_cache().write().clear();
    adaptive_cache().write().clear();
}

/// 嗅探上游 HTTP 错误响应中的 thinking 协议不兼容特征
///
/// 当检测到协议不匹配时，返回对应的纠正方向：
/// - SwitchToAdaptive：上游报错提示 thinking.type.enabled 不受支持，要求使用 adaptive
/// - SwitchToBudget：上游报错提示 thinking.type.adaptive 不受支持，要求使用 enabled
pub fn detect_thinking_mode_correction(
    status: u16,
    error_text: &str,
) -> Option<ThinkingModeCorrection> {
    if status != 400 {
        return None;
    }

    let lower = error_text.to_ascii_lowercase();

    // 1. 检查是否明确说明 adaptive 不受支持（降级回退）
    let adaptive_unsupported = lower.contains("adaptive\" is not supported")
        || lower.contains("adaptive` is not supported")
        || lower.contains("adaptive' is not supported")
        || lower.contains("adaptive is not supported")
        || lower.contains("adaptive is not a valid");

    let requests_budget = lower.contains("use \"thinking.type.enabled\"")
        || lower.contains("use `thinking.type.enabled`")
        || lower.contains("should be 'enabled'")
        || lower.contains("input should be 'enabled'");

    if adaptive_unsupported || requests_budget {
        return Some(ThinkingModeCorrection::SwitchToBudget);
    }

    // 2. 检查是否明确说明 enabled 不受支持，或要求使用 adaptive（自愈升级）
    let enabled_unsupported = lower.contains("enabled\" is not supported")
        || lower.contains("enabled` is not supported")
        || lower.contains("enabled' is not supported")
        || lower.contains("enabled is not supported")
        || lower.contains("budget_tokens in thinking configuration")
        || lower.contains("unknown field: budget_tokens")
        || lower.contains("unknown field `budget_tokens`");

    let requests_adaptive = lower.contains("use \"thinking.type.adaptive\"")
        || lower.contains("use `thinking.type.adaptive`")
        || lower.contains("should be 'adaptive'")
        || lower.contains("input should be 'adaptive'");

    if enabled_unsupported || requests_adaptive {
        return Some(ThinkingModeCorrection::SwitchToAdaptive);
    }

    None
}
