//! 记忆目录与路径解析模块，对齐 CC 规范。

use std::path::{Path, PathBuf};

pub const MAX_SANITIZED_LENGTH: usize = 200;

/// 对项目根路径做规范化清洗，用于作为 projects/<sanitized>/ 目录名。
///
/// 算法完全对齐 CC sanitize_path:
/// 将所有非 ASCII 字母数字 `[^a-zA-Z0-9]` 替换为 `-`。
/// 长度超限（>200）时截断前 200 字符并追加简单哈希后缀。
pub fn sanitize_project_path(raw: &str) -> String {
    // 移除 Windows 原生路径的 \\?\ 前缀
    let clean = raw.strip_prefix(r"\\?\").unwrap_or(raw);
    let sanitized: String = clean
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect();

    if sanitized.len() <= MAX_SANITIZED_LENGTH {
        sanitized
    } else {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        let mut hasher = DefaultHasher::new();
        clean.hash(&mut hasher);
        let hash = format!("{:x}", hasher.finish());
        format!("{}-{}", &sanitized[..MAX_SANITIZED_LENGTH], hash)
    }
}

/// 寻找规范的 Git 根目录（如果是 worktree 则解析真正的共同根目录，保证所有 worktree 共享一份记忆）
pub fn find_canonical_project_root(cwd: &Path) -> PathBuf {
    let mut cur: Option<&Path> = Some(cwd);
    while let Some(dir) = cur {
        let git_entry = dir.join(".git");
        if git_entry.is_dir() {
            return dir.to_path_buf();
        } else if git_entry.is_file() {
            // Git worktree: .git 是一个包含 "gitdir: /path/to/main/.git/worktrees/name" 的文件
            if let Ok(content) = std::fs::read_to_string(&git_entry) {
                if let Some(gitdir_line) = content.lines().find(|l| l.starts_with("gitdir:")) {
                    let gitdir_path = gitdir_line.trim_start_matches("gitdir:").trim();
                    let resolved = if Path::new(gitdir_path).is_absolute() {
                        PathBuf::from(gitdir_path)
                    } else {
                        dir.join(gitdir_path)
                    };
                    // 通常位于 <repo_root>/.git/worktrees/<name>
                    if let Some(worktrees_dir) = resolved.parent() {
                        if worktrees_dir.file_name().and_then(|n| n.to_str()) == Some("worktrees") {
                            if let Some(dot_git) = worktrees_dir.parent() {
                                if let Some(common_root) = dot_git.parent() {
                                    return common_root.to_path_buf();
                                }
                            }
                        }
                    }
                }
            }
            return dir.to_path_buf();
        }
        cur = dir.parent();
    }
    cwd.to_path_buf()
}

/// 解析自动记忆存储目录（结尾带目录分隔符以符合契约）。
///
/// 优先级（100% 互通已有记忆）：
/// 1. 若已有历史记忆（~/.claude/projects/），透明继承复用
/// 2. 否则统一落盘在 ~/.cc-code/projects/<sanitized>/memory/
pub fn get_auto_memory_dir(cwd: &str) -> PathBuf {
    let canonical_root = find_canonical_project_root(Path::new(cwd));
    let sanitized = sanitize_project_path(&canonical_root.to_string_lossy());

    let home = dirs_next::home_dir().unwrap_or_else(|| PathBuf::from("."));
    let cc_code_project_mem = home.join(".cc-code").join("projects").join(&sanitized).join("memory");
    let legacy_claude_project_mem = home.join(".claude").join("projects").join(&sanitized).join("memory");

    if legacy_claude_project_mem.exists() && !cc_code_project_mem.exists() {
        legacy_claude_project_mem
    } else {
        cc_code_project_mem
    }
}

/// 获取自动记忆的脱敏展示目录（用于 System Prompt 呈现，绝不泄露宿主真实用户名和绝对盘符）。
/// 无论 Windows 还是 Unix，一律呈现为规范的 `~/.cc-code/projects/<sanitized>/memory/`
pub fn get_auto_memory_display_dir(cwd: &str) -> String {
    let canonical_root = find_canonical_project_root(Path::new(cwd));
    let sanitized = sanitize_project_path(&canonical_root.to_string_lossy());
    format!("~/.cc-code/projects/{}/memory/", sanitized)
}

/// 获取入口文件的脱敏展示路径
pub fn get_auto_memory_display_entrypoint(cwd: &str) -> String {
    let canonical_root = find_canonical_project_root(Path::new(cwd));
    let sanitized = sanitize_project_path(&canonical_root.to_string_lossy());
    format!("~/.cc-code/projects/{}/memory/MEMORY.md", sanitized)
}

/// 获取入口索引文件路径 `.../memory/MEMORY.md`
pub fn get_auto_memory_entrypoint(cwd: &str) -> PathBuf {
    get_auto_memory_dir(cwd).join(super::entrypoint::ENTRYPOINT_NAME)
}

/// 确保记忆目录物理存在，以便模型调用 Write 工具时不因目录缺失报错
pub fn ensure_memory_dir_exists(memory_dir: &Path) -> std::io::Result<()> {
    if !memory_dir.exists() {
        std::fs::create_dir_all(memory_dir)?;
    }
    Ok(())
}

/// 检查给定路径是否落在当前项目的自动记忆目录内。
///
/// 用于 HITL / Jev 安全门豁免：自动记忆目录位于 ~/.claude/projects/ 或 ~/.cc-code/projects/，
/// 默认包含受保护的 .claude / .cc-code 片段；如果不对该目录进行针对性豁免，
/// 模型正常写入记忆时会被误拦截或强弹审批窗。
pub fn is_auto_mem_path(candidate: &Path, cwd: &str) -> bool {
    let mem_dir = get_auto_memory_dir(cwd);
    if candidate.starts_with(&mem_dir) {
        return true;
    }

    let cand_str = candidate.to_string_lossy();
    let cand_norm = if let Some(rest) = cand_str.strip_prefix("~/").or_else(|| cand_str.strip_prefix("~\\")) {
        dirs_next::home_dir().map(|h| h.join(rest)).unwrap_or_else(|| candidate.to_path_buf())
    } else {
        candidate.to_path_buf()
    };

    if cand_norm.starts_with(&mem_dir) {
        return true;
    }

    // Canonicalize 匹配（若在磁盘已存在，解析符号链接）
    if let (Ok(cand_canon), Ok(mem_dir_canon)) = (cand_norm.canonicalize(), mem_dir.canonicalize()) {
        if cand_canon.starts_with(&mem_dir_canon) {
            return true;
        }
    } else if let (Ok(mem_dir_canon), Some(parent)) = (mem_dir.canonicalize(), cand_norm.parent()) {
        if let Ok(parent_canon) = parent.canonicalize() {
            if parent_canon.starts_with(&mem_dir_canon) {
                return true;
            }
        }
    }

    false
}
