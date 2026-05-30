use fuzzy_matcher::skim::SkimMatcherV2;
use fuzzy_matcher::FuzzyMatcher;
use std::path::Path;

/// 文件搜索候选结果
#[derive(Clone)]
pub struct FileCandidate {
    pub path: String,
    /// 用于显示的相对路径
    pub display: String,
    pub is_dir: bool,
    pub score: i64,
}

const MAX_GLOB_RESULTS: usize = 200;
const MAX_CANDIDATES: usize = 15;

/// glob 时需要忽略的目录名
const IGNORED_DIRS: &[&str] = &[
    "target",
    "node_modules",
    ".git",
    "dist",
    "build",
    ".next",
    "__pycache__",
    ".venv",
    "venv",
];

/// 根据 cwd 和查询字符串搜索文件候选
pub fn search_files(cwd: &str, query: &str) -> Vec<FileCandidate> {
    if query.is_empty() {
        return Vec::new();
    }

    let base = Path::new(cwd);
    let matcher = SkimMatcherV2::default();

    // 解析目录部分和文件名部分
    let (dir_part, file_part): (String, &str) = if let Some(slash_pos) = query.rfind('/') {
        (query[..=slash_pos].to_string(), &query[slash_pos + 1..])
    } else {
        (String::new(), query)
    };

    // 构建 glob 模式：基于 cwd 的绝对路径
    let dir_abs = if dir_part.is_empty() {
        cwd.to_string()
    } else {
        format!(
            "{}/{}",
            cwd.trim_end_matches('/'),
            dir_part.trim_end_matches('/')
        )
    };

    let pattern = if file_part.is_empty() {
        format!("{}/**/*", dir_abs)
    } else {
        format!("{}/**/*{}*", dir_abs, file_part)
    };

    let Ok(paths) = glob::glob(&pattern) else {
        return Vec::new();
    };

    let mut raw: Vec<(String, bool, i64)> = Vec::new();

    for entry in paths.take(MAX_GLOB_RESULTS) {
        let Ok(entry) = entry else { continue };
        let Ok(rel) = entry.strip_prefix(base) else {
            continue;
        };
        let rel_str = rel.to_string_lossy().to_string();

        // 跳过忽略目录
        if should_ignore(&rel_str) {
            continue;
        }

        let is_dir = entry.is_dir();
        let file_name = rel
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();

        let name_score = if file_part.is_empty() {
            50 // 目录浏览时给基准分
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

    // 排序：分数降序，路径长度升序
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

/// 计算所有候选路径的公共前缀（用于 Tab 补全）
#[allow(dead_code)]
pub fn find_common_prefix(candidates: &[FileCandidate]) -> Option<String> {
    if candidates.is_empty() {
        return None;
    }
    let first = &candidates[0].path;
    let mut end = first.len();
    for cand in &candidates[1..] {
        let common: String = first
            .chars()
            .zip(cand.path.chars())
            .take_while(|(a, b)| a == b)
            .map(|(a, _)| a)
            .collect();
        if common.len() < end {
            end = common.len();
        }
    }
    if end == 0 {
        return None;
    }
    Some(first.chars().take(end).collect())
}

/// 从已有候选列表中过滤匹配 query 的结果（纯内存操作，无 IO）
/// 用于 query 变长时从缓存过滤，避免重新 glob
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
            // 路径必须以 dir_part 开头
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

fn should_ignore(rel_path: &str) -> bool {
    for component in rel_path.split('/') {
        if IGNORED_DIRS.contains(&component) {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn test_search_by_name() {
        let dir = tempdir().unwrap();
        let base = dir.path();
        fs::write(base.join("main.rs"), "").unwrap();
        fs::write(base.join("lib.rs"), "").unwrap();
        fs::create_dir_all(base.join("src")).unwrap();
        fs::write(base.join("src/main.rs"), "").unwrap();

        let results = search_files(&base.to_string_lossy(), "main");
        assert!(!results.is_empty(), "应搜索到 main 相关文件");
        // src/main.rs 和 main.rs 都应出现
        let paths: Vec<&str> = results.iter().map(|r| r.path.as_str()).collect();
        assert!(paths.iter().any(|p| p.contains("main.rs")));
    }

    #[test]
    fn test_search_empty_query() {
        let results = search_files("/tmp", "");
        assert!(results.is_empty(), "空查询应返回空结果");
    }

    #[test]
    fn test_search_ignores_target() {
        let dir = tempdir().unwrap();
        let base = dir.path();
        fs::create_dir_all(base.join("target")).unwrap();
        fs::write(base.join("target/secret.rs"), "").unwrap();
        fs::write(base.join("visible.rs"), "").unwrap();

        let results = search_files(&base.to_string_lossy(), "visible");
        assert!(results.iter().all(|r| !r.path.contains("target")));
    }

    #[test]
    fn test_find_common_prefix_basic() {
        let candidates = vec![
            FileCandidate {
                path: "src/main.rs".into(),
                display: "src/main.rs".into(),
                is_dir: false,
                score: 0,
            },
            FileCandidate {
                path: "src/lib.rs".into(),
                display: "src/lib.rs".into(),
                is_dir: false,
                score: 0,
            },
        ];
        assert_eq!(find_common_prefix(&candidates), Some("src/".to_string()));
    }

    #[test]
    fn test_find_common_prefix_empty() {
        assert_eq!(find_common_prefix(&[]), None);
    }
}
