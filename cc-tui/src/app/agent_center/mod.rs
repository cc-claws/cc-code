//! 跨项目根会话中心。列表分页、轻量索引计数；消息仅在选中后延迟读取。
mod input;
pub(crate) mod render;
mod state;

pub(crate) use state::{AgentCenter, Bucket, CenterRow};

use std::time::{Duration, Instant};

use tokio::sync::oneshot;

use super::{subagent_session::SubagentRunStatus, App};
use crate::thread::ThreadMeta;

const PAGE_SIZE: usize = 10;
type StoreResult<T> = Result<T, String>;

pub(crate) struct CenterPage {
    catalog: Vec<ThreadMeta>,
    threads: Vec<ThreadMeta>,
    offset: usize,
}

impl App {
    pub fn open_agent_center(&mut self) {
        if self.is_interaction_popup_active() || self.global_ui.setup_wizard.is_some() {
            return;
        }
        self.global_ui.agent_center = Some(AgentCenter::new());
        self.load_agent_center_page();
    }

    fn load_agent_center_page(&mut self) {
        let Some(center) = &mut self.global_ui.agent_center else {
            return;
        };
        if center.page_rx.is_some() {
            return;
        }
        let offset = center.offset;
        let store = self.services.thread_store.clone();
        let (tx, rx) = oneshot::channel();
        center.page_rx = Some(rx);
        center.error = None;
        tokio::spawn(async move {
            let result = async {
                let catalog = store.list_threads_metadata().await?;
                let threads = store.list_root_threads_paged(PAGE_SIZE, offset).await?;
                Ok::<_, anyhow::Error>(CenterPage {
                    catalog,
                    threads,
                    offset,
                })
            }
            .await
            .map_err(|e| e.to_string());
            let _ = tx.send(result);
        });
    }

    /// 主循环轮询；I/O 不在渲染线程运行。接收器随页面丢弃，旧结果不会污染新页面。
    pub fn poll_agent_center(&mut self) -> bool {
        let mut changed = false;
        let mut schedule_count = false;
        if self
            .global_ui
            .agent_count_checked
            .is_none_or(|time| time.elapsed() >= Duration::from_secs(5))
            && self.global_ui.agent_count_rx.is_none()
        {
            schedule_count = true;
            self.global_ui.agent_count_checked = Some(Instant::now());
        }
        if schedule_count {
            let store = self.services.thread_store.clone();
            let (tx, rx) = oneshot::channel();
            self.global_ui.agent_count_rx = Some(rx);
            tokio::spawn(async move {
                let _ = tx.send(
                    store
                        .list_threads_metadata()
                        .await
                        .map_err(|e| e.to_string()),
                );
            });
        }
        let mut refreshed_catalog = None;
        if let Some(result) = receive(&mut self.global_ui.agent_count_rx) {
            match result {
                Ok(catalog) => {
                    self.global_ui.agent_catalog = catalog;
                    refreshed_catalog = Some(self.global_ui.agent_catalog.clone());
                    changed = true;
                }
                Err(error) => tracing::warn!(%error, "会话索引读取失败"),
            }
        }
        let Some(mut center) = self.global_ui.agent_center.take() else {
            return changed;
        };
        if let Some(catalog) = refreshed_catalog {
            let selected = center
                .selected_meta(self)
                .map(|meta| (meta.id.clone(), meta.updated_at));
            center.refresh_catalog(catalog);
            if let Some((id, updated_at)) = selected {
                if let Some(index) = center.rows(self).iter().position(|row| matches!(row, CenterRow::Thread(index) if center.threads[*index].id == id)) {
                    center.selected = index;
                }
                if !center
                    .threads
                    .iter()
                    .any(|meta| meta.id == id && meta.updated_at == updated_at)
                {
                    center.reset_detail();
                }
            }
        }
        if let Some(result) = receive(&mut center.page_rx) {
            match result {
                Ok(page) => {
                    center.refresh_catalog(page.catalog);
                    self.global_ui.agent_catalog = center.catalog.clone();
                    center.offset = page.offset + page.threads.len();
                    for meta in page.threads {
                        if !center.threads.iter().any(|old| old.id == meta.id) {
                            center.threads.push(meta);
                        }
                    }
                    center.reset_detail();
                }
                Err(error) => center.error = Some(error),
            }
            changed = true;
        }
        if let Some(result) = receive(&mut center.mutation_rx) {
            match result {
                Ok((id, title)) => {
                    if let Some(title) = title {
                        for meta in center.threads.iter_mut().chain(center.catalog.iter_mut()) {
                            if meta.id == id {
                                meta.title = Some(title.clone());
                            }
                        }
                        if self.session_mgr.current().current_thread_id.as_deref() == Some(&id) {
                            self.session_mgr.current_mut().metadata.thread_title = Some(title);
                            self.session_mgr.current_mut().metadata.user_renamed = true;
                            self.refresh_terminal_title();
                        }
                    } else {
                        center.threads.retain(|meta| meta.id != id);
                        center.catalog.retain(|meta| meta.id != id);
                        center.offset = center.offset.saturating_sub(1);
                        self.global_ui.agent_catalog.retain(|meta| meta.id != id);
                    }
                    center.reset_detail();
                }
                Err(error) => center.error = Some(error),
            }
            changed = true;
        }
        if let Some(result) = receive(&mut center.detail_rx) {
            match result {
                Ok((id, messages)) if center.detail_id.as_deref() == Some(&id) => {
                    center.detail = Some(state::Detail::from_messages(&messages));
                }
                Ok(_) => {}
                Err(error) => {
                    center.detail = Some(state::Detail {
                        prompt: String::new(),
                        last: error,
                    })
                }
            }
            changed = true;
        }
        let rows = center.rows(self);
        center.selected = center.selected.min(rows.len().saturating_sub(1));
        let selected_id = rows.get(center.selected).and_then(|row| match row {
            CenterRow::Thread(index) => Some(center.threads[*index].id.clone()),
            CenterRow::More => None,
        });
        if selected_id != center.detail_id {
            center.detail_id = selected_id;
            center.detail = None;
            center.detail_rx = None;
            center.detail_due = Instant::now() + Duration::from_millis(150);
            changed = true;
        }
        if center.detail.is_none()
            && center.detail_rx.is_none()
            && Instant::now() >= center.detail_due
        {
            if let Some(id) = center.detail_id.clone() {
                let store = self.services.thread_store.clone();
                let (tx, rx) = oneshot::channel();
                center.detail_rx = Some(rx);
                tokio::spawn(async move {
                    let result = store
                        .load_messages(&id)
                        .await
                        .map(|messages| (id, messages))
                        .map_err(|e| e.to_string());
                    let _ = tx.send(result);
                });
            }
        }
        // 中心打开时继续消费执行事件；状态桶由内存态每帧推导。
        let counts = center.counts(self);
        if counts != center.last_counts {
            center.last_counts = counts;
            changed = true;
        }
        self.global_ui.agent_center = Some(center);
        changed
    }

