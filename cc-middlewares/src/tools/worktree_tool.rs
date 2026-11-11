//! `EnterWorktree` / `ExitWorktree` 工具：在会话运行中创建与清理隔离 checkout。
//!
//! 语义对齐参考实现的两个工具：
//!
//! - `EnterWorktree { name? }`：在托管目录下创建独立 worktree，并把**会话工作目录**
//!   切过去；切换对该工具之后的轮次生效。
//! - `ExitWorktree { action: keep|remove, discard_changes? }`：回到进入前的目录；
//!   `remove` 会先统计变更，有未提交内容或新提交时必须显式 `discard_changes: true`，
//!   统计失败（`None`）一律拒绝删除（fail-closed）。
//!
//! 生命周期 hook（`WorktreeCreate` / `WorktreeRemove`）由注入的 [`HookRuntime`]
//! 触发，hook 返回 Block 时创建/删除被拒绝。

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::Arc,
};

use async_trait::async_trait;

use cc_agent::{
    session_cwd::{self, SessionCwd},
    tools::BaseTool,
};
use cc_tools::worktree;
use serde_json::Value;

use crate::hooks::{
    runtime::{HookRuntime, WorktreeCreateHookOutcome},
    types::HookAction,
};

/// 进入过的 worktree 会话信息。
///
/// 会持久化到 `~/.cc-code/worktree_sessions.json`：进程重启后 `ExitWorktree`
/// 仍能识别本会话创建的 checkout，而不是退化成 no-op 留下孤儿目录。
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub(crate) struct WorktreeSession {
    /// 进入 worktree 之前的工作目录。
    original_cwd: String,
    /// 创建时的 HEAD，用于统计后续提交。
    base_commit: String,
    /// 创建的命名分支（对齐 CC 的 `worktree/<slug>`）；退出删除时一并清理。
    branch: Option<String>,
    /// 是否由 `WorktreeCreate` hook 创建。
    ///
    /// hook 创建的 checkout 不归 git 流程管：没有基线 commit（无法统计变更），
    /// 删除也只能交给 `WorktreeRemove` hook。参考实现对该类 worktree 同样
    /// 「不自动清理」。
    hook_based: bool,
}

/// worktree 托管目录。
///
/// 优先仓库内的 `.claude/worktrees`（对齐参考实现，创建时会自动写入
/// `.git/info/exclude` 避免污染 `git status`）；不在 git 仓库内时回退到
/// `~/.cc-code/worktrees`。
pub fn storage_root(cwd: &str) -> PathBuf {
    cc_tools::worktree::list(Path::new(cwd))
        .ok()
        .map(|listing| {
            PathBuf::from(listing.repository_root)
                .join(".claude")
                .join("worktrees")
        })
        .unwrap_or_else(|| cc_agent::app_home::app_home_dir().join("worktrees"))
}

/// 会话登记文件路径（`override_path` 为测试注入的位置）。
///
/// 测试必须注入临时路径：默认落在 `~/.cc-code` 下，跑测试会清掉开发者本地登记
/// （仓库规范禁止测试写全局配置）。这里用依赖注入而不是环境变量——env 是进程级
/// 的，并发测试之间会互相覆盖。
fn sessions_path(override_path: Option<&Path>) -> PathBuf {
    override_path
        .map(Path::to_path_buf)
        .unwrap_or_else(|| cc_agent::app_home::app_home_dir().join("worktree_sessions.json"))
}

/// 写入单条登记（读-改-写）。
///
/// 登记表在每次 `build_agent` 时新建，整份覆盖会把其它 thread / 其它会话的记录
/// 抹掉，因此必须以磁盘快照为基准合并。
fn persist_session(override_path: Option<&Path>, path: &str, session: &WorktreeSession) {
    let mut all = load_sessions(override_path);
    all.insert(path.to_string(), session.clone());
    save_sessions(override_path, &all);
}

/// 删除单条登记（读-改-写），不动其它条目。
fn forget_session(override_path: Option<&Path>, path: &str) {
    let mut all = load_sessions(override_path);
    if all.remove(path).is_some() {
        save_sessions(override_path, &all);
    }
}

