use std::{
    fs,
    path::{Path, PathBuf},
};

use crate::{
    hooks::types::{HooksConfig, RegisteredHook},
    plugin::types::PluginManifest,
};

/// Extract hooks config from a plugin.
///
/// Priority:
/// 1. `hooks/hooks.json` file in plugin install directory
/// 2. `hooks` field in `plugin.json` manifest
pub(crate) fn extract_hooks(manifest: &PluginManifest, install_path: &Path) -> Option<HooksConfig> {
    // Priority 1: hooks/hooks.json file
    let hooks_file = install_path.join("hooks").join("hooks.json");
    if hooks_file.exists() {
        if let Ok(content) = fs::read_to_string(&hooks_file) {
            if let Ok(config) = serde_json::from_str::<HooksConfig>(&content) {
                return Some(config);
            }
        }
    }

    // Priority 2: plugin.json hooks field
    manifest.hooks.clone()
}

/// Load hooks from `~/.claude/settings.json` global `hooks` field.
///
/// Returns a list of `RegisteredHook` with `plugin_name = "settings.json"`.
pub fn load_global_settings_hooks() -> Vec<RegisteredHook> {
    let claude_dir = match dirs_next::home_dir() {
        Some(d) => d.join(".claude"),
        None => {
            tracing::debug!("Cannot determine home directory for global settings");
            return Vec::new();
        }
    };
    let settings_path = claude_dir.join("settings.json");
    if !settings_path.exists() {
        tracing::debug!("No settings.json at {}", settings_path.display());
        return Vec::new();
    }

    let content = match fs::read_to_string(&settings_path) {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!("Failed to read {}: {}", settings_path.display(), e);
            return Vec::new();
        }
    };

    let value: serde_json::Value = match serde_json::from_str(&content) {
        Ok(v) => v,
        Err(e) => {
            tracing::warn!("Failed to parse {}: {}", settings_path.display(), e);
            return Vec::new();
        }
    };

    let hooks_value = match value.get("hooks") {
        Some(h) if h.is_object() => h,
        _ => return Vec::new(),
    };

    let hooks_config: HooksConfig = match serde_json::from_value(hooks_value.clone()) {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!(
                "Failed to parse hooks config from {}: {}",
                settings_path.display(),
                e
            );
            return Vec::new();
        }
    };

    let mut hooks = Vec::new();
    for (event, rules) in &hooks_config {
        for rule in rules {
            for hook_def in &rule.hooks {
                hooks.push(RegisteredHook {
                    hook: hook_def.clone(),
                    event: event.clone(),
                    matcher: rule
                        .matcher
                        .clone()
                        .or_else(|| hook_def.get_matcher().cloned()),
                    plugin_name: "settings.json".to_string(),
                    plugin_id: "settings.global".to_string(),
                    plugin_root: claude_dir.clone(),
                    plugin_data_dir: claude_dir.clone(),
                    plugin_options: std::collections::HashMap::new(),
                });
            }
        }
    }

    tracing::info!(
        "Loaded {} hooks from ~/.claude/settings.json ({} events)",
        hooks.len(),
        hooks_config.len()
    );

    hooks
}

/// Load hooks from `{cwd}/.claude/settings.local.json` `hooks` field.
///
/// Returns a list of `RegisteredHook` with `plugin_name = "settings.local.json"`.

/// Canonicalized-cwd comparison against trusted entries (component-aware:
/// `/proj` trusts `/proj/sub` but not the sibling `/proj2`).
fn is_cwd_trusted(cwd_canon: &Path, trusted_entries: &[PathBuf]) -> bool {
    trusted_entries
        .iter()
        .any(|t| cwd_canon == t || cwd_canon.starts_with(t))
}

/// Check if a project directory is trusted for loading local hooks (#18).
///
/// Trust is granted via:
/// 1. `CC_CODE_TRUST_PROJECT_HOOKS=1` env var (applies to all projects), or
/// 2. Project path listed in `~/.cc-code/trusted_projects` (one per line).
pub fn is_project_trusted(cwd: &str) -> bool {
    // Env var override: trust all
    if std::env::var("CC_CODE_TRUST_PROJECT_HOOKS").as_deref() == Ok("1") {
        return true;
    }
    // Check trusted_projects file
    let home = match std::env::var("HOME") {
        Ok(h) => h,
        Err(_) => return false,
    };
    let trust_file = Path::new(&home).join(".cc-code").join("trusted_projects");
    let content = match std::fs::read_to_string(&trust_file) {
        Ok(c) => c,
        Err(_) => return false,
    };
    // Canonicalize for comparison (resolve symlinks, normalize)
    let cwd_canon = Path::new(cwd)
        .canonicalize()
        .unwrap_or_else(|_| Path::new(cwd).to_path_buf());
    let entries: Vec<PathBuf> = content
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(|l| {
            Path::new(l)
                .canonicalize()
                .unwrap_or_else(|_| Path::new(l).to_path_buf())
        })
        .collect();
    is_cwd_trusted(&cwd_canon, &entries)
}

