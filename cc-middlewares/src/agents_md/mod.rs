use std::{
    collections::{hash_map::DefaultHasher, HashSet},
    hash::{Hash, Hasher},
    path::{Path, PathBuf},
};

use async_trait::async_trait;
use cc_agent::{
    agent::state::State, error::AgentResult, messages::BaseMessage, middleware::r#trait::Middleware,
};

mod bounded_read;
mod config;
pub use config::{
    default_user_global_candidates, user_global_candidates_in, AgentsMdConfig, ImportScope,
    DEFAULT_MAX_BYTES, DEFAULT_MAX_SOURCE_BYTES,
};

/// AgentsMdMiddleware - 注入项目指引文件（`AGENTS.md` / `CLAUDE.md` 及变体）
///
/// 加载模型对齐 dsh（deepseek-harness）：
///
/// - **同目录**：候选文件**都加载并合并**（不是「先命中者独占」），trim 后内容相同的去重；
/// - **跨目录**：从项目根（含 `.git` 的目录）逐级向下到 cwd 拼接，越靠后越具体、优先级越高；
/// - **用户全局层**：`{APP_HOME}/AGENTS.md`（`~/.cc-code/AGENTS.md`），置于链首（最宽），
///   且它属**用户自有**文件 → `@import` 不受项目范围限制（项目树内的文件才受限）；
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
/// 未冻结时（子 Agent 在**跨 cwd** 等无法继承父快照的场景）回退到同样的发现 + 渲染逻辑。
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
    /// 已有界读取、归一 CRLF 并展开 `@import` 的内容，字节数不超过单文件限额。
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
    ///
    /// 与 [`Self::with_config`] 同时使用时**后调用者覆盖前者**（两者都写 `config.excludes`）。
    pub fn with_excludes(mut self, patterns: Vec<String>) -> Self {
        self.config.excludes = patterns;
        self
    }

    /// 注入冻结好的**整段**指引内容（`session/new` 时用 [`load_instructions`] 渲染一次）。
    ///
    /// 整段作为**一条** System 消息前插——拆成多条会改变 Prompt Cache 前缀结构。
    pub fn with_frozen_instructions(mut self, rendered: String) -> Self {
        self.frozen = Some(rendered);
        self
    }

    /// 读一次项目级指引原文（`AGENTS.md` > `CLAUDE.md` > `.claude/AGENTS.md`，各取首个存在的），
    /// 并展开 `@import`；返回 `(main, local)`，两者都可能为 `None`。
    ///
    /// **不做冻结、也不写任何状态**：调用方（`session/new`）自己决定何时捕获一次。
    /// 注意它仍是旧的「先命中者独占」语义，且**空文件会遮蔽后续候选**（未随新加载器修）；
    /// 它只喂 Jev 规则提炼，与注入上下文的 `load_instructions` 是两条独立路径。
    /// 用于需要「项目级 / 个人级」两段语义的消费者（如 HITL 的 Jev 规则提炼）；
    /// 注入进上下文的整段指引请用 [`load_instructions`]。
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
                let content = normalize_content(
                    &bounded_read::read_bounded_file(&path, DEFAULT_MAX_SOURCE_BYTES).ok()?,
                );
                if content.trim().is_empty() {
                    return None;
                }
                // 项目树内的文件属**不可信**输入 → `@import` 默认限项目根内。
                Some(read_with_imports(
                    &path,
                    &content,
                    &AgentsMdConfig::default(),
                    ImportScope::ProjectRoot,
                ))
            });
        let local_content = {
            let local_path = Path::new(cwd).join("CLAUDE.local.md");
            if local_path.is_file() {
                let c = bounded_read::read_bounded_file(&local_path, DEFAULT_MAX_SOURCE_BYTES)
                    .unwrap_or_default();
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

    /// 读取**用户全局**指引文件内容（`~/.cc-code` / `~/.claude` 下的 `AGENTS.md` / `CLAUDE.md`）。
    ///
    /// 与上下文指引注入链路对齐：统一使用 [`load_user_global_instruction`] 加载，
    /// 确保 Jev 安全语义门与 System Prompt 指引使用完全一致的全局文件来源。
    /// 同样解析 `@import`；空文件视为不存在。
    pub fn read_global_content() -> Option<String> {
        let cfg = AgentsMdConfig::default();
        let seen_abs = HashSet::new();
        load_user_global_instruction(&cfg, &seen_abs).map(|f| f.content)
    }
}

// ── 加载器（dsh 模型）────────────────────────────────────────────────────────

/// 从 cwd **向上**找项目根：第一个含 `project_root_markers`（默认 `.git`）的目录。
/// 始终未命中标记时，根就是 cwd 本身——此时**目录链退化为 `[cwd]`**，
/// 不会去扫祖先目录的指引文件（避免 `/tmp`、`$HOME` 的 `AGENTS.md` 泄漏到无关会话）。
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

/// 发现链上所有存在的指引文件（自带 provenance），宽 → 具体有序。
///
/// 规则：
/// 1. excludes 命中 → **在去重之前**丢弃（被排除的候选不占去重槽位）；
/// 2. 同目录候选**全部加载**（基础层在前，`.local` 覆盖层在后）；
/// 3. 同一绝对路径只加载一次（符号链接 / 同名重复）；
/// 4. **同目录**内 trim 后内容相同的只保留最早的一个；
/// 5. 空文件（trim 后为空）跳过——不遮蔽同目录的其它候选。
pub fn discover_instruction_files(cwd: &Path, cfg: &AgentsMdConfig) -> Vec<InstructionFile> {
    // `..` 必须按真实目录解析，不能把已抵消的目录当成 cwd 的祖先。
    // 无 `..` 时保留发现路径，避免改变 macOS/Windows 上 excludes 的匹配来源。
    let instruction_cwd = if cwd
        .components()
        .any(|c| c == std::path::Component::ParentDir)
    {
        match normalize_parent_components(cwd) {
            Ok(path) => Some(path),
            Err(error) => {
                tracing::warn!(path = %cwd.display(), %error, "无法解析指引工作目录，跳过项目指引");
                None
            }
        }
    } else {
        Some(cwd.to_path_buf())
    };
    let root = instruction_cwd
        .as_deref()
        .map(|path| find_project_root(path, &cfg.project_root_markers))
        .unwrap_or_else(|| cwd.to_path_buf());
    let chain = instruction_cwd
        .as_deref()
        .map(|cwd| ancestor_chain(&root, cwd))
        .unwrap_or_default();
    let mut out: Vec<InstructionFile> = Vec::new();
    let mut seen_abs: HashSet<PathBuf> = HashSet::new();

    // ① 用户全局层（链首，最宽）。用户自有文件 → `@import` 不受项目范围限制。
    if let Some(file) = load_user_global_instruction(cfg, &seen_abs) {
        seen_abs.insert(file.abs_path.clone());
        out.push(file);
    }

    // ② 逐目录（root → cwd），同目录内容去重
    for dir in &chain {
        let mut seen_digest: HashSet<u64> = HashSet::new();
        for name in cfg.all_candidates() {
            let path = dir.join(name);
            let Some(file) = load_candidate(
                &path,
                display_for(&root, &path),
                &seen_abs,
                cfg,
                cfg.import_scope,
            ) else {
                continue;
            };
            if !seen_digest.insert(content_digest(&file.content)) {
                tracing::debug!(
                    path = %file.display,
                    "同目录指引内容重复，跳过（保留最早的候选）"
                );
                continue;
            }
            // 仅真正输出的文件占路径槽位，内容重复候选不能挡住后续目录的规则。
            seen_abs.insert(file.abs_path.clone());
            out.push(file);
        }
    }

    out
}

/// 按平台的路径解析语义消除 `..`，保留等价的发现路径供 excludes 匹配。
/// 全路径 canonicalize 会把 `/var`、junction 等别名替换掉，破坏 excludes。
fn normalize_parent_components(path: &Path) -> std::io::Result<PathBuf> {
    use std::path::Component;
    let mut normalized = if path.is_absolute() {
        PathBuf::new()
    } else {
        std::env::current_dir()?
    };
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                // Win32 在解析 junction 之前先词法消除 ..，也允许 missing/..。
                #[cfg(windows)]
                normalized.pop();
                // Unix 在进入链接指向的目录后再解析 ..，不能直接词法 pop。
                #[cfg(not(windows))]
                {
                    let physical = normalized.canonicalize()?;
                    let physical_parent = physical.parent().unwrap_or(&physical);
                    let lexical_parent = normalized.parent().unwrap_or(&normalized);
                    normalized = if lexical_parent.canonicalize()? == physical_parent {
                        lexical_parent.to_path_buf()
                    } else {
                        physical_parent.to_path_buf()
                    };
                }
            }
            other => normalized.push(other),
        }
    }
    let resolved = path.canonicalize()?;
    if normalized.canonicalize()? == resolved {
        Ok(normalized)
    } else {
        // verbatim 等特殊路径由系统最终解析结果兜底。
        Ok(resolved)
    }
}