fn load_sessions(override_path: Option<&Path>) -> HashMap<String, WorktreeSession> {
    std::fs::read_to_string(sessions_path(override_path))
        .ok()
        .and_then(|json| serde_json::from_str(&json).ok())
        .unwrap_or_default()
}

fn save_sessions(override_path: Option<&Path>, sessions: &HashMap<String, WorktreeSession>) {
    let path = sessions_path(override_path);
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if let Ok(json) = serde_json::to_string_pretty(sessions) {
        let _ = std::fs::write(path, json);
    }
}

/// 校验用户给出的 worktree 名称（对齐参考实现的 `validateWorktreeSlug`）。
///
/// 规则：按 `/` 分段，每段只允许字母、数字、`.`、`_`、`-`，不允许空段、
/// `.`/`..`（防路径穿越），总长 ≤64。**非法输入直接报错**——静默把非法字符
/// 替换掉会让模型拿到一个和请求不符的目录名。
fn validate_name(raw: &str) -> Result<String, String> {
    let name = raw.trim();
    if name.is_empty() {
        return Err("worktree name must not be empty".to_string());
    }
    if name.chars().count() > 64 {
        return Err("worktree name must be at most 64 characters".to_string());
    }
    for segment in name.split('/') {
        if segment.is_empty() {
            return Err("worktree name must not contain empty path segments".to_string());
        }
        if segment == "." || segment == ".." {
            return Err("worktree name must not contain '.' or '..' segments".to_string());
        }
        if !segment
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
        {
            return Err(format!(
                "invalid worktree name segment {segment:?}: only letters, digits, '.', '_' and '-' are allowed"
            ));
        }
    }
    Ok(name.to_string())
}

fn slug(input: &Value) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    match input.get("name").and_then(Value::as_str) {
        Some(raw) => validate_name(raw).map_err(tool_error),
        None => Ok(format!(
            "wt-{}",
            &uuid::Uuid::new_v4().simple().to_string()[..8]
        )),
    }
}

fn is_managed(path: &str, storage: &Path) -> bool {
    Path::new(path).starts_with(storage)
}

/// 进入（创建）一个 worktree。
pub struct EnterWorktreeTool {
    cwd: SessionCwd,
    hooks: Option<Arc<HookRuntime>>,
    /// 会话登记文件位置；`None` = `~/.cc-code/worktree_sessions.json`。
    sessions_file: Option<PathBuf>,
}

impl EnterWorktreeTool {
    pub(crate) fn new(cwd: SessionCwd, hooks: Option<Arc<HookRuntime>>) -> Self {
        Self {
            cwd,
            hooks,
            sessions_file: None,
        }
    }

    /// 注入会话登记文件位置（测试专用，避免写真实 home 目录）。
    #[cfg(test)]
    fn with_sessions_file(mut self, path: impl Into<PathBuf>) -> Self {
        self.sessions_file = Some(path.into());
        self
    }
}

/// 退出（可选删除）当前 worktree。
pub struct ExitWorktreeTool {
    cwd: SessionCwd,
    hooks: Option<Arc<HookRuntime>>,
    /// 会话登记文件位置；`None` = `~/.cc-code/worktree_sessions.json`。
    sessions_file: Option<PathBuf>,
}

impl ExitWorktreeTool {
    pub(crate) fn new(cwd: SessionCwd, hooks: Option<Arc<HookRuntime>>) -> Self {
        Self {
            cwd,
            hooks,
            sessions_file: None,
        }
    }

    /// 注入会话登记文件位置（测试专用）。
    #[cfg(test)]
    fn with_sessions_file(mut self, path: impl Into<PathBuf>) -> Self {
        self.sessions_file = Some(path.into());
        self
    }
}

type ToolResult = Result<String, Box<dyn std::error::Error + Send + Sync>>;
fn tool_error(message: impl Into<String>) -> Box<dyn std::error::Error + Send + Sync> {
    Box::new(std::io::Error::other(message.into()))
}

#[async_trait]
impl BaseTool for EnterWorktreeTool {
    fn name(&self) -> &str {
        "EnterWorktree"
    }

