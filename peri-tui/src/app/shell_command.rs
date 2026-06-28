use std::sync::Arc;

use chrono::Utc;
use tokio::{sync::{mpsc, oneshot}, task::AbortHandle};

use super::*;
use crate::shell_exec::CommandOutput;
use crate::shell_history::ShellCommandRecord;
use crate::thread::{ThreadId, ThreadMeta, ThreadStore};

#[derive(Default)]
pub struct ShellCommandRuntime {
    pub stdin_tx: Option<mpsc::Sender<String>>,
    pub running_record_id: Option<String>,
    pub stdin_lines: Vec<String>,
    pub abort_handle: Option<AbortHandle>,
    pub command: String,
    pub cwd: String,
    pub thread_id: Option<ThreadId>,
    pub started_at: Option<chrono::DateTime<Utc>>,
    pub anchor_message_id: Option<String>,
}

impl ShellCommandRuntime {
    fn is_running(&self) -> bool {
        self.running_record_id.is_some()
    }
}

/// 前台 Shell 命令：包装 [`ShellCommandRuntime`] + 流式输出 channel。
///
/// `output_rx` / `result_rx` 在 Ctrl+B 后台化时通过 `Option::take` 移交给
/// [`BackgroundShell`] / DiskOutput writer，进程全程不中断。
/// B-1 阶段 streaming 字段保持 None（仍走阻塞式 execute_shell_command_with_stdin），
/// B-2 起由 run_shell_command 填充。
pub struct ForegroundShell {
    /// 前台命令 runtime（abort_handle / stdin_tx / running_record_id 等）
    pub runtime: ShellCommandRuntime,
    /// 流式输出 channel（stdout + stderr 合并），App poll 循环 drain 丢弃（防 channel 满阻塞进程）；
    /// record 的 stdout/stderr 来自 result_rx 的 CommandOutput，无需累积。
    pub output_rx: Option<mpsc::Receiver<Vec<u8>>>,
    /// 进程退出 result channel，App poll 检测退出
    pub result_rx: Option<oneshot::Receiver<anyhow::Result<CommandOutput>>>,
    /// 前台命令启动时刻（后台化时传递给 BackgroundShell，避免 elapsed 归零）
    pub started_instant: std::time::Instant,
}

impl Default for ForegroundShell {
    fn default() -> Self {
        Self {
            runtime: ShellCommandRuntime::default(),
            output_rx: None,
            result_rx: None,
            started_instant: std::time::Instant::now(),
        }
    }
}

/// Shell 命令池：管理前台命令（最多 1 个）。后台命令由
/// [`super::ChatSession::background_shells`] 管理。
#[derive(Default)]
pub struct ShellCommandPool {
    /// 当前前台命令（runtime=default 表示无命令运行）
    pub foreground: ForegroundShell,
}

impl ShellCommandPool {
    /// 是否有前台命令运行中。
    pub fn is_running(&self) -> bool {
        self.foreground.runtime.is_running()
    }
}

