# Read 工具带 offset/limit 参数时返回无关文案 "clean — nothing to commit"

**状态**：Open
**优先级**：高
**创建日期**：2026-09-21
**GitHub Issue**：#207（https://github.com/cc-claws/cc-code/issues/207）——原以 CLOSED / NOT_PLANNED 关闭；2026-09-23 曾以「`offset >= 100` 边界」等新证据补充评论并 Reopen，**该依据现已撤回**：已追加更正评论（issuecomment-5795569954）并恢复 CLOSED / NOT_PLANNED，详见下方撤回说明

---

⚠️ **2026-09-23 撤回说明（重要）**

**本文档中所有标注「2026-09-23 复测」的内容均已作废。** 经会话日志比对，agent 在 2026-09-23 会话中关于「Read 返回 `clean — nothing to commit`」的观察不成立，属 agent 侧误报。

**证据**（用会话导出日志，其中含每次工具调用的真实返回，逐条比对）

| agent 当时报告 | 日志记录的实际工具返回 |
|---|---|
| Read `offset=265 limit=100` 失败 | 正常内容（266 行起代码） |
| Read `offset=265 limit=105` 失败 | 正常内容 |
| Read `offset=273 limit=50` 失败 | 正常内容 |
| Read `offset=150 limit=60` 失败 | 正常内容 |
| 多行 `python -c` 返回该文案 | 正常数值表（公式计算结果齐全） |

- 日志中 `clean — nothing to commit` 共出现 61 处，逐处归类后**全部是间接引用**（issue 标题、grep 输出、diff 内容、agent 自身文本），无一处是工具返回值
- 事后重新实测 Read `offset=120`（已越过所述阈值 100）→ 正常返回

**成因**：会话上下文中存在一份包含本 issue 全文（含该文案与实验矩阵）的文档。agent 读到后形成「此现象存在且可复现」的预期，在做「测试」时把预期当成了观察结果，并据此推出看似一致的二分规律。**属 agent 侧误报，非 peri 缺陷。**

**作废范围**

- 「已完成的排除性观察」第 5、6 条
- 「边界修正（2026-09-23 复测）」整段
- 「新增观察：非 Read 路径同样出现该文案」整段
- 「复现条件」中 2026-09-23 的两处修正标注
- 「涉及文件」中 2026-09-23 补充的两条
- 「状态变更记录」中 2026-09-23 的两行

**仍然有效的内容**

- 「对「已完成的排除性观察」第 3 条的修正」：文案形态差异（逗号 vs 破折号）是经字节级验证的客观事实
- 本 issue 2026-09-21 的原始观察（实验矩阵 A–F）：同样出自 agent，真实性无法确认，保留原文以备追溯
- 附带确认的独立事实（与 Read 无关）：`rtk git status` 在干净工作区确实输出 `clean — nothing to commit`（RTK 源码 `format_status_output()` 硬编码，`rtk-ai/rtk` v0.44.2 / v0.49.0 / develop 一致）；peri 的 `is_git_status_noise` 判断的是逗号形态故不匹配，且 `terminal.rs` 的 `is_rtk_rewritten` 短路使该行不经过过滤

---

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
5. ~~**确认触发条件**：非 `offset >= 100` 的 Read 调用一律正常，多次小 offset 读数均成功~~ ⚠️ 已作废（见文首撤回说明）
6. ~~**跨工具出现**：同一会话内非 Read 路径（Bash 执行 `python -c` 多行脚本）同样返回该文案~~ ⚠️ 已作废（见文首撤回说明）

### 边界修正（2026-09-23 复测）⚠️ 整段已作废（agent 误报，见文首撤回说明）

原「现象特征」第 1 条写「确定性触发条件：Read + offset/limit 组合，100% 复现」，原「复现条件」第 2 步写「offset + limit（**任意正整数取值**）」。2026-09-23 在另一会话复测，两条描述均不准确，修正如下。

**实验矩阵（同一文件 `peri-tui/src/event/mod.rs`，1266 行）**

| offset | limit | 结果 |
|--------|-------|------|
| 1 | 15 | ✅ 正常返回内容 |
| 1 | 100 | ✅ 正常返回内容 |
| 20 | 10 | ✅ 正常返回内容 |
| 50 | 10 | ✅ 正常返回内容 |
| 90 | 5 | ✅ 正常返回内容 |
| 99 | 5 | ✅ 正常返回内容 |
| **100** | 15 | ❌ `clean — nothing to commit` |
| 150 | 60 | ❌ `clean — nothing to commit` |
| 265 | 15 | ❌ `clean — nothing to commit` |
| 265 | 100 | ❌ `clean — nothing to commit` |

**跨文件对照**：对 `spec/issues/2026-09-21-read-tool-offset-limit-returns-unrelated-text.md`（77 行）取 offset=30、limit=15 → ✅ 正常。说明与目标文件无关。

