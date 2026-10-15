pub mod file_search;
pub mod popup;

use std::collections::HashMap;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use file_search::FileCandidate;

/// 关键词搜索防抖间隔（毫秒）
const SEARCH_DEBOUNCE_MS: u64 = 200;
/// 缓存上限：超过后清空重建
const CACHE_MAX_ENTRIES: usize = 64;
/// 搜索线程闲置超时
const IDLE_TIMEOUT: Duration = Duration::from_secs(1);

/// @ 提及状态：管理文件搜索候选、选择和弹窗
pub struct AtMentionState {
    pub active: bool,
    pub searching: bool,
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
            searching: false,
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
            self.close();
            self.search_cache.clear();
            self.last_search_query.clear();
            self.cwd = cwd;
        }
    }

    pub fn detect(text: &str, cursor_pos: usize) -> Option<(String, usize)> {
        if cursor_pos == 0 || !text.is_char_boundary(cursor_pos) {
            return None;
        }
        let before_cursor = &text[..cursor_pos];
        let at_pos = before_cursor.rfind('@')?;
        let query = &before_cursor[at_pos + '@'.len_utf8()..];
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
        self.searching = true;
        self.candidates.clear();
        self.query = query;
        self.query_start = query_start;
        self.selected = 0;
        self.scroll_offset = 0;
    }

    pub fn close(&mut self) {
        self.active = false;
        self.searching = false;
        self.query.clear();
        self.candidates.clear();
        self.selected = 0;
        self.scroll_offset = 0;
    }

    pub fn update_candidates(&mut self, candidates: Vec<FileCandidate>) {
        self.searching = false;
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
        if query.starts_with(&self.last_search_query)
            && !self.last_search_query.is_empty()
            && query.rfind('/') == self.last_search_query.rfind('/')
        {
            if let Some(base_results) = self.search_cache.get(&self.last_search_query) {
                // 已截断的候选或新进入的目录不能代表完整搜索空间。
                if base_results.len() < file_search::MAX_CANDIDATES {
                    let filtered = file_search::filter_candidates(base_results, query);
                    return Some(filtered);
                }
            }
        }
        None
    }

    pub fn cache_result(&mut self, query: &str, candidates: Vec<FileCandidate>) {
        if self.search_cache.len() >= CACHE_MAX_ENTRIES {
            self.search_cache.clear();
        }
        self.search_cache.insert(query.to_string(), candidates);
    }

    pub fn set_last_search_query(&mut self, query: &str) {
        self.last_search_query = query.to_string();
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

        // 空查询立即列目录；关键词等输入稳定后搜索，始终保留最后一次输入。
        let mut latest = query;
        while !latest.is_empty() {
            match query_rx.recv_timeout(Duration::from_millis(SEARCH_DEBOUNCE_MS)) {
                Ok(query) => latest = query,
                Err(mpsc::RecvTimeoutError::Timeout) => break,
                Err(mpsc::RecvTimeoutError::Disconnected) => return,
            }
        }
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
#[path = "at_mention_test.rs"]
mod tests;