impl App {
    pub fn run_shell_command(&mut self, command: String) {
        let command = command.trim().to_string();
        if command.is_empty() {
            self.push_system_note("请输入 shell 命令".to_string());
            self.render_rebuild();
            return;
        }
        if self.session_mgr.current().ui.loading {
            self.push_system_note("已有任务正在运行，暂不能启动 shell 命令".to_string());
            self.render_rebuild();
            return;
        }

        let thread_id = match self.ensure_shell_thread(&command) {
            Some(id) => id,
            None => return,
        };
        let record_id = uuid::Uuid::now_v7().to_string();
        let cwd = self.services.cwd.clone();
        let started_at = Utc::now();
        let anchor_message_id = self
            .session_mgr
            .current()
            .agent
            .origin_messages
            .last()
            .map(|m| m.id().as_uuid().to_string());
        let (stdin_tx, stdin_rx) = mpsc::channel(32);

        self.push_input_history(format!("!{}", command));
        self.session_mgr.current_mut().metadata.last_human_message = Some(format!("!{}", command));
        self.session_mgr.current_mut().messages.view_messages.push(
            MessageViewModel::shell_command_pending(
                record_id.clone(),
                command.clone(),
                cwd.clone(),
            ),
        );
        self.set_loading(true);
        self.session_mgr
            .current_mut()
            .spinner_state
            .set_mode(peri_widgets::SpinnerMode::ToolUse);
        self.session_mgr
            .current_mut()
            .spinner_state
            .set_verb(Some("Bash"));
        self.scroll_to_bottom();
        self.render_rebuild();

        // B-2: 流式执行（output_rx/result_rx 存入 ForegroundShell），支持后续 Ctrl+B
        // 后台化时切换 output_rx 给 DiskOutput writer，进程全程不中断。
        // 进程退出由 poll_foreground_shell() 在主循环 poll 检测，不再依赖 spawned task send event。
        let execution = crate::shell_exec::execute_shell_command_streaming(
            &command,
            &cwd,
            Some(stdin_rx),
        );

        self.session_mgr.current_mut().shell_pool.foreground = ForegroundShell {
            runtime: ShellCommandRuntime {
                stdin_tx: Some(stdin_tx),
                running_record_id: Some(record_id),
                stdin_lines: Vec::new(),
                abort_handle: Some(execution.abort),
                command,
                cwd,
                thread_id: Some(thread_id),
                started_at: Some(started_at),
                anchor_message_id,
            },
            output_rx: Some(execution.output_rx),
            result_rx: Some(execution.result),
            started_instant: std::time::Instant::now(),
        };
    }

    pub(crate) fn is_shell_command_running(&self) -> bool {
        self.session_mgr.current().shell_pool.foreground.runtime.is_running()
    }

    /// 轮询前台 shell 命令：消费流式输出 + 检测进程退出。
    ///
    /// - 消费 `output_rx` 的 chunk 累积到 `accumulated_output`（防止 channel 满阻塞进程）
    /// - 检测 `result_rx` 退出 → 构造 `ShellCommandRecord` → `handle_shell_command_completed`
    ///
    /// 分阶段 borrow：阶段1 消费/检测（&mut session）→ 阶段2 读 runtime 构造 record
    /// （&session）→ 阶段3 handle（&mut self）→ 阶段4 清理 streaming 字段。
    /// 返回 true 表示进程退出（有状态变化，需 redraw）。
    pub fn poll_foreground_shell(&mut self) -> bool {
        // 阶段1：消费 output + 检测退出，取出 result（block 结束释放 session 借用）
        let exit = {
            let session = self.session_mgr.current_mut();
            let foreground = &mut session.shell_pool.foreground;
            // 消费 output_rx 丢弃 chunk（防止 channel 满，进程 stdout/stderr 阻塞）；
            // record 的 stdout/stderr 来自 result_rx 的 CommandOutput，无需在此累积（避免大输出 OOM）。
            if let Some(rx) = foreground.output_rx.as_mut() {
                while rx.try_recv().is_ok() {}
            }
            // 检测 result_rx 退出
            match foreground.result_rx.as_mut() {
                Some(rx) => match rx.try_recv() {
                    Ok(res) => {
                        foreground.result_rx = None;
                        Some(res)
                    }
                    Err(oneshot::error::TryRecvError::Empty) => None,
                    Err(oneshot::error::TryRecvError::Closed) => {
                        foreground.result_rx = None;
                        Some(Err(anyhow::anyhow!("shell task terminated")))
                    }
                },
                None => None,
            }
        };

        let Some(result) = exit else {
            return false;
        };

        // 阶段2：读 runtime 构造 record（read-only clone，借用释放后调用 handle）
        let record = {
            let session = self.session_mgr.current();
            let runtime = &session.shell_pool.foreground.runtime;
            let output = match result {
                Ok(o) => o,
                Err(e) => CommandOutput {
                    stdout: String::new(),
                    stderr: format!("{e:#}"),
                    exit_code: -1,
                },
            };
            ShellCommandRecord {
                id: runtime.running_record_id.clone().unwrap_or_default(),
                thread_id: runtime.thread_id.clone().unwrap_or_default(),
                command: runtime.command.clone(),
                cwd: runtime.cwd.clone(),
                stdin: runtime.stdin_lines.clone(),
                stdout: output.stdout,
                stderr: output.stderr,
                exit_code: output.exit_code,
                started_at: runtime.started_at.unwrap_or_else(Utc::now),
                completed_at: Utc::now(),
                anchor_message_id: runtime.anchor_message_id.clone(),
            }
        };

        // 阶段3：handle 匹配 runtime.running_record_id，清理 runtime，set_loading(false)，更新 VM
        self.handle_shell_command_completed(record);

        // 阶段4：清理前台 streaming 字段（runtime 已被 handle 清为 default）
        let session = self.session_mgr.current_mut();
        session.shell_pool.foreground.output_rx = None;
        session.shell_pool.foreground.result_rx = None;
        true
    }