    fn description(&self) -> &str {
        "Create an isolated git worktree and switch this session's working directory into it.\n\n\
         ## When to Use\n\
         - Only when the user explicitly asks to work in a worktree (\"start a worktree\", \
         \"work in a worktree\", \"create a worktree\")\n\n\
         ## When NOT to Use\n\
         - The user asks to create or switch branches — use git commands instead\n\
         - The user asks to fix a bug or build a feature without mentioning worktrees\n\n\
         ## Requirements\n\
         - Must be in a git repository, or have WorktreeCreate/WorktreeRemove hooks configured\n\
         - Must not already be inside a worktree created by this tool\n\n\
         ## Behavior\n\
         - Creates a checkout with a new branch `worktree/<name>` based on HEAD\n\
         - Switches the session's working directory to it; the switch applies from the next turn\n\
         - Use ExitWorktree to leave the worktree mid-session (keep or remove)"
    }

    fn parameters(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "name": {
                    "type": "string",
                    "description": "Optional worktree name. Each '/'-separated segment may contain only letters, digits, dots, underscores and dashes; max 64 chars. A random name is generated if omitted."
                }
            },
            "required": []
        })
    }

    async fn invoke(&self, input: Value) -> ToolResult {
        let current = session_cwd::current(&self.cwd);
        let storage = storage_root(&current);
        if is_managed(&current, &storage) {
            return Err(tool_error(format!(
                "Already inside a managed worktree ({current}). Call ExitWorktree first."
            )));
        }

        let name = slug(&input)?;

        // hook 优先：配了 WorktreeCreate 且交出路径时，完全不调用 git。
        if let Some(runtime) = &self.hooks {
            match runtime.run_worktree_create_hook(&name).await {
                WorktreeCreateHookOutcome::Blocked(reason) => {
                    return Err(tool_error(format!(
                        "WorktreeCreate hook rejected the worktree: {reason}"
                    )));
                }
                WorktreeCreateHookOutcome::Path(path) => {
                    let path = PathBuf::from(path);
                    if !path.is_dir() {
                        return Err(tool_error(format!(
                            "WorktreeCreate hook returned a path that is not a directory: {}",
                            path.display()
                        )));
                    }
                    let display = path.display().to_string();
                    // hook 创建的目录没有基线 commit：`ExitWorktree remove` 的变更
                    // 统计会返回 None —— 按「未知即不安全」拒绝删除，与参考实现一致。
                    let session = WorktreeSession {
                        original_cwd: current.clone(),
                        base_commit: String::new(),
                        branch: None,
                        hook_based: true,
                    };
                    persist_session(self.sessions_file.as_deref(), &display, &session);
                    session_cwd::switch(&self.cwd, display.clone());
                    runtime.fire_cwd_changed(&current, &display).await;
                    return Ok(format!(
                        "Switched to hook-provided worktree\n{display}\nTool calls from the next \
                         turn on run in that directory. Use ExitWorktree when done."
                    ));
                }
                WorktreeCreateHookOutcome::NotConfigured
                | WorktreeCreateHookOutcome::AllowedWithoutPath => {}
            }
        }

        let cwd = current.clone();
        let storage_for_create = storage.clone();
        // 建命名分支而不是 detached HEAD：状态栏与 `git status` 都要能显示
        // `worktree/<slug>`（对齐参考实现），detached 会让分支段直接消失。
        let branch_name = format!("worktree/{name}");
        let branch_for_create = branch_name.clone();
        // 目录名用 slug：同名重复进入时复用已有 checkout（参考实现的快速恢复）。
        // 复用与新建在渲染上要区分（参考实现的 `existed` 标记）。
        let reused = storage.join(&name).exists();
        let name_for_create = name.clone();
        let created = tokio::task::spawn_blocking(move || {
            worktree::create_named(
                Path::new(&cwd),
                &name_for_create,
                Some(branch_for_create.as_str()),
                None,
                &storage_for_create,
            )
        })
        .await
        .map_err(|error| tool_error(format!("Worktree task failed: {error}")))?
        .map_err(|error| tool_error(format!("Cannot create worktree: {error:#}")))?;

        // 复用已有 checkout 时分支名以 git 记录为准（可能来自更早的进入）。
        let effective_branch = created.branch.clone().unwrap_or(branch_name.clone());

        let session = WorktreeSession {
            original_cwd: current.clone(),
            base_commit: created.head.clone(),
            branch: Some(effective_branch.clone()),
            hook_based: false,
        };
        persist_session(self.sessions_file.as_deref(), &created.path, &session);
        session_cwd::switch(&self.cwd, created.path.clone());
        if let Some(runtime) = &self.hooks {
            // CwdChanged 是通知型事件：参考实现里它不能被阻断，只用于刷新
            // 环境感知（乃至动态扩展监控范围）。这里同样忽略返回值。
            runtime.fire_cwd_changed(&current, &created.path).await;
        }

        Ok(format!(
            // 前两行对齐参考实现的渲染（结论行 + 路径行），第三行是给模型的
            // 生效说明——TUI 的工具结果按纯文本渲染，排版靠返回文本本身。
            "{}\n{}\nTool calls from the next turn on run in that directory. \
             Use ExitWorktree when done.",
            if reused {
                format!("Reusing worktree on branch {effective_branch}")
            } else {
                format!("Switched to worktree on branch {effective_branch}")
            },
            created.path
        ))
    }
}

