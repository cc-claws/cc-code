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
    /// 模型不支持思考（如 Haiku 全系无思考支持，或经上游报错确认不支持），不传 thinking 与 output_config
    Disabled,
}

/// 运行时自学习的模式纠正指示
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThinkingModeCorrection {
    /// 需自愈升级为 Adaptive 协议
    SwitchToAdaptive,
    /// 需降级回退为 Budget 协议
    SwitchToBudget,
    /// 需彻底禁用 Thinking 协议
    DisableThinking,
}

/// 运行时动态学习的模式覆盖集合（进程生命周期内存有效）
static ADAPTIVE_LEARNED_MODELS: OnceLock<RwLock<HashSet<String>>> = OnceLock::new();
static BUDGET_LEARNED_MODELS: OnceLock<RwLock<HashSet<String>>> = OnceLock::new();
static DISABLED_LEARNED_MODELS: OnceLock<RwLock<HashSet<String>>> = OnceLock::new();

fn adaptive_cache() -> &'static RwLock<HashSet<String>> {
    ADAPTIVE_LEARNED_MODELS.get_or_init(|| RwLock::new(HashSet::new()))
}

fn budget_cache() -> &'static RwLock<HashSet<String>> {
    BUDGET_LEARNED_MODELS.get_or_init(|| RwLock::new(HashSet::new()))
}

fn disabled_cache() -> &'static RwLock<HashSet<String>> {
    DISABLED_LEARNED_MODELS.get_or_init(|| RwLock::new(HashSet::new()))
}

fn normalize_model_key(model: &str) -> String {
    model.trim().to_ascii_lowercase()
}

/// 静态规则预判：判断模型是否已知天生不支持 thinking（如所有轻量快速的 Haiku 3.x / 4.x 系列）
pub fn is_statically_disabled_model(model: &str) -> bool {
    let m = normalize_model_key(model);
    m.contains("haiku") && !m.contains("haiku-5")
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
    m.contains("-4-6")
        || m.contains("-4.6")
        || m.contains("-4-7")
        || m.contains("-4.7")
        || m.contains("-4-8")
        || m.contains("-4.8")
        || m.contains("sonnet-4-6")
        || m.contains("opus-4-6")
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
/// 优先级：动态禁用缓存 > 动态降级缓存 > 动态自适应缓存 > 静态预判规则
pub fn resolve_thinking_mode(model: &str) -> AnthropicThinkingMode {
    let key = normalize_model_key(model);

    // 1. 检查运行时学习的强制覆盖
    if disabled_cache().read().contains(&key) {
        return AnthropicThinkingMode::Disabled;
    }
    if budget_cache().read().contains(&key) {
        return AnthropicThinkingMode::Budget;
    }
    if adaptive_cache().read().contains(&key) {
        return AnthropicThinkingMode::Adaptive;
    }

    // 2. 静态规则：Haiku 系列天生无 thinking
    if is_statically_disabled_model(&key) {
        return AnthropicThinkingMode::Disabled;
    }

    // 3. 静态规则预判
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
    disabled_cache().write().remove(&key);
    adaptive_cache().write().insert(key);
}

/// 运行时学习：标记该模型降级回退到 Budget 协议
pub fn mark_model_as_budget(model: &str) {
    let key = normalize_model_key(model);
    adaptive_cache().write().remove(&key);
    disabled_cache().write().remove(&key);
    budget_cache().write().insert(key);
}

/// 运行时学习：标记该模型彻底关闭 Thinking 协议
pub fn mark_model_as_disabled(model: &str) {
    let key = normalize_model_key(model);
    adaptive_cache().write().remove(&key);
    budget_cache().write().remove(&key);
    disabled_cache().write().insert(key);
}

/// 针对单测场景仅清理特定模型的学习状态，避免并发测试互相污染
#[cfg(test)]
pub fn remove_learned_model(model: &str) {
    let key = normalize_model_key(model);
    budget_cache().write().remove(&key);
    adaptive_cache().write().remove(&key);
    disabled_cache().write().remove(&key);
}

/// 针对单测或重置场景清空内存自学习缓存
#[cfg(test)]
pub fn clear_learned_cache() {
    budget_cache().write().clear();
    adaptive_cache().write().clear();
    disabled_cache().write().clear();
}

/// 嗅探上游 HTTP 错误响应中的 thinking 协议不兼容特征
///
/// 当检测到协议不匹配时，返回对应的纠正方向：
/// - SwitchToAdaptive：上游明确要求使用 adaptive
/// - SwitchToBudget：上游明确要求使用 enabled
/// - DisableThinking：当前模式已是 Adaptive 仍报错，或上游提示该模型完全不支持 thinking / 缺少字段
pub fn detect_thinking_mode_correction(
    status: u16,
    error_text: &str,
    current_mode: AnthropicThinkingMode,
) -> Option<ThinkingModeCorrection> {
    if status != 400 {
        return None;
    }

    // 归一化：转小写，彻底剥离反斜杠转义与各类引号（双引号、单引号、反引号），
    // 消除因上游错误被包裹在转义 JSON 字符串中（如 \"thinking.type.enabled\"）导致的失配。
    let clean = error_text
        .to_ascii_lowercase()
        .replace('\\', "")
        .replace(['`', '\'', '"'], "");

    // 1. 检查是否明确说明 adaptive 不受支持，或要求使用 enabled（降级回退到 Budget）
    let adaptive_unsupported =
        clean.contains("adaptive is not supported") || clean.contains("adaptive is not a valid");

    let requests_budget = clean.contains("use thinking.type.enabled")
        || clean.contains("should be enabled")
        || clean.contains("input should be enabled");

    if adaptive_unsupported || requests_budget {
        return Some(ThinkingModeCorrection::SwitchToBudget);
    }

    // 2. 检查是否明确说明 enabled 不受支持，或要求使用 adaptive（自愈升级到 Adaptive）
    let enabled_unsupported = clean.contains("enabled is not supported")
        || clean.contains("unknown field: budget_tokens")
        || clean.contains("unknown field budget_tokens");

    let requests_adaptive = clean.contains("use thinking.type.adaptive")
        || clean.contains("should be adaptive")
        || clean.contains("input should be adaptive");

    if current_mode != AnthropicThinkingMode::Adaptive && (enabled_unsupported || requests_adaptive)
    {
        return Some(ThinkingModeCorrection::SwitchToAdaptive);
    }

    // 3. 终极兜底：必须明确包含 thinking 相关的错误特征，才允许触发禁用 thinking 自愈降级；
    // 其余非 thinking 相关的常规业务 400（如无效请求体、key 错误、上下文等）必须直接返回 None，杜绝误拦截。
    let thinking_err = clean.contains("thinking")
        || clean.contains("budget_tokens")
        || clean.contains("output_config");

    if !thinking_err {
        return None;
    }

    if current_mode == AnthropicThinkingMode::Adaptive
        && (clean.contains("not supported")
            || clean.contains("field required")
            || clean.contains("unrecognized")
            || clean.contains("unknown field"))
    {
        return Some(ThinkingModeCorrection::DisableThinking);
    }

    let thinking_completely_unsupported = clean.contains("does not support thinking")
        || clean.contains("thinking is not supported")
        || clean.contains("extended thinking is not available")
        || clean.contains("budget_tokens: field required")
        || clean.contains("budget_tokens in thinking configuration");

    if thinking_completely_unsupported {
        return Some(ThinkingModeCorrection::DisableThinking);
    }

    None
}