    /// 轮询后台 shell 任务：检测进程退出，生成完成通知并注入对话流（Phase 7）。
    ///
    /// - 检测 `background_shells` 中各任务的 `result_rx` 退出
    /// - 退出时更新 status/exit_code，abort watchdog，生成结构化通知
    /// - agent idle（!loading）→ `submit_message` 直接注入；推理中（loading）→ 暂存到
    ///   `pending_bg_shell_notifications`，Done 后由 lifecycle 消费
    pub fn poll_background_shell_events(&mut self) -> bool {
        // 阶段1：检测完成，收集通知（&mut session）
        let notifications: Vec<String> = {
            let session = self.session_mgr.current_mut();
            let mut notifs = Vec::new();
            for bg in session.background_shells.iter_mut() {
                if bg.notified {
                    continue;
                }
                let Some(rx) = bg.result_rx.as_mut() else {
                    continue;
                };
                let outcome = match rx.try_recv() {
                    Ok(res) => {
                        let (status, exit_code) = match res {
                            Ok(output) => {
                                let st = if output.exit_code == 0 {
                                    super::ShellStatus::Completed
                                } else {
                                    super::ShellStatus::Failed
                                };
                                (st, Some(output.exit_code))
                            }
                            Err(_) => (super::ShellStatus::Failed, Some(-1)),
                        };
                        Some((status, exit_code))
                    }
                    Err(oneshot::error::TryRecvError::Empty) => None,
                    Err(oneshot::error::TryRecvError::Closed) => {
                        Some((super::ShellStatus::Failed, Some(-1)))
                    }
                };
                if let Some((status, exit_code)) = outcome {
                    bg.result_rx = None;
                    bg.mark_ended(status, exit_code);
                    bg.notified = true;
                    if let Some(w) = bg.stall_watchdog.take() {
                        w.abort();
                    }
                    let n = super::background_shell::shell_completion_notification(
                        &bg.id,
                        &bg.command,
                        bg.exit_code,
                        &bg.output_path,
                    );
                    notifs.push(n);
                }
            }
            notifs
        };

        if notifications.is_empty() {
            return false;
        }

        // 阶段2：注入通知（idle 时仅注入首个触发新轮次，其余 + 推理中的全部入 pending 队列，
        // 由 handle_done 逐个 flush，避免连续 submit_message 触发多次 begin_round 互相覆盖）
        let loading = self.session_mgr.current().ui.loading;
        let mut first_injected = false;
        for n in notifications {
            if !loading && !first_injected {
                self.submit_message(n);
                first_injected = true;
            } else {
                self.session_mgr
                    .current_mut()
                    .pending_bg_shell_notifications
                    .push_back(n);
            }
        }
        // 清理已完成任务（防 background_shells 无限增长 + 回收磁盘 output 文件）
        self.cleanup_finished_background_shells();
        true
    }

