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
        self.session_mgr.current_mut().ui.text_selection.clear();
        self.session_mgr.current_mut().ui.screen_selection.clear();
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
        self.session_mgr.current_mut().ui.text_selection.clear();
        self.session_mgr.current_mut().ui.screen_selection.clear();
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
        self.session_mgr.current_mut().agent.clear_system_notice();
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
        self.session_mgr.current_mut().agent.recent_tools.clear();
        self.session_mgr
            .current_mut()
            .agent
            .session_tool_stats
            .clear();
        self.session_mgr.current_mut().messages.last_submitted_text = None;
        self.session_mgr.current_mut().spinner_state.reset();
    }

    /// 有运行任务或待处理交互时保持原会话，避免后台结果落入另一项目。
    pub fn can_switch_thread_context(&self) -> bool {
        let session = self.session_mgr.current();
        !session.ui.loading
            && session.agent.cancel_token.is_none()
            && session.agent.interaction_prompt.is_none()
            && session.agent.pending_hitl_items.is_none()
            && session.agent.pending_ask_user.is_none()
            && self.global_ui.oauth_prompt.is_none()
            && session.pending_thread_approvals.is_empty()
            && !session
                .subagent_sessions
                .values()
                .any(|agent| agent.status.is_active())
            && !session.shell_pool.is_running()
            && !session.agent_shells.iter().any(|shell| !shell.ended)
            && !self.has_running_background_shell_tasks()
            && session.agent.pending_bg_continuation.is_none()
            && !session.agent.agent_done_pending_bg
            && session.messages.pending_messages.is_empty()
    }

    fn validate_thread_context_switch(&mut self, cwd: &str) -> bool {
        if !self.can_switch_thread_context() {
            let note = self.services.lc.tr("thread-busy");
            self.push_system_note(note);
            return false;
        }
        if !std::path::Path::new(cwd).is_dir() {
            let note = self
                .services
                .lc
                .tr_args("thread-cwd-unavailable", &[("cwd".into(), cwd.into())]);
            self.push_system_note(note);
            return false;
        }
        if let Some(config) = self.services.peri_config.as_ref() {
            if let Err(error) =
                cc_acp::provider::store::load_for_session(std::path::Path::new(cwd), config)
            {
                let note = self.services.lc.tr_args(
                    "thread-config-unavailable",
                    &[("error".into(), error.to_string().into())],
                );
                self.push_system_note(note);
                return false;
            }
        }
        true
    }

    /// services.cwd 仅是当前 thread 的派生值，不改变进程 current_dir。
    fn activate_thread_context(&mut self, cwd: String) {
        let changed = self.services.cwd != cwd;
        self.services.cwd = cwd.clone();
        self.session_mgr.current_mut().messages.pipeline =
            message_pipeline::MessagePipeline::new(cwd.clone());
        self.session_mgr
            .current_mut()
            .ui
            .at_mention
            .set_cwd(cwd.clone());
        let plugin_skill_dirs = self
            .services
            .plugin_data
            .as_ref()
            .map(|data| data.all_skill_dirs.clone())
            .unwrap_or_default();
        let dirs = cc_middlewares::SkillsMiddleware::resolve_dirs_static(&cwd, &plugin_skill_dirs);
        self.session_mgr.current_mut().commands.skills = cc_middlewares::skills::list_skills(&dirs);
        self.session_mgr
            .current_mut()
            .commands
            .agent_commands
            .clear();
        self.session_mgr.current_mut().ui.textarea = build_textarea(false);
        if changed {
            if let Some(config) = self.services.peri_config.as_ref() {
                match cc_acp::provider::store::load_for_session(std::path::Path::new(&cwd), config)
                {
                    Ok(config) => {
                        if let Some(provider) = agent::LlmProvider::from_config(&config) {
                            self.services.provider_name = provider.display_name().to_string();
                            self.services.model_name = provider.model_name().to_string();
                        }
                        self.services.peri_config = Some(config);
                    }
                    Err(error) => {
                        tracing::warn!(%error, %cwd, "Failed to refresh project settings")
                    }
                }
            }
            self.services.git_branch_cache.lock().set_status(None);
            // 换掉 channel，旧目录在途查询的结果不会覆盖新目录缓存。
            *self.services.git_branch_poller.lock() = service_registry::GitBranchPoller::new();
            if self.services.mcp_pool.is_some() {
                self.spawn_mcp_init();
            }
            self.global_panels.close_if(PanelKind::Mcp);
            self.session_mgr
                .current_mut()
                .session_panels
                .close_if(PanelKind::Hooks);
        }
        if let Some(mode) = self
            .services
            .peri_config
            .as_ref()
            .and_then(|config| config.config.streaming_mode.as_deref())
        {
            self.session_mgr
                .current_mut()
                .messages
                .pipeline
                .init_streaming_mode_from_config(mode);
        }
        let strategy = self
            .services
            .peri_config
            .as_ref()
            .and_then(|config| config.config.detail_render_strategy.clone())
            .unwrap_or_else(|| "auto".to_owned());
        let _ = self.session_mgr.current_mut().messages.render_tx.try_send(
            crate::ui::render_thread::RenderEvent::SetDetailStrategy(strategy),
        );
    }

    /// 恢复历史 thread：先确认元数据、目录和 ACP 加载成功，再替换 UI。
    pub fn open_thread(&mut self, thread_id: ThreadId) -> bool {
        if !self.can_switch_thread_context() {
            let note = self.services.lc.tr("thread-busy");
            self.push_system_note(note);
            return false;
        }
        let store = self.services.thread_store.clone();
        let tid = thread_id.clone();
        let loaded = tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(store.load_meta(&tid))
        });
        let thread_meta = match loaded {
            Ok(loaded) => loaded,
            Err(error) => {
                let note = self.services.lc.tr_args(
                    "thread-load-failed",
                    &[("error".into(), error.to_string().into())],
                );
                self.push_system_note(note);
                return false;
            }
        };
        if !self.validate_thread_context_switch(&thread_meta.cwd) {
            return false;
        }
        let base_msgs = match tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(store.load_context(&tid))
        }) {
            Ok(messages) => messages,
            Err(error) => {
                let note = self.services.lc.tr_args(
                    "thread-load-failed",
                    &[("error".into(), error.to_string().into())],
                );
                self.push_system_note(note);
                return false;
            }
        };
        if let Some(client) = self.acp_client.clone() {
            let result = tokio::task::block_in_place(|| {
                tokio::runtime::Handle::current().block_on(client.load_session(
                    &thread_id,
                    &thread_meta.cwd,
                    None,
                ))
            });
            if let Err(error) = result {
                let note = self.services.lc.tr_args(
                    "thread-enter-failed",
                    &[("error".into(), error.to_string().into())],
                );
                self.push_system_note(note);
                return false;
            }
        }
        self.session_mgr.current_mut().metadata.session_id =
            uuid::Uuid::new_v7(uuid::Timestamp::now(uuid::NoContext));
        self.session_mgr.current_mut().ui.text_selection.clear();
        self.session_mgr.current_mut().ui.pending_text_copy = None;
        self.session_mgr
            .current_mut()
            .ui
            .presented_selection_snapshot = None;
        self.session_mgr.current_mut().messages.last_viewport = None;
        self.clear_subagent_sessions();
        self.activate_thread_context(thread_meta.cwd.clone());
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

        self.session_mgr.current_mut().current_thread_id = Some(thread_id);
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
        let thread_meta = Some(thread_meta);
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
        true
    }

    pub fn open_thread_with_feedback(&mut self, thread_id: ThreadId) {
        self.open_thread(thread_id);
    }

    /// 新建 thread：清空消息，关闭 browser（thread id 在首次发送时创建）
    pub fn new_thread(&mut self) {
        self.new_thread_in_cwd(self.services.cwd.clone());
    }

    /// 在选中项目打开空白 Composer；thread 在首次发送时创建。
    pub fn new_thread_in_cwd(&mut self, cwd: String) -> bool {
        if !self.validate_thread_context_switch(&cwd) {
            return false;
        }
        if let Some(client) = self.acp_client.clone() {
            if let Err(error) = tokio::task::block_in_place(|| {
                tokio::runtime::Handle::current().block_on(client.close_current_session())
            }) {
                let note = self.services.lc.tr_args(
                    "thread-new-failed",
                    &[("error".into(), error.to_string().into())],
                );
                self.push_system_note(note);
                return false;
            }
        }
        self.session_mgr.current_mut().metadata.session_id =
            uuid::Uuid::new_v7(uuid::Timestamp::now(uuid::NoContext));
        self.session_mgr.current_mut().ui.text_selection.clear();
        self.session_mgr.current_mut().ui.pending_text_copy = None;
        self.session_mgr
            .current_mut()
            .ui
            .presented_selection_snapshot = None;
        self.session_mgr.current_mut().messages.last_viewport = None;
        self.clear_subagent_sessions();
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
                        // /clear 新建 thread：对齐 CC SessionEnd reason="clear"
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

        self.activate_thread_context(cwd);
        self.refresh_terminal_title();
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
        true
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
