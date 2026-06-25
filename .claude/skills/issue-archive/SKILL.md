---
name: issue-archive
description: >
  归档已关闭/已修复的 issues，提取经验教训并同步更新 CLAUDE.md 和 spec/global。
  当用户说"归档 issue"、"archive issues"、"清理已修复的 issue"、
  "归档已关闭的 issue"、"整理一下 issues"、"把修好的 issue 归档了"时触发。
  也适用于用户想要清理 spec/issues/ 目录或将已解决问题归档的场景。
  如果 spec/issues/ 中有 Fixed/Closed/Done 状态的 issue 积压，应主动建议使用此 skill。
---

# issue-archive: Issue 归档与问题领域沉淀

将 `spec/issues/` 中已解决的 issue 归档到 `spec/archive-issues/`，从每个 issue 中提炼**问题领域认知**（而非零散 TRAP），更新到对应的 domain 文件。

## 归档条件

通过 Grep 扫描 `spec/issues/` 中所有 issue 的 `**状态**` 字段：

| 状态模式 | 归档 | 说明 |
|---------|------|------|
| `Verified` | 是 | 用户已验证通过（规范终态） |
| `Closed` | 是 | 已关闭 |
| `Fixed` | 是 | 已修复（旧 issue 兼容：`Fixed`、`Fixed + Verify`、`Fixed（待用户验证）`等变体） |
| `Done`/`已完成`/`Resolved`/`完成`/`verify` | 是 | 旧格式兼容 |
| `Open`、`Open (搁置)` | 否 | 仍需处理 |
| `Partial`、`Reopen` | 否 | 未完全解决 |

如果扫描结果为空（没有可归档的 issue），直接报告并结束。

## 工作流程

### 阶段一：扫描

1. 用 Grep 在 `spec/issues/` 中搜索 `**状态**` 行
2. 按上述规则筛选可归档 issue
3. 输出清单（标题 + 状态），直接进入归档

### 阶段二：领域识别与认知提炼

逐个 Read 可归档的 issue 文件，执行：

**步骤 1：识别所属领域**

从 issue 的标题、涉及文件、问题描述中识别属于哪个领域：
- `message-pipeline` — 消息渲染、事件处理、视图模型
- `agent` — ReAct 循环、工具系统、LLM 适配
- `tui` — TUI 渲染、交互、面板
- `mcp` — MCP 连接、工具桥接
- `storage` — 持久化、数据库
- `compact` — 上下文压缩
- `token-tracking` — Token 追踪
- `langfuse` — 可观测性
- 等等（参考 `spec/global/domains/` 现有列表）

**如果领域不存在**，创建新的 domain 文件 `spec/global/domains/<domain>.md`，使用标准模板：

```markdown
# <领域名称> 领域

## 领域综述

<一句话概括这个领域的核心职责>

## 核心流程

（后续通过 issue 归档逐步填充）

## 技术方案总结

| 维度 | 选型 |
|------|------|
（后续通过 issue 归档逐步填充）

---

## 相关 Feature
```

同时在 `spec/global/index.md` 的领域索引表中追加该领域。

**步骤 2：关键词提取**

从 issue 中提取 2-4 个**搜索关键词**，用于快速索引。关键词应选择：
- 技术术语：`HashMap 顺序`、`Prompt Cache`、`BaseMessage vs MessageViewModel`
- 错误模式：`缓存失效`、`维度混淆`、`并发竞争`
- 涉及概念：`reasoning_content`、`parking_lot::RwLock`、`RebuildAll`

**步骤 3：提炼领域级认知**

不是记录零散的 TRAP，而是提炼**领域理解**：

- **问题本质**：这类问题的根本原因是什么？（如 "HashMap 非确定性顺序导致缓存前缀不稳定"）
- **通用模式**：以后遇到类似问题应该如何思考？（如 "所有需要跨进程复用的序列化内容必须保证顺序稳定"）
- **架构影响**：这个修复对整体架构有什么启示？（如 "统一 RebuildAll 路径消除了增量更新的复杂度"）
- **技术决策**：这个 issue 背后代表了一个什么样的技术选型？
- **CLAUDE.md 链接标记**：此 issue 是否需要在 CLAUDE.md 中添加内联链接？（仅高价值 TRAP 标记 `link: true`）