    /// 清理已完成的后台 shell 任务（防止 background_shells 无限增长 + 回收磁盘文件）。
    ///
    /// 当 background_shells 超过 [`MAX_BACKGROUND_SHELLS`] 时，移除最旧的已完成
    /// （terminal + notified）任务，并 spawn [`peri_agent::task_output::DiskOutput::cleanup`]
    /// 删除其输出文件。运行中任务永不移除。
    fn cleanup_finished_background_shells(&mut self) {
        const MAX_BACKGROUND_SHELLS: usize = 20;
        // 获取 panel 正在 Detail 查看的 task id，淘汰时排除（避免查看中任务被移除导致闪烁）
        let viewing_id: Option<String> = self
            .global_panels
            .get::<super::background_tasks_panel::BackgroundTasksPanel>()
            .and_then(|p| match &p.view {
                super::background_tasks_panel::BackgroundTaskView::Detail { item_id } => {
                    Some(item_id.clone())
                }
                _ => None,
            });
        let to_cleanup = {
            let session = self.session_mgr.current_mut();
            if session.background_shells.len() <= MAX_BACKGROUND_SHELLS {
                return;
            }
            // 找最旧的已完成任务 index（排除正在 Detail 查看的，避免淘汰后闪烁"任务不存在"）
            let mut oldest: Option<(usize, std::time::Instant)> = None;
            for (i, bg) in session.background_shells.iter().enumerate() {
                if bg.status.is_terminal()
                    && bg.notified
                    && viewing_id.as_deref() != Some(bg.id.as_str())
                {
                    let ended = bg.ended_at.unwrap_or(bg.started_at);
                    if oldest.is_none_or(|(_, t)| ended < t) {
                        oldest = Some((i, ended));
                    }
                }
            }
            let Some((idx, _)) = oldest else {
                return;
            };
            session.background_shells.remove(idx).output_path
        };
        // spawn cleanup 文件（fire and forget，不阻塞 poll）
        tokio::spawn(async move {
            let _ = peri_agent::task_output::DiskOutput::cleanup(&to_cleanup).await;
        });
    }

    /// 将当前前台 shell 命令转入后台（Ctrl+B 触发）。
    ///
    /// **核心原则：进程不中断，只切换输出目标。** output_rx 从"累积到 UI"切换为
    /// "写入磁盘文件"，result_rx / abort_handle 移交给 [`BackgroundShell`]。
    /// 进程退出由 poll_background_shell_events（批次 E）检测。
    pub fn background_foreground(&mut self) -> bool {
        // 阶段1：take foreground 的 runtime + streaming 字段（含 abort_handle）
        let (mut runtime, output_rx, result_rx, started_instant) = {
            let foreground = &mut self.session_mgr.current_mut().shell_pool.foreground;
            (
                std::mem::take(&mut foreground.runtime),
                foreground.output_rx.take(),
                foreground.result_rx.take(),
                foreground.started_instant,
            )
        };
        let Some(record_id) = runtime.running_record_id.clone() else {
            return false; // 无前台命令运行
        };
        let Some(result_rx) = result_rx else {
            return false; // 状态异常
        };
        let Some(abort_handle) = runtime.abort_handle.take() else {
            return false;
        };

        // 阶段2：计算 output_path + 创建 BackgroundShell
        let id = record_id;
        let cwd_path = std::path::PathBuf::from(&runtime.cwd);
        let session_id = self.session_mgr.current().metadata.session_id.to_string();
        let output_path =
            peri_agent::task_output::task_output_path(&id, &cwd_path, &session_id);
        let command = runtime.command;
        // spawn DiskOutput writer 消费 output_rx 写磁盘（进程继续运行，输出转存）
        if let Some(rx) = output_rx {
            peri_agent::task_output::DiskOutput::spawn_writer(output_path.clone(), rx);
        }
        // 启动 stall watchdog（Phase 4）：检测命令卡在等待输入
        let watchdog = super::background_shell::spawn_stall_watchdog(
            id.clone(),
            command.clone(),
            output_path.clone(),
            self.services.bg_event_tx.clone(),
        );
        let bg_id = id.clone();
        let mut bg = BackgroundShell::new(id, command, cwd_path, output_path, result_rx, abort_handle, started_instant);
        bg.stall_watchdog = Some(watchdog);

        // 阶段3：push 到 background_shells + 标记前台 VM moved + set_loading(false)
        {
            let session = self.session_mgr.current_mut();
            session.background_shells.push(bg);
            // 标记前台 pending VM 为 moved to background（对齐效果图场景 2/5，避免显示矛盾）
            for vm in &mut session.messages.view_messages {
                if let MessageViewModel::ShellCommand {
                    id: vm_id,
                    moved_to_background,
                    ..
                } = vm
                {
                    if vm_id == &bg_id {
                        *moved_to_background = true;
                        vm.recompute_hash();
                        break;
                    }
                }
            }
        }
        self.set_loading(false);
        true
    }

