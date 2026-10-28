use std::{
    fs,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};

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

/// 信任清单文件名（JSONL，一行一条记录）。
const TRUST_FILE_NAME: &str = "trusted_projects.jsonl";
/// 旧版纯文本清单文件名，仅用于一次性迁移。
const LEGACY_TRUST_FILE_NAME: &str = "trusted_projects";
/// 迁移后旧文件的留档名（不删除用户数据）。
const LEGACY_BACKUP_FILE_NAME: &str = "trusted_projects.legacy.bak";

/// `trusted_projects.jsonl` 的一行。
///
/// ```json
/// {"path":"D:\\code\\nt_order","trusted_at":"2026-10-28T07:12:10Z"}
/// ```
///
/// 目前只认 `path`；`trusted_at` 供人工审计与排障用。反序列化保持宽松
/// （未知字段忽略），以便后续在不破坏旧文件的前提下加字段
/// （如按 hook 事件分粒度授权）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrustedProject {
    /// 已信任的项目目录绝对路径。
    pub path: String,
    /// 记录写入时间（RFC 3339，UTC）。旧数据可能没有。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trusted_at: Option<String>,
}

/// 去掉 Windows verbatim 前缀。
///
/// `Path::canonicalize` 在 Windows 上返回 `\\?\D:\x`，直接写进清单会让
/// 用户看到莫名其妙的前缀；而且带前缀与不带前缀的 `Path` 字符串不相等，
/// 会让 `starts_with` 判断失效。写入与比较两侧都要过这个函数。
fn strip_verbatim(path: &str) -> String {
    if let Some(rest) = path.strip_prefix(r"\\?\UNC\") {
        format!(r"\\{rest}")
    } else if let Some(rest) = path.strip_prefix(r"\\?\") {
        rest.to_string()
    } else {
        path.to_string()
    }
}

/// 归一化路径：尽量 canonicalize（解析 symlink、`..`、盘符大小写），
/// 再剥掉 Windows verbatim 前缀。失败时退回原字符串。
///
/// 归一化必须对**写入端与比较端**一致地做，否则
/// `\\?\D:\x` 与 `D:\x` 会被判为不同路径。
fn normalize_path(raw: &str) -> PathBuf {
    let canon = Path::new(raw).canonicalize();
    let path = canon.as_deref().unwrap_or_else(|_| Path::new(raw));
    PathBuf::from(strip_verbatim(&path.to_string_lossy()))
}

/// Canonicalized-cwd comparison against trusted entries (component-aware:
/// `/proj` trusts `/proj/sub` but not the sibling `/proj2`).
fn is_cwd_trusted(cwd_canon: &Path, trusted_entries: &[PathBuf]) -> bool {
    trusted_entries
        .iter()
        .any(|t| cwd_canon == t || cwd_canon.starts_with(t))
}

/// 解析 JSONL 清单。**坏行只跳过该行**，不影响其余记录
/// （这正是选 JSONL 而非整体 JSON 的原因：一条坏记录不该让白名单全废）。
fn parse_trusted_projects_jsonl(content: &str) -> Vec<TrustedProject> {
    content
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .filter_map(|line| match serde_json::from_str::<TrustedProject>(line) {
            Ok(record) => Some(record),
            Err(e) => {
                tracing::warn!("忽略 trust 清单中无法解析的行（{e}）：{line}");
                None
            }
        })
        .collect()
}

/// 解析旧版纯文本清单（一行一个路径）。
fn parse_legacy_trusted_projects(content: &str) -> Vec<TrustedProject> {
    content
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(|l| TrustedProject {
            path: strip_verbatim(l),
            trusted_at: None,
        })
        .collect()
}