#[async_trait]
impl BaseTool for ExitWorktreeTool {
    fn name(&self) -> &str {
        "ExitWorktree"
    }

    fn description(&self) -> &str {
        "Exit a worktree session created by EnterWorktree and return this session to the \
         original working directory.\n\n\
         ## Scope\n\
         Only operates on worktrees created by EnterWorktree in this session. It will NOT touch \
         worktrees created manually with `git worktree add`, worktrees from a previous session, \
         or the directory you are in if EnterWorktree was never called. Called outside an \
         EnterWorktree session it is a no-op that reports no active worktree session.\n\n\
         ## When to Use\n\
         - Only when the user asks to exit/leave the worktree or go back. Do not call it proactively.\n\n\
         ## Parameters\n\
         - `action` (required): \"keep\" leaves the directory and branch on disk; \"remove\" deletes \
         both, and refuses when the worktree still has uncommitted files or commits not on the \
         original branch — re-invoke with `discard_changes: true` (after confirming with the user) \
         to remove it anyway.\n\n\
         ## Behavior\n\
         - Restores the session's working directory to where it was before EnterWorktree"
    }

    fn parameters(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "enum": ["keep", "remove"],
                    "description": "keep: leave the checkout on disk; remove: delete it."
                },
                "discard_changes": {
                    "type": "boolean",
                    "description": "Set true to remove a worktree that still has uncommitted files or new commits."
                }
            },
            "required": ["action"]
        })
    }

    async fn invoke(&self, input: Value) -> ToolResult {
        let current = session_cwd::current(&self.cwd);
        // 进程重启后内存登记表为空：回退读取磁盘快照，避免把已存在的
        // isolation worktree 误判成「没有活动会话」。
        let session = load_sessions(self.sessions_file.as_deref()).remove(&current);
        // 对齐参考实现：不在本会话创建的 worktree 内时是 **no-op**（报告无活动
        // worktree 会话、不做任何改动），而不是报错——模型可能误调，报错只会让
        // 它反复重试。
        let Some(session) = session else {
            return Ok(format!(
                "No active worktree session (current directory: {current}). Nothing was changed."
            ));
        };

        let action = match input.get("action").and_then(Value::as_str) {
            Some("keep") => "keep",
            Some("remove") => "remove",
            Some(other) => {
                return Err(tool_error(format!(
                    "Invalid action {other:?}: expected \"keep\" or \"remove\"."
                )))
            }
            None => return Err(tool_error("Missing required parameter: action")),
        };
        let discard = input
            .get("discard_changes")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        // 删除时的托管根按进入前的主仓库解析（当前目录已在 worktree 内）。
        let storage = storage_root(&session.original_cwd);

        if action == "remove" {
            // hook 创建的 checkout 没有基线 commit、也没有 git 侧登记：变更统计
            // 无从谈起，删除只能交给 `WorktreeRemove` hook（参考实现同样把
            // hook-based worktree 排除在自动清理之外）。
            if !session.hook_based {
                let worktree_path = current.clone();
                let base = session.base_commit.clone();
                match worktree::count_changes(Path::new(&worktree_path), &base) {
                    None => {
                        return Err(tool_error(
                        "Cannot determine the worktree's changes (git failed); refusing to remove it. \
                         Inspect it manually or use action='keep'.",
                    ))
                    }
                    Some(changes) if changes.is_dirty() && !discard => {
                        return Err(tool_error(format!(
                        "Worktree has {} uncommitted file(s) and {} new commit(s). \
                         Pass discard_changes: true to remove it anyway, or action='keep' to leave it.",
                        changes.uncommitted_files, changes.new_commits
                    )))
                    }
                    Some(_) => {}
                }
            }

            if let Some(runtime) = &self.hooks {
                if let HookAction::Block { reason } = runtime.fire_worktree_remove(&current).await {
                    return Err(tool_error(format!(
                        "WorktreeRemove hook rejected the removal: {reason}"
                    )));
                }
            }

            if !session.hook_based {
                let repo = session.original_cwd.clone();
                let target = current.clone();
                let branch = session.branch.clone();
                tokio::task::spawn_blocking(move || {
                    worktree::remove(
                        Path::new(&repo),
                        Path::new(&target),
                        branch.as_deref(),
                        &storage,
                    )
                })
                .await
                .map_err(|error| tool_error(format!("Worktree task failed: {error}")))?
                .map_err(|error| tool_error(format!("Cannot remove worktree: {error:#}")))?;
            }
        }

        forget_session(self.sessions_file.as_deref(), &current);
        session_cwd::switch(&self.cwd, session.original_cwd.clone());
        if let Some(runtime) = &self.hooks {
            runtime
                .fire_cwd_changed(&current, &session.original_cwd)
                .await;
        }

        // 首行是结论（保留 / 删除 + 分支），次行是回到的目录，对齐参考实现的渲染。
        let action_label = if action == "remove" {
            "Removed worktree"
        } else {
            "Kept worktree"
        };
        let branch_label = session
            .branch
            .as_deref()
            .map(|branch| format!(" (branch {branch})"))
            .unwrap_or_default();
        Ok(format!(
            "{action_label}{branch_label}\nReturned to {}",
            session.original_cwd
        ))
    }
}