/// 查找并加载用户全局指引文件（按候选列表有序尝试，首个存在且非空的文件，@import 不受项目范围限制）。
pub fn load_user_global_instruction(
    cfg: &AgentsMdConfig,
    seen_abs: &HashSet<PathBuf>,
) -> Option<InstructionFile> {
    for candidate in cfg.user_global_candidates() {
        let display = config::display_with_home(&candidate);
        if let Some(file) = load_candidate(
            &candidate,
            display,
            seen_abs,
            cfg,
            ImportScope::Unrestricted,
        ) {
            return Some(file);
        }
    }
    None
}

/// 单个候选文件的完整加载管线（全局层与逐目录两条路径共用）：
/// 存在性 → excludes → 绝对路径去重 → 读 UTF-8 → 归一 BOM/CRLF → 空文件跳过 → 展开 `@import`。
///
/// `scope` 为这份文件的 `@import` 范围策略：项目树内发现 → 配置值（默认 `ProjectRoot`），
/// 用户自有的全局文件 → `Unrestricted`（可信输入，见 `read_global_content` 的说明）。
fn load_candidate(
    path: &Path,
    display: String,
    seen_abs: &HashSet<PathBuf>,
    cfg: &AgentsMdConfig,
    scope: ImportScope,
) -> Option<InstructionFile> {
    // 绝大多数候选不存在，先短路（也避免为缺失文件打 IO 错误日志）
    if !path.is_file() {
        return None;
    }
    // canonicalize 失败（权限 / 环 / 长路径）时退回词法路径：这里只用于**去重与 excludes 匹配**，
    // 不承担安全判定（`@import` 的越界判定另有 canonicalize 必须成功的约束）。
    let abs_path = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    if is_excluded(path, &abs_path, &cfg.excludes) {
        tracing::debug!(path = %path.display(), "指引文件命中 excludes，跳过");
        return None;
    }
    // 同一绝对路径（符号链接 / 同名重复）只加载一次
    if seen_abs.contains(&abs_path) {
        return None;
    }
    let content = match bounded_read::read_bounded_file(path, cfg.max_source_bytes) {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!(path = %path.display(), error = %e, "读取指引文件失败，跳过");
            return None;
        }
    };
    let content = normalize_content(&content);
    if content.trim().is_empty() {
        return None;
    }
    let content = read_with_imports(path, &content, cfg, scope);
    Some(InstructionFile {
        source_path: path.to_path_buf(),
        abs_path,
        display,
        content,
    })
}