**提炼模板**（写入临时文件）：

```markdown
## Issue 经验附录

### issue_<filename>

**摘要:** <issue 标题>
**状态:** Fixed/Closed/Done
**归档日期:** YYYY-MM-DD
**关键词:** <2-4 个搜索关键词，逗号分隔>
**问题本质:** <这个问题的根本原因>
**通用模式:** <以后遇到类似问题的思考模式>
**架构影响:** <对整体架构的启示，如无则省略>
**技术决策:** <代表的技术选型，如无则省略>
**涉及文件:** <从 issue 中提取的文件列表>
**CLAUDE.md 链接:** <true/false，是否在 CLAUDE.md 添加内联链接>
```

将所有 issue 的提炼结果写入临时文件 `/tmp/issue-archive-domain-learnings-<YYYYMMDD-HHMMSS>.md`，按领域分组：

```markdown
# Issue 归档领域认知提炼

归档日期：YYYY-MM-DD
归档 issue 数量：N

## message-pipeline

### issue_2026-05-12-deferred-tool-list-nondeterministic-order
**摘要:** 多处 HashMap 非确定性顺序导致 Anthropic Prompt Cache 前缀不稳定
**关键词:** HashMap 顺序, Prompt Cache, 缓存前缀
**问题本质:** HashMap 迭代顺序不确定（Rust 默认 RandomState），跨进程重启时 API 请求前缀变化
**通用模式:** 所有需要跨进程复用的序列化内容（system prompt、tools 数组）必须保证顺序稳定
**技术决策:** 工具列表按名称排序；ToolSearchIndex 会话级缓存
**涉及文件:** peri-middlewares/src/tool_search/tool_index.rs, peri-agent/src/agent/executor/mod.rs
**CLAUDE.md 链接:** true

## agent

...

## tui

...
```

**提炼原则**：
- 只提炼有**领域级价值**的认知，不记录一次性修复细节
- 如果某个 issue 只是纯 UI 小 bug（如错位、样式），在临时文件中标注"无可提炼认知"，不添加关键词
- 关键词选择用户可能搜索的术语，而非内部实现细节

### 阶段三：文件归档

1. 用 Bash 创建 `spec/archive-issues/` 目录（如不存在）
2. 用 Bash `mv` 将每个可归档 issue 移动到 `spec/archive-issues/`
3. 移动后用 Edit 在每个归档文件顶部（`#` 标题之前）插入归档标记：

```
> 归档于 YYYY-MM-DD，原路径 spec/issues/<filename>
```

### 阶段四：更新 Domain 文件

使用 Agent tool 更新 domain 文件：

**Agent 任务描述**：

```
读取临时经验文件 /tmp/issue-archive-domain-learnings-<timestamp>.md

对每个领域：
1. Read 对应的 domain 文件（spec/global/domains/<domain>.md）
2. **先 Grep 检查是否已存在 `### issue_<filename>` 标题**，存在则跳过（去重）
3. 在文件末尾的「Issue 经验附录」段追加该领域的所有 issue 提炼（仅追加不存在的）
4. 如果 domain 文件不存在「Issue 经验附录」段，在「相关 Feature」之前插入该段标题
5. 如果某个 issue 标注"无可提炼认知"，跳过
6. 更新文件末尾的「相关 Feature」引用，如有需要

格式要求：
- 每个 issue 使用三级标题 ### issue_<filename>
- 字段：摘要、状态、归档日期、关键词、问题本质、通用模式、架构影响（可选）、技术决策（可选）、涉及文件、CLAUDE.md 链接
- 保持与现有 Feature 附录的格式一致
```

### 阶段五：更新全局问题索引

使用 Agent tool 更新 `spec/global/problems.md`：

**如果文件不存在**，创建标准模板：

```markdown
# 问题索引

按关键词索引已归档 issue，遇到相似问题时快速定位历史经验。

## 关键词索引

（后续按关键词分组添加）

## 更新记录