/// 读出信任清单；`.jsonl` 不存在时尝试迁移旧纯文本清单。
///
/// 迁移是**尽力而为**：即使写盘失败也不影响本次的信任判断。
fn load_trusted_projects(app_home: &Path) -> Vec<TrustedProject> {
    let jsonl_path = app_home.join(TRUST_FILE_NAME);
    if jsonl_path.exists() {
        return match fs::read_to_string(&jsonl_path) {
            Ok(content) => parse_trusted_projects_jsonl(&content),
            Err(e) => {
                tracing::warn!("读取 {} 失败：{e}", jsonl_path.display());
                Vec::new()
            }
        };
    }

    let legacy_path = app_home.join(LEGACY_TRUST_FILE_NAME);
    if !legacy_path.exists() {
        return Vec::new();
    }
    let content = match fs::read_to_string(&legacy_path) {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!("读取旧版信任清单 {} 失败：{e}", legacy_path.display());
            return Vec::new();
        }
    };
    let records = parse_legacy_trusted_projects(&content);

    // 迁移：写 jsonl，旧文件改名留档（不删除）。
    match write_trust_records(&jsonl_path, &records) {
        Ok(()) => {
            let backup = app_home.join(LEGACY_BACKUP_FILE_NAME);
            match fs::rename(&legacy_path, &backup) {
                Ok(()) => tracing::info!(
                    "已把旧版信任清单 {} 迁移为 {}（原文件留档为 {}）",
                    legacy_path.display(),
                    jsonl_path.display(),
                    backup.display()
                ),
                Err(e) => tracing::warn!("已生成 {}，但旧文件改名失败：{e}", jsonl_path.display()),
            }
        }
        // 另一个进程/线程先完成了迁移：以先写入者为准，不要覆盖（否则可能
        // 截断它刚追加的记录）。
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
            tracing::debug!("{} 已被其他进程创建，跳过迁移", jsonl_path.display());
        }
        Err(e) => tracing::warn!(
            "迁移旧版信任清单到 {} 失败（本次信任判断不受影响）：{e}",
            jsonl_path.display()
        ),
    }

    records
}

/// 新建清单文件并整体写入（仅用于迁移）。
///
/// 用 `create_new(true)` 保证**绝不覆盖已存在的文件**：迁移发生在读路径上，
/// 可能与正在追加记录的 `trust_project` 并发，普通 `fs::write` 会截断对方
/// 刚写入的内容。
fn write_trust_records(path: &Path, records: &[TrustedProject]) -> std::io::Result<()> {
    let mut out = String::new();
    for record in records {
        out.push_str(&serde_json::to_string(record).map_err(std::io::Error::other)?);
        out.push('\n');
    }
    use std::io::Write;
    let mut f = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)?;
    f.write_all(out.as_bytes())
}

/// 取用户主目录 `~/.cc-code`。
///
/// 用 [`dirs_next::home_dir`]（Windows 走 Known Folder / `USERPROFILE`，
/// Unix 走 `HOME`），**不能**用 `std::env::var("HOME")`：PowerShell / cmd
/// 默认不设置 `HOME`，会导致信任清单读不到、`trust-project` 直接报错。
fn app_home() -> Option<PathBuf> {
    match dirs_next::home_dir() {
        Some(home) => Some(cc_agent::app_home::app_home_dir_in(&home)),
        None => {
            tracing::warn!(
                "无法确定用户主目录，信任清单不可用（Unix 检查 HOME，Windows 检查 USERPROFILE）"
            );
            None
        }
    }
}

/// Check if a project directory is trusted for loading local hooks (#18).
///
/// Trust is granted via:
/// 1. `CC_CODE_TRUST_PROJECT_HOOKS=1` env var (applies to all projects), or
/// 2. Project path listed in `~/.cc-code/trusted_projects.jsonl`
///    (`{"path":"D:\\code\\proj"}` one per line).
pub fn is_project_trusted(cwd: &str) -> bool {
    // Env var override: trust all
    if std::env::var("CC_CODE_TRUST_PROJECT_HOOKS").as_deref() == Ok("1") {
        return true;
    }
    match app_home() {
        Some(home) => is_project_trusted_in(&home, cwd),
        None => false,
    }
}

/// [`is_project_trusted`] 的可注入 `app_home` 版本（不做 env 覆盖，便于测试）。
pub fn is_project_trusted_in(app_home: &Path, cwd: &str) -> bool {
    let entries: Vec<PathBuf> = load_trusted_projects(app_home)
        .iter()
        .map(|record| normalize_path(&record.path))
        .collect();
    is_cwd_trusted(&normalize_path(cwd), &entries)
}