/// 展开 `@import`（深度 3 + 环检测 + 范围限制）。
fn read_with_imports(
    path: &Path,
    content: &str,
    cfg: &AgentsMdConfig,
    scope: ImportScope,
) -> String {
    let dir = path.parent().unwrap_or(Path::new("."));
    let mut visited = HashSet::new();
    if let Ok(canonical) = path.canonicalize() {
        visited.insert(canonical);
    }
    let allowed_root = import_allowed_root(path, cfg, scope);
    resolve_imports(
        content,
        dir,
        IMPORT_MAX_DEPTH,
        &mut visited,
        allowed_root.as_deref(),
        cfg.max_source_bytes,
    )
}

/// `@import` 允许读取的根（`None` = 不限制）。
///
/// - [`ImportScope::Unrestricted`] → `None`（用户自有文件；跨项目共享规则是常见需求）；
/// - [`ImportScope::ProjectRoot`] → 该指引文件**所属项目根**（找不到标记时 =
///   该文件所在目录），并 canonicalize 以便与 canonical 化的 import 目标做前缀比较。
fn import_allowed_root(file: &Path, cfg: &AgentsMdConfig, scope: ImportScope) -> Option<PathBuf> {
    match scope {
        ImportScope::Unrestricted => None,
        ImportScope::ProjectRoot => {
            let dir = file.parent().unwrap_or(Path::new("."));
            let root = find_project_root(dir, &cfg.project_root_markers);
            Some(root.canonicalize().unwrap_or(root))
        }
    }
}

/// `@import` 递归深度上限。
pub(crate) const IMPORT_MAX_DEPTH: u32 = 3;

