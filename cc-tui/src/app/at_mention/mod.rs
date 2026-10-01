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
    /// 搜索线程的结果接收端
    result_rx: Option<mpsc::Receiver<(String, Vec<FileCandidate>)>>,
    /// 缓存：query → 搜索结果
    search_cache: HashMap<String, Vec<FileCandidate>>,
    /// 上次搜索的 query 前缀
    last_search_query: String,
    /// 上次触发搜索的时间戳
    last_search_time: Option<Instant>,
    /// 工作目录
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

    /// 确保 cwd 已设置（惰性初始化，仅设置一次）
    pub fn ensure_cwd(&mut self, cwd: String) {
        if self.cwd.is_empty() {
            self.set_cwd(cwd);
        }
    }

    /// 设置工作目录
    pub fn set_cwd(&mut self, cwd: String) {
        if self.cwd != cwd {
            self.kill_thread();
            self.cwd = cwd;
        }
    }

    pub fn detect(text: &str, cursor_pos: usize) -> Option<(String, usize)> {
        if cursor_pos == 0 || cursor_pos > text.len() {
            return None;
        }
        let before_cursor = &text[..cursor_pos];
        let at_pos = before_cursor.rfind('@')?;
        let query = &before_cursor[at_pos + '@'.len_utf8()..];
        if query.is_empty() {
            return None;
        }
        if at_pos > 0 {
            let char_before = before_cursor[..at_pos].chars().next_back().unwrap();
            if !char_before.is_whitespace() && char_before != '\n' {
                return None;
            }
        }
        Some((query.to_string(), at_pos))
    }

    pub fn activate(&mut self, query: String, query_start: usize) {
        self.active = true;
        self.query = query;
        self.query_start = query_start;
        self.selected = 0;
        self.scroll_offset = 0;
    }

    pub fn close(&mut self) {
        self.active = false;
        self.query.clear();
        self.candidates.clear();
        self.selected = 0;
        self.scroll_offset = 0;
    }

    pub fn update_candidates(&mut self, candidates: Vec<FileCandidate>) {
        let len = candidates.len();
        self.candidates = candidates;
        if self.selected >= len && len > 0 {
            self.selected = len - 1;
        }
    }

    pub fn try_filter_from_cache(&self, query: &str) -> Option<Vec<FileCandidate>> {
        if let Some(cached) = self.search_cache.get(query) {
            return Some(cached.clone());
        }
        if query.starts_with(&self.last_search_query) && !self.last_search_query.is_empty() {
            if let Some(base_results) = self.search_cache.get(&self.last_search_query) {
                let filtered = file_search::filter_candidates(base_results, query);
                return Some(filtered);
            }
        }
        None
    }

    pub fn cache_result(&mut self, query: &str, candidates: Vec<FileCandidate>) {
        if self.search_cache.len() >= CACHE_MAX_ENTRIES {
            self.search_cache.clear();
        }
        self.search_cache.insert(query.to_string(), candidates);
        self.last_search_time = Some(Instant::now());
    }

    pub fn set_last_search_query(&mut self, query: &str) {
        self.last_search_query = query.to_string();
    }

    pub fn should_search_now(&self) -> bool {
        match self.last_search_time {
            Some(t) => t.elapsed().as_millis() as u64 >= SEARCH_DEBOUNCE_MS,
            None => true,
        }
    }

    /// 启动搜索：确保搜索线程存活，发送 query
    pub fn start_search(&mut self, query: String) {
        self.ensure_thread_alive();
        if let Some(tx) = &self.query_tx {
            let _ = tx.send(query);
        }
    }

    /// 检查搜索结果，返回 true 表示有新结果需要更新 UI
    pub fn poll_search_result(&mut self) -> bool {
        // take + put-back 模式避免借用冲突
        let rx = match self.result_rx.take() {
            Some(rx) => rx,
            None => return false,
        };

        let mut updated = false;
        let mut disconnected = false;
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
                Err(std::sync::mpsc::TryRecvError::Empty) => break,
                Err(_) => {
                    disconnected = true;
                    break;
                }
            }
        }

        // 非 disconnected 时放回 rx
        if !disconnected {
            self.result_rx = Some(rx);
        }
        updated
    }

    fn ensure_thread_alive(&mut self) {
        let thread_dead = self.search_thread.as_ref().is_none_or(|h| h.is_finished());
        if thread_dead {
            self.spawn_search_thread();
        }
    }

    fn spawn_search_thread(&mut self) {
        self.kill_thread();

        let (query_tx, query_rx) = mpsc::channel::<String>();
        let (result_tx, result_rx) = mpsc::channel::<(String, Vec<FileCandidate>)>();
        let cwd = self.cwd.clone();

        let handle = thread::Builder::new()
            .name("at-mention-search".into())
            .stack_size(2 * 1024 * 1024)
            .spawn(move || {
                search_thread_main(cwd, query_rx, result_tx);
            })
            .expect("搜索线程启动失败");

        self.query_tx = Some(query_tx);
        self.result_rx = Some(result_rx);
        self.search_thread = Some(handle);
    }

    fn kill_thread(&mut self) {
        self.query_tx = None;
        if let Some(handle) = self.search_thread.take() {
            let _ = handle.join();
        }
        self.result_rx = None;
    }

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

    pub fn selected_candidate(&self) -> Option<&FileCandidate> {
        self.candidates.get(self.selected)
    }
}

