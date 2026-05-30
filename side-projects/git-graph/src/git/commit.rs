use git2::Oid;

/// 轻量拓扑节点（一次性扫描所有 commit 生成）
#[derive(Debug, Clone)]
pub struct TopoNode {
    pub oid: Oid,
    pub parent_oids: Vec<Oid>,
    /// commit 时间戳（用于排序）
    pub time: i64,
    /// 单行 commit message（第一行）
    pub message_short: String,
}

/// 完整 commit 详情（按需加载）
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct CommitDetail {
    pub oid: Oid,
    pub short_hash: String,
    pub author_name: String,
    pub author_email: String,
    pub time: i64,
    pub message: String,
    pub parent_oids: Vec<Oid>,
    /// 关联的 branch 名
    pub branches: Vec<String>,
    /// 关联的 tag 名
    pub tags: Vec<String>,
    /// 文件变更统计（按需加载）
    pub stats: Option<DiffStats>,
}

/// 文件变更统计
#[derive(Debug, Clone)]
pub struct DiffStats {
    pub files: Vec<FileChange>,
    pub insertions: usize,
    pub deletions: usize,
    pub files_changed: usize,
}

/// 单文件变更
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct FileChange {
    pub path: String,
    pub old_path: Option<String>,
    pub status: FileStatus,
    pub insertions: usize,
    pub deletions: usize,
}

/// 文件变更类型
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileStatus {
    Added,
    Deleted,
    Modified,
    Renamed,
    Copied,
    TypeChange,
    Unmodified,
}

impl FileStatus {
    pub fn from_delta(delta: git2::Delta) -> Self {
        match delta {
            git2::Delta::Added => Self::Added,
            git2::Delta::Deleted => Self::Deleted,
            git2::Delta::Modified => Self::Modified,
            git2::Delta::Renamed => Self::Renamed,
            git2::Delta::Copied => Self::Copied,
            git2::Delta::Typechange => Self::TypeChange,
            _ => Self::Unmodified,
        }
    }
}
