use super::*;
use std::fs;
use tempfile::tempdir;

#[test]
fn test_search_empty_query_limits_large_directory() {
    let dir = tempdir().unwrap();
    for index in (0..500).rev() {
        fs::write(dir.path().join(format!("file_{index:03}.rs")), "").unwrap();
    }
    let results = search_files(&dir.path().to_string_lossy(), "");
    assert_eq!(results.len(), MAX_CANDIDATES, "大目录候选仍限制在 15 项");
    assert_eq!(results[0].path, "file_000.rs");
    assert_eq!(results[MAX_CANDIDATES - 1].path, "file_014.rs");
    let matches = search_files(&dir.path().to_string_lossy(), "file_499");
    assert!(
        matches
            .iter()
            .any(|candidate| candidate.path == "file_499.rs"),
        "关键词仍能找到默认列表外的文件"
    );
}

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
    let paths: Vec<&str> = results.iter().map(|r| r.path.as_str()).collect();
    assert!(paths.iter().any(|p| p.contains("main.rs")));
}

#[test]
fn test_search_empty_query() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("readme.md"), "").unwrap();
    fs::create_dir_all(dir.path().join("src")).unwrap();
    fs::write(dir.path().join("src/nested.rs"), "").unwrap();
    fs::create_dir_all(dir.path().join("target")).unwrap();
    let results = search_files(&dir.path().to_string_lossy(), "");
    let paths: Vec<_> = results
        .iter()
        .map(|candidate| candidate.path.as_str())
        .collect();
    assert_eq!(
        paths,
        vec!["src", "readme.md"],
        "只列当前目录，目录优先且排除构建目录"
    );
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
fn test_search_finds_directory() {
    let dir = tempdir().unwrap();
    let base = dir.path();
    fs::create_dir_all(base.join("src")).unwrap();
    fs::write(base.join("src/main.rs"), "").unwrap();

    let results = search_files(&base.to_string_lossy(), "src");
    assert!(
        results.iter().any(|r| r.path == "src" && r.is_dir),
        "应搜索到 src 目录"
    );
}
