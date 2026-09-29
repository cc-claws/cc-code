use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// 审批记忆：**路径级、会话作用域**。
///
/// 背景：Jev 语义门是**无状态**的——每次工具调用独立判定。同一文件反复编辑时，
/// 模型对 `local_scope` 等条件打分会在阈值附近抖动（观测到 0.07~0.16 区间），
/// 于是同一操作可能一次 `allow`、一次 `review`，用户被反复弹窗。
///
/// 本结构记住「已批准过的 (工具名, 规范化路径)」，命中即免问：
/// - **粒度**：`(tool_name, normalized_path)`。`Edit(a.md)` 批准后，改 `a.md` 免问；
///   改 `b.md` 仍会问。不做工具名级（太松）也不做内容指纹级（Edit 内容每次不同，几乎不命中）。
/// - **作用域**：session 级（随 session 新建/切换清空），不落盘。
/// - **仅记录 `Approve`**：`Edit`（用户改了参数）与 `Reject` 不记录，避免把「改过的版本」当成放行依据。
///
/// 只对**带明确路径**的调用生效（Edit / Write / Read 等）；无路径的命令类工具（Bash）
/// 不参与记忆——`build_gate_call` 中 `path` 为 `None` 时直接跳过。
#[derive(Default)]
pub struct ApprovalMemory {
    approved: Mutex<HashSet<Fingerprint>>,
}

/// 审批记忆的键：`(工具名, 规范化绝对路径)`。
///
/// 路径经词法规范化（`..`/`.`/重复分隔符折叠），确保 `a/./b` 与 `a/b` 视为同一目标。
/// 规范化失败（如相对路径且无法拼出绝对）时退化为原样字符串，宁可少命中不可误命中。
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
        Some((
            tool_name.to_string(),
            normalized.to_string_lossy().into_owned(),
        ))
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
}

/// 词法规范化：折叠 `..` / `.` / 重复分隔符，不触碰文件系统。
///
/// 返回绝对路径；输入为相对路径且无根时返回 `None`（调用方退化为原样）。
fn normalize_lexical(path: &Path) -> Option<PathBuf> {
    use std::path::Component;
    let mut out: Vec<Component> = Vec::new();
    let mut prefix: Option<std::path::Prefix> = None;
    let mut has_root = false;

    for comp in path.components() {
        match comp {
            Component::Prefix(p) => {
                prefix = Some(p.kind());
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
        result.push(match p {
            std::path::Prefix::Disk(d) => format!("{}:", d as char),
            std::path::Prefix::VerbatimDisk(d) => format!("{}:", d as char),
            _ => String::new(),
        });
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
