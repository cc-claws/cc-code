# @ Mention 线程模型文件搜索 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 将 @ mention 文件搜索从 `spawn_blocking` 迁移为专用线程 + `walkdir` 遍历（复用 Glob 工具逻辑），线程闲置 1s 自动退出，修复搜索遗漏和内存暴涨。

**Architecture:** 用 `walkdir` + `should_skip_dir`（与 GlobFilesTool 对齐）替代裸 `glob::glob()`，一次性构建全量文件索引，在索引上做 fuzzy 匹配。搜索在专用 `std::thread` 中执行，通过 `std::sync::mpsc` 接收 query、通过 `tokio::sync::mpsc` 回传结果，线程用 `recv_timeout(1s)` 实现 idle 自动退出。`AtMentionState` 持有 `SearchThread` 管理线程生命周期。

**Tech Stack:** `walkdir`（已有于 peri-middlewares，新增于 peri-tui）、`std::thread` + `std::sync::mpsc`（线程通信）、`fuzzy-matcher`（已有）。无需新 crate、无子命令、无 IPC 协议。

---

## File Structure

| 操作 | 路径 | 职责 |
|------|------|------|
| Modify | `peri-tui/src/app/at_mention/file_search.rs` | 替换 `glob::glob()` 为 `walkdir` + `should_skip_dir`，一次性建索引 + fuzzy |
| Modify | `peri-tui/src/app/at_mention/mod.rs` | 替换 `spawn_blocking` 为 `SearchThread`（专用线程 + idle timeout） |
| Modify | `peri-tui/Cargo.toml` | 添加 `walkdir` 依赖，移除 `glob` 依赖 |
| No change | `peri-tui/src/app/at_mention/popup.rs` | 渲染层不变 |
| No change | `peri-tui/src/event/keyboard.rs` | `update_at_mention_detection` 接口不变 |
| No change | `peri-tui/src/app/agent_ops/polling.rs` | `poll_at_mention` 接口不变 |
| No change | `peri-tui/src/main.rs` | 无新子命令 |
| No change | `peri-tui/src/app/ui_state.rs` | `AtMentionState::new()` 不变 |

---

### Task 1: 用 walkdir 重写 file_search.rs

**Files:**
- Modify: `peri-tui/src/app/at_mention/file_search.rs`
- Modify: `peri-tui/Cargo.toml`

核心变更：用 `walkdir::WalkDir` + `should_skip_dir`（对齐 GlobFilesTool）替代裸 `glob::glob()`。`search_files` 改为先构建全量文件列表，再 fuzzy 匹配——不会被深度优先遍历截断，彻底解决 `side-projects` / `spec/issues` 搜不到的问题。

- [ ] **Step 1: 添加 `walkdir` 依赖，移除 `glob`**

在 `peri-tui/Cargo.toml` 中：
- 添加 `walkdir = "2.5"`
- 移除 `glob = "0.3"`

- [ ] **Step 2: 重写 file_search.rs**

完整替换 `peri-tui/src/app/at_mention/file_search.rs`：

```rust
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

const MAX_CANDIDATES: usize = 15;

/// 目录过滤列表——与 GlobFilesTool (peri-middlewares/src/tools/filesystem/glob.rs) 对齐
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
/// 使用 walkdir 遍历（与 GlobFilesTool 对齐），一次性构建全量文件列表，再 fuzzy 匹配。
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

    // 使用 walkdir 遍历，与 GlobFilesTool 相同的 skip 逻辑
    let walker = WalkDir::new(base)
        .follow_links(true)
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

/// 从已有候选列表中过滤匹配 query 的结果（纯内存操作，无 IO）
/// 用于 query 变长时从缓存过滤，避免重新遍历
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
}
```

- [ ] **Step 3: 运行测试**

Run: `cargo test -p peri-tui --lib -- at_mention::file_search 2>&1`
Expected: 4 个测试全部通过

- [ ] **Step 4: Commit**

```bash
git add peri-tui/src/app/at_mention/file_search.rs peri-tui/Cargo.toml
git commit -m "refactor(at-mention): replace glob with walkdir, align with GlobFilesTool skip dirs"
```

---

### Task 2: 实现 SearchThread（专用线程 + idle timeout）

**Files:**
- Modify: `peri-tui/src/app/at_mention/mod.rs`

核心变更：将 `tokio::spawn` + `spawn_blocking` + `CancellationToken` 替换为 `std::thread::spawn` + `std::sync::mpsc` + `recv_timeout(1s)`。线程持有文件索引（惰性构建，首次 query 时构建），idle 1s 后自动 return 退出。