/// 信任清单文件的展示路径（供 CLI 回显"写到哪了"）。
///
/// 取不到 home 时返回 `~/.cc-code/trusted_projects.jsonl` 这种提示性路径。
pub fn trust_file_path_display() -> String {
    match dirs_next::home_dir() {
        Some(home) => cc_agent::app_home::app_home_dir_in(&home)
            .join(TRUST_FILE_NAME)
            .display()
            .to_string(),
        None => format!("~/.cc-code/{TRUST_FILE_NAME}"),
    }
}

/// Add a project directory to the trusted list (#18).
pub fn trust_project(cwd: &str) -> std::io::Result<()> {
    let home = dirs_next::home_dir().ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "无法确定用户主目录（Unix 检查 HOME，Windows 检查 USERPROFILE）",
        )
    })?;
    trust_project_in(&cc_agent::app_home::app_home_dir_in(&home), cwd)
}

/// [`trust_project`] 的可注入 `app_home` 版本。
///
/// 已存在时是幂等的（不重复写入）。
pub fn trust_project_in(app_home: &Path, cwd: &str) -> std::io::Result<()> {
    fs::create_dir_all(app_home)?;
    let jsonl_path = app_home.join(TRUST_FILE_NAME);

    // 先跑一遍读取，顺带把旧版纯文本清单迁移过来，
    // 否则迁移前的项目会被重新追加一遍。
    let target = normalize_path(cwd);
    let existing = load_trusted_projects(app_home);
    if existing
        .iter()
        .any(|record| normalize_path(&record.path) == target)
    {
        return Ok(());
    }

    let record = TrustedProject {
        path: target.to_string_lossy().to_string(),
        trusted_at: Some(chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true)),
    };
    let line = serde_json::to_string(&record).map_err(std::io::Error::other)?;

    use std::io::Write;
    let mut f = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&jsonl_path)?;
    writeln!(f, "{line}")?;
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
    // is in the trusted list (~/.cc-code/trusted_projects.jsonl) or CC_CODE_TRUST_PROJECT_HOOKS=1.
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

/// 项目里是否存在带 hooks 的 settings 文件（不看信任状态）。
fn project_has_hook_files(cwd: &str) -> bool {
    ["settings.json", "settings.local.json"]
        .iter()
        .any(|file_name| {
            let path = Path::new(cwd).join(".claude").join(file_name);
            fs::read_to_string(&path)
                .map(|c| {
                    serde_json::from_str::<serde_json::Value>(&c)
                        .ok()
                        .and_then(|v| v.get("hooks").cloned())
                        .map(|h| h.is_object())
                        .unwrap_or(false)
                })
                .unwrap_or(false)
        })
}

/// 若本项目本该加载 hooks 却因未信任被跳过，返回一条给用户看的提示。
///
/// 信任被拒时 `load_settings_*_hooks` 只打 `tracing::warn`（埋在日志里），
/// 用户看到的现象是"hooks 不生效"却不知为何。TUI 应把这条提示显示出来。
pub fn untrusted_project_hooks_notice(cwd: &str) -> Option<String> {
    untrusted_project_hooks_notice_with(cwd, is_project_trusted(cwd))
}

/// [`untrusted_project_hooks_notice`] 的可注入实现（信任状态由调用方给出）。
fn untrusted_project_hooks_notice_with(cwd: &str, trusted: bool) -> Option<String> {
    if trusted || !project_has_hook_files(cwd) {
        return None;
    }
    Some(format!(
        "本项目 hooks 未加载：{} 未被信任。执行 `cc-code trust-project \"{}\"` 放行（或设 CC_CODE_TRUST_PROJECT_HOOKS=1）。",
        cwd, cwd
    ))
}

/// 测试用：指定 `app_home` 的提示版本（沿用生产同一条文案）。
#[cfg(test)]
fn untrusted_project_hooks_notice_in(app_home: &Path, cwd: &str) -> Option<String> {
    untrusted_project_hooks_notice_with(cwd, is_project_trusted_in(app_home, cwd))
}

#[cfg(test)]
#[path = "loader_test.rs"]
mod tests;