    // ── Agent shell 路径（BashTool 经 ShellExecutor 注入）──────────────────────
    //
    // 与上面 !command 路径平行但独立：result_rx 由 BashTool::invoke 独占 await，
    // UI 仅用 ExitSignal 检测退出 + background_tx 响应 Ctrl+B。详见 agent_shell_executor.rs。

    /// 轮询 agent shell 注册事件：从 registration channel 消费，登记到当前会话。
    /// 由主循环每帧调用。返回是否有新注册（用于触发重绘）。
    pub fn poll_agent_shell_registrations(&mut self) -> bool {
        // 先 drain 到 Vec，避免 &self（rx borrow）与 register_agent_shell 的 &mut self 冲突。
        let pending: Vec<super::AgentShellRegistration> = match self
            .agent_shell_registrations_rx
            .as_mut()
        {
            Some(rx) => {
                let mut v = Vec::new();
                while let Ok(reg) = rx.try_recv() {
                    v.push(reg);
                }
                v
            }
            None => Vec::new(),
        };
        if pending.is_empty() {
            return false;
        }
        for reg in pending {
            self.register_agent_shell(reg);
        }
        true
    }

    /// 注册一个 agent shell 到当前会话（由 App 主循环从 registration channel 消费后调用）。
    ///
    /// 前台命令（direct_background=false）：push 到 agent_shells，可被 Ctrl+B 后台化。
    /// 直接后台命令（direct_background=true）：push 到 agent_shells 并启动 stall watchdog，
    /// BashTool::invoke 会立即返回 task_id 占位串，退出后注入完成通知。
    pub fn register_agent_shell(&mut self, reg: super::AgentShellRegistration) {
        let direct_background = reg.direct_background;
        let mut slot = super::AgentShellSlot::from_registration(reg);
        // 直接后台命令启动 stall watchdog（检测卡在等待输入）
        if direct_background {
            let watchdog = super::background_shell::spawn_stall_watchdog(
                slot.task_id.clone(),
                slot.command.clone(),
                slot.output_path.clone(),
                self.services.bg_event_tx.clone(),
            );
            slot.stall_watchdog = Some(watchdog);
        }
        self.session_mgr.current_mut().agent_shells.push(slot);
        self.render_rebuild();
    }

    /// 把当前会话中所有前台 agent shell 后台化（Ctrl+B 触发）。
    ///
    /// 与 [`Self::background_foreground`]（!command 路径）协同：Ctrl+B 时先尝试
    /// !command 前台，再尝试 agent 前台。返回是否有任何 agent shell 被后台化。
    pub fn background_agent_foreground(&mut self) -> bool {
        let mut any = false;
        let cwd_path = std::path::PathBuf::from(
            self.session_mgr.current().shell_pool.foreground.runtime.cwd.clone(),
        );
        let session_id = self.session_mgr.current().metadata.session_id.to_string();
        for slot in self.session_mgr.current_mut().agent_shells.iter_mut() {
            if !slot.is_foreground_running() {
                continue;
            }
            // 后台化：启动 stall watchdog（输出已全程写磁盘，无需切 output 目标）
            let watchdog = super::background_shell::spawn_stall_watchdog(
                slot.task_id.clone(),
                slot.command.clone(),
                slot.output_path.clone(),
                self.services.bg_event_tx.clone(),
            );
            slot.stall_watchdog = Some(watchdog);
            slot.mark_backgrounded();
            any = true;
        }
        // 抑制未使用变量（cwd_path/session_id 预留后续 VM 标记用）
        let _ = (&cwd_path, &session_id);
        any
    }

