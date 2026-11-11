# Grep 的 files_with_matches 输出在 Windows 下路径未归一化出现反斜杠

**状态**：Open
**优先级**：低
**创建日期**：2026-11-11

## 问题描述

Grep 以 `files_with_matches` 模式（默认模式）返回结果时，消息区展示的文件路径是带反斜杠的 Windows 原生形式，且可能带 `.\` 前缀；而同一工具的 `content` 模式与 Glob 的输出都是归一化后的正斜杠相对路径。纯展示问题，Windows + 本机 rg 环境必现。

## 症状详情

`files_with_matches` 模式（真实会话实例）：

```
● Search(pattern: "…")
  ⎿ Found 3 files
    .\spec\features\agent-command-center-prd.md
    .\cc-tui\src\app\agent_center\render.rs
    … +1 line (ctrl+o to expand)
```

同一工具的 `content` 模式展示的是归一化后的正斜杠相对路径：

```
    docs/2026-11-11-xxx.md:15: 内容…
```

另一例出现**混合分隔符**（更说明是未经归一化的原样输出）：

```
    cc-tui/src/command\core\help.rs
    cc-tui/src/command\kernel.rs
```

注意首段 `cc-tui/src/command` 已是正斜杠，而 `core\help.rs` 仍是反斜杠。

## 复现条件

- **复现频率**：必现（Windows + rg 可用 + `output_mode` 非 `content`/`count`）
- **触发步骤**：
  1. 在 Windows 上执行一次 Grep，`output_mode` 保持默认（`files_with_matches`）或不传
  2. 观察工具结果正文：路径为 `.\dir\file.ext` 形式
  3. 改为 `output_mode: "content"` 再执行：路径变为 `dir/file.ext:行号: 内容`
- **环境**：Windows + 本机 rg（PATH 探测命中）；若 `rg` 不可用回退纯 Rust 引擎则不触发
- **影响面**：`files_with_matches` 是默认命中路径；会话库扫描（927 个会话）该形态 1182 条输出中 1171 条（99%）路径带反斜杠，其中 177 条以 `.\` 开头；Glob 为 0

## 涉及文件

- `cc-tools/src/tools/filesystem/rg_engine.rs` —— rg 输出行转换（无行号行的归一化缺口所在）
- `cc-tools/src/tools/filesystem/grep_args.rs` —— `output_mode` 默认 `files_with_matches`
- `docs/2026-11-11-grep-files-only-path-not-normalized.md` —— 完整机制与根因分析（补充材料）

## 状态变更记录

| 日期 | 从 | 到 | 操作人 | 说明 |
|------|-----|-----|--------|------|
| 2026-11-11 | — | Open | agent | 创建 |

## 修复记录

### 修复 #1（2026-11-11）

- **操作人**：agent
- **用户原意**：files 模式输出的路径应与 content 模式、Glob 一致，显示正斜杠相对路径
- **修复内容**：rg 输出转换按 `output_mode` 分流，files 模式（-l/-L）整行路径走统一的归一化 helper（转相对 cwd、反斜杠转正斜杠、剥 `./` 前缀）；content/count 行为不变
- **涉及 commit**：59205d63
- **验证状态**：待验证
