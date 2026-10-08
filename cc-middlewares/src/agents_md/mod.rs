use std::{
    collections::{hash_map::DefaultHasher, HashSet},
    hash::{Hash, Hasher},
    path::{Path, PathBuf},
};

use async_trait::async_trait;
use cc_agent::{
    agent::state::State, error::AgentResult, messages::BaseMessage, middleware::r#trait::Middleware,
};

mod config;
pub use config::{AgentsMdConfig, DEFAULT_MAX_BYTES, DEFAULT_MAX_SOURCE_BYTES};

/// AgentsMdMiddleware - 注入项目指引文件（`AGENTS.md` / `CLAUDE.md` 及变体）
///
/// 加载模型对齐 dsh（deepseek-harness）：
///
/// - **同目录**：候选文件**都加载并合并**（不是「先命中者独占」），trim 后内容相同的去重；
/// - **跨目录**：从项目根（含 `.git` 的目录）逐级向下到 cwd 拼接，越靠后越具体、优先级越高；
/// - **用户全局层**：`{APP_HOME}/AGENTS.md`（`~/.cc-code/AGENTS.md`），置于链首（最宽）；
/// - 每段带 provenance 头 `## <相对路径>`；
/// - 单文件超 `max_source_bytes` 头 70% + 尾 20% 截断，渲染总量超 `max_bytes` 停止追加并标注。
///
/// 完整顺序（宽 → 具体）：
///
/// ```text
/// ~/.cc-code/AGENTS.md
/// {root}/AGENTS.md, {root}/CLAUDE.md, {root}/AGENTS.local.md, {root}/CLAUDE.local.md
/// …（逐级）…
/// {cwd}/AGENTS.md, {cwd}/CLAUDE.md, {cwd}/AGENTS.local.md, {cwd}/CLAUDE.local.md
/// ```
///
/// 冻结数据（`session/new` 一次性捕获）优先：`with_frozen_instructions` 设过之后
/// `before_agent` 完全跳过磁盘 I/O，保 Prompt Cache 前缀稳定。
/// 未冻结时（子 Agent 等场景）回退到同样的发现 + 渲染逻辑。
pub struct AgentsMdMiddleware {
    config: AgentsMdConfig,
    /// Frozen rendered instruction set (merged, deduped, provenance-tagged).
    /// When set, `before_agent` skips disk I/O entirely.
    frozen: Option<String>,
}

/// 链上的一个指引文件。
#[derive(Debug, Clone)]
pub struct InstructionFile {
    /// 发现路径（未规范化）。excludes glob 按此匹配——与历史行为一致，
    /// 且 macOS 上 `/var` → `/private/var` 之类的符号链接不会让用户写的绝对 glob 失效。
    pub source_path: PathBuf,
    /// 规范化绝对路径（去重用：符号链接 / 重复访问只加载一次）。
    pub abs_path: PathBuf,
    /// provenance 头展示文本（相对项目根、`~/...` 或绝对路径）。
    pub display: String,
    /// 原始内容（已归一 CRLF、已展开 `@import`、未截断）。
    pub content: String,
}

impl AgentsMdMiddleware {
    pub fn new() -> Self {
        Self {
            config: AgentsMdConfig::default(),
            frozen: None,
        }
    }

    /// 覆盖加载配置（候选名、项目根标记、限额、excludes、用户全局文件）。
    pub fn with_config(mut self, config: AgentsMdConfig) -> Self {
        self.config = config;
        self
    }

    /// 设置指引文件排除 glob 模式（匹配路径字符串，发现阶段早期生效）。
    ///
    /// **仅非冻结路径生效**：冻结内容在 `session/new` 就已渲染好，中间件拿到的
    /// 是成品字符串，无法再按文件过滤。
    pub fn with_excludes(mut self, patterns: Vec<String>) -> Self {
        self.config.excludes = patterns;
        self
    }

    /// 注入冻结好的**整段**指引内容（`session/new` 时由 [`load_frozen_instructions`] 产出）。
    ///
    /// 整段作为**一条** System 消息前插——拆成多条会改变 Prompt Cache 前缀结构。
    pub fn with_frozen_instructions(mut self, rendered: String) -> Self {
        self.frozen = Some(rendered);
        self
    }

