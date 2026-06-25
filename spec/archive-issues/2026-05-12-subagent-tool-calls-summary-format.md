> 归档于 2026-05-16，原路径 spec/issues/2026-05-12-subagent-tool-calls-summary-format.md

# SubAgent 工具调用列表格式优化

**状态**：Fixed + Verify
**优先级**：低
**创建日期**：2026-05-12
**修复日期**：2026-05-12

## 问题描述

SubAgent 完成后显示的工具调用列表采用展开格式（如 `Glob, Glob, Glob, Grep, Grep...`），当调用次数较多时（如 91 次调用）会占用大量显示空间，难以快速了解工具使用分布。

## 症状详情

### 当前显示

```
⎿ [Sub-agent executed 91 tool calls: Glob, Glob, Glob, Glob, Grep, Grep, Grep, Grep, Grep, Grep, Read, Read, Read, Read, Read, Read, Read, Read, Read, Read, Read, Read, Read, Read, Read, Read, Read, Read, Read, Read, Read, Read, Read, Read, Read, Read, Read, Read, Read, Read, Read, Read, Read, Read, Read, Read, Read, Read, Read, Read, Read, Read, Read, Read, Read, Read, Read, Read, Read, Read, Read, Grep, Grep, Grep, Grep, Glob, Read, Read, Read, Read, Read, Read, Grep, Grep, Read, Read, Read, Read, Read, Read, Read, Read, Read, Read, Read, Read, Read, Read, Read, Read, Read]
```

### 期望显示

```
⎿ [Sub-agent executed 91 tool calls: Glob 5 times, Grep 12 times, Read 74 times]
```

或更简洁：

```
⎿ [Sub-agent executed 91 tool calls: Glob×5, Grep×12, Read×74]
```

## 相关代码

- `peri-middlewares/src/subagent/tool.rs:945-963` — `format_subagent_result()` 函数，当前使用 `.join(", ")` 展开所有工具名

  ```rust
  let tool_summary = output
      .tool_calls
      .iter()
      .map(|(call, _result)| call.name.as_str())
      .collect::<Vec<_>>()
      .join(", ");
  ```

- `peri-tui/src/ui/message_view.rs:826-850` — `parse_subagent_tool_count()` 解析工具调用次数，修改格式时需同步更新

## 期望改进方向

1. 统计各工具的调用次数，按调用次数降序排列
2. 使用聚合格式（如 `Glob 13 times` 或 `Glob×13`）替代展开列表
3. 当工具种类较多时（如 >5 种），可考虑只显示前 N 种，其余合并为 `...`

## 影响范围

- SubAgent 完成后的工具调用汇总消息
- 不影响功能，仅影响显示信息密度和可读性