- YYYY-MM-DD: 首次创建，归档 N 个 issue
```

**更新逻辑**：

```
读取临时经验文件 /tmp/issue-archive-domain-learnings-<timestamp>.md

对每个 issue：
1. 提取「关键词」字段（逗号分隔）
2. 对每个关键词：
   - 在「关键词索引」段查找或创建该关键词的三级标题
   - **先 Grep 检查是否已存在对该 issue 的引用（`issue_<filename>`）**，存在则跳过
   - 追加条目：`- [<摘要>](domains/<domain>.md#issue_<filename>) — <domain>`
3. 在「更新记录」段追加本次归档记录
```

**索引格式示例**：

```markdown
## 关键词索引

### HashMap 顺序
- [多处 HashMap 非确定性顺序导致 Anthropic Prompt Cache 前缀不稳定](domains/message-pipeline.md#issue_2026-05-12-deferred-tool-list-nondeterministic-order) — message-pipeline

### Prompt Cache
- [多处 HashMap 非确定性顺序导致 Anthropic Prompt Cache 前缀不稳定](domains/message-pipeline.md#issue_2026-05-12-deferred-tool-list-nondeterministic-order) — message-pipeline
- [Skill Preload 注入消息到历史最前面导致首轮 Prompt Cache 失效](domains/message-pipeline.md#issue_2026-05-12-skill-preload-invalidates-prompt-cache) — message-pipeline

### BaseMessage vs MessageViewModel
- [BaseMessage 与 MessageViewModel 维度混淆](domains/message-pipeline.md) — message-pipeline（CLAUDE.md 开发注意事项）
```

### 阶段六：更新 CLAUDE.md 内联链接

使用 Agent tool 更新 `CLAUDE.md`：

**更新逻辑**：

```
读取临时经验文件 /tmp/issue-archive-domain-learnings-<timestamp>.md

对每个「CLAUDE.md 链接: true」的 issue：
1. Read 当前 CLAUDE.md
2. **先 Grep 检查 CLAUDE.md 是否已包含 `#issue_<filename>` 链接**，存在则跳过
3. 在「开发注意事项」段查找相关的 TRAP 或注意事项
4. 在对应条目末尾追加内联链接：`（详见 spec/global/domains/<domain>.md#issue_<filename>）`
5. 如果找不到相关条目，在「开发注意事项」段末尾追加新条目
```

**格式示例**：

```markdown
## 开发注意事项

- **BaseMessage 与 MessageViewModel 维度混淆 [TRAP]**：`prefix_len` 应使用 `round_start_vm_idx`（VM 维度）...（详见 spec/global/domains/message-pipeline.md#issue_2026-05-12-base-message-vm-dimension-confusion）
- **HashMap 顺序问题 [TRAP]**：所有需要跨进程复用的序列化内容必须保证顺序稳定（详见 spec/global/domains/message-pipeline.md#issue_2026-05-12-deferred-tool-list-nondeterministic-order）
```

### 阶段七：清理与报告

1. 删除临时经验文件（`rm /tmp/issue-archive-domain-learnings-<timestamp>.md`）
2. 输出归档报告：

```
✅ Issue 归档完成

归档数量：N 个
归档位置：spec/archive-issues/
  - issue-1.md
  - issue-2.md
  - ...

Domain 更新：
  - message-pipeline: 2 条新增
  - agent: 3 条新增
  - tui: 1 条新增
  - mcp: 无新增

问题索引更新：
  - 新增关键词：HashMap 顺序, Prompt Cache, BaseMessage vs MessageViewModel
  - 更新条目：5 个

CLAUDE.md 链接：
  - 新增内联链接：2 条
```

## 示例

用户输入：`/issue-archive`

Agent 执行：
1. 扫描发现 18 个 Fixed/Closed issue
2. 逐个读取，识别领域（如 message-pipeline、agent、tui 等）
3. 提炼领域级认知，写入临时文件（按领域分组）
4. 移动 18 个文件到 `spec/archive-issues/`
5. 派出 Agent 更新各 domain 文件的「Issue 经验附录」段
6. 清理临时文件，输出报告
