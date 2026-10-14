# 子 Agent 回复完成后切入其视口底部缺失任务总结/Spinner 行

**状态**：Open
**优先级**：中（体验缺陷，导致子视口任务完成态缺乏状态反馈）
**创建日期**：2026-10-14
**Issue**：[#485](https://github.com/cc-claws/cc-code/issues/485)

---

## 一、现象与表现

在 TUI 中启动后台子 Agent 后：
- 在主视口或抽屉栏看到子 Agent 已经运行完成并返回了结果；
- 用户通过抽屉栏按 `Enter` 切换进入该子 Agent 的独立视口（`focused_instance_id = Some(sub_id)`）；
- **视口底部完全空白，缺少主视口完成时应有的完成总结行**（如 `✻ Brewed for 32s · done 16:30`）。

对比主会话视口：主会话任务完成后，底部会常驻一行静态总结行，展示任务总耗时、完成动词与完成时刻；而子 Agent 视口完成后该行直接消失。

## 二、根因分析

该缺陷源于**子 Agent 完成生命周期中，遗漏了对其专属 `SpinnerState` 切换至 `Idle` 的模式结算调用**：

1. **主视口总结行的渲染机制**（`cc-tui/src/ui/main_ui/message_area.rs:185-251`）：
   ```rust
   let spinner_line: Option<Line<'static>> = if app.session_mgr.current().viewport_loading() {
       // 分支 1：处于活跃加载态，渲染动态旋转 Spinner
       ...
   } else if app.session_mgr.current().viewport_spinner().last_summary_elapsed_ms() > 0 {
       // 分支 2：加载结束，但存在结算耗时，渲染完成总结行：✻ Brewed for Xm Xs · done HH:MM
       ...
   } else {
       // 分支 3：两者皆不是，不渲染任何行
       None
   };
   ```

2. **主会话正常结算链路**（`cc-tui/src/app/mod.rs:600-620`）：
   主会话结束时，`App::set_loading(false)` 会显式调用 `s.spinner_state.set_mode(SpinnerMode::Idle)`。
   在 `cc-widgets/src/spinner/mod.rs:86-91` 中，从活跃态切至 `Idle` 会自动捕获并记录耗时：
   ```rust
   if was_active && self.mode == SpinnerMode::Idle {
       self.last_summary_elapsed_ms = self.elapsed_ms();
       self.last_summary_verb = verb::pick_summary_verb();
       self.last_summary_done_at = Some(std::time::SystemTime::now());
   }
   ```
   因此主视口的 `last_summary_elapsed_ms` 大于 0，正常命中分支 2。

3. **子 Agent 状态流转遗漏（Bug 点）**：
   - 查阅 `cc-tui/src/app/agent_ops/subagent.rs:309-326` 的 `"stopped"` 生命周期事件处理：
     代码更新了 `sub.status = Completed`，清空了工具与行为描述，并给渲染管线派发了 `AgentEvent::Done`，**但从未调用 `sub.spinner.set_mode(SpinnerMode::Idle)`**！
   - 同时查阅 `cc-tui/src/app/subagent_session.rs:166-258` 的 `handle_event()`，`AgentEvent::Done` 直接落入 `_ => {}` 通配分支，亦未处置 `self.spinner`。
   - 导致子 Agent 的 `sub.spinner` 一直停留在初始化或最后的运行态，从未触发切入 `Idle` 的结算逻辑，其 `last_summary_elapsed_ms` **恒为 0**。
   - 用户切入子视口时，`viewport_loading()` 为 `false` 且 `last_summary_elapsed_ms()` 为 `0`，渲染层直接退化为 `None`，总结行彻底缺失。

## 三、涉及文件

- `cc-tui/src/app/agent_ops/subagent.rs:309-326` —— 子 Agent 生命周期 `"stopped"` 事件分支。
- `cc-tui/src/app/subagent_session.rs:166-258` —— `BackgroundAgentSession::handle_event` 事件处理。
- `cc-tui/src/ui/main_ui/message_area.rs:185-251` —— 视口 Spinner/总结行渲染层。

## 四、修复建议

在子 Agent 结束链路中补齐 Spinner 状态流转结算：
1. 在 `cc-tui/src/app/agent_ops/subagent.rs` 的 `"stopped"` 分支中，增加调用：
   ```rust
   sub.spinner.set_mode(cc_widgets::SpinnerMode::Idle);
   ```
2. 并在 `cc-tui/src/app/subagent_session.rs` 的 `handle_event` 中增加对 `AgentEvent::Done` 的显式处理，同样调用 `self.spinner.set_mode(cc_widgets::SpinnerMode::Idle)`，确保前后台所有完成路径均正确结算耗时。

## 五、状态变更记录

| 日期 | 从 | 到 | 操作人 | 说明 |
|------|-----|-----|--------|------|
| 2026-10-14 | — | Open | agent | 创建 Issue #485 及本地 Spec 文档 |