    /// 轮询 agent shell 退出状态：检测 ExitSignal，退出则标记 + 注入完成通知。
    ///
    /// 由主循环每帧调用（与 [`Self::poll_background_shell_events`] 平行）。
    /// 返回是否有任何状态变化（用于触发重绘）。
    pub fn poll_agent_shells(&mut self) -> bool {
        // 阶段1：收集完成通知（&mut session）
        let notifications: Vec<String> = {
            let session = self.session_mgr.current_mut();
            let mut notifs = Vec::new();
            for slot in session.agent_shells.iter_mut() {
                if slot.ended {
                    continue;
                }
                if !slot.exit_signal.is_exited() {
                    continue;
                }
                // 退出：exit_code 未知（ExitSignal 不携带），用 -1 兜底。
                // 更精确的 exit_code 由 BashTool::invoke 经 result_rx 拿到，通知里不影响 agent 判断。
                slot.mark_ended(None);
                let n = super::background_shell::shell_completion_notification(
                    &slot.task_id,
                    &slot.command,
                    slot.exit_code,
                    &slot.output_path,
                );
                notifs.push(n);
            }
            notifs
        };

        if notifications.is_empty() {
            return false;
        }

        // 阶段2：注入通知（复用 !command 路径的注入机制：idle 注入首个触发新轮次，其余入 pending）
        let loading = self.session_mgr.current().ui.loading;
        let mut first_injected = false;
        for n in notifications {
            if !loading && !first_injected {
                self.submit_message(n);
                first_injected = true;
            } else {
                self.session_mgr
                    .current_mut()
                    .pending_bg_shell_notifications
                    .push_back(n);
            }
        }
        // 清理已完成的 agent shell（与 background_shells 一致的容量管理）
        self.cleanup_finished_agent_shells();
        true
    }

    /// 清理已完成的 agent shell（防无限增长）。
    fn cleanup_finished_agent_shells(&mut self) {
        const MAX_AGENT_SHELLS: usize = 20;
        let session = self.session_mgr.current_mut();
        if session.agent_shells.len() <= MAX_AGENT_SHELLS {
            return;
        }
        // 移除最旧的已完成任务（运行中永不移除）
        let to_remove = session
            .agent_shells
            .iter()
            .filter(|s| s.ended)
            .take(session.agent_shells.len().saturating_sub(MAX_AGENT_SHELLS))
            .map(|s| s.task_id.clone())
            .collect::<Vec<_>>();
        session.agent_shells.retain(|s| !to_remove.contains(&s.task_id));
    }

    /// 直接创建后台 shell 任务（跳过前台阶段，agent 工具调用路径，参考 PRD §9）。
    ///
    /// output_rx 直接交给 DiskOutput writer 写磁盘，不经前台 UI。
    /// 返回任务 id，调用方可用于引用该后台任务。
    pub fn spawn_shell_task(&mut self, command: String, cwd: String) -> String {
        let id = uuid::Uuid::now_v7().to_string();
        let execution =
            crate::shell_exec::execute_shell_command_streaming(&command, &cwd, None);
        let cwd_path = std::path::PathBuf::from(&cwd);
        let session_id = self.session_mgr.current().metadata.session_id.to_string();
        let output_path =
            peri_agent::task_output::task_output_path(&id, &cwd_path, &session_id);
        // output_rx 直接写磁盘（不经前台）
        peri_agent::task_output::DiskOutput::spawn_writer(
            output_path.clone(),
            execution.output_rx,
        );
        let watchdog = super::background_shell::spawn_stall_watchdog(
            id.clone(),
            command.clone(),
            output_path.clone(),
            self.services.bg_event_tx.clone(),
        );
        let mut bg = BackgroundShell::new(
            id.clone(),
            command,
            cwd_path,
            output_path,
            execution.result,
            execution.abort,
            std::time::Instant::now(),
        );
        bg.stall_watchdog = Some(watchdog);
        self.session_mgr.current_mut().background_shells.push(bg);
        id
    }