/// 归一正文：去掉 UTF-8 BOM、统一换行为 LF。
///
/// BOM 必须在此去掉，不能指望 `trim()`：`\u{feff}` 不具备 Unicode White_Space 属性，
/// 留在正文里既会把不可见字符送进 prompt，也会**破坏同目录内容去重**
/// （有 BOM 与无 BOM 的同内容文件会被判成两份不同内容）。Notepad 另存为就常见这种文件。
fn normalize_content(content: &str) -> String {
    let content = content.strip_prefix('\u{feff}').unwrap_or(content);
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

/// provenance 头：优先相对项目根（发现路径都由 root 逐级 join 而来，正常必命中），
/// 命中不了（跨盘符 / 左右大小写不一致等）则退化为绝对路径。
/// 用户全局文件另走 [`AgentsMdConfig::user_global_display`]。
fn display_for(root: &Path, path: &Path) -> String {
    match path.strip_prefix(root) {
        Ok(rel) if !rel.as_os_str().is_empty() => config::slash_display(rel),
        _ => config::slash_display(path),
    }
}

/// 渲染单元：provenance 头 + 已按单文件限额截断的正文。
///
/// 头与正文分开存，是为了在「首个文件自己就顶破总量」时**优先保住 provenance 头**。
struct Segment {
    header: String,
    body: String,
}

impl Segment {
    fn new(file: &InstructionFile, cfg: &AgentsMdConfig) -> Self {
        Self {
            header: format!("## {}\n\n", file.display),
            body: truncate_per_file(&file.content, cfg.max_source_bytes, &file.display),
        }
    }

    fn len(&self) -> usize {
        self.header.len() + self.body.len()
    }

    fn push_to(&self, out: &mut String) {
        out.push_str(&self.header);
        out.push_str(&self.body);
    }
}

/// 把 `segments[..take]` 拼成单段（段间空行）。
fn join_segments(segments: &[Segment], take: usize) -> String {
    let mut out = String::new();
    for (idx, seg) in segments[..take].iter().enumerate() {
        if idx > 0 {
            out.push_str("\n\n");
        }
        seg.push_to(&mut out);
    }
    out
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
    // 宽→具体的前缀一旦超限，后续段必定省略，不再为它们复制正文。
    let mut segments = Vec::new();
    let mut total_length = 0usize;
    for file in files {
        let segment = Segment::new(file, cfg);
        total_length = total_length
            .saturating_add(usize::from(!segments.is_empty()) * 2)
            .saturating_add(segment.len());
        segments.push(segment);
        if total_length > cfg.max_bytes {
            break;
        }
    }
    // 第 k 段（含段间空行）的累计长度
    let len_of = |take: usize| -> usize {
        segments[..take].iter().map(Segment::len).sum::<usize>() + 2 * take.saturating_sub(1)
    };

    if segments.len() == files.len() && len_of(segments.len()) <= cfg.max_bytes {
        return join_segments(&segments, segments.len());
    }

    // 需要省略：给「最长省略标记」预留额度（多文件时才有标记）
    let reserve = if files.len() > 1 {
        omit_marker(files.len(), files.len(), cfg.max_bytes).len()
    } else {
        0
    };
    let body_limit = cfg.max_bytes.saturating_sub(reserve);
    let take = (0..=segments.len())
        .rev()
        .find(|&k| len_of(k) <= body_limit)
        .unwrap_or(0);

    let mut out = if take > 0 {
        join_segments(&segments, take)
    } else {
        // 首个文件自己就顶破总量：硬截断正文，但**尽量保住 provenance 头**（§2.2）。
        // 视作「保留了 1 段」，故下面 omitted 需按 1 段算。
        let first = &segments[0];
        let mut head = String::new();
        if first.header.len() < body_limit {
            head.push_str(&first.header);
            head.push_str(&truncate_bytes_head_tail(
                &first.body,
                body_limit - first.header.len(),
                &files[0].display,
            ));
        } else {
            // 额度连头都放不下：整段硬切（保留多少算多少）
            let mut whole = String::new();
            first.push_to(&mut whole);
            head.push_str(char_boundary_prefix(&whole, body_limit));
        }
        head
    };

    let kept = take.max(1);
    let omitted = files.len().saturating_sub(kept);
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

/// 头/尾保留比例；预留标记后，正文预算按 70:20 分配。
const TRUNCATE_HEAD_PERCENT: usize = 70;
const TRUNCATE_TAIL_PERCENT: usize = 20;
/// 标记之外的余量：避免把 head/tail 挤成 0 字节（标记自身也占额度）。
const TRUNCATE_MARKER_SLACK: usize = 16;

/// 单文件超限：按字节预算保留头尾，在 UTF-8 字符边界截断。
fn truncate_per_file(content: &str, max_source_bytes: usize, name: &str) -> String {
    truncate_bytes_head_tail(content, max_source_bytes, name)
}

/// 按字节上限保留头尾（UTF-8 字符边界安全），中间插标记。
/// 保证返回值字节数不超过 `max_bytes`。
fn truncate_bytes_head_tail(content: &str, max_bytes: usize, name: &str) -> String {
    if content.len() <= max_bytes {
        return content.to_string();
    }
    let Some((marker, head_budget, tail_budget)) =
        bounded_read::truncation_layout(max_bytes, name, content.len() as u64)
    else {
        return char_boundary_prefix(content, max_bytes).to_string();
    };
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

/// `@import` 语法前缀 / 结尾（长度即切片偏移，避免手算魔法数）。
const IMPORT_PREFIX: &str = "<!-- @import ";
const IMPORT_SUFFIX: &str = " -->";

/// 解析并读取一个 `@import` 目标；返回 `None` 表示**该 import 不生效**（保留占位符）。
///
/// 失败即 `None`（调用方统一保留占位符 + 已 warn 留痕）：
/// 越出允许范围、`canonicalize` 失败（受限范围下无法确认是否越界 → **fail-closed**）、
/// 成环、不存在、存在但不可读。
fn read_import_target(
    import_path: &str,
    base_dir: &Path,
    allowed_root: Option<&Path>,
    visited: &HashSet<PathBuf>,
    max_bytes: usize,
) -> Option<(PathBuf, String)> {
    let joined = base_dir.join(import_path);
    let resolved = match (joined.canonicalize(), allowed_root) {
        (Ok(path), _) => path,
        // 不受限时容忍 canonicalize 失败（保留旧的宽松行为）
        (Err(_), None) => joined,
        (Err(e), Some(_)) => {
            tracing::warn!(
                path = %joined.display(),
                error = %e,
                "@import 目标无法规范化，受限范围下保留占位符"
            );
            return None;
        }
    };
    if let Some(root) = allowed_root {
        if !resolved.starts_with(root) {
            tracing::warn!(
                path = %resolved.display(),
                "@import 目标越出允许范围，保留占位符（ImportScope::ProjectRoot）"
            );
            return None;
        }
    }
    if visited.contains(&resolved) {
        return None; // 成环
    }
    if !resolved.is_file() {
        return None; // 不存在（或非普通文件：目录 / FIFO，不读）
    }
    match bounded_read::read_bounded_file(&resolved, max_bytes) {
        Ok(content) => Some((resolved, content)),
        Err(e) => {
            tracing::warn!(
                path = %resolved.display(),
                error = %e,
                "@import 目标不可读，保留占位符"
            );
            None
        }
    }
}

/// 递归解析 `<!-- @import path -->` 引用，替换为引用文件内容。
/// `base_dir` 为包含 @import 的文件所在目录。
/// `depth` 递归深度上限，`visited` 防循环。
/// `allowed_root` 非 `None` 时，解析结果必须落在该目录内，否则保留占位符（防路径穿越）。
pub(crate) fn resolve_imports(
    content: &str,
    base_dir: &Path,
    depth: u32,
    visited: &mut HashSet<PathBuf>,
    allowed_root: Option<&Path>,
    max_bytes: usize,
) -> String {
    match resolve_imports_with_budget(
        content,
        base_dir,
        depth,
        visited,
        allowed_root,
        max_bytes,
        max_bytes,
    ) {
        Ok(result) => result,
        Err(prefix) => mark_import_limit(prefix, max_bytes),
    }
}

/// 每个目标有独立的读取上限，展开结果共用剩余预算。
/// Err 携带已展开前缀，表示真正发生了增长溢出；满预算后的空导入仍可完成。
fn resolve_imports_with_budget(
    content: &str,
    base_dir: &Path,
    depth: u32,
    visited: &mut HashSet<PathBuf>,
    allowed_root: Option<&Path>,
    source_limit: usize,
    max_bytes: usize,
) -> Result<String, String> {
    if depth == 0 {
        let mut result = String::new();
        return if append_with_limit(&mut result, content, max_bytes) {
            Ok(result)
        } else {
            Err(result)
        };
    }
    let mut result = String::with_capacity(content.len().min(max_bytes));
    let mut pos = 0;
    while pos < content.len() {
        if let Some(offset) = content[pos..].find(IMPORT_PREFIX) {
            let abs_pos = pos + offset;
            if !append_with_limit(&mut result, &content[pos..abs_pos], max_bytes) {
                return Err(result);
            }
            // 提取 path：从 "<!-- @import " 之后到 " -->"
            let after = &content[abs_pos + IMPORT_PREFIX.len()..];
            if let Some(end) = after.find(IMPORT_SUFFIX) {
                let placeholder =
                    &content[abs_pos..abs_pos + IMPORT_PREFIX.len() + end + IMPORT_SUFFIX.len()];
                let import_path = after[..end].trim();
                let remaining = max_bytes.saturating_sub(result.len());
                let imported =
                    read_import_target(import_path, base_dir, allowed_root, visited, source_limit);
                match imported {
                    Some((resolved, imported_content)) => {
                        visited.insert(resolved.clone());
                        let imported_content = normalize_content(&imported_content);
                        let import_dir = resolved.parent().unwrap_or(base_dir);
                        let expanded = resolve_imports_with_budget(
                            &imported_content,
                            import_dir,
                            depth - 1,
                            visited,
                            allowed_root,
                            source_limit,
                            remaining,
                        );
                        match expanded {
                            Ok(content) => result.push_str(&content),
                            Err(prefix) => {
                                result.push_str(&prefix);
                                return Err(result);
                            }
                        }
                    }
                    // 越界 / 不存在 / 成环 / 不可读 → 一律保留占位符（并已 warn 留痕）
                    None => {
                        if !append_with_limit(&mut result, placeholder, max_bytes) {
                            return Err(result);
                        }
                    }
                }
                pos = abs_pos + IMPORT_PREFIX.len() + end + IMPORT_SUFFIX.len();
            } else {
                // 没找到 " -->"，不是有效的 @import，原样保留
                if !append_with_limit(&mut result, IMPORT_PREFIX, max_bytes) {
                    return Err(result);
                }
                pos = abs_pos + IMPORT_PREFIX.len();
            }
        } else {
            if !append_with_limit(&mut result, &content[pos..], max_bytes) {
                return Err(result);
            }
            break;
        }
    }
    Ok(result)
}

/// 导入展开逐段消耗同一份字节预算，不能先构造完整展开结果再截断。
fn append_with_limit(out: &mut String, content: &str, max_bytes: usize) -> bool {
    let remaining = max_bytes.saturating_sub(out.len());
    out.push_str(char_boundary_prefix(content, remaining));
    content.len() <= remaining
}

fn mark_import_limit(mut out: String, max_bytes: usize) -> String {
    let marker = "\n\n[...truncated imports: source byte limit reached. Use file tools to read the full file.]";
    if marker.len() > max_bytes {
        return out;
    }
    let keep = char_boundary_prefix(&out, max_bytes - marker.len()).len();
    out.truncate(keep);
    out.push_str(marker);
    out
}

impl Default for AgentsMdMiddleware {
    fn default() -> Self {
        Self::new()
    }
}

impl AgentsMdMiddleware {
    /// 本轮要注入的整段内容（`None` = 无指引，不必注入）。
    ///
    /// 冻结数据优先（`session/new` 已渲染好，跳过全部磁盘 I/O）；未冻结时
    /// （子 Agent 等）按 cwd 现场发现 + 渲染——两条路径共用同一套加载器。
    fn content_for(&self, cwd: &str) -> Option<String> {
        match self.frozen {
            // 空串也是合法的冻结快照，不注入，也不再读盘。
            Some(ref frozen) => (!frozen.trim().is_empty()).then(|| frozen.clone()),
            None => load_instructions(Path::new(cwd), &self.config),
        }
    }
}

#[async_trait]
impl<S: State> Middleware<S> for AgentsMdMiddleware {
    fn name(&self) -> &str {
        "AgentsMdMiddleware"
    }

    async fn before_agent(&self, state: &mut S) -> AgentResult<()> {
        let cwd = state.cwd().to_string();
        if let Some(content) = self.content_for(&cwd) {
            // 前插系统消息（置于消息历史开头，优先于 Human 消息）；
            // **单条**，保 Prompt Cache 前缀稳定。
            state.prepend_message(BaseMessage::system(content));
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "regression_test.rs"]
mod regression_tests;

#[cfg(test)]
mod tests {
    use super::*;
    use cc_agent::agent::state::AgentState;
    include!("agents_md_test.rs");
}