- [ ] **Step 1: 重写 AtMentionState**

完整替换 `peri-tui/src/app/at_mention/mod.rs`：

```rust
pub mod file_search;
pub mod popup;

use std::collections::HashMap;
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use file_search::FileCandidate;

/// 搜索节流间隔（毫秒）
const SEARCH_DEBOUNCE_MS: u64 = 200;
/// 缓存上限：超过后清空重建
const CACHE_MAX_ENTRIES: usize = 64;
/// 搜索线程闲置超时
const IDLE_TIMEOUT: Duration = Duration::from_secs(1);

/// @ 提及状态：管理文件搜索候选、选择和弹窗
pub struct AtMentionState {
    pub active: bool,
    pub query: String,
    /// @ 符号在文本中的字符位置
    pub query_start: usize,
    pub candidates: Vec<FileCandidate>,
    pub selected: usize,
    pub scroll_offset: usize,
    /// 搜索线程的 query 发送端
    query_tx: Option<mpsc::Sender<String>>,
    /// 搜索线程 handle
    search_thread: Option<thread::JoinHandle<()>>,
    /// 搜索线程的结果接收端（与 AtMentionState 在同一线程，try_recv 非阻塞）
    result_rx: Option<mpsc::Receiver<(String, Vec<FileCandidate>)>>,
    /// 缓存：query → 搜索结果（避免重复遍历）
    search_cache: HashMap<String, Vec<FileCandidate>>,
    /// 上次搜索的 query 前缀（query 变长时从缓存过滤，无需重新搜索）
    last_search_query: String,
    /// 上次触发搜索的时间戳：节流用
    last_search_time: Option<Instant>,
    /// 工作目录（延迟设置，线程启动时使用）
    cwd: String,
}

impl Default for AtMentionState {
    fn default() -> Self {
        Self::new()
    }
}

impl AtMentionState {
    pub fn new() -> Self {
        Self {
            active: false,
            query: String::new(),
            query_start: 0,
            candidates: Vec::new(),
            selected: 0,
            scroll_offset: 0,
            query_tx: None,
            search_thread: None,
            result_rx: None,
            search_cache: HashMap::new(),
            last_search_query: String::new(),
            last_search_time: None,
            cwd: String::new(),
        }
    }

    /// 设置工作目录
    pub fn set_cwd(&mut self, cwd: String) {
        if self.cwd != cwd {
            self.kill_thread();
            self.cwd = cwd;
        }
    }

    /// 检测光标位置前是否有 @ 触发模式
    /// 返回 (查询字符串不含@, @的位置)
    pub fn detect(text: &str, cursor_pos: usize) -> Option<(String, usize)> {
        if cursor_pos == 0 || cursor_pos > text.len() {
            return None;
        }

        let before_cursor = &text[..cursor_pos];

        // 查找最后一个 @
        let at_pos = before_cursor.rfind('@')?;
        let query = &before_cursor[at_pos + '@'.len_utf8()..];

        // @ 后面至少要有 1 个字符
        if query.is_empty() {
            return None;
        }

        // 检查 @ 前面的字符：必须是行首或空白
        if at_pos > 0 {
            let char_before = before_cursor[..at_pos].chars().next_back().unwrap();
            if !char_before.is_whitespace() && char_before != '\n' {
                return None;
            }
        }

        Some((query.to_string(), at_pos))
    }

    /// 激活 @ 提及模式
    pub fn activate(&mut self, query: String, query_start: usize) {
        self.active = true;
        self.query = query;
        self.query_start = query_start;
        self.selected = 0;
        self.scroll_offset = 0;
    }

    /// 关闭 @ 提及模式
    pub fn close(&mut self) {
        self.active = false;
        self.query.clear();
        self.candidates.clear();
        self.selected = 0;
        self.scroll_offset = 0;
        // 不 kill 搜索线程：让它 idle timeout 自行退出
    }

    /// 更新候选列表
    pub fn update_candidates(&mut self, candidates: Vec<FileCandidate>) {
        let len = candidates.len();
        self.candidates = candidates;
        if self.selected >= len && len > 0 {
            self.selected = len - 1;
        }
    }

    /// 判断当前 query 是否可以从缓存获取结果。
    /// 返回 Some(candidates) 表示可以从缓存/过滤得到结果，None 表示需要搜索。
    pub fn try_filter_from_cache(&self, query: &str) -> Option<Vec<FileCandidate>> {
        // 精确缓存命中
        if let Some(cached) = self.search_cache.get(query) {
            return Some(cached.clone());
        }

        // query 是上次搜索 query 的延续（变长）：从上次结果中过滤
        if query.starts_with(&self.last_search_query) && !self.last_search_query.is_empty() {
            if let Some(base_results) = self.search_cache.get(&self.last_search_query) {
                let filtered = file_search::filter_candidates(base_results, query);
                return Some(filtered);
            }
        }

        None
    }

    /// 缓存搜索结果并记录时间戳
    pub fn cache_result(&mut self, query: &str, candidates: Vec<FileCandidate>) {
        if self.search_cache.len() >= CACHE_MAX_ENTRIES {
            self.search_cache.clear();
        }
        self.search_cache.insert(query.to_string(), candidates);
        self.last_search_time = Some(Instant::now());
    }

    /// 记录本次搜索对应的 query 前缀
    pub fn set_last_search_query(&mut self, query: &str) {
        self.last_search_query = query.to_string();
    }

    /// 判断是否应该执行搜索（节流）
    pub fn should_search_now(&self) -> bool {
        match self.last_search_time {
            Some(t) => t.elapsed().as_millis() as u64 >= SEARCH_DEBOUNCE_MS,
            None => true,
        }
    }

    /// 启动搜索：确保搜索线程存活，发送 query。
    /// 如果线程已退出（idle timeout），自动重新 spawn。
    pub fn start_search(&mut self, query: String) {
        self.ensure_thread_alive();
        if let Some(tx) = &self.query_tx {
            // 非阻塞发送：如果线程来不及消费就丢弃旧 query
            let _ = tx.send(query);
        }
    }

    /// 检查搜索结果，返回 true 表示有新结果需要更新 UI
    pub fn poll_search_result(&mut self) -> bool {
        let rx = match self.result_rx.as_ref() {
            Some(rx) => rx,
            None => return false,
        };

        let mut updated = false;
        loop {
            match rx.try_recv() {
                Ok((query, candidates)) => {
                    if self.active && self.query == query {
                        self.cache_result(&query, candidates.clone());
                        self.set_last_search_query(&query);
                        self.update_candidates(candidates);
                        updated = true;
                    } else if !self.active || !query.starts_with(&self.query) {
                        self.cache_result(&query, candidates);
                        self.set_last_search_query(&query);
                    }
                }
                Err(_) => break,
            }
        }
        updated
    }

    /// 确保搜索线程存活
    fn ensure_thread_alive(&mut self) {
        // 检查线程是否已退出
        let thread_dead = self.search_thread.as_ref().map_or(true, |h| h.is_finished());
        if thread_dead {
            self.spawn_search_thread();
        }
    }

    /// 启动搜索线程
    fn spawn_search_thread(&mut self) {
        self.kill_thread();

        let (query_tx, query_rx) = mpsc::channel::<String>();
        let (result_tx, result_rx) = mpsc::channel::<(String, Vec<FileCandidate>)>();
        let cwd = self.cwd.clone();

        let handle = thread::Builder::new()
            .name("at-mention-search".into())
            .stack_size(2 * 1024 * 1024) // 2MB stack（默认 8MB 太大）
            .spawn(move || {
                search_thread_main(cwd, query_rx, result_tx);
            })
            .expect("搜索线程启动失败");

        self.query_tx = Some(query_tx);
        self.result_rx = Some(result_rx);
        self.search_thread = Some(handle);
    }

    /// 终止搜索线程
    fn kill_thread(&mut self) {
        // drop query_tx 会触发线程退出（query_rx.recv 返回 Err）
        self.query_tx = None;
        if let Some(handle) = self.search_thread.take() {
            let _ = handle.join();
        }
        self.result_rx = None;
    }

    /// 上移选择
    pub fn move_up(&mut self) {
        if self.candidates.is_empty() {
            return;
        }
        if self.selected > 0 {
            self.selected -= 1;
        } else {
            self.selected = self.candidates.len() - 1;
        }
        self.adjust_scroll();
    }

    /// 下移选择
    pub fn move_down(&mut self) {
        if self.candidates.is_empty() {
            return;
        }
        if self.selected < self.candidates.len() - 1 {
            self.selected += 1;
        } else {
            self.selected = 0;
        }
        self.adjust_scroll();
    }

    /// 调整滚动偏移，确保选中项在视口内
    pub fn adjust_scroll(&mut self) {
        let viewport = popup::MAX_VIEWPORT.min(self.candidates.len());
        if viewport == 0 {
            self.scroll_offset = 0;
            return;
        }
        if self.selected < self.scroll_offset {
            self.scroll_offset = self.selected;
        } else if self.selected >= self.scroll_offset + viewport {
            self.scroll_offset = self.selected - viewport + 1;
        }
    }

    /// 获取当前选中的候选
    pub fn selected_candidate(&self) -> Option<&FileCandidate> {
        self.candidates.get(self.selected)
    }
}

impl Drop for AtMentionState {
    fn drop(&mut self) {
        self.kill_thread();
    }
}

/// 搜索线程主循环：
/// 1. 等待 query（recv_timeout 1s idle 退出）
/// 2. 调用 file_search::search_files
/// 3. 发送结果回主线程
fn search_thread_main(
    cwd: String,
    query_rx: mpsc::Receiver<String>,
    result_tx: mpsc::Sender<(String, Vec<FileCandidate>)>,
) {
    loop {
        // 等待 query，idle 1s 无消息则退出
        let query = match query_rx.recv_timeout(IDLE_TIMEOUT) {
            Ok(q) => q,
            Err(_) => return, // Timeout 或 Disconnected
        };

        // 排空队列，只处理最新 query
        let mut latest = query;
        while let Ok(q) = query_rx.try_recv() {
            latest = q;
        }

        // 执行搜索
        let candidates = file_search::search_files(&cwd, &latest);

        // 发送结果（如果主线程已关闭则退出）
        if result_tx.send((latest, candidates)).is_err() {
            return;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_at_sign_with_text() {
        let text = "请看 @main";
        let result = AtMentionState::detect(text, text.len());
        assert!(result.is_some(), "应检测到 @ 提及");
        let (query, pos) = result.unwrap();
        assert_eq!(query, "main");
        assert_eq!(pos, "请看 ".len());
    }

    #[test]
    fn test_detect_no_at_sign() {
        let result = AtMentionState::detect("hello world", "hello world".len());
        assert!(result.is_none(), "无 @ 应返回 None");
    }

    #[test]
    fn test_detect_at_sign_only() {
        let result = AtMentionState::detect("看 @", "看 @".len());
        assert!(result.is_none(), "@ 后无内容应返回 None");
    }

    #[test]
    fn test_detect_path_with_slash() {
        let text = "看 @src/main";
        let result = AtMentionState::detect(text, text.len());
        assert!(result.is_some());
        let (query, _) = result.unwrap();
        assert_eq!(query, "src/main");
    }

    #[test]
    fn test_detect_not_at_line_start() {
        let result = AtMentionState::detect("user@example", "user@example".len());
        assert!(result.is_none(), "非空白前导的 @ 不应触发");
    }

    #[test]
    fn test_move_up_down() {
        let mut state = AtMentionState::new();
        state.active = true;
        state.candidates = vec![
            FileCandidate { path: "a.rs".into(), display: "a.rs".into(), is_dir: false, score: 10 },
            FileCandidate { path: "b.rs".into(), display: "b.rs".into(), is_dir: false, score: 5 },
            FileCandidate { path: "c.rs".into(), display: "c.rs".into(), is_dir: false, score: 1 },
        ];
        assert_eq!(state.selected, 0);
        state.move_down();
        assert_eq!(state.selected, 1);
        state.move_down();
        assert_eq!(state.selected, 2);
        state.move_down(); // 循环回 0
        assert_eq!(state.selected, 0);
        state.move_up(); // 循环到末尾
        assert_eq!(state.selected, 2);
    }

    #[test]
    fn test_cache_hit() {
        let mut state = AtMentionState::new();
        let candidates = vec![FileCandidate {
            path: "main.rs".into(),
            display: "main.rs".into(),
            is_dir: false,
            score: 10,
        }];
        state.cache_result("main", candidates.clone());
        let cached = state.try_filter_from_cache("main");
        assert!(cached.is_some());
        assert_eq!(cached.unwrap().len(), 1);
    }

    #[test]
    fn test_should_search_now_first_time() {
        let state = AtMentionState::new();
        assert!(state.should_search_now(), "首次应允许搜索");
    }

    #[test]
    fn test_search_thread_idle_exit() {
        let mut state = AtMentionState::new();
        let dir = tempfile::tempdir().unwrap();
        state.set_cwd(dir.path().to_string_lossy().to_string());
        std::fs::write(dir.path().join("test.rs"), "").unwrap();

        state.start_search("test".to_string());
        // 线程应存活
        assert!(state.search_thread.as_ref().unwrap().is_finished() == false);

        // 等待 idle timeout
        std::thread::sleep(Duration::from_millis(1200));
        assert!(state.search_thread.as_ref().unwrap().is_finished(), "线程应在 1s idle 后退出");
    }
}
```