    pub(crate) fn send_shell_stdin_line(&mut self, line: String) {
        let record_id = self
            .session_mgr
            .current()
            .shell_pool.foreground.runtime
            .running_record_id
            .clone();
        let Some(record_id) = record_id else {
            return;
        };
        let tx = self.session_mgr.current().shell_pool.foreground.runtime.stdin_tx.clone();
        let Some(tx) = tx else {
            self.push_system_note("shell stdin 已关闭".to_string());
            self.render_rebuild();
            return;
        };

        self.session_mgr
            .current_mut()
            .shell_pool.foreground.runtime
            .stdin_lines
            .push(line.clone());
        self.append_shell_stdin_to_vm(&record_id, line.clone());
        self.session_mgr.current_mut().ui.textarea = build_textarea(true);
        self.scroll_to_bottom();
        self.render_rebuild();

        tokio::spawn(async move {
            let _ = tx.send(line).await;
        });
    }

    pub(crate) fn close_shell_stdin(&mut self) {
        if !self.is_shell_command_running() {
            return;
        }
        self.session_mgr.current_mut().shell_pool.foreground.runtime.stdin_tx = None;
        self.session_mgr
            .current_mut()
            .spinner_state
            .set_verb(Some("Bash"));
        self.session_mgr.current_mut().ui.textarea = build_textarea(true);
    }

    pub(crate) fn cancel_shell_command(&mut self) -> bool {
        // take 整个 ForegroundShell（含 streaming 字段 + runtime），避免 poll_foreground_shell
        // 对已 cancel 的任务重复处理（result_rx/output_rx 随 take 释放）
        let shell = std::mem::take(
            &mut self.session_mgr.current_mut().shell_pool.foreground,
        )
        .runtime;
        let Some(record_id) = shell.running_record_id.clone() else {
            return false;
        };
        if let Some(abort_handle) = shell.abort_handle {
            abort_handle.abort();
        }
        let record = ShellCommandRecord {
            id: record_id,
            thread_id: shell.thread_id.unwrap_or_default(),
            command: shell.command,
            cwd: shell.cwd,
            stdin: shell.stdin_lines.clone(),
            stdout: String::new(),
            stderr: "shell command cancelled by user".to_string(),
            exit_code: -1,
            started_at: shell.started_at.unwrap_or_else(Utc::now),
            completed_at: Utc::now(),
            anchor_message_id: shell.anchor_message_id,
        };
        // take 后 foreground 已 default，handle 的 matches_running=false 跳过清理；
        // 手动 set_loading(false)，VM 更新由 handle 基于 thread_id 执行。
        self.set_loading(false);
        self.handle_shell_command_completed(record);
        true
    }

    pub(crate) fn handle_shell_command_completed(
        &mut self,
        mut record: ShellCommandRecord,
    ) -> (bool, bool, bool) {
        let matches_running = self
            .session_mgr
            .current()
            .shell_pool.foreground.runtime
            .running_record_id
            .as_deref()
            == Some(record.id.as_str());
        if matches_running {
            record.stdin = self.session_mgr.current().shell_pool.foreground.runtime.stdin_lines.clone();
            self.session_mgr.current_mut().shell_pool.foreground.runtime = ShellCommandRuntime::default();
            self.set_loading(false);
        }

        self.persist_shell_record(record.clone());

        let current_thread_id = self.session_mgr.current().current_thread_id.clone();
        if current_thread_id.as_deref() == Some(record.thread_id.as_str()) {
            let mut replaced = false;
            let session = self.session_mgr.current_mut();
            for vm in &mut session.messages.view_messages {
                if let MessageViewModel::ShellCommand { id, .. } = vm {
                    if id == &record.id {
                        *vm = MessageViewModel::shell_command_completed(&record);
                        replaced = true;
                        break;
                    }
                }
            }
            if !replaced {
                session
                    .messages
                    .view_messages
                    .push(MessageViewModel::shell_command_completed(&record));
            }
            self.scroll_to_bottom();
            self.render_rebuild();
        }

        (true, false, false)
    }

