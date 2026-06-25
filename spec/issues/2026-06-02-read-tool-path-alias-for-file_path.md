# Read/Write/Edit 工具缺 file_path：LLM 使用了 path 别名

**状态**：Open
**优先级**：中
**创建日期**：2026-06-02
**类型**：Bug

## 问题描述

LLM 在调用 Read/Write/Edit 文件操作工具时，有时会使用 `path` 参数名代替 `file_path`。这是因为 Claude Code 原生 schema 中 Glob/Grep 搜索工具使用 `path`（可选参数，描述搜索范围），而 Read/Write/Edit 文件操作工具使用 `file_path`（必需参数，描述操作目标文件）。LLM 在工具间切换时产生参数名混淆。

数据验证：158/9,058 次 Read 调用（1.74%）使用了 `path` 而非 `file_path`。一个典型案例中，用户正在反馈 "Write 工具缺少 file_path" 的 issue，agent 去读 Skill 文件时自己也用了 `path`。

## 症状详情

| 症状 | 数据 |
|------|------|
| Read 用 `path` 替代 `file_path` | 158/9,058 (1.74%) |
| Write 完全空参数 `{}` | 20 次（plan 类 skill 独立问题） |
| Edit 缺 `old_string` 或 `new_string` | 12 次（一半是 legal 空 string） |
| Glob/Grep 正常 | 搜索工具使用 `path` 是正确的 |

### Read 的典型错误调用

```json
// 错误 (158次): LLM 沿用了 Glob/Grep 的 path 参数名
{"path": "/Users/konghayao/.claude/skills/issue-create/SKILL.md"}

// 正确 (8,900次):
{"file_path": "/Users/konghayao/.claude/skills/issue-create/SKILL.md"}
```

### 根因

Claude Code 工具 schema 存在有意的不一致（非 bug——设计决策）：

| 工具类别 | 参数名 | 语义 |
|---------|--------|------|
| Read/Write/Edit | `file_path` | "这个文件"（必需，绝对路径） |
| Glob/Grep | `path` | "这个范围"（可选，默认 cwd） |

peri 项目沿用了同一 schema。LLM 在前后调用 Glob/Grep 再调 Read 时，容易沿用 `path` 命名。

## 涉及文件

- `peri-agent/src/agent/executor/tool_dispatch.rs:274-287` —— 工具调用入参克隆点 + 执行点，在此处做参数名归一化
- `peri-agent/src/agent/executor/tool_dispatch.rs:18-19` —— 已有的 `TOOL_ALIASES` 常量（工具名别名，模式可复用）

## 修复方案

在 `tool_dispatch.rs` 执行层加参数名别名表 `PARAM_ALIASES`，在 `t.invoke(input)` 之前将 `path` 归一化为 `file_path`。

**不改工具 schema**（继续要求 LLM 输出 `file_path`），只在执行层做静默兼容。同时打 tracing::warn 日志供后续追踪。

类似现有的 `TOOL_ALIASES` 模式：

```rust
// 已有：工具名别名
const TOOL_ALIASES: &[(&str, &str)] = &[("task", "Agent"), ("shell", "Bash"), ("reading", "Read")];

// 新增：参数名别名
const PARAM_ALIASES: &[(&str, &str)] = &[("path", "file_path")];
```

在 `tool_dispatch.rs:274`（`input` 克隆后、`t.invoke(input)` 调用前）做统一转换，遍历 `PARAM_ALIASES`，检查 input 中有无 `alias` 键但无 `real` 键 → 拷贝值。
