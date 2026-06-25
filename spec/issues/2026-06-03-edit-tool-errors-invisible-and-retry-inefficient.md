# Edit 工具 284 次错误对监控系统不可见且重试效率低

**状态**：Pending
**优先级**：高
**创建日期**：2026-06-03
**类型**：Bug

## 问题描述

通过 `agent-defect-analyzer` 的 Edit 专项分析器（`--focus edit`）发现，Edit 工具存在 3 个系统性缺陷，导致 6,233 次调用中有 284 次失败（4.6%），而这些失败**全部对现有监控系统不可见**。主要症状包括：错误未标记 `is_error`、Agent 用过期内容重试、以及 old_string 上下文不足。

## 症状详情

### 症状 1：Edit 错误未标记 is_error=true

| 指标 | 值 |
|------|-----|
| Edit 总调用 | 6,233 |
| 失败次数 | 284 (4.6%) |
| 其中 is_error=true | **0** |
| 其中 is_error=false | 284 (100%) |

所有 284 次 Edit 失败均以 `Ok("Error: ...")` 返回，`is_error` 字段恒为 false。`tool_errors` 分析器依赖 `is_error=true` 筛选，完全遗漏了这些错误。

涉及代码位置（均使用 `Ok(format!("Error: ..."))` 而非 `Err()`）：

| 行号 | 错误场景 |
|------|----------|
| `edit.rs:85` | `old_string` 为空 |
| `edit.rs:93` | 文件不存在 |
| `edit.rs:124` | `old_string not found`（replace_all 模式） |
| `edit.rs:151` | `old_string not found`（单次替换模式） |
| `edit.rs:157` | `old_string not unique` |

### 症状 2：Agent 用过期的文件内容构造 old_string

`old_string_not_found` 错误共 213 次，其中 62%（132 次）发生在文件已被同会话之前的 Edit 成功修改之后。Agent 使用的是之前 Read 时的旧内容，文件已变更但未重新读取。

- old_string 平均长度 842 字符（试图精确匹配大段内容，一旦文件有微小变动就全部失败）
- 37 个同文件连续失败链，总计 84 次重试
- 失败后恢复率 79.5%（182 次失败后重试成功），但每次失败浪费一轮工具调用

### 症状 3：old_string 上下文不足导致不唯一

`old_string_not_unique` 错误共 70 次。LLM 提供的 old_string 在文件中存在多处匹配。

| 重复次数 | 案例数 |
|----------|--------|
| 2 次 | 50 |
| 3 次 | 7 |
| 4 次 | 5 |
| 6 次及以上 | 8 |

old_string 平均长度仅 162 字符（比 not_found 的 842 短得多），部分极端案例仅 1 个字符（如 `}`）。

### 按文件类型失败率

| 扩展名 | 失败/总数 | 失败率 |
|--------|-----------|--------|
| .ps1 | 6/9 | 66.7% |
| .md | 56/928 | **6.0%** |
| .ftl | 4/59 | 6.8% |
| .rs | 180/4,136 | 4.4% |
| .ts | 28/823 | 3.4% |

Markdown 失败率最高（在常见类型中），可能因为 md 文件经常被 Agent 在同一会话中反复修改。

## 复现条件

- **复现频率**：必现（每次 Edit 工具返回错误时 `is_error` 均为 false）
- **触发步骤**：
  1. 让 Agent 在同一会话中对同一文件执行多次 Edit
  2. 第二次 Edit 使用第一次 Read 时的 old_string
  3. Edit 返回 `Error: old_string not found`，但 `is_error=false`
- **环境**：所有模型、所有操作系统

## 涉及文件

- `peri-middlewares/src/tools/filesystem/edit.rs` —— Edit 工具实现，错误返回方式为 `Ok("Error: ...")` 而非 `Err()`
- `side-projects/agent-defect-analyzer/src/analyzers/edit_errors.ts` —— Edit 专项分析器（本次新增）

## 关联 Issue

- `spec/issues/2026-06-03-edit-tool-tab-indent-mismatch.md` —— Tab 缩进匹配失败（Edit 错误的一个子类型，占 old_string_not_found 中约 8.5%）

## 状态变更记录

| 日期 | 从 | 到 | 操作人 | 说明 |
|------|-----|-----|--------|------|
| 2026-06-03 | — | Open | agent | 基于 agent-defect-analyzer 数据分析创建 |
| 2026-06-03 | Open | Pending | agent | 修复 #1 已提交，等待用户实际验证 |

## 修复记录

### 修复 #1（2026-06-03）

- **操作人**：agent
- **修复内容**：
  1. 5 处 `Ok("Error: ...")` → `Err(...into())`，`is_error` 正确标记为 true
  2. 新增 `build_not_found_hint` 模糊匹配提示（前缀匹配 + 行数近似回退）
  3. `not_unique` 错误列出匹配行号范围（超 10 处截断）
- **涉及 commit**：
  - `b2daf8ef` feat(tui): config panel description on separate line（基线，Task 1 包含在其中）
  - `1775678b` feat: Edit not found 错误增加模糊匹配提示
  - `9d79d192` feat: Edit not_unique 错误增加匹配行号定位
- **验证**：817 tests passed, 0 clippy warnings
- **状态**：待用户验证