/// Add a project directory to the trusted list (#18).
pub fn trust_project(cwd: &str) -> std::io::Result<()> {
    let home = std::env::var("HOME")
        .map_err(|_| std::io::Error::new(std::io::ErrorKind::NotFound, "HOME not set"))?;
    let cc_dir = Path::new(&home).join(".cc-code");
    std::fs::create_dir_all(&cc_dir)?;
    let trust_file = cc_dir.join("trusted_projects");
    let cwd_canon = Path::new(cwd)
        .canonicalize()
        .unwrap_or_else(|_| Path::new(cwd).to_path_buf());
    let cwd_str = cwd_canon.to_string_lossy().to_string();
    // Avoid duplicates
    let existing = std::fs::read_to_string(&trust_file).unwrap_or_default();
    if existing.lines().any(|l| l.trim() == cwd_str) {
        return Ok(());
    }
    use std::io::Write;
    let mut f = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&trust_file)?;
    writeln!(f, "{}", cwd_str)?;
    Ok(())
}

/// Read the `hooks` field from `{cwd}/.claude/<file_name>` and build
/// `RegisteredHook`s. Callers own the trust decision; this only parses.
///
/// `plugin_name` is shown in the /hooks panel, so the project file uses
/// `"settings.json (project)"` to stay distinguishable from the global
/// `~/.claude/settings.json` (which reports `"settings.json"`).
fn load_hooks_from_project_file(
    cwd: &str,
    file_name: &str,
    plugin_name: &str,
    plugin_id: &str,
) -> Vec<RegisteredHook> {
    let settings_path = Path::new(cwd).join(".claude").join(file_name);
    if !settings_path.exists() {
        tracing::debug!("No {} at {}", file_name, settings_path.display());
        return Vec::new();
    }

    let content = match fs::read_to_string(&settings_path) {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!("Failed to read {}: {}", settings_path.display(), e);
            return Vec::new();
        }
    };

    // Parse the top-level JSON to extract the `hooks` field
    let value: serde_json::Value = match serde_json::from_str(&content) {
        Ok(v) => v,
        Err(e) => {
            tracing::warn!("Failed to parse {}: {}", settings_path.display(), e);
            return Vec::new();
        }
    };

    let hooks_value = match value.get("hooks") {
        Some(h) if h.is_object() => h,
        _ => return Vec::new(),
    };

    let hooks_config: HooksConfig = match serde_json::from_value(hooks_value.clone()) {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!(
                "Failed to parse hooks config from {}: {}",
                settings_path.display(),
                e
            );
            return Vec::new();
        }
    };

    let mut hooks = Vec::new();
    for (event, rules) in &hooks_config {
        for rule in rules {
            for hook_def in &rule.hooks {
                hooks.push(RegisteredHook {
                    hook: hook_def.clone(),
                    event: event.clone(),
                    matcher: rule
                        .matcher
                        .clone()
                        .or_else(|| hook_def.get_matcher().cloned()),
                    plugin_name: plugin_name.to_string(),
                    plugin_id: plugin_id.to_string(),
                    plugin_root: Path::new(cwd).to_path_buf(),
                    plugin_data_dir: Path::new(cwd).join(".claude"),
                    plugin_options: std::collections::HashMap::new(),
                });
            }
        }
    }

    tracing::info!(
        "Loaded {} hooks from {} ({} events)",
        hooks.len(),
        settings_path.display(),
        hooks_config.len()
    );

    hooks
}

pub fn load_settings_local_hooks(cwd: &str) -> Vec<RegisteredHook> {
    // #18 fix: Require explicit trust for project-local hooks to prevent clone-to-RCE.
    // Hooks from .claude/settings.local.json are only loaded if the project directory
    // is in the trusted list (~/.cc-code/trusted_projects) or CC_CODE_TRUST_PROJECT_HOOKS=1.
    if !is_project_trusted(cwd) {
        tracing::warn!(
            "Skipping hooks from untrusted project at {}. To allow, run: cc-code trust-project '{}' (or set CC_CODE_TRUST_PROJECT_HOOKS=1)",
            cwd, cwd
        );
        return Vec::new();
    }
    load_hooks_from_project_file(
        cwd,
        "settings.local.json",
        "settings.local.json",
        "settings.local",
    )
}

/// Load hooks from `{cwd}/.claude/settings.json` (project settings).
///
/// This is the committed, team-shared project config — previously no code
/// path read it at all, so project-level hooks silently never fired while
/// global (`~/.claude/settings.json`) and local (`.claude/settings.local.json`)
/// hooks worked. Same trust gate as the local file: a cloned repo must not
/// auto-execute hooks before the user trusts it (see #18).
pub fn load_settings_project_hooks(cwd: &str) -> Vec<RegisteredHook> {
    if !is_project_trusted(cwd) {
        tracing::warn!(
            "Skipping project hooks from untrusted project at {}. To allow, run: cc-code trust-project '{}' (or set CC_CODE_TRUST_PROJECT_HOOKS=1)",
            cwd, cwd
        );
        return Vec::new();
    }
    load_hooks_from_project_file(
        cwd,
        "settings.json",
        "settings.json (project)",
        "settings.project",
    )
}

#[cfg(test)]
#[path = "loader_test.rs"]
mod tests;
