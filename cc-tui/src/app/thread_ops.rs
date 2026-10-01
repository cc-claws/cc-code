use super::*;

impl App {
    pub fn scroll_up(&mut self) {
        let ui = &mut self.session_mgr.current_mut().ui;
        let max_scroll = ui.scrollbar_max_offset;
        let min_scroll = ui.scrollbar_min_offset.min(max_scroll);
        let current = if ui.scroll_follow {
            max_scroll
        } else {
            ui.scroll_offset.clamp(min_scroll, max_scroll)
        };
        ui.scroll_offset = current.saturating_sub(3).max(min_scroll);
        ui.scroll_follow = false;
    }

    pub fn scroll_down(&mut self) {
        let ui = &mut self.session_mgr.current_mut().ui;
        let max_scroll = ui.scrollbar_max_offset;
        let min_scroll = ui.scrollbar_min_offset.min(max_scroll);
        let current = if ui.scroll_follow {
            max_scroll
        } else {
            ui.scroll_offset.clamp(min_scroll, max_scroll)
        };
        let next = current.saturating_add(3).min(max_scroll);
        ui.scroll_offset = next;
        ui.scroll_follow = next >= max_scroll;
    }

    /// 滚动到底部（恢复 follow 模式）
    pub fn scroll_to_bottom(&mut self) {
        self.session_mgr.current_mut().ui.scroll_offset = usize::MAX;
        self.session_mgr.current_mut().ui.scroll_follow = true;
    }

    /// 滚动到顶部
    pub fn scroll_to_top(&mut self) {
        let ui = &mut self.session_mgr.current_mut().ui;
        ui.scroll_offset = ui.scrollbar_min_offset.min(ui.scrollbar_max_offset);
        ui.scroll_follow = false;
    }

    /// 展开/折叠所有工具调用消息
    pub fn toggle_collapsed_messages(&mut self) {
        self.session_mgr.current_mut().ui.show_tool_messages =
            !self.session_mgr.current_mut().ui.show_tool_messages;
        let show_tool_messages = self.session_mgr.current().ui.show_tool_messages;
        let _ = self
            .session_mgr
            .current_mut()
            .messages
            .render_tx
            .try_send(RenderEvent::ToggleToolMessages(show_tool_messages));
    }

    pub fn toggle_detail_mode(&mut self) {
        let new_visible = !self.session_mgr.current_mut().ui.detail_mode;
        self.session_mgr.current_mut().ui.detail_mode = new_visible;

        // ToggleDetail 会清空 hash 缓存并触发全量重渲染
        let _ = self
            .session_mgr
            .current()
            .messages
            .render_tx
            .try_send(RenderEvent::ToggleDetail(new_visible));
    }

    /// 切换 Write/Edit 工具结果内联 diff 的显隐
    pub fn toggle_diff(&mut self) {
        let new_visible = !self.session_mgr.current_mut().ui.diff_visible;
        self.session_mgr.current_mut().ui.diff_visible = new_visible;

        // ToggleDiff 会清空 hash 缓存并触发全量重渲染
        let _ = self
            .session_mgr
            .current()
            .messages
            .render_tx
            .try_send(RenderEvent::ToggleDiff(new_visible));
    }

    /// 添加一个图片附件到待发送列表
    pub fn add_pending_attachment(&mut self, att: PendingAttachment) {
        self.session_mgr
            .current_mut()
            .metadata
            .pending_attachments
            .push(att);
    }

    /// 删除最后一个图片附件
    pub fn pop_pending_attachment(&mut self) {
        self.session_mgr
            .current_mut()
            .metadata
            .pending_attachments
            .pop();
    }

    // ─── Thread 操作 ──────────────────────────────────────────────────────────

    /// 将 `latest_recap` 窄更新写回 thread 元数据（用于 `-c`/`-r` 恢复）。
    ///
    /// 无 current_thread_id 时静默跳过（首轮发送前 thread 尚未创建）。
    pub(crate) fn persist_latest_recap(&self, recap: Option<String>) {
        let Some(tid) = self.session_mgr.current().current_thread_id.clone() else {
            return;
        };
        let store = self.services.thread_store.clone();
        tokio::spawn(async move {
            if let Err(e) = store.update_latest_recap(&tid, recap).await {
                tracing::warn!(error = %e, thread_id = %tid, "persist latest_recap 失败");
            }
        });
    }

