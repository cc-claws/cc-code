//! ACP protocol state builders.
//!
//! Converts internal agent state into ACP protocol types
//! (modes, models, config options) for `session/new` and `session/set_*` responses.

use parking_lot::RwLock;
use cc_middlewares::prelude::{PermissionMode, SharedPermissionMode};

use crate::provider::{
    format_model_selection_value, LlmProvider, PeriConfig, ProviderConfig, ProviderModels,
    ThinkingConfig,
};

pub use agent_client_protocol_schema::{
    ModelId, ModelInfo, SessionConfigId, SessionConfigOption, SessionConfigOptionCategory,
    SessionConfigSelectOption, SessionConfigSelectOptions, SessionConfigValueId, SessionMode,
    SessionModeId, SessionModeState, SessionModelState,
};

/// Parse a mode ID string into a `PermissionMode`.
///
/// 只剩两档；**未知取值一律回退 `AutoMode`**（默认档），
/// 避免一个陈旧/异常的值意外滑进 `Bypass`。
pub fn parse_permission_mode(mode_id: &str) -> PermissionMode {
    match mode_id {
        "bypass" => PermissionMode::Bypass,
        _ => PermissionMode::AutoMode,
    }
}

/// Apply a thinking effort level to `PeriConfig` (writes through `RwLock`).
pub fn apply_thinking_effort(peri_config: &RwLock<PeriConfig>, effort: &str) {
    let mut cfg = peri_config.write();
    let thinking = cfg.config.thinking.get_or_insert_with(|| ThinkingConfig {
        enabled: true,
        budget_tokens: 8000,
        effort: "medium".to_string(),
        max_tokens: 32000,
    });
    thinking.enabled = true;
    thinking.effort = effort.to_string();
}

/// Build ACP `SessionModeState` from the current permission mode.
pub fn build_mode_state(pm: &SharedPermissionMode) -> SessionModeState {
    let current = pm.load();
    let current_id = match current {
        PermissionMode::AutoMode => "auto",
        PermissionMode::Bypass => "bypass",
    };
    let all_modes = vec![
        SessionMode::new(SessionModeId::new("auto"), "Auto")
            .description("Semantic gate decides per call; asks only when unsure"),
        SessionMode::new(SessionModeId::new("bypass"), "Bypass").description("Allow everything"),
    ];
    SessionModeState::new(SessionModeId::new(current_id), all_modes)
}

/// Build ACP `SessionModelState` from provider and config.
pub fn build_model_state(provider: &LlmProvider, peri_config: &PeriConfig) -> SessionModelState {
    let active_value = active_model_selection_value(peri_config);
    let mut available = build_model_infos(&peri_config.config.providers);
    if available.is_empty() {
        available.push(ModelInfo::new(
            ModelId::new("current".to_string()),
            provider.model_name().to_string(),
        ));
    }

    SessionModelState::new(ModelId::new(active_value), available)
}

/// Build ACP `SessionConfigOption` list from config.
///
/// Per ACP spec, config options supersede the older Session Modes API.
/// Returns mode, model, and thinking_effort in priority order (higher priority first).
pub fn build_config_options(
    peri_config: &PeriConfig,
    provider: &LlmProvider,
    current_mode: PermissionMode,
) -> Vec<SessionConfigOption> {
    let mut options = Vec::with_capacity(3);

    // ── Mode (category: mode) ──
    let current_mode_id = match current_mode {
        PermissionMode::AutoMode => "auto",
        PermissionMode::Bypass => "bypass",
    };
    let mode_options = vec![
        SessionConfigSelectOption::new(SessionConfigValueId::new("auto"), "Auto"),
        SessionConfigSelectOption::new(SessionConfigValueId::new("bypass"), "Bypass"),
    ];
    options.push(
        SessionConfigOption::select(
            SessionConfigId::new("mode"),
            "Session Mode",
            SessionConfigValueId::new(current_mode_id),
            SessionConfigSelectOptions::Ungrouped(mode_options),
        )
        .category(SessionConfigOptionCategory::Mode),
    );

    // ── Model (category: model) ──
    let active_value = active_model_selection_value(peri_config);
    let mut model_options = build_model_config_options(&peri_config.config.providers);
    if model_options.is_empty() {
        model_options.push(SessionConfigSelectOption::new(
            SessionConfigValueId::new("current".to_string()),
            provider.model_name().to_string(),
        ));
    }
    options.push(
        SessionConfigOption::select(
            SessionConfigId::new("model"),
            "Model",
            SessionConfigValueId::new(active_value),
            SessionConfigSelectOptions::Ungrouped(model_options),
        )
        .category(SessionConfigOptionCategory::Model),
    );

    // ── Thinking effort (category: thought_level) ──
    let effort = peri_config
        .config
        .thinking
        .as_ref()
        .map(|t| t.effort.as_str())
        .unwrap_or("medium");
    let thinking_options = vec![
        SessionConfigSelectOption::new(SessionConfigValueId::new("low"), "Low".to_string()),
        SessionConfigSelectOption::new(SessionConfigValueId::new("medium"), "Medium".to_string()),
        SessionConfigSelectOption::new(SessionConfigValueId::new("high"), "High".to_string()),
        SessionConfigSelectOption::new(SessionConfigValueId::new("xhigh"), "XHigh".to_string()),
        SessionConfigSelectOption::new(SessionConfigValueId::new("max"), "Max".to_string()),
    ];
    options.push(
        SessionConfigOption::select(
            SessionConfigId::new("thinking_effort"),
            "Thinking Effort",
            SessionConfigValueId::new(effort),
            SessionConfigSelectOptions::Ungrouped(thinking_options),
        )
        .category(SessionConfigOptionCategory::ThoughtLevel),
    );

    options
}

fn active_model_selection_value(peri_config: &PeriConfig) -> String {
    format_model_selection_value(
        &peri_config.config.active_provider_id,
        &peri_config.config.active_alias,
    )
}

fn build_model_infos(providers: &[ProviderConfig]) -> Vec<ModelInfo> {
    providers
        .iter()
        .flat_map(|provider| {
            ProviderModels::ALL_ALIASES
                .into_iter()
                .filter_map(move |alias| model_entry(provider, alias))
        })
        .map(|(provider, alias, model_name)| {
            ModelInfo::new(
                ModelId::new(format_model_selection_value(&provider.id, alias)),
                format!("{} / {} ({})", provider.display_name(), alias, model_name),
            )
        })
        .collect()
}

fn build_model_config_options(providers: &[ProviderConfig]) -> Vec<SessionConfigSelectOption> {
    providers
        .iter()
        .flat_map(|provider| {
            ProviderModels::ALL_ALIASES
                .into_iter()
                .filter_map(move |alias| model_entry(provider, alias))
        })
        .map(|(provider, alias, model_name)| {
            SessionConfigSelectOption::new(
                SessionConfigValueId::new(format_model_selection_value(&provider.id, alias)),
                format!("{} / {} ({})", provider.display_name(), alias, model_name),
            )
        })
        .collect()
}

fn model_entry<'a>(
    provider: &'a ProviderConfig,
    alias: &'static str,
) -> Option<(&'a ProviderConfig, &'static str, &'a str)> {
    provider
        .models
        .get_model(alias)
        .filter(|model_name| !model_name.is_empty())
        .map(|model_name| (provider, alias, model_name))
}