    /// Read and freeze CLAUDE.md content once (with @import resolution).
    ///
    /// Returns `(main_content, local_content)`, either may be `None`.
    /// 用于需要「项目级 / 个人级」两段语义的消费者（如 HITL 的 Jev 规则提炼）；
    /// 注入进上下文的整段指引请用 [`load_frozen_instructions`]。
    pub fn read_frozen_content(cwd: &str) -> (Option<String>, Option<String>) {
        let candidates = vec![
            Path::new(cwd).join("AGENTS.md"),
            Path::new(cwd).join("CLAUDE.md"),
            Path::new(cwd).join(".claude").join("AGENTS.md"),
        ];
        let main_content = candidates
            .into_iter()
            .find(|p| p.is_file())
            .and_then(|path| {
                let content = std::fs::read_to_string(&path).ok()?;
                if content.trim().is_empty() {
                    return None;
                }
                Some(read_with_imports(&path, &content))
            });
        let local_content = {
            let local_path = Path::new(cwd).join("CLAUDE.local.md");
            if local_path.is_file() {
                let c = std::fs::read_to_string(&local_path).unwrap_or_default();
                if c.trim().is_empty() {
                    None
                } else {
                    Some(c)
                }
            } else {
                None
            }
        };
        (main_content, local_content)
    }

    /// 读取**用户全局** CLAUDE.md（`~/.claude/CLAUDE.md`，缺失则回退 `~/.claude/AGENTS.md`）。
    ///
    /// 与 [`Self::read_frozen_content`] 的项目级读取相互独立：那是"项目上下文"，
    /// 这是"个人规则"。供需要跨项目生效的消费者使用（如 HITL 的 Jev 语义门策略）。
    /// 同样解析 `@import`；空文件视为不存在。
    pub fn read_global_content() -> Option<String> {
        let claude_dir = dirs_next::home_dir()?.join(".claude");
        for name in ["CLAUDE.md", "AGENTS.md"] {
            let path = claude_dir.join(name);
            if !path.is_file() {
                continue;
            }
            let Ok(content) = std::fs::read_to_string(&path) else {
                continue;
            };
            if content.trim().is_empty() {
                continue;
            }
            return Some(read_with_imports(&path, &content));
        }
        None
    }
}

// ── 加载器（dsh 模型）────────────────────────────────────────────────────────

/// 从 cwd 向上找项目根：第一个含 `project_root_markers`（默认 `.git`）的目录。
/// 找不到则根就是 cwd 本身（**只查 cwd，不向上**——避免 `/tmp`、`$HOME` 的指引泄漏到无关会话）。
pub fn find_project_root(cwd: &Path, markers: &[String]) -> PathBuf {
    let mut cur: Option<&Path> = Some(cwd);
    while let Some(dir) = cur {
        if markers.iter().any(|m| dir.join(m).exists()) {
            return dir.to_path_buf();
        }
        cur = dir.parent();
    }
    cwd.to_path_buf()
}

/// 目录链 `root → … → cwd`（含两端），宽 → 具体。
/// cwd 不在 root 下时退化为单目录 `[cwd]`。
fn ancestor_chain(root: &Path, cwd: &Path) -> Vec<PathBuf> {
    let Ok(rel) = cwd.strip_prefix(root) else {
        return vec![cwd.to_path_buf()];
    };
    let mut chain = vec![root.to_path_buf()];
    let mut cur = root.to_path_buf();
    for comp in rel.components() {
        cur = cur.join(comp);
        chain.push(cur.clone());
    }
    chain
}