    /// 将完成态总结行窄更新写回 thread 元数据（用于 `-c`/`-r` 恢复）。
    ///
    /// 从 `spinner_state` 读取当前值；无记录时写入 `None`（清除旧值）。
    pub(crate) fn persist_last_task_summary(&self) {
        let Some(tid) = self.session_mgr.current().current_thread_id.clone() else {
            return;
        };
        let spinner = &self.session_mgr.current().spinner_state;
        let summary = if spinner.last_summary_elapsed_ms() > 0 {
            spinner
                .last_summary_done_at()
                .map(|done_at| cc_agent::thread::TaskSummary {
                    verb: spinner.last_summary_verb().to_string(),
                    elapsed_ms: spinner.last_summary_elapsed_ms(),
                    done_at: done_at.into(),
                })
        } else {
            None
        };
        let store = self.services.thread_store.clone();
        tokio::spawn(async move {
            if let Err(e) = store.update_last_task_summary(&tid, summary).await {
                tracing::warn!(error = %e, thread_id = %tid, "persist last_task_summary 失败");
            }
        });
    }

    /// 重置 AgentComm 会话状态（token tracker、重试、subagent 等）
    /// 在 open_thread / new_thread 时调用，确保切换 thread 后上下文干净
    fn reset_agent_session(&mut self) {
        self.session_mgr
            .current_mut()
            .agent
            .session_token_tracker
            .reset();
        self.session_mgr.current_mut().agent.retry_status = None;
        self.session_mgr.current_mut().agent.subagent_depth = 0;
        self.session_mgr.current_mut().agent.task_start_time = None;
        self.session_mgr.current_mut().agent.last_task_duration = None;
        self.session_mgr.current_mut().agent.agent_id = None;
        self.session_mgr.current_mut().agent.interaction_prompt = None;
        self.session_mgr.current_mut().agent.pending_hitl_items = None;
        self.session_mgr.current_mut().agent.pending_ask_user = None;
        self.session_mgr.current_mut().agent.cancel_token = None;
        self.session_mgr.current_mut().agent.active_tool = None;
        self.session_mgr.current_mut().agent.running_tools.clear();
        self.session_mgr
            .current_mut()
            .agent
            .session_tool_stats
            .clear();
        self.session_mgr.current_mut().messages.last_submitted_text = None;
        self.session_mgr.current_mut().spinner_state.reset();
    }