- [ ] **Step 2: 适配 keyboard.rs**

`peri-tui/src/event/keyboard.rs` 的 `update_at_mention_detection` 只需将 `at.start_async_search(cwd, query)` 替换为 `at.start_search(query)`：

```rust
// 原代码（第 200-202 行）：
        // 异步搜索：spawn 后台任务，不阻塞 UI 线程
        let cwd = app.services.cwd.clone();
        at.start_async_search(cwd, query);

// 替换为：
        // 启动搜索线程
        at.start_search(query);
```

同时需要确保 `at.set_cwd()` 在适当时机被调用。在 `update_at_mention_detection` 函数开头添加：

```rust
    // 确保 cwd 已设置
    if at.cwd.is_empty() {
        at.set_cwd(app.services.cwd.clone());
    }
```

注意：`cwd` 字段在 Task 2 的 `AtMentionState` 中是私有的，需要在 struct 中改为 `pub` 或提供一个 getter。实际上 `update_at_mention_detection` 不需要直接访问 `cwd`，可以用一个 `ensure_cwd` 方法：

在 `AtMentionState` 中添加：
```rust
    /// 确保 cwd 已设置
    pub fn ensure_cwd(&mut self, cwd: String) {
        if self.cwd.is_empty() {
            self.set_cwd(cwd);
        }
    }
```

