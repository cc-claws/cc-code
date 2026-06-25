use super::{App, PastedTextBlock};

impl App {
    pub(crate) fn paste_text_into_textarea(&mut self, text: &str) {
        let text = normalize_paste_text(text);

        // 单行粘贴：尝试识别为路径并归一化（file:// / UNC / Windows drive / shell 转义）
        // 多行文本直接走多行粘贴流程
        if paste_line_count(&text) <= 1 {
            let normalized = crate::clipboard::path_normalize::normalize_pasted_path(&text)
                .map(|p| p.to_string_lossy().into_owned())
                .unwrap_or(text);
            self.session_mgr
                .current_mut()
                .ui
                .textarea
                .insert_str(&normalized);
            return;
        }

        let placeholder = {
            let ui = &mut self.session_mgr.current_mut().ui;
            let id = ui.next_pasted_text_id;
            ui.next_pasted_text_id += 1;
            format!("[Pasted text #{} +{} lines]", id, paste_line_count(&text))
        };
        let insertion = if needs_space_before_placeholder(self) {
            format!(" {}", placeholder)
        } else {
            placeholder.clone()
        };
        self.session_mgr
            .current_mut()
            .ui
            .textarea
            .insert_str(&insertion);
        self.session_mgr
            .current_mut()
            .ui
            .pasted_text_blocks
            .push(PastedTextBlock {
                placeholder,
                content: text,
            });
    }

    pub(crate) fn expand_pasted_text(&self, input: &str) -> String {
        self.session_mgr
            .current()
            .ui
            .pasted_text_blocks
            .iter()
            .fold(input.to_string(), |acc, block| {
                acc.replace(&block.placeholder, &block.content)
            })
    }

    pub(crate) fn input_contains_pasted_text_placeholder(&self, input: &str) -> bool {
        self.session_mgr
            .current()
            .ui
            .pasted_text_blocks
            .iter()
            .any(|block| input.contains(&block.placeholder))
    }

    pub(crate) fn clear_pasted_text_blocks(&mut self) {
        let ui = &mut self.session_mgr.current_mut().ui;
        ui.pasted_text_blocks.clear();
        ui.next_pasted_text_id = 1;
    }
}

fn normalize_paste_text(text: &str) -> String {
    text.replace('\r', "\n")
}

fn paste_line_count(text: &str) -> usize {
    text.lines().count().max(1)
}

fn needs_space_before_placeholder(app: &App) -> bool {
    let textarea = &app.session_mgr.current().ui.textarea;
    let (row, col) = textarea.cursor();
    let Some(line) = textarea.lines().get(row) else {
        return false;
    };
    line.chars()
        .take(col)
        .last()
        .is_some_and(|ch| !ch.is_whitespace())
}

#[cfg(test)]
#[path = "paste_ops_test.rs"]
mod paste_ops_test;
