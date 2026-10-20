use fuzzy_matcher::{skim::SkimMatcherV2, FuzzyMatcher};
use std::path::Path;
use walkdir::WalkDir;

/// 文件搜索候选结果
#[derive(Clone)]
pub struct FileCandidate {
    pub path: String,
    /// 用于显示的相对路径
    pub display: String,
    pub is_dir: bool,
    pub score: i64,
}

pub(super) const MAX_CANDIDATES: usize = 15;

/// 目录过滤列表——与 GlobFilesTool (cc-middlewares/src/tools/filesystem/glob.rs) 对齐
const SKIP_DIRS: &[&str] = &[
    "node_modules",
    ".git",
    "dist",
    "build",
    ".next",
    ".turbo",
    "coverage",
    ".nyc_output",
    "temp",
    ".cache",
    "vendor",
    "venv",
    "__pycache__",
    "target",
    "out",
    ".output",
];

fn should_skip_dir(name: &str) -> bool {
    SKIP_DIRS.contains(&name)
}

/// 根据 cwd 和查询字符串搜索文件候选。
/// 使用 walkdir 遍历（与 GlobFilesTool 对齐），一次性遍历全量文件，再 fuzzy 匹配。
pub fn search_files(cwd: &str, query: &str) -> Vec<FileCandidate> {
    if query.is_empty() {
        return list_current_directory(cwd);
    }

    let base = Path::new(cwd);
    let matcher = SkimMatcherV2::default();

    // 解析目录部分和文件名部分
    let (dir_part, file_part): (String, &str) = if let Some(slash_pos) = query.rfind('/') {
        (query[..=slash_pos].to_string(), &query[slash_pos + 1..])
    } else {
        (String::new(), query)
    };

    let walker = WalkDir::new(base)
        .follow_links(false) // 禁止跟随符号链接，防止 /proc、/sys 等虚拟文件系统引发指数级膨胀
        .into_iter()
        .filter_entry(|e| {
            if e.file_type().is_dir() {
                let name = e.file_name().to_string_lossy();
                !should_skip_dir(&name)
            } else {
                true
            }
        });

    let mut raw: Vec<(String, bool, i64)> = Vec::new();

    for entry in walker {
        let Ok(entry) = entry else { continue };
        let Ok(rel) = entry.path().strip_prefix(base) else {
            continue;
        };
        let rel_str = rel.to_string_lossy().replace('\\', "/");

        if rel_str.is_empty() {
            continue;
        }

        // 目录前缀过滤
        if !dir_part.is_empty() && !rel_str.starts_with(&dir_part) {
            continue;
        }

        let is_dir = entry.file_type().is_dir();
        let file_name = rel
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();

        let name_score = if file_part.is_empty() {
            50
        } else {
            matcher.fuzzy_match(&file_name, file_part).unwrap_or(0)
        };

        if name_score <= 0 && !file_part.is_empty() {
            continue;
        }

        let path_score = matcher.fuzzy_match(&rel_str, query).unwrap_or(0);
        let score = name_score * 2 + path_score;

        if score > 0 || file_part.is_empty() {
            raw.push((rel_str, is_dir, score));
        }
    }

    raw.sort_by(|a, b| b.2.cmp(&a.2).then_with(|| a.0.len().cmp(&b.0.len())));
    raw.truncate(MAX_CANDIDATES);

    raw.into_iter()
        .map(|(path, is_dir, score)| FileCandidate {
            display: path.clone(),
            path,
            is_dir,
            score,
        })
        .collect()
}

/// 默认候选仅枚举当前目录，目录优先、名称排序，内存中最多保留 15 项。
fn list_current_directory(cwd: &str) -> Vec<FileCandidate> {
    let Ok(entries) = std::fs::read_dir(cwd) else {
        return Vec::new();
    };
    let mut candidates = Vec::with_capacity(MAX_CANDIDATES + 1);
    for entry in entries.flatten() {
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        let name = entry.file_name().to_string_lossy().into_owned();
        let is_dir = file_type.is_dir();
        if is_dir && should_skip_dir(&name) {
            continue;
        }
        candidates.push(FileCandidate {
            display: name.clone(),
            path: name,
            is_dir,
            score: 0,
        });
        candidates.sort_by(|a, b| b.is_dir.cmp(&a.is_dir).then_with(|| a.path.cmp(&b.path)));
        candidates.truncate(MAX_CANDIDATES);
    }
    candidates
}

/// 从已有候选列表中过滤匹配 query 的结果（纯内存操作，无 IO）
pub fn filter_candidates(candidates: &[FileCandidate], query: &str) -> Vec<FileCandidate> {
    let matcher = SkimMatcherV2::default();
    let (dir_part, file_part): (String, &str) = if let Some(slash_pos) = query.rfind('/') {
        (query[..=slash_pos].to_string(), &query[slash_pos + 1..])
    } else {
        (String::new(), query)
    };

    let mut results: Vec<FileCandidate> = candidates
        .iter()
        .filter_map(|c| {
            if !dir_part.is_empty() && !c.path.starts_with(&dir_part) {
                return None;
            }

            if file_part.is_empty() {
                return Some(FileCandidate {
                    score: c.score,
                    ..c.clone()
                });
            }

            let file_name = c.path.rsplit('/').next().unwrap_or(&c.path).to_string();
            let name_score = matcher.fuzzy_match(&file_name, file_part).unwrap_or(0);
            let path_score = matcher.fuzzy_match(&c.path, query).unwrap_or(0);
            let score = name_score * 2 + path_score;

            if score > 0 {
                Some(FileCandidate { score, ..c.clone() })
            } else {
                None
            }
        })
        .collect();

    results.sort_by(|a, b| {
        b.score
            .cmp(&a.score)
            .then_with(|| a.path.len().cmp(&b.path.len()))
    });
    results.truncate(MAX_CANDIDATES);
    results
}

#[cfg(test)]
#[path = "file_search_test.rs"]
mod tests;