impl Drop for AtMentionState {
    fn drop(&mut self) {
        self.kill_thread();
    }
}

/// 搜索线程主循环：recv_timeout 实现闲置退出，排空队列只处理最新 query
fn search_thread_main(
    cwd: String,
    query_rx: mpsc::Receiver<String>,
    result_tx: mpsc::Sender<(String, Vec<FileCandidate>)>,
) {
    loop {
        let query = match query_rx.recv_timeout(IDLE_TIMEOUT) {
            Ok(q) => q,
            Err(_) => return,
        };

        // 排空队列，只处理最新 query
        let mut latest = query;
        while let Ok(q) = query_rx.try_recv() {
            latest = q;
        }

        let candidates = file_search::search_files(&cwd, &latest);

        if result_tx.send((latest, candidates)).is_err() {
            return;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

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
            FileCandidate {
                path: "a.rs".into(),
                display: "a.rs".into(),
                is_dir: false,
                score: 10,
            },
            FileCandidate {
                path: "b.rs".into(),
                display: "b.rs".into(),
                is_dir: false,
                score: 5,
            },
            FileCandidate {
                path: "c.rs".into(),
                display: "c.rs".into(),
                is_dir: false,
                score: 1,
            },
        ];
        assert_eq!(state.selected, 0);
        state.move_down();
        assert_eq!(state.selected, 1);
        state.move_down();
        assert_eq!(state.selected, 2);
        state.move_down();
        assert_eq!(state.selected, 0);
        state.move_up();
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
    fn test_ensure_cwd_sets_once() {
        let mut state = AtMentionState::new();
        assert!(state.cwd.is_empty());
        state.ensure_cwd("/tmp".to_string());
        assert_eq!(state.cwd, "/tmp");
        state.ensure_cwd("/other".to_string());
        assert_eq!(state.cwd, "/tmp", "ensure_cwd 只设置一次");
    }

    #[test]
    fn test_search_thread_idle_exit() {
        let mut state = AtMentionState::new();
        let dir = tempfile::tempdir().unwrap();
        state.set_cwd(dir.path().to_string_lossy().to_string());
        std::fs::write(dir.path().join("test.rs"), "").unwrap();

        state.start_search("test".to_string());
        assert!(!state.search_thread.as_ref().unwrap().is_finished());

        std::thread::sleep(Duration::from_millis(1200));
        assert!(
            state.search_thread.as_ref().unwrap().is_finished(),
            "线程应在 1s idle 后退出"
        );
    }
}