    pub(crate) fn agent_center_bucket(&self, meta: &ThreadMeta) -> Bucket {
        let session = self.session_mgr.current();
        if session.current_thread_id.as_deref() == Some(&meta.id) {
            if session.agent.interaction_prompt.is_some()
                || !session.pending_thread_approvals.is_empty()
                || session
                    .subagent_sessions
                    .values()
                    .any(|sub| sub.status == SubagentRunStatus::AwaitingApproval)
            {
                return Bucket::NeedsYou;
            }
            if session.ui.loading
                || session
                    .subagent_sessions
                    .values()
                    .any(|sub| sub.status == SubagentRunStatus::Running)
            {
                return Bucket::Working;
            }
        }
        Bucket::from_meta(meta, chrono::Utc::now())
    }

    pub(crate) fn agent_center_navigation_available(&self) -> bool {
        let session = self.session_mgr.current();
        self.focused
            && self.global_ui.agent_center.is_none()
            && self.global_ui.setup_wizard.is_none()
            && self.global_ui.oauth_prompt.is_none()
            && !self.is_interaction_popup_active()
            && !self.global_panels.is_any_open()
            && !session.session_panels.is_any_open()
            && session.ui.textarea.lines().iter().all(String::is_empty)
            && session.metadata.pending_attachments.is_empty()
            && session.ui.pasted_text_blocks.is_empty()
            && !session.ui.at_mention.active
            && session.ui.hint_cursor.is_none()
            && session.focused_instance_id.is_none()
            && session.subagent_control_prefix.is_none()
            && !session.ui.background_tasks_bar_focused
            && session.ui.bg_bar_cursor.is_none()
    }

    pub(crate) fn current_project_agent_count(&self) -> usize {
        self.global_ui
            .agent_catalog
            .iter()
            .filter(|meta| meta.is_root() && meta.cwd == self.services.cwd)
            .count()
    }
}

fn receive<T>(receiver: &mut Option<oneshot::Receiver<StoreResult<T>>>) -> Option<StoreResult<T>> {
    match receiver.as_mut()?.try_recv() {
        Ok(result) => {
            *receiver = None;
            Some(result)
        }
        Err(oneshot::error::TryRecvError::Empty) => None,
        Err(oneshot::error::TryRecvError::Closed) => {
            *receiver = None;
            Some(Err("会话读取任务已结束，请重新打开中心".to_string()))
        }
    }
}

#[cfg(test)]
#[path = "center_test.rs"]
mod tests;
