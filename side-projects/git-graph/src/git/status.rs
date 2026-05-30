use anyhow::Result;
use git2::Repository;

/// 单个文件的 git 状态
#[derive(Debug, Clone)]
pub struct StatusEntry {
    pub path: String,
    pub status: FileStatus,
}

/// 文件 git 状态分类
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileStatus {
    New,
    Modified,
    Deleted,
    Renamed,
    TypeChange,
    WorkingModified,
    WorkingDeleted,
    Untracked,
    Conflicted,
}

/// git status 查询结果
#[derive(Debug, Clone, Default)]
pub struct StatusResult {
    /// 已暂存的变更
    pub staged: Vec<StatusEntry>,
    /// 工作区变更（未暂存）
    pub unstaged: Vec<StatusEntry>,
    /// 未跟踪文件
    pub untracked: Vec<StatusEntry>,
}

impl StatusResult {
    #[allow(dead_code)]
    pub fn total_count(&self) -> usize {
        self.staged.len() + self.unstaged.len() + self.untracked.len()
    }

    #[allow(dead_code)]
    pub fn is_empty(&self) -> bool {
        self.total_count() == 0
    }
}

impl FileStatus {
    #[allow(dead_code)]
    pub fn display_char(&self) -> char {
        match self {
            Self::New => 'A',
            Self::Modified | Self::WorkingModified => 'M',
            Self::Deleted | Self::WorkingDeleted => 'D',
            Self::Renamed => 'R',
            Self::TypeChange => 'T',
            Self::Untracked => '?',
            Self::Conflicted => 'U',
        }
    }

    pub fn is_staged(&self) -> bool {
        matches!(
            self,
            Self::New | Self::Modified | Self::Deleted | Self::Renamed | Self::TypeChange
        )
    }
}

/// 读取仓库的 git status
pub fn read_status(repo: &Repository) -> Result<StatusResult> {
    let mut opts = git2::StatusOptions::new();
    opts.include_untracked(true)
        .recurse_untracked_dirs(true)
        .exclude_submodules(true);

    let statuses = repo.statuses(Some(&mut opts))?;
    let mut result = StatusResult::default();

    for entry in statuses.iter() {
        let path = entry.path().unwrap_or("").to_string();
        let s = entry.status();

        let status = if s.is_index_new() {
            FileStatus::New
        } else if s.is_index_modified() {
            FileStatus::Modified
        } else if s.is_index_deleted() {
            FileStatus::Deleted
        } else if s.is_index_renamed() {
            FileStatus::Renamed
        } else if s.is_index_typechange() {
            FileStatus::TypeChange
        } else if s.is_conflicted() {
            FileStatus::Conflicted
        } else if s.is_wt_modified() {
            FileStatus::WorkingModified
        } else if s.is_wt_deleted() {
            FileStatus::WorkingDeleted
        } else if s.is_wt_new() {
            FileStatus::Untracked
        } else {
            continue;
        };

        let se = StatusEntry {
            path,
            status,
        };

        if status.is_staged() {
            result.staged.push(se);
        } else if status == FileStatus::Untracked {
            result.untracked.push(se);
        } else {
            result.unstaged.push(se);
        }
    }

    Ok(result)
}