/// 匹配 excludes glob（按「发现路径」+ canonical 两种形式都试）。
///
/// 按发现路径匹配是关键：macOS 上 `tempfile` 的 `/var/folders/...` 会被
/// `canonicalize` 解析成 `/private/var/...`，Windows 上会多出 `\\?\` verbatim 前缀——
/// 只拿 canonical 串匹配会让用户写的绝对 glob **静默失效**。
fn is_excluded(source_path: &Path, abs_path: &Path, excludes: &[String]) -> bool {
    if excludes.is_empty() {
        return false;
    }
    let raw = source_path.to_string_lossy().to_string();
    let plain = raw
        .strip_prefix(r"\\?\")
        .map(str::to_string)
        .unwrap_or_else(|| raw.clone());
    let canonical = abs_path.to_string_lossy().to_string();
    excludes.iter().any(|pat| {
        glob::Pattern::new(pat)
            .map(|g| g.matches(&raw) || g.matches(&plain) || g.matches(&canonical))
            .unwrap_or(false)
    })
}

/// 解析出「发现路径 + canonical 路径」并按 exclude 规则判定（不该加载则返回 None）。
fn canonicalize_unless_excluded(path: &Path, excludes: &[String]) -> Option<PathBuf> {
    let abs_path = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    if is_excluded(path, &abs_path, excludes) {
        return None;
    }
    Some(abs_path)
}

/// 发现链上所有存在的指引文件（自带 provenance），宽 → 具体有序。
///
/// 规则：
/// 1. excludes 命中 → **在去重之前**丢弃（被排除的候选不占去重槽位）；
/// 2. 同目录候选**全部加载**（基础层在前，`.local` 覆盖层在后）；
/// 3. 同一绝对路径只加载一次（符号链接 / 同名重复）；
/// 4. **同目录**内 trim 后内容相同的只保留最早的一个；
/// 5. 空文件（trim 后为空）跳过——不遮蔽同目录的其它候选。
pub fn discover_instruction_files(cwd: &Path, cfg: &AgentsMdConfig) -> Vec<InstructionFile> {
    let root = find_project_root(cwd, &cfg.project_root_markers);
    let chain = ancestor_chain(&root, cwd);
    let mut out: Vec<InstructionFile> = Vec::new();
    let mut seen_abs: HashSet<PathBuf> = HashSet::new();

    // ① 用户全局层（链首，最宽）
    if cfg.user_global_file.is_file() {
        if let Some(abs_path) =
            canonicalize_unless_excluded(&cfg.user_global_file, &cfg.excludes)
        {
            let display = cfg.user_global_display();
            if let Some(file) =
                load_instruction_file(&cfg.user_global_file, display, abs_path, &mut seen_abs)
            {
                out.push(file);
            }
        }
    }

    // ② 逐目录（root → cwd），同目录内容去重
    for dir in &chain {
        let mut seen_digest: HashSet<u64> = HashSet::new();
        for name in cfg.all_candidates() {
            let path = dir.join(name);
            if !path.is_file() {
                continue;
            }
            let Some(abs_path) = canonicalize_unless_excluded(&path, &cfg.excludes) else {
                tracing::debug!(path = %path.display(), "指引文件命中 excludes，跳过");
                continue;
            };
            let display = display_for(&root, &path);
            let Some(file) = load_instruction_file(&path, display, abs_path, &mut seen_abs) else {
                continue;
            };
            if !seen_digest.insert(content_digest(&file.content)) {
                tracing::debug!(
                    path = %file.display,
                    "同目录指引内容重复，跳过（保留最早的候选）"
                );
                continue;
            }
            out.push(file);
        }
    }

    out
}

/// 加载单个指引文件：读 UTF-8 → 归一 CRLF → 空文件跳过 → 去重槽位 → 展开 `@import`。
fn load_instruction_file(
    path: &Path,
    display: String,
    abs_path: PathBuf,
    seen_abs: &mut HashSet<PathBuf>,
) -> Option<InstructionFile> {
    if seen_abs.contains(&abs_path) {
        return None;
    }
    let content = match std::fs::read_to_string(path) {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!(path = %path.display(), error = %e, "读取指引文件失败，跳过");
            return None;
        }
    };
    let content = normalize_newlines(&content);
    if content.trim().is_empty() {
        return None;
    }
    // 只有真正会 emit 的文件才占去重槽位（空文件 / 读失败不占）
    seen_abs.insert(abs_path.clone());
    let content = read_with_imports(path, &content);
    Some(InstructionFile {
        source_path: path.to_path_buf(),
        abs_path,
        display,
        content,
    })
}