在 `update_at_mention_detection` 中调用：
```rust
    at.ensure_cwd(app.services.cwd.clone());
```

- [ ] **Step 3: 移除 polling.rs 中的旧 poll 逻辑**

`peri-tui/src/app/agent_ops/polling.rs` 的 `poll_at_mention` 保持接口不变（`&mut self -> bool`），内部从 `at.poll_search_result()` 自动使用新实现，无需修改。

- [ ] **Step 4: 验证编译**

Run: `cargo build -p peri-tui 2>&1 | head -30`
Expected: 编译通过

- [ ] **Step 5: 运行测试**

Run: `cargo test -p peri-tui --lib -- at_mention 2>&1`
Expected: 所有测试通过

- [ ] **Step 6: Commit**

```bash
git add peri-tui/src/app/at_mention/mod.rs peri-tui/src/event/keyboard.rs
git commit -m "refactor(at-mention): replace spawn_blocking with dedicated thread + idle timeout"
```

---

### Task 3: 端到端验证

- [ ] **Step 1: 运行全量 at_mention 测试**

Run: `cargo test -p peri-tui --lib -- at_mention 2>&1`
Expected: 所有测试通过（包括新增的 idle timeout 测试）

- [ ] **Step 2: 手动验证**

1. `cargo run -p peri-tui` 启动 TUI
2. 输入 `@main` → 候选列表应出现 `main.rs` 等
3. 输入 `@issue` → 应搜到 `spec/issues/` 下的文件
4. 输入 `@side` → 应搜到 `side-projects/` 目录
5. 连续快速输入 3-4 个字符 → 无卡顿
6. 停止输入 1s+ → 搜索线程应已退出（观察内存不再增长）

