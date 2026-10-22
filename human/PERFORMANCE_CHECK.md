# 性能走查清单

- [ ] 检查输入卡顿问题
- [ ] 测试时间非常长

## 待修复问题

| 优先级 | 问题 | 位置 |
|--------|------|------|
| 高 | AppendChunk 每个 chunk 都全量重解析 markdown，O(n²) | `cc-tui/src/ui/render_thread.rs` AppendChunk 分支 |
| 中 | poll 超时返回 `Some(Redraw)`，空闲时每 50ms 无条件重绘 | `cc-tui/src/event/mod.rs`、`cc-tui/src/main.rs` 主循环 |
| 中 | SubAgent 每步全量 clone SubAgentGroup（含已渲染 spans）再重渲染 | `cc-tui/src/app/agent_ops/subagent.rs` |
| 低 | `chars().count()` 全量扫描，改 `chars().nth(N).is_some()` | `cc-tui/src/ui/message_render/` |
| 中 | `Vec::remove(0)` 滑动窗口，O(n) 移位 | `cc-tui/src/app/agent_ops/` |
| 中 | ReAct 每次迭代 `messages.to_vec()` 全量复制消息历史 | `cc-agent/src/agent/executor/mod.rs` |
| 中 | CompactDone 逐条发送 RenderEvent，批量操作变串行 | `cc-tui/src/app/agent_ops/` |
| 中 | render_title 每帧执行 format 字符串拼接 | `cc-tui/src/ui/main_ui/mod.rs` |
| 中 | 渲染路径 `spans.extend(line.spans.clone())` 逐行 clone | `cc-tui/src/ui/message_render/` |
| 低 | SQLite `append_messages` 每次执行 `COUNT(*)` 子查询 | `cc-agent/src/thread/sqlite_store.rs` |