/// 展开 `@import`（深度 3 + 环检测）。
fn read_with_imports(path: &Path, content: &str) -> String {
    let dir = path.parent().unwrap_or(Path::new("."));
    let mut visited = HashSet::new();
    if let Ok(canonical) = path.canonicalize() {
        visited.insert(canonical);
    }
    resolve_imports(content, dir, IMPORT_MAX_DEPTH, &mut visited)
}

/// `@import` 递归深度上限。
pub(crate) const IMPORT_MAX_DEPTH: u32 = 3;

fn normalize_newlines(content: &str) -> String {
    if content.contains('\r') {
        content.replace("\r\n", "\n").replace('\r', "\n")
    } else {
        content.to_string()
    }
}

fn content_digest(content: &str) -> u64 {
    let mut hasher = DefaultHasher::new();
    content.trim().hash(&mut hasher);
    hasher.finish()
}

/// provenance 头：优先相对项目根；否则绝对路径（所有路径都由 root 逐级 join 而来，
/// 所以 `strip_prefix(root)` 必成功；用户全局文件另走 [`AgentsMdConfig::user_global_display`]）。
fn display_for(root: &Path, path: &Path) -> String {
    if let Ok(rel) = path.strip_prefix(root) {
        let rel = rel.to_string_lossy().replace('\\', "/");
        if !rel.is_empty() {
            return rel;
        }
    }
    path.display().to_string()
}

/// 合并成单段（去重已在发现阶段完成；这里做限额 + provenance），供注入。
///
/// 超 `max_bytes` 时停止追加后续文件，并在末尾标注省略了多少个文件
/// （提示模型用 file 工具读全文）。第一个文件即使单个超限也会保留（硬截断）。
///
/// **硬保证**：`out.len() <= cfg.max_bytes` 恒成立——省略标记本身也算在额度内
/// （先按最长标记预留），放不下就整体按字节切；宁可标记残缺，也不越界。
pub fn render_instruction_set(files: &[InstructionFile], cfg: &AgentsMdConfig) -> String {
    if files.is_empty() {
        return String::new();
    }

    // 先按单文件限额切好每段（provenance 头 + 正文分开存，便于极端情况下保头）
    let headers: Vec<String> = files
        .iter()
        .map(|file| format!("## {}\n\n", file.display))
        .collect();
    let bodies: Vec<String> = files
        .iter()
        .map(|file| truncate_per_file(&file.content, cfg.max_source_bytes, &file.display))
        .collect();
    let segment_len = |idx: usize| headers[idx].len() + bodies[idx].len();

    // 第 k 段（含段间空行）的累计长度
    let len_of =
        |take: usize| -> usize { (0..take).map(segment_len).sum::<usize>() + 2 * take.saturating_sub(1) };

    if len_of(files.len()) <= cfg.max_bytes {
        return (0..files.len())
            .map(|i| format!("{}{}", headers[i], bodies[i]))
            .collect::<Vec<_>>()
            .join("\n\n");
    }

    // 需要省略：给「最长省略标记」预留额度（多文件时才有标记）
    let reserve = if files.len() > 1 {
        omit_marker(files.len(), files.len(), cfg.max_bytes).len()
    } else {
        0
    };
    let body_limit = cfg.max_bytes.saturating_sub(reserve);

    let mut take = 0;
    while take < files.len() && len_of(take + 1) <= body_limit {
        take += 1;
    }

    let mut out = String::new();
    if take == 0 {
        // 首个文件自己就顶破总量：硬截断正文，但**尽量保住 provenance 头**（§2.2）
        take = 1;
        if headers[0].len() < body_limit {
            out.push_str(&headers[0]);
            out.push_str(&truncate_bytes_head_tail(
                &bodies[0],
                body_limit - headers[0].len(),
                &files[0].display,
            ));
        } else {
            // 额度连头都放不下：只能硬切（保留多少算多少）
            out.push_str(char_boundary_prefix(
                &format!("{}{}", headers[0], bodies[0]),
                body_limit,
            ));
        }
    } else {
        out.push_str(
            &(0..take)
                .map(|i| format!("{}{}", headers[i], bodies[i]))
                .collect::<Vec<_>>()
                .join("\n\n"),
        );
    }

    let omitted = files.len().saturating_sub(take);
    if omitted > 0 {
        out.push_str(&omit_marker(omitted, files.len(), cfg.max_bytes));
    }

    if out.len() > cfg.max_bytes {
        // 兜底（上限小到连标记都放不下时）：按 char boundary 硬切到上限
        return char_boundary_prefix(&out, cfg.max_bytes).to_string();
    }
    out
}

