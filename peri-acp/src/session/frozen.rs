//! Shared frozen-data construction for session/new.
//!
//! Both TUI and Stdio paths build identical frozen data at session creation.
//! This module provides a single entry point to eliminate duplication.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use peri_agent::llm::BaseModel;
use peri_middlewares::hitl::jev::{self, config::JevConfig, rules, JevRuleLoader, JevRulesSlot};

use crate::session::executor::FrozenSessionData;

/// 会话内共享的规则槽。惰性提炼，落位后门自动开始携带 CLAUDE.md 策略。
pub type SessionJevRulesSlot = JevRulesSlot;

/// 用当前 provider 构造规则提炼模型（`session/new` 调用方传入 `build_frozen_session_data`）。
pub fn rule_model_from(provider: &crate::provider::LlmProvider) -> Option<Arc<dyn BaseModel>> {
    Some(provider.clone().into_model().into())
}

/// Build frozen session data from the given parameters.
///
/// Called once at session/new, capturing date/language/CLAUDE.md/skills/system_prompt.
/// `language` should be the user's configured language (`None` → auto-detect).
///
/// `rule_model` 用于把 CLAUDE.md 提炼成 Jev 安全规则（一次 LLM 调用，结果进程内缓存）。
/// 传 `None` 则不做提炼，门不携带用户策略。
pub fn build_frozen_session_data(
    cwd: &str,
    language: Option<&str>,
    plugin_skill_dirs: &[PathBuf],
    plugin_agent_dirs: &[PathBuf],
    frozen_date: &str,
    rule_model: Option<Arc<dyn BaseModel>>,
) -> FrozenSessionData {
    let (frozen_claude_md, frozen_claude_local_md) =
        peri_middlewares::AgentsMdMiddleware::read_frozen_content(cwd);

    // 个人 → 项目 → hooks → 全局，越靠前越权威（超长时先丢全局）。
    // 优先级数字与 `source=` 写进标题：模型据此按小号覆盖大号，并给每条规则标注来源。
    let global_claude_md = peri_middlewares::AgentsMdMiddleware::read_global_content();
    let hook_rules = peri_middlewares::hitl::jev::sources::collect_hook_rules(cwd);
    let jev_rule_loader = build_jev_rule_loader(
        &[
            (
                "personal",
                "Priority 1 (highest) — personal rules ({cwd}/CLAUDE.local.md)",
                frozen_claude_local_md.as_deref(),
            ),
            (
                "project",
                "Priority 2 — project rules ({cwd}/CLAUDE.md, {cwd}/AGENTS.md)",
                frozen_claude_md.as_deref(),
            ),
            (
                "hooks",
                "Priority 3 — project hook scripts ({cwd}/.claude/hooks/*.sh): each guard \
                 condition and its BLOCKED message states a standing rule",
                hook_rules.as_deref(),
            ),
            (
                "global",
                "Priority 4 (lowest) — global user rules (~/.claude/CLAUDE.md)",
                global_claude_md.as_deref(),
            ),
        ],
        rule_model,
    );

    let frozen_skill_summary =
        peri_middlewares::SkillsMiddleware::build_frozen_summary(cwd, plugin_skill_dirs);

    let features = crate::prompt::PromptFeatures::detect();
    let frozen_system_prompt = crate::prompt::build_system_prompt(
        None,
        cwd,
        features,
        plugin_agent_dirs,
        Some(frozen_date),
        language,
    );

    let is_git_repo = std::path::Path::new(cwd).join(".git").exists();

    FrozenSessionData {
        system_prompt: frozen_system_prompt,
        claude_md: frozen_claude_md,
        claude_local_md: frozen_claude_local_md,
        jev_rule_loader,
        skill_summary: frozen_skill_summary,
        date: frozen_date.to_string(),
        is_git_repo,
        language: language.map(|s| s.to_string()),
    }
}

/// 组装 CLAUDE.md 来源文本并建**惰性**规则加载器。
///
/// **这里不调用模型。** 很多会话是"问一个问题就结束"，根本不会触发门控；在建会话时
/// 提炼等于为每个会话白付一次 LLM 调用。真正提炼发生在门第一次判定时
/// （[`peri_middlewares::hitl::jev::JevRuleLoader::ensure_loaded`]）。
///
/// 提炼结果按内容哈希缓存在进程内：同一份 CLAUDE.md 只提炼一次。
/// CLAUDE.md 保持**语义化自由文本**——用户不需要写任何特定格式，也不做解析。
fn build_jev_rule_loader(
    sections: &[(&str, &str, Option<&str>)],
    model: Option<Arc<dyn BaseModel>>,
) -> Option<Arc<JevRuleLoader>> {
    let model = model?;
    let config = JevConfig::from_env();
    let source = jev::compose_policy(sections, config.max_rule_source_len);
    if source.trim().is_empty() {
        return None;
    }
    // 进程缓存已命中 → 直接放进槽，连惰性触发都省了
    let slot = rules::empty_slot();
    if let Some(hit) = rules::cached_rules(&source) {
        tracing::debug!("Jev 规则命中进程缓存，无需提炼");
        *slot.write() = Some(hit);
    }
    Some(Arc::new(JevRuleLoader::new(
        source,
        model,
        Duration::from_millis(config.rule_timeout_ms),
        config.rule_chunk_len,
        config.max_rule_chunks,
        slot,
    )))
}
