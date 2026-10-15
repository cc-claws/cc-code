use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use tokio::sync::oneshot;

use super::{App, CenterRow};

impl App {
    /// 返回 true 表示按键已由全页中心消费，禁止继续传给编辑器。
    pub(crate) fn handle_agent_center_key(&mut self, key: KeyEvent) -> bool {
        let Some(mut center) = self.global_ui.agent_center.take() else {
            if key.kind == KeyEventKind::Press
                && key.code == KeyCode::Left
                && key.modifiers.is_empty()
                && self.agent_center_navigation_available()
            {
                self.open_agent_center();
                return true;
            }
            return false;
        };
        if key.kind == KeyEventKind::Release {
            self.global_ui.agent_center = Some(center);
            return true;
        }
        if key.code == KeyCode::Char('?') && center.rename.is_none() && !center.search {
            center.help = !center.help;
        } else if center.help {
            if key.code == KeyCode::Esc {
                center.help = false;
            }
        } else if center.mutation_rx.is_some() {
            // 保留接收器直至写入完成，确保错误与当前会话标题锁定不会丢失。
        } else if let Some(id) = center.confirm_delete.clone() {
            match key.code {
                KeyCode::Esc | KeyCode::Char('n') => center.confirm_delete = None,
                KeyCode::Char('y') if key.kind == KeyEventKind::Press => {
                    if !self.can_switch_thread_context() {
                        center.error = Some(self.services.lc.tr("agent-center-busy"));
                        center.confirm_delete = None;
                    } else {
                        let deleting_current =
                            self.session_mgr.current().current_thread_id.as_deref() == Some(&id);
                        if deleting_current && !self.new_thread_in_cwd(self.services.cwd.clone()) {
                            center.error = Some(self.services.lc.tr("agent-center-open-failed"));
                        } else {
                            let store = self.services.thread_store.clone();
                            let (tx, rx) = oneshot::channel();
                            center.mutation_rx = Some(rx);
                            tokio::spawn(async move {
                                let result = store
                                    .delete_thread_tree(&id)
                                    .await
                                    .map(|()| (id, None))
                                    .map_err(|e| e.to_string());
                                let _ = tx.send(result);
                            });
                        }
                        center.confirm_delete = None;
                    }
                }
                _ => {}
            }
        } else if let Some(mut title) = center.rename.take() {
            match key.code {
                KeyCode::Esc => {
                    center.rename_id = None;
                }
                KeyCode::Enter if !title.trim().is_empty() => {
                    if let Some(id) = center.rename_id.take() {
                        let title = title.trim().to_string();
                        self.global_ui
                            .manual_thread_titles
                            .write()
                            .insert(id.clone());
                        if self.session_mgr.current().current_thread_id.as_deref() == Some(&id) {
                            // 提交时立即锁定，阻止在途自动标题事件覆盖用户意图。
                            let metadata = &mut self.session_mgr.current_mut().metadata;
                            metadata.user_renamed = true;
                            metadata.title_generation_attempted = true;
                        }
                        let store = self.services.thread_store.clone();
                        let title_write_lock = self.global_ui.thread_title_write_lock.clone();
                        let (tx, rx) = oneshot::channel();
                        center.mutation_rx = Some(rx);
                        tokio::spawn(async move {
                            let _guard = title_write_lock.lock().await;
                            let result = store
                                .update_title(&id, &title)
                                .await
                                .map(|()| (id, Some(title)))
                                .map_err(|e| e.to_string());
                            let _ = tx.send(result);
                        });
                    }
                }
                KeyCode::Backspace => {
                    title.pop();
                    center.rename = Some(title);
                }
                KeyCode::Char(ch)
                    if !key
                        .modifiers
                        .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
                {
                    title.push(ch);
                    center.rename = Some(title);
                }
                _ => center.rename = Some(title),
            }
        } else if center.search
            && matches!(
                key.code,
                KeyCode::Char(_) | KeyCode::Backspace | KeyCode::Esc | KeyCode::Enter
            )
        {
            match key.code {
                KeyCode::Esc | KeyCode::Char('/') => {
                    center.search = false;
                    center.query.clear();
                }
                KeyCode::Enter => center.search = false,
                KeyCode::Backspace => {
                    center.query.pop();
                }
                KeyCode::Char(ch)
                    if !key
                        .modifiers
                        .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
                {
                    center.query.push(ch)
                }
                _ => {}
            }
            center.reset_selection();
        } else if key.modifiers.is_empty() || key.code == KeyCode::BackTab {
            match key.code {
                KeyCode::Esc => {
                    if !center.query.is_empty() {
                        center.query.clear();
                        center.reset_selection();
                    } else {
                        return true;
                    }
                }
                KeyCode::Up => center.selected = center.selected.saturating_sub(1),
                KeyCode::Down => {
                    center.selected =
                        (center.selected + 1).min(center.rows(self).len().saturating_sub(1))
                }
                KeyCode::Home => center.selected = 0,
                KeyCode::End => center.selected = center.rows(self).len().saturating_sub(1),
                KeyCode::Tab => {
                    center.filter = (center.filter + 1) % 5;
                    center.reset_selection();
                }
                KeyCode::BackTab => {
                    center.filter = (center.filter + 4) % 5;
                    center.reset_selection();
                }
                KeyCode::Char('g') => {
                    center.grouping = center.grouping.next();
                    center.reset_selection();
                }
                KeyCode::Char('/') => {
                    center.search = true;
                    center.query.clear();
                    center.reset_selection();
                }
                KeyCode::Char('o') => {
                    self.open_thread_browser();
                    return true;
                }
                KeyCode::Char('n') => {
                    let blocked = !self.can_switch_thread_context();
                    let cwd = center
                        .selected_meta(self)
                        .map(|meta| meta.cwd.clone())
                        .unwrap_or_else(|| self.services.cwd.clone());
                    if self.new_thread_in_cwd(cwd) {
                        return true;
                    }
                    center.error = Some(self.services.lc.tr(if blocked {
                        "agent-center-busy"
                    } else {
                        "agent-center-open-failed"
                    }));
                }
                KeyCode::Char('r') => {
                    if center.page_rx.is_none() {
                        if let Some(meta) = center.selected_meta(self) {
                            let id = meta.id.clone();
                            let title = meta.title.clone().unwrap_or_default();
                            center.rename = Some(title);
                            center.rename_id = Some(id);
                        }
                    }
                }
                KeyCode::Backspace | KeyCode::Delete => {
                    if center.page_rx.is_none() {
                        if let Some(meta) = center.selected_meta(self) {
                            center.confirm_delete = Some(meta.id.clone());
                        }
                    }
                }
                KeyCode::Enter => {
                    if matches!(
                        center.rows(self).get(center.selected),
                        Some(CenterRow::More)
                    ) {
                        self.global_ui.agent_center = Some(center);
                        self.load_agent_center_page();
                        return true;
                    }
                    if let Some(id) = center.selected_meta(self).map(|meta| meta.id.clone()) {
                        // 返回当前会话只关闭视图，不触发上下文切换或打断任务。
                        if self.session_mgr.current().current_thread_id.as_deref() == Some(&id) {
                            return true;
                        }
                        let blocked = !self.can_switch_thread_context();
                        if self.open_thread(id) {
                            return true;
                        }
                        center.error = Some(self.services.lc.tr(if blocked {
                            "agent-center-busy"
                        } else {
                            "agent-center-open-failed"
                        }));
                    }
                }
                _ => {}
            }
        } else if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
            return true;
        }
        self.global_ui.agent_center = Some(center);
        true
    }

    pub(crate) fn paste_agent_center_text(&mut self, text: &str) {
        if let Some(center) = &mut self.global_ui.agent_center {
            if center.help || center.confirm_delete.is_some() || center.mutation_rx.is_some() {
                return;
            }
            let single: String = text.chars().filter(|ch| !ch.is_control()).collect();
            if let Some(title) = &mut center.rename {
                title.push_str(&single);
            } else if center.search {
                center.query.push_str(&single);
                center.reset_selection();
            }
        }
    }
}