/// 总量超限时的省略标记。`omitted` = 被省略的文件数。
fn omit_marker(omitted: usize, total: usize, max_bytes: usize) -> String {
    format!(
        "\n\n[...{omitted} of {total} instruction files omitted: total limit of {max_bytes} bytes \
         reached. Use file tools to read the omitted files.]"
    )
}

/// 发现 + 渲染，得到可直接注入的整段内容（`None` = 没有任何指引）。
pub fn load_instructions(cwd: &Path, cfg: &AgentsMdConfig) -> Option<String> {
    let files = discover_instruction_files(cwd, cfg);
    if files.is_empty() {
        return None;
    }
    let rendered = render_instruction_set(&files, cfg);
    if rendered.trim().is_empty() {
        None
    } else {
        Some(rendered)
    }
}

/// 会话创建时调用一次，产物写入 `FrozenSessionData.instructions`（默认配置）。
pub fn load_frozen_instructions(cwd: &Path) -> Option<String> {
    load_instructions(cwd, &AgentsMdConfig::default())
}

/// 单文件超 `max_source_bytes`：按**字符**（CJK 安全）保留头 70% + 尾 20%，中间插标记。
fn truncate_per_file(content: &str, max_source_bytes: usize, name: &str) -> String {
    if content.len() <= max_source_bytes {
        return content.to_string();
    }
    let total_chars = content.chars().count();
    let head_chars = total_chars * 70 / 100;
    let tail_chars = total_chars * 20 / 100;
    let head: String = content.chars().take(head_chars).collect();
    let tail: String = content.chars().skip(total_chars - tail_chars).collect();
    let marker = format!(
        "\n\n[...truncated {name}: kept {head_chars}+{tail_chars} of {total_chars} chars. \
         Use file tools to read the full file.]\n\n"
    );
    let out = format!("{head}{marker}{tail}");
    if out.len() > max_source_bytes {
        // 多字节内容下字符数与字节数不同量纲，兜底按字节再切一次
        truncate_bytes_head_tail(content, max_source_bytes, name)
    } else {
        out
    }
}

/// 按**字节**上限保留头 70% + 尾 20%（char boundary 安全），中间插标记。
/// 保证返回值字节数不超过 `max_bytes`。
fn truncate_bytes_head_tail(content: &str, max_bytes: usize, name: &str) -> String {
    if content.len() <= max_bytes {
        return content.to_string();
    }
    let full_marker = format!(
        "\n\n[...truncated {name}: kept head+tail of {} bytes. \
         Use file tools to read the full file.]\n\n",
        content.len()
    );
    // 上限太小则退化为短标记；连短标记都放不下就只留头部（char boundary 安全）
    let (marker, budget) = if full_marker.len() + 16 <= max_bytes {
        let len = full_marker.len();
        (full_marker, max_bytes - len)
    } else {
        let short = "[...truncated]\n\n";
        if short.len() + 8 <= max_bytes {
            let len = short.len();
            (short.to_string(), max_bytes - len)
        } else {
            return char_boundary_prefix(content, max_bytes).to_string();
        }
    };
    let head_budget = budget * 70 / 100;
    let tail_budget = budget - head_budget;
    let head = char_boundary_prefix(content, head_budget);
    let tail = char_boundary_suffix(content, tail_budget);
    format!("{head}{marker}{tail}")
}

