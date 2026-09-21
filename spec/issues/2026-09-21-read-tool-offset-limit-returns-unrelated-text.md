# Read 工具带 offset/limit 参数时返回无关文案 "clean — nothing to commit"

**状态**：Open
**优先级**：高
**创建日期**：2026-09-21

## 问题描述

在 agent 会话中调用 Read 工具读取已存在的文件，当请求带 `offset` + `limit` 分页参数时，工具不返回文件内容，而是返回固定文案 `clean — nothing to commit`（git 工作区状态语义的英文句子）。不带这两个参数时，同一文件可正常返回内容；文件不存在时也能正常返回 `File not found` 错误。异常在多次、跨文件、跨对话轮次的调用中稳定复现。

## 症状详情

### 实验矩阵（2026-09-21 会话内实测）

| 调用 | 文件 | offset/limit | 实际返回 |
|------|------|--------------|----------|
| A | `D:\code\peri\CLAUDE.md`（存在） | 无 | ✅ 正常返回文件内容 |
| B | `D:\code\peri\not_exist_at_all.txt`（不存在） | 无 | ✅ 正常报 `Error: File not found at ...` |
| C | `D:\code\peri\peri-widgets\src\markdown\render_state.rs`（存在） | offset=460, limit=220 | ❌ `clean — nothing to commit` |
| D | 同 C 文件（重试，参数 offset=460, limit=200） | 有 | ❌ `clean — nothing to commit` |
| E | `D:\code\peri\peri-tui\src\ui\message_render.rs`（存在） | offset=660, limit=100 | ❌ `clean — nothing to commit` |
| F | C 文件（会话后期再次复测 offset=460, limit=60） | 有 | ❌ `clean — nothing to commit` |

### 现象特征

- **确定性触发条件**：Read + offset/limit 组合，100% 复现（C/D/E/F 共 4 次）
- **返回内容固定**：无论目标文件与 offset/limit 取值如何变化，返回文案一字不差
- **错误链路本身正常**：不存在文件报 `File not found`（实验 B）；其他工具的参数校验错误也会明确报错（如同会话中 Grep 缺参数报 `Missing required parameter 'pattern'`）
- **对 agent 的危害形态**：错误内容形态上像"正常输出"，不会被当作报错，可能被 agent 当作真实文件内容使用

### 已完成的排除性观察

以下为排查过程中可直接观察到的事实，供定位参考（不含根因结论）：

1. **调用参数与 peri 源码 schema 一致**：`peri-middlewares/src/tools/filesystem/read.rs` 中 Read 工具 schema 为 `file_path`(string, REQUIRED) + `offset`(integer) + `limit`(integer)；实测调用传参名称、类型与之严格一致（file_path 为真实存在的绝对路径，offset/limit 为正整数）
2. **Read 工具实现代码中不存在产生该文案的分支**：`read.rs` 的 invoke 实现正常路径返回 `cat -n` 格式文本、错误路径返回 `Err(message)`，无任何路径输出 `clean — nothing to commit`
3. **该文案在 peri 全仓库源码中不存在**：全仓库搜索 `nothing to commit` 仅 `output_filter.rs:31` 一处（`filter_git_status` 过滤器，作用是把 `nothing to commit, working tree clean` 从 git status 输出中**剔除**，属 Bash 命令输出路径），与 Read 工具返回通道无代码交集
4. **当前 agent 运行时与 peri 同源**：会话中 Read 工具的英文描述与 `read.rs` 的 `READ_FILE_DESCRIPTION` 逐字一致

## 复现条件

- **复现频率**：必现（确定性，4/4 次）
- **触发步骤**：
  1. 在 agent 会话中让工具系统执行 Read
  2. 参数传存在的文件绝对路径 + `offset` + `limit`（任意正整数取值）
  3. 观察工具返回：得到 `clean — nothing to commit` 而非文件内容
  4. 对照：去掉 offset/limit 后重读同一文件，返回正常
- **环境**：
  - OS：Windows（win32, Windows 10 Pro）
  - 运行时：peri 系 agent 运行时会话（工具描述与 peri 仓库 read.rs 一致）
  - 日期：2026-09-21

## 涉及文件

- `peri-middlewares/src/tools/filesystem/read.rs` —— peri 仓库中 Read 工具的实现（参数 schema + invoke）。排查时已核对：schema 与调用方传参一致；实现代码中不存在返回该文案的路径
- `peri-middlewares/src/tools/output_filter.rs` —— 全仓库唯一包含 "nothing to commit" 字样代码的位置（git status 输出噪音过滤器，Bash 输出路径，与 Read 无交集）

## 现象边界（留给排查方向）

- 异常仅出现在 **Read + offset/limit** 组合；同一 Read 工具的无参数路径、错误返回路径均正常
- 异常文案不是 peri 源码可生成的字符串，形态与 git 状态查询输出一致
- 排查建议方向：工具执行层之上的结果传递/调度环节（谁可能把一次 git 状态查询的结果填入 Read 的响应槽位），而非 read.rs 工具实现本身

## 关联 Issue

- `spec/issues/2026-09-21-read-tool-fake-image-validation.md` —— 同日记录的 Read 工具另一问题（伪图片文件不校验直接 base64 发给模型导致 400）。同一工具的两个独立问题，互不相同

## 状态变更记录

| 日期 | 从 | 到 | 操作人 | 说明 |
|------|-----|-----|--------|------|
| 2026-09-21 | — | Open | agent | 创建：会话内 Read+offset/limit 确定性返回无关文案，实验矩阵与源码排除观察已记录 |

## 修复记录

（由 fix-issue 或 issue-verify skill 追加，创建时留空）
