pub mod file_search;
pub mod popup;

use file_search::FileCandidate;
use tokio_util::sync::CancellationToken;

/// @ 提及状态：管理���件搜索候选、选择和弹窗
pub struct AtMentionState {
    pub active: bool,
    pub query: String,
    /// @ 符号在文本中的字符位置
    pub query_start: usize,
    pub candidates: Vec<FileCandidate>,
    pub selected: usize,
    pub scroll_offset: usize,
    /// 防抖 oneshot sender：新搜索启动时 cancel 上一次
    pub debounce_tx: Option<tokio::sync::oneshot::Sender<()>>,
    /// 异步搜索取消令牌
    pub cancel_token: Option<CancellationToken>,
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
            debounce_tx: None,
            cancel_token: None,
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
        if let Some(tx) = self.debounce_tx.take() {
            let _ = tx.send(());
        }
        if let Some(token) = self.cancel_token.take() {
            token.cancel();
        }
    }

    /// 更新候选列表
    pub fn update_candidates(&mut self, candidates: Vec<FileCandidate>) {
        let len = candidates.len();
        self.candidates = candidates;
        if self.selected >= len && len > 0 {
            self.selected = len - 1;
        }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_at_sign_with_text() {
        // "请看 @main" → Some(("main", 3))
        let text = "请看 @main";
        // "请" = 3 bytes, "看" = 3 bytes, " " = 1 byte → @ 在 byte 7
        // 但我们用字符位置：cursor_pos = 6 (在 'n' 之后)
        // 先找出正确的 cursor_pos
        let result = AtMentionState::detect(text, text.len());
        assert!(result.is_some(), "应检测到 @ 提及");
        let (query, pos) = result.unwrap();
        assert_eq!(query, "main");
        // @ 在 char index 3
        assert_eq!(pos, "请看 ".len());
    }

    #[test]
    fn test_detect_no_at_sign() {
        let result = AtMentionState::detect("hello world", "hello world".len());
        assert!(result.is_none(), "无 @ 应返回 None");
    }

    #[test]
    fn test_detect_at_sign_only() {
        // "看 @" → @ 后无字符
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
        // "user@example" → @ 前不是空白
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
        state.move_down(); // 循环回 0
        assert_eq!(state.selected, 0);
        state.move_up(); // 循环到末尾
        assert_eq!(state.selected, 2);
    }
}