/// 把 worktree 工具挂到中间件链上（工具集按轮次收集）。
///
/// **只挂主 Agent 的链，不给子 Agent**：会话工作目录（[`SessionCwd`]）是会话级
/// 共享的，子 Agent 调用 `EnterWorktree` 会把父会话的目录一起切走；而子 Agent
/// 的 HITL 是 `binary_only` 且没有 broker，无法让用户确认这次切换。参考实现的
/// 子 Agent 虽然能拿到这两个工具（`getTools` 全量），但它的 `EnterWorktree` 走
/// 进程级 `chdir`，同样会波及父会话——这里选择不放宽。
pub struct WorktreeMiddleware {
    cwd: SessionCwd,
    hooks: Option<Arc<HookRuntime>>,
}

impl WorktreeMiddleware {
    pub fn new(cwd: SessionCwd, hooks: Option<Arc<HookRuntime>>) -> Self {
        Self { cwd, hooks }
    }
}

#[async_trait]
impl<S: cc_agent::agent::state::State> cc_agent::middleware::Middleware<S> for WorktreeMiddleware {
    fn name(&self) -> &str {
        "WorktreeMiddleware"
    }

    fn collect_tools(&self, _cwd: &str) -> Vec<Box<dyn BaseTool>> {
        vec![
            Box::new(EnterWorktreeTool::new(
                Arc::clone(&self.cwd),
                self.hooks.clone(),
            )),
            Box::new(ExitWorktreeTool::new(
                Arc::clone(&self.cwd),
                self.hooks.clone(),
            )),
        ]
    }
}

#[cfg(test)]
#[path = "worktree_tool_test.rs"]
mod tests;