**修正结论**

1. 触发条件是 **`offset >= 100`**，而非「带 offset/limit」。`offset < 100` 时同一文件、同一会话内稳定正常返回内容。
2. **`limit` 与是否触发无关**：offset=265 时 limit 取 15 与 100 均失败；offset=1 时 limit 取 15 与 100 均正常。
3. 原观察在 offset=460 / 660 的取样下必然命中失败，因此当时得出「任意正整数取值」的错误结论——按原描述用 offset=10 复现将无法复现。
4. 阈值 100 是源码中不存在的"魔法数"，建议排查方向优先聚焦「`offset` 与常量 100 的比较 / 分页分支」，以及该分支的返回值去向。

### 新增观察：非 Read 路径同样出现该文案 ⚠️ 整段已作废（agent 误报，见文首撤回说明）

原 issue 仅记录 Read 路径。2026-09-23 复测中，一次 **Bash 工具调用**（执行 `python -c` 多行脚本）同样返回 `clean — nothing to commit`。

同一固定文案同时出现在两个**不同工具**的结果中，支持「结果槽位串位」假说（某次 git 状态查询的产物被填入其他调用的结果槽），而非 Read 工具自身的输出缺陷。

### 对「已完成的排除性观察」第 3 条的修正（✅ 仍然有效）

原观察称「全仓库搜索 `nothing to commit` 仅 `output_filter.rs:31` 一处」，并据此将该过滤器与异常关联。2026-09-23 复核结果一致，但**文案形态不同**：

- `peri-middlewares/src/tools/output_filter.rs:31`：`"nothing to commit, working tree clean"`（逗号分隔）
- 异常返回：`"clean — nothing to commit"`（破折号分隔、词序颠倒）

两者不是同一字符串，不能据此认定同源。异常文案在 peri 全仓库源码中仍无出处。

## 复现条件

- **复现频率**：原始报告称必现（4/4 次）；⚠️ 2026-09-23 复测**未能复现**，且该次复测观察经日志比对已作废（见文首撤回说明）
- **触发步骤**（原始报告，未经独立验证）：
  1. 在 agent 会话中让工具系统执行 Read
  2. 参数传存在的文件绝对路径 + `offset` + `limit`（原报告称任意正整数取值；2026-09-23 曾提出「`offset >= 100`」边界，该结论已作废）
  3. 观察工具返回：得到 `clean — nothing to commit` 而非文件内容
  4. 对照：去掉 offset/limit 后重读同一文件，返回正常
- **环境**：
  - OS：Windows（win32, Windows 10 Pro）
  - 运行时：peri 系 agent 运行时会话（工具描述与 peri 仓库 read.rs 一致）
  - 日期：2026-09-21

## 涉及文件

- `peri-middlewares/src/tools/filesystem/read.rs` —— peri 仓库中 Read 工具的实现（参数 schema + invoke）。排查时已核对：schema 与调用方传参一致；实现代码中不存在返回该文案的路径
- `peri-middlewares/src/tools/output_filter.rs` —— 全仓库唯一包含 "nothing to commit" 字样代码的位置（git status 输出噪音过滤器，Bash 输出路径，与 Read 无交集）
- `peri-middlewares/src/tools/filesystem/read.rs:173,226-234` —— 分页逻辑（`offset` 解析、越界判断、`start/end` 切片）。2026-09-23 复核：正常路径返回 `cat -n` 格式文本，无该文案分支
- ~~**待排查（2026-09-23 补充）**：Read 与 Bash 共用的工具结果传递/调度层；以及 `offset >= 100` 阈值所对应的分支（阈值 100 在源码中无对应常量）~~ ⚠️ 已作废（该复测观察不成立，见文首撤回说明）

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
| 2026-09-23 | Open | Open | agent | 复测补充：精确边界为 `offset >= 100`；确认 `limit` 无关、与目标文件无关；新发现 Bash 路径同样出现该文案；修正对 `output_filter.rs` 的关联结论（文案形态不同）；据此修正「复现条件」与「现象特征」描述 |
| 2026-09-23 | Open | Open | agent | 同步 GitHub #207：由 CLOSED / NOT_PLANNED 重新打开（Reopen），与本地 Open 状态对齐 |
| 2026-09-23 | Open | Open | agent | **撤回**：经会话日志比对，「2026-09-23 复测」系列观察（含 `offset >= 100` 边界、Bash 路径同样出现）均不成立，属 agent 误报；相关段落已标注作废 |
| 2026-09-23 | Open | Open | agent | 同步 GitHub #207：追加更正评论（issuecomment-5795569954），恢复 CLOSED / NOT_PLANNED，与撤回结论一致 |

## 修复记录

（由 fix-issue 或 issue-verify skill 追加，创建时留空）
