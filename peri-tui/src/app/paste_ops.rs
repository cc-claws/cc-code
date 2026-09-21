use super::{App, PastedTextBlock, PendingAttachment};

impl App {
    pub(crate) fn paste_text_into_textarea(&mut self, text: &str) {
        let text = normalize_paste_text(text);

        // 单行粘贴：尝试识别为路径并归一化（file:// / UNC / Windows drive / shell 转义）
        // 多行文本直接走多行粘贴流程
        if paste_line_count(&text) <= 1 {
            let path = crate::clipboard::path_normalize::normalize_pasted_path(&text);
            // 仅聊天草稿自动转换附件，不能改写本机 shell 命令或其 stdin。
            let shell_input = self.is_shell_command_running()
                || self
                    .session_mgr
                    .current()
                    .ui
                    .textarea
                    .lines()
                    .first()
                    .is_some_and(|line| line.trim_start().starts_with('!'));
            if !shell_input {
                // 图片名常含括号等字符；仅对完整绝对路径按字面量读取，不执行 shell。
                // 不改变通用路径归一化对命令元字符的拒绝规则。
                let image_path = path.clone().or_else(|| {
                    let literal = text.trim();
                    let literal = literal
                        .strip_prefix('"')
                        .and_then(|value| value.strip_suffix('"'))
                        .or_else(|| {
                            literal
                                .strip_prefix('\'')
                                .and_then(|value| value.strip_suffix('\''))
                        })
                        .unwrap_or(literal);
                    let path = std::path::PathBuf::from(literal);
                    path.is_absolute().then_some(path)
                });
                if let Some(path) = image_path
                    .as_ref()
                    .filter(|path| crate::clipboard::image_file::is_image_path(path))
                {
                    let resolved = if path.is_absolute() {
                        path.clone()
                    } else {
                        std::path::Path::new(&self.services.cwd).join(path)
                    };
                    match crate::clipboard::image_file::image_file_as_png_base64(&resolved) {
                        Ok((base64, size, _, _)) => {
                            self.attach_pasted_image(base64, size);
                            return;
                        }
                        Err(error) => {
                            tracing::debug!(%error, "image path paste failed; keeping text")
                        }
                    }
                }
            }
            let normalized = path
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

    pub(crate) fn attach_pasted_image(&mut self, base64_data: String, size_bytes: usize) {
        let session = self.session_mgr.current_mut();
        let image_id = session.metadata.alloc_image_id();
        session
            .metadata
            .pending_attachments
            .push(PendingAttachment {
                label: format!("clipboard_{image_id}.png"),
                media_type: "image/png".into(),
                base64_data,
                size_bytes,
                image_id,
            });
        session
            .ui
            .textarea
            .insert_str(crate::clipboard::image_placeholder::format_placeholder(
                image_id,
            ));
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

#[cfg(test)]
#[path = "image_paste_test.rs"]
mod image_paste_test;