    fn append_shell_stdin_to_vm(&mut self, record_id: &str, line: String) {
        for vm in &mut self.session_mgr.current_mut().messages.view_messages {
            if let MessageViewModel::ShellCommand {
                id,
                stdin,
                exit_code,
                ..
            } = vm
            {
                if id == record_id && exit_code.is_none() {
                    stdin.push(line);
                    vm.recompute_hash();
                    break;
                }
            }
        }
    }

    fn ensure_shell_thread(&mut self, command: &str) -> Option<ThreadId> {
        if let Some(id) = self.session_mgr.current().current_thread_id.clone() {
            return Some(id);
        }
        let store = self.services.thread_store.clone();
        let cwd = self.services.cwd.clone();
        let mut meta = ThreadMeta::new(cwd);
        meta.title = Some(shell_thread_title(command));
        let created = tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(store.create_thread(meta))
        });
        match created {
            Ok(thread_id) => {
                self.session_mgr.current_mut().current_thread_id = Some(thread_id.clone());
                Some(thread_id)
            }
            Err(e) => {
                tracing::error!(error = %e, "创建 shell thread 失败");
                self.push_system_note(format!("创建 shell 会话失败: {e:#}"));
                self.render_rebuild();
                None
            }
        }
    }

    fn persist_shell_record(&self, record: ShellCommandRecord) {
        let shell_store = self.services.shell_command_store.clone();
        let thread_store = self.services.thread_store.clone();
        let title = shell_thread_title(&record.command);
        tokio::spawn(async move {
            if let Err(e) = shell_store.append(&record).await {
                tracing::warn!(error = %e, "保存 shell 命令历史失败");
            }
            touch_shell_thread(thread_store, &record.thread_id, &title, record.completed_at).await;
        });
    }

    pub(crate) fn merge_shell_records_into_view(
        &self,
        mut view_msgs: Vec<MessageViewModel>,
        base_msgs: &[BaseMessage],
        shell_records: Vec<ShellCommandRecord>,
    ) -> Vec<MessageViewModel> {
        let mut anchor_positions = std::collections::HashMap::new();
        let mut visible_pos = 0usize;
        for msg in base_msgs {
            if !matches!(msg, BaseMessage::System { .. }) {
                visible_pos += 1;
                anchor_positions.insert(
                    msg.id().as_uuid().to_string(),
                    visible_pos.min(view_msgs.len()),
                );
            }
        }

        let mut inserted_base_positions = Vec::new();
        for record in shell_records {
            let base_pos = record
                .anchor_message_id
                .as_ref()
                .and_then(|id| anchor_positions.get(id))
                .copied()
                .unwrap_or_else(|| {
                    if record.anchor_message_id.is_some() {
                        view_msgs.len()
                    } else {
                        0
                    }
                });
            let inserted_before = inserted_base_positions
                .iter()
                .filter(|&&pos| pos <= base_pos)
                .count();
            let insert_pos = (base_pos + inserted_before).min(view_msgs.len());
            view_msgs.insert(
                insert_pos,
                MessageViewModel::shell_command_completed(&record),
            );
            inserted_base_positions.push(base_pos);
        }
        view_msgs
    }
}

fn shell_thread_title(command: &str) -> String {
    let title = format!("!{}", command.trim());
    if title.chars().count() <= 50 {
        title
    } else {
        let prefix: String = title.chars().take(49).collect();
        format!("{}…", prefix)
    }
}

async fn touch_shell_thread(
    thread_store: Arc<dyn ThreadStore>,
    thread_id: &ThreadId,
    title: &str,
    completed_at: chrono::DateTime<Utc>,
) {
    let mut meta = match thread_store.load_meta(thread_id).await {
        Ok(meta) => meta,
        Err(e) => {
            tracing::warn!(error = %e, thread_id = %thread_id, "加载 shell thread meta 失败");
            return;
        }
    };
    if meta.title.is_none() {
        meta.title = Some(title.to_string());
    }
    meta.updated_at = completed_at;
    if let Err(e) = thread_store.update_meta(thread_id, meta).await {
        tracing::warn!(error = %e, thread_id = %thread_id, "更新 shell thread meta 失败");
    }
}

#[cfg(test)]
#[path = "shell_command_test.rs"]
mod tests;