    /// 恢复历史 thread：加载消息，关闭 browser
    pub fn open_thread(&mut self, thread_id: ThreadId) {
        let store = self.services.thread_store.clone();
        let tid = thread_id.clone();
        let base_msgs = tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current()
                .block_on(store.load_context(&tid))
                .unwrap_or_default()
        });
        self.session_mgr
            .current_mut()
            .messages
            .view_messages
            .clear();
        self.session_mgr
            .current_mut()
            .messages
            .ephemeral_notes
            .clear();
        self.session_mgr.current_mut().agent.origin_messages = base_msgs.clone();
        self.session_mgr.current_mut().ui.scrollbar_min_offset = 0;

        // 使用统一管线转换：与流式路径共享同一个 messages_to_view_models()
        let mut view_msgs = message_pipeline::MessagePipeline::messages_to_view_models(
            &base_msgs,
            &self.services.cwd,
        );
        // 历史恢复时聚合连续的已完成 SubAgentGroup 为批次汇总
        message_pipeline::aggregate_batch_groups(&mut view_msgs);

        // 合并 shell 命令记录到 view messages
        let shell_store = self.services.shell_command_store.clone();
        let shell_tid = thread_id.to_string();
        let shell_records = tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current()
                .block_on(shell_store.load_for_thread(&shell_tid))
                .unwrap_or_default()
        });
        let view_msgs = self.merge_shell_records_into_view(view_msgs, &base_msgs, shell_records);
        self.session_mgr.current_mut().messages.view_messages = view_msgs;

        // 同步 Pipeline 内部状态，确保后续流式事件能正确续接
        self.session_mgr.current_mut().messages.pipeline.clear();
        self.session_mgr
            .current_mut()
            .messages
            .pipeline
            .restore_completed(base_msgs.clone());

        let thread_id_str = thread_id.to_string();
        self.session_mgr.current_mut().current_thread_id = Some(thread_id);
        // 同步 ACP 服务器端 session 状态：确保 state.history 包含当前 thread 的消息，
        // 这样 /compact 命令和后续 prompt 能正确读到完整历史
        if let Some(ref acp_client) = self.acp_client {
            let client = acp_client.clone();
            let cwd = self.services.cwd.clone();
            let model = self.services.peri_config.as_ref().map(|c| {
                cc_acp::provider::format_model_selection_value(
                    &c.config.active_provider_id,
                    &c.config.active_alias,
                )
            });
            tokio::task::block_in_place(|| {
                tokio::runtime::Handle::current().block_on(async {
                    match client.load_session(&thread_id_str, &cwd, model.as_deref()).await {
                        Ok(sid) => tracing::info!(session_id = %sid, "open_thread: ACP session synced"),
                        Err(e) => tracing::warn!(error = %e, "open_thread: ACP session sync failed (compact may not work until first prompt)"),
                    }
                })
            });
        }
        self.session_mgr
            .current_mut()
            .session_panels
            .close_if(PanelKind::ThreadBrowser);
        self.session_mgr
            .current_mut()
            .metadata
            .pending_attachments
            .clear();
        self.clear_pasted_text_blocks();
        self.session_mgr.current_mut().langfuse.langfuse_session = None;
        self.session_mgr.current_mut().todo_items.clear();

        self.reset_agent_session();
        // 恢复会话主题短标题 + 持久化的提示行（recap / 完成态总结），并刷新终端标题
        let thread_meta = tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current()
                .block_on(store.load_meta(&tid))
                .ok()
        });
        // recap 行与完成态总结行是纯展示态、不进 message history，需从 meta 回填。
        // 无条件覆盖（Some/None 都写），顺带修复切换 thread 时 recap 残留串台问题。
        self.session_mgr.current_mut().latest_recap =
            thread_meta.as_ref().and_then(|m| m.latest_recap.clone());
        if let Some(summary) = thread_meta
            .as_ref()
            .and_then(|m| m.last_task_summary.as_ref())
        {
            self.session_mgr
                .current_mut()
                .spinner_state
                .restore_summary(
                    summary.verb.clone(),
                    summary.elapsed_ms,
                    summary.done_at.into(),
                );
        }
        self.session_mgr.current_mut().metadata.thread_title = thread_meta.and_then(|m| m.title);
        if !base_msgs.is_empty() {
            self.session_mgr
                .current_mut()
                .metadata
                .title_generation_attempted = true;
        }
        self.refresh_terminal_title();

        // 回收释放的内存给 OS
        crate::alloc_config::alloc_collect();

        // 恢复 sticky header：找到 thread 中最后一条 Human 消息
        self.session_mgr.current_mut().metadata.last_human_message = base_msgs
            .iter()
            .filter_map(|m| {
                if let BaseMessage::Human { content, .. } = m {
                    let text = content.text_content();
                    if text.trim().is_empty() {
                        None
                    } else {
                        Some(text)
                    }
                } else {
                    None
                }
            })
            .next_back();

        // 通知渲染线程加载历史消息
        let vms = self.session_mgr.current().messages.view_messages.clone();
        let _ = self
            .session_mgr
            .current_mut()
            .messages
            .render_tx
            .try_send(RenderEvent::Rebuild(vms));
    }

    pub fn open_thread_with_feedback(&mut self, thread_id: ThreadId) {
        self.open_thread(thread_id);
    }

    /// 新建 thread：清空消息，关闭 browser（thread id 在首次发送时创建）
    pub fn new_thread(&mut self) {
        // Fire SessionEnd hooks before clearing session state
        {
            let mut hooks = self
                .services
                .plugin_data
                .as_ref()
                .map(|pd| pd.all_hooks.clone())
                .unwrap_or_default();
            hooks.extend(cc_middlewares::hooks::loader::load_global_settings_hooks());
            hooks.extend(cc_middlewares::hooks::loader::load_settings_local_hooks(
                &self.services.cwd,
            ));
            if !hooks.is_empty() {
                let cwd = self.services.cwd.clone();
                let provider_name = self.services.provider_name.clone();
                tokio::spawn(async move {
                    cc_middlewares::hooks::middleware::fire_standalone_lifecycle_hooks(
                        &hooks,
                        cc_middlewares::hooks::types::HookEvent::SessionEnd,
                        &cwd,
                        "",
                        "",
                        &provider_name,
                        None,
                        // /clear 新建 thread：对齐 Claude Code SessionEnd reason="clear"
                        Some("clear"),
                    )
                    .await;
                });
            }
        }

        self.session_mgr
            .current_mut()
            .messages
            .view_messages
            .clear();
        self.session_mgr
            .current_mut()
            .messages
            .view_messages
            .shrink_to_fit();
        self.session_mgr
            .current_mut()
            .messages
            .ephemeral_notes
            .clear();
        self.session_mgr.current_mut().agent.origin_messages.clear();
        self.session_mgr.current_mut().ui.scrollbar_min_offset = 0;
        self.session_mgr
            .current_mut()
            .agent
            .origin_messages
            .shrink_to_fit();
        self.session_mgr.current_mut().messages.pipeline.clear();
        self.session_mgr
            .current_mut()
            .messages
            .pipeline
            .shrink_to_fit();
        self.session_mgr.current_mut().current_thread_id = None;
        self.session_mgr.current_mut().todo_items.clear();
        self.session_mgr
            .current_mut()
            .metadata
            .pending_attachments
            .clear();
        self.clear_pasted_text_blocks();
        self.session_mgr
            .current_mut()
            .session_panels
            .close_if(PanelKind::ThreadBrowser);
        self.session_mgr.current_mut().langfuse.langfuse_session = None;
        self.session_mgr.current_mut().metadata.last_human_message = None;
        self.session_mgr.current_mut().messages.last_submitted_text = None;
        self.session_mgr.current_mut().metadata.pre_submit_state_len = 0;
        // 清空上一会话的 recap 展示态（新一轮 /clear 后不应残留旧回顾行）
        self.session_mgr.current_mut().latest_recap = None;

        self.reset_agent_session();
        let meta = &mut self.session_mgr.current_mut().metadata;
        meta.thread_title = None;
        meta.user_renamed = false;
        meta.title_generation_attempted = false;
        self.refresh_terminal_title();

        // 通过 ACP 协议创建新 session，清空 server 端 history
        if let Some(ref acp_client) = self.acp_client {
            let client = acp_client.clone();
            let cwd = self.services.cwd.clone();
            let model = self.services.peri_config.as_ref().map(|c| {
                cc_acp::provider::format_model_selection_value(
                    &c.config.active_provider_id,
                    &c.config.active_alias,
                )
            });
            tokio::task::block_in_place(|| {
                tokio::runtime::Handle::current().block_on(async {
                    match client.new_session(&cwd, model.as_deref()).await {
                        Ok(sid) => tracing::info!(session_id = %sid, "new_thread: ACP new_session succeeded"),
                        Err(e) => tracing::warn!(error = %e, "new_thread: ACP new_session failed"),
                    }
                })
            });
        }
        // 回收释放的内存给 OS
        crate::alloc_config::alloc_collect();

        let _ = self
            .session_mgr
            .current_mut()
            .messages
            .render_tx
            .try_send(RenderEvent::Clear);

        // 归还已释放内存页给 OS
        crate::alloc_config::alloc_collect();
    }

    /// 打开 thread 浏览面板（通过命令触发）
    pub fn open_thread_browser(&mut self) {
        let store = self.services.thread_store.clone();
        let cwd = self.services.cwd.clone();
        let threads = tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current()
                .block_on(store.list_threads())
                .unwrap_or_default()
        });
        let filtered: Vec<_> = threads.into_iter().filter(|t| t.cwd == cwd).collect();

        // 检测当前 cwd 的 git 分支
        let branch = std::process::Command::new("git")
            .args(["rev-parse", "--abbrev-ref", "HEAD"])
            .current_dir(&self.services.cwd)
            .output()
            .ok()
            .filter(|o| o.status.success())
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
            .filter(|s| !s.is_empty());

        let browser = ThreadBrowser::new(filtered, self.services.thread_store.clone(), branch);
        self.open_panel(PanelState::ThreadBrowser(browser));
    }
}

#[cfg(test)]
mod tests {
    use crate::thread::ThreadMeta;
    include!("thread_ops_test.rs");
}
