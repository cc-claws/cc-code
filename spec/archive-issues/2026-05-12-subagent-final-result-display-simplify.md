> 归档于 2026-05-16，原路径 spec/issues/2026-05-12-subagent-final-result-display-simplify.md

# 工具显示格式简化

**状态**：Fixed + Verify
**优先级**：低
**创建日期**：2026-05-12

## 问题描述

当前 SubAgent 和 Write 工具的显示格式冗余，用户希望简化：

1. SubAgent 完成后移除 "——执行结果——" 分隔符，直接显示结果
2. Write 工具显示格式改为紧凑单行，结果直接在工具名下方显示

## 症状详情

### 改进 1：SubAgent 执行结果

#### 当前显示

```
❯ Agent(explore)
  I need to understand the acpx-g crate's
  ⏺ Grep(acpx)
  ⏺ Read(acpx-g/prod.sh)
  ⏺ Read(acpx-g/progress.md)
  ●

  ── 执行结果 ──
  ⎿ [Sub-agent executed 16 tool calls: Glob, Read, Read, Grep, Grep, ...]
  ⎿
  ⎿ Here's the complete findings report:
  ⎿
  ⎿ ---
```

### 期望显示

```
❯ Agent(explore)
  I need to understand the acpx-g crate's
  ⏺ Grep(acpx)
  ⏺ Read(acpx-g/prod.sh)
  ⏺ Read(acpx-g/progress.md)
  ●

  ⎿ [Sub-agent executed 16 tool calls: Glob, Read, Read, Grep, Grep, ...]
  ⎿
  ⎿ Here's the complete findings report:
  ⎿
  ⎿ ---
```

### 改进 2：Write 工具显示格式

#### 当前显示

```
⏺ Write
  ⎿ Wrote 71 lines to spec/issues/2026-05-12-subagent-final-result-display-simplify.md
```

#### 期望显示

```
⏺ Write(spec/issues/2026-05-12-subagent-final-result-display-simplify.md)
  ⎿ Wrote 71 lines to spec/issues/2026-05-12-subagent-final-result-display-simplify.md
```

## 相关代码

- `peri-tui/src/ui/message_render.rs:403-406` — 渲染 "——执行结果——" 分隔符

  ```rust
  lines.push(Line::from(vec![Span::styled(
      "  ── 执行结果 ──".to_string(),
      Style::default().fg(theme::DIM).bg(theme::SUB_AGENT_BG),
  )]));
  ```

- `peri-tui/src/ui/message_render.rs:397-429` — final_result 渲染逻辑

- `peri-tui/src/ui/message_view.rs:233-252` — SubAgentGroup 定义

## 期望改进方向

1. 移除 `── 执行结果 ──` 分隔符行（第 403-406 行）
2. 直接渲染 final_result 内容，保留 `⎿` 前缀和缩进

## 影响范围

- SubAgent 完成后的结果显示
- 不影响功能，仅影响显示美观度
