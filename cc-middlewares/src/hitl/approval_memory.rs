use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// 会话级审批记忆，只记录用户明确选择「本次会话同意」的调用，不落盘。
///
/// Read/Write/Edit 按工具与路径记忆；Bash 按完整命令、执行目录与当前分支记忆，
/// 不按命令前缀放行。其他工具按真实工具名、完整参数与目录记忆。
/// ExecuteExtraTool 使用解包后的真实调用，与直接调用共享同一条目。
#[derive(Default)]
pub struct ApprovalMemory {
    approved: Mutex<HashSet<Fingerprint>>,
}

/// 键：`(真实工具名, 路径或带类型标记的完整调用)`。
type Fingerprint = (String, String);

impl ApprovalMemory {
    /// 创建会话级审批记忆，返回 `Arc` 供跨 prompt 复用
    /// （middleware 实例每次 prompt 重建，记忆必须由更外层持有）。
    pub fn new() -> std::sync::Arc<Self> {
        std::sync::Arc::new(Self::default())
    }

    /// 生成某个调用的指纹。`path` 为 `None`（命令类工具）时返回 `None`，不参与记忆。
    ///
    /// `cwd` 用于把相对路径拼成绝对路径后再规范化，保证同一文件的不同写法命中同一条目。
    pub fn fingerprint(tool_name: &str, path: Option<&Path>, cwd: &Path) -> Option<Fingerprint> {
        let raw = path?;
        let abs = if raw.is_absolute() {
            raw.to_path_buf()
        } else {
            cwd.join(raw)
        };
        let normalized = normalize_lexical(&abs).unwrap_or(abs);
        Some((tool_name.to_string(), normalized.to_str()?.to_string()))
    }

    /// 使用实际执行参数生成会话记忆键，参数不完整时不记录。
    pub fn call_fingerprint(
        tool_name: &str,
        input: &serde_json::Value,
        cwd: &Path,
    ) -> Option<Fingerprint> {
        let target = super::effective_tool_name(tool_name, input);
        let params = super::jev::effective_params(tool_name, input);
        if matches!(target.as_str(), "Read" | "Write" | "Edit") {
            let path = params
                .get("file_path")
                .or_else(|| params.get("path"))?
                .as_str()?;
            return Self::fingerprint(&target, Some(Path::new(path)), cwd);
        }
        let actual_cwd = std::fs::canonicalize(cwd).unwrap_or_else(|_| cwd.to_path_buf());
        let actual_cwd = normalize_lexical(&actual_cwd).unwrap_or(actual_cwd);
        let directory = actual_cwd.to_str()?;
        let key = if target == "Bash" {
            let command = params.get("command")?.as_str()?;
            let branch = super::jev::policy::git_branch(cwd);
            serde_json::to_string(&(directory, command, branch)).ok()?
        } else {
            serde_json::to_string(&(directory, params)).ok()?
        };
        // NUL 不可能出现在真实文件路径中，避免与路径键碰撞。
        Some((target, format!("\0call:{key}")))
    }

    /// 是否已批准过该指纹
    pub fn is_approved(&self, fp: &Fingerprint) -> bool {
        self.approved
            .lock()
            .map(|set| set.contains(fp))
            .unwrap_or(false)
    }

    /// 记录一次用户批准
    pub fn record(&self, fp: Fingerprint) {
        if let Ok(mut set) = self.approved.lock() {
            set.insert(fp);
        }
    }

    /// 清空记忆（session 切换/新建时调用）
    pub fn clear(&self) {
        if let Ok(mut set) = self.approved.lock() {
            set.clear();
        }
    }

    /// 当前条目数（测试用）
    #[cfg(test)]
    pub fn len(&self) -> usize {
        self.approved.lock().map(|s| s.len()).unwrap_or(0)
    }

    /// 是否为空（与 `len` 配对，满足 clippy::len_without_is_empty）
    #[cfg(test)]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// 词法规范化：折叠 `..` / `.` / 重复分隔符，不触碰文件系统。
///
/// 返回绝对路径；输入为相对路径且无根时返回 `None`（调用方退化为原样）。
fn normalize_lexical(path: &Path) -> Option<PathBuf> {
    use std::path::Component;
    let mut out: Vec<Component> = Vec::new();
    let mut prefix = None;
    let mut has_root = false;

    for comp in path.components() {
        match comp {
            Component::Prefix(p) => {
                prefix = Some(p.as_os_str());
            }
            Component::RootDir => {
                has_root = true;
                out.push(Component::RootDir);
            }
            Component::CurDir => {}
            Component::ParentDir => {
                // 只在能消掉上一个普通段时才 pop，且不越过根
                if matches!(out.last(), Some(Component::Normal(_))) {
                    out.pop();
                } else if !has_root {
                    out.push(Component::ParentDir);
                }
            }
            Component::Normal(seg) => out.push(Component::Normal(seg)),
        }
    }

    if !has_root && prefix.is_none() {
        return None; // 相对路径：无法保证绝对唯一，退化
    }

    let mut result = PathBuf::new();
    if let Some(p) = prefix {
        result.push(p);
    }
    for comp in &out {
        match comp {
            Component::RootDir => result.push(std::path::MAIN_SEPARATOR.to_string()),
            Component::Normal(seg) => result.push(seg),
            _ => {}
        }
    }
    Some(result)
}

#[cfg(test)]
#[path = "approval_memory_test.rs"]
mod tests;
