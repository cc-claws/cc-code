> 归档于 2026-05-16，原路径 spec/issues/2026-05-15-tool-output-truncation-with-disk-persist.md

# 工具输出超长时截断 + 持久化磁盘 + 提示 Read 读取剩余内容

**状态**：Fixed
**优先级**：中
**创建日期**：2026-05-15
**关闭日期**：2026-05-16

## 问题描述

Bash、Grep、Glob 等工具在输出超长时会截断（Bash 2000 行/100KB，Grep 按行数限制），截断后的数据直接丢弃。LLM 无法获取完整输出，只能重新执行工具（浪费 token 和时间）。应改为：截断时将完整数据写入 `/tmp/` 临时文件，tool_result 中保留截断的前 N 行 + 提示信息告知 LLM 可用 Read 工具读取完整内容。Read 工具自身不做此处理（已天然支持 offset/limit 参数）。

## 现状数据

| 工具 | 截断条件 | 截断后数据 | 提示信息 |
|------|----------|-----------|---------|
| Bash | >2000 行 或 >100KB | head + tail 保留，中间丢弃 | `[Output truncated: exceeds N byte limit]` |
| Grep | >head_limit 行 | 超出部分丢弃 | `... (truncated at N lines)` |
| Glob | 无截断 | — | — |
| Read | 单行 >65536 字符截断，2000 行上限 | 超出部分丢弃 | 无 |
| WebFetch | 无截断 | — | — |

## 期望改进方向

1. 截断时将完整输出写入 `/tmp/peri-tool-output-{uuid}.txt`（或类似命名）
2. tool_result 中保留截断的前 N 行内容 + 追加提示：

```
... [输出已截断，完整内容已保存到 /tmp/peri-tool-output-abc123.txt]
使用 Read 工具读取该文件可查看完整输出。
```

3. Read 工具不做此处理（它已有 offset/limit 参数，LLM 可自行分段读取）
4. 截断阈值保持当前值不变，仅改变截断后的处理方式（丢弃 → 持久化）

## 涉及文件

- `peri-middlewares/src/middleware/terminal.rs` — Bash 工具的 `truncate_output` 函数
- `peri-middlewares/src/tools/filesystem/grep.rs` — Grep 工具的截断逻辑
- `peri-middlewares/src/tools/filesystem/glob.rs` — Glob 工具（如需添加截断）
- `peri-middlewares/src/tools/filesystem/read.rs` — Read 工具（排除，不做修改）

## 解决方案（已实现）

新增共享函数 `persist_truncated_output`（`tools/output_persist.rs`），写入 `/tmp/peri-tool-output-{uuid}.txt` 后返回 `[Full output saved to {path} — use Read tool to view complete content]` 英文提示。以下工具在截断时调用此函数：

| 工具 | 阈值 | 文件 |
|------|------|------|
| Bash | >2000 行 / >100KB | `terminal.rs` |
| Grep | >head_limit 行 | `grep.rs` |
| Glob | >1000 结果 | `glob.rs` |
| FolderOperations | "list" >500 条目 | `folder.rs` |
| WebFetch | >2000 行 | `web_fetch.rs` |
| MCP ToolBridge | >2000 行 | `tool_bridge.rs` |
| MCP ResourceTool | >2000 行 | `resource_tool.rs` |

**排除项**：WebSearch（逐条 snippet 截断，语义不同）、Read（已有 offset/limit 参数）。

**测试**：771 passed, 0 failed（全量 `peri-middlewares`）。
