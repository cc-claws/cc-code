# Code Review: agent_state_messages → origin_messages 重命名

> **日期**：2026-05-29
> **范围**：peri-tui/src/app/ — 10 个文件，纯改名
> **审查模式**：定向型
> **语言/技术栈**：Rust

---

## 总览

这是一次纯粹的字段重命名，意图是消除命名误导——原名 `agent_state_messages` 暗示它是 agent 的权威状态，新名 `origin_messages` 明确表达它是 ViewModel 的原始输入数据。改动量虽小（20 处引用），但语义收益明显。编译通过，11 个相关测试全部通过，零旧名残留。

| 维度 | 评级 | 问题数 |
|------|------|--------|
| 架构与设计 | 🟢 良好 | 0 |
| 错误处理 | 🟢 良好 | 0 |
| 性能 | 🟢 良好 | 0 |
| 代码风格 | 🟢 良好 | 0 |
| 技术债 | 🟡 待改进 | 2 |

---

## 问题列表

### 🟡 Major（强烈建议修复）

*无重大 bug 或架构问题。*

---

### 🟢 Minor（建议修复）

- `app/mod.rs:518-538` & `agent_ops/lifecycle.rs:248-283` — "无工具调用中断 → 撤销" 路径存在两份高度相似的代码（截断 origin_messages、恢复 textarea、清除 pending/sticky header、pipeline done/restore、添加系统通知）。`app/mod.rs` 版本使用 `RenderEvent::Rebuild`，`lifecycle.rs` 版本使用 `PipelineAction::RebuildAll`，差异微妙。建议后续统一为一个方法避免漂移。

---

### 💡 Suggestions（可选改进）

- `agent_events_bg.rs:156-163` — 已知的 ACP 绕过路径：executor 结束后直接 `origin_messages.push(BaseMessage::human(...))` 不经过 ACP。当前作为显示兜底机制存在，已在 `acp-improve.md` 中登记为后续修补项。

---

## 亮点

- 改名仅触及字段引用，不改变任何逻辑路径，风险极低
- 注释随字段一起更新，清晰标注了"TUI 侧缓存，非权威数据源"的语义边界
- grep 验证：零旧名残留，clean rename

---

## 行动清单

- [ ] 🟢 `app/mod.rs` 与 `lifecycle.rs` 中的中断撤销逻辑去重（后续独立任务）
- [ ] 💡 `agent_events_bg.rs` 中的 ACP 绕过路径——已登记在 `acp-improve.md`

---

*由 code-review skill 自动生成 · 2026-05-29*