/// 最长不超过 `n` 字节的前缀（不切断 UTF-8 字符）。
fn char_boundary_prefix(s: &str, n: usize) -> &str {
    if s.len() <= n {
        return s;
    }
    let mut end = n;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    &s[..end]
}

/// 最长不超过 `n` 字节的后缀（不切断 UTF-8 字符）。
fn char_boundary_suffix(s: &str, n: usize) -> &str {
    if s.len() <= n {
        return s;
    }
    let mut start = s.len() - n;
    while start < s.len() && !s.is_char_boundary(start) {
        start += 1;
    }
    &s[start..]
}

/// 递归解析 `<!-- @import path -->` 引用，替换为引用文件内容。
/// `base_dir` 为包含 @import 的文件所在目录。
/// `depth` 递归深度上限，`visited` 防循环。
pub(crate) fn resolve_imports(
    content: &str,
    base_dir: &Path,
    depth: u32,
    visited: &mut HashSet<PathBuf>,
) -> String {
    if depth == 0 {
        return content.to_string();
    }
    let mut result = String::with_capacity(content.len());
    let mut pos = 0;
    while pos < content.len() {
        if let Some(offset) = content[pos..].find("<!-- @import ") {
            let abs_pos = pos + offset;
            result.push_str(&content[pos..abs_pos]);
            // 提取 path：从 "<!-- @import " 之后到 " -->"
            let after = &content[abs_pos + 13..]; // 13 = "<!-- @import ".len()
            if let Some(end) = after.find(" -->") {
                let import_path = after[..end].trim();
                let resolved = base_dir
                    .join(import_path)
                    .canonicalize()
                    .unwrap_or_else(|_| base_dir.join(import_path));
                if visited.contains(&resolved) || !resolved.is_file() {
                    // 循环引用或文件不存在，保留原始占位符
                    result.push_str(&content[abs_pos..abs_pos + 13 + end + 4]);
                } else {
                    match std::fs::read_to_string(&resolved) {
                        Ok(imported_content) => {
                            visited.insert(resolved.clone());
                            let imported_content = normalize_newlines(&imported_content);
                            let import_dir = resolved.parent().unwrap_or(base_dir);
                            let resolved_content =
                                resolve_imports(&imported_content, import_dir, depth - 1, visited);
                            result.push_str(&resolved_content);
                        }
                        Err(e) => {
                            // 存在但不可读（权限 / IO 错）：保留占位符 + 留痕，不静默丢内容
                            tracing::warn!(
                                path = %resolved.display(),
                                error = %e,
                                "@import 目标不可读，保留占位符"
                            );
                            result.push_str(&content[abs_pos..abs_pos + 13 + end + 4]);
                        }
                    }
                }
                pos = abs_pos + 13 + end + 4; // 4 = " -->".len()
            } else {
                // 没找到 " -->"，不是有效的 @import，原样保留
                result.push_str("<!-- @import ");
                pos = abs_pos + 13;
            }
        } else {
            result.push_str(&content[pos..]);
            break;
        }
    }
    result
}

impl Default for AgentsMdMiddleware {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl<S: State> Middleware<S> for AgentsMdMiddleware {
    fn name(&self) -> &str {
        "AgentsMdMiddleware"
    }

    async fn before_agent(&self, state: &mut S) -> AgentResult<()> {
        // 冻结数据优先：跳过全部磁盘 I/O。
        let content = if let Some(ref frozen) = self.frozen {
            frozen.clone()
        } else {
            let cwd = state.cwd().to_string();
            let files = discover_instruction_files(Path::new(&cwd), &self.config);
            if files.is_empty() {
                return Ok(());
            }
            render_instruction_set(&files, &self.config)
        };

        if content.trim().is_empty() {
            return Ok(());
        }

        // 前插系统消息（置于消息历史开头，优先于 Human 消息）；**单条**，保 Prompt Cache 前缀稳定。
        state.prepend_message(BaseMessage::system(content));

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cc_agent::agent::state::AgentState;
    include!("agents_md_test.rs");
}