- [ ] **Step 3: 更新 issue**

将 `spec/issues/2026-05-31-at-mention-blocking-glob-search.md` 状态改为 `Fixed`。

---

## Self-Review

### 1. Spec Coverage
- ✅ 线程模型：Task 2 `std::thread::spawn`
- ✅ 闲置 1s 自动退出：`recv_timeout(IDLE_TIMEOUT)`
- ✅ Debounce 200ms：`should_search_now()`
- ✅ 内存可控：`walkdir` + `should_skip_dir` 跳过 node_modules/target，线程退出后 drop
- ✅ 搜不到 issues/side-projects：`walkdir` 替代 `glob::glob()`，不再被深度优先截断
- ✅ 复用 Glob 工具逻辑：`should_skip_dir` 对齐 `GlobFilesTool`

### 2. Placeholder Scan
- 无 TBD/TODO/占位符

### 3. Type Consistency
- `FileCandidate` 结构体不变，`filter_candidates` 签名不变
- `AtMentionState` 公共接口（`detect`/`activate`/`close`/`update_candidates`/`move_up`/`move_down`/`selected_candidate`/`try_filter_from_cache`/`cache_result`/`should_search_now`/`poll_search_result`）保持不变
- 仅 `start_async_search(cwd, query)` → `start_search(query)`，`close()` 不再 cancel token
