# dsh/pi 内核工具参数名与上游 harness 不一致，导致模型按上游习惯调用时硬失败；read.offset 语义相反导致静默错行；upstream 归因头标注了不存在的分支与早于仓库创建的日期

**状态**：Fixed
**优先级**：中（`read.offset` 为静默数据错位，是本单最高危项；其余为模型跨内核习惯导致的显式报错，可恢复）
**创建日期**：2026-11-10
**工单号**：GitHub Issue [#1006](https://github.com/cc-claws/cc-code/issues/1006)（`cc-claws/cc-code`，标签 `bug` / `priority:medium`）
**后续**：按仓库约定，本文件是详细分析补充，工单号唯一来源为 GitHub Issue #1006。
**豁免声明**：本文为上游实现的逐条参数对照分析，需保留源码路径与符号引用（hook:allow-foreign-source）。

## 一、问题描述

dsh 内核（`agent_kernel: "dsh"`）的定位是「用上游 dsh 的方言呈现 peri 的工具」，其提示词明确要求：

> **Always use the exact names and parameter names listed below; other names are not available.**
> —— `cc-agent/src/kernel/prompts/dsh.md:4`

但 dsh 内核实际暴露给模型的**工具名与参数名并不等于上游 dsh 的真实 schema**。工具名（`read`/`write`/`edit`/`glob`/`grep`/`bash`/`todo_write`/`web_fetch`/`subagent`/`ask_user_question`/`skill`）已全部对齐，**参数名有 3 处不一致**：

| 工具 | 上游 dsh | peri dsh 内核 | 后果 |
|------|-----------|---------------|------|
| `skill` | `name`（必填） | `skill`（必填）+ `args` | **硬失败**：模型传 `name` 被拒 |
| `bash` | `timeoutMs` | `timeout` | **硬失败**：模型传 `timeoutMs` 被拒 |
| `bash` | `workdir` | 无 | **硬失败**：模型传 `workdir` 被拒 |
| `ask_user_question` | `multi_select` | `multiSelect` | **硬失败**：模型传 `multi_select` 被拒（嵌套字段） |

另有 `read.offset` 的**语义**与上游相反（上游 1-based first line，peri 0-based skip），模型按上游习惯传 `offset=1` 会**静默少读第 1 行**——不报错，最难发现。

以及 `upstream:` 归因头标注了**上游不存在的分支**（`@ main`）和**早于上游仓库创建时间的日期**（`checked 2026-06-01`，上游仓库 2026-08-13 才创建），会误导后续维护者。

## 二、事实基线（上游版本已核对）

| 项 | 值 |
|---|---|
| 上游仓库 | `deepseek-ai/deepseek-harness` |
| 分支 | **仅 `master`**（`main` → HTTP 404） |
| 最新 commit | `fcc0ad462172522aaf5c93a7b2e01f6b9f0b0159`（2026-11-04） |
| 最新 release | `dsh-v0.2.2-rc.1`（2026-11-04） |
| `package.json` version | `0.2.2-rc.1` |
| 仓库创建时间 | **2026-08-13** |
| 首个 release | `dsh-v0.1.0-rc.7`（2026-08-17），共 37 个 release |

**已核对 master 与 tag `dsh-v0.2.2-rc.1` 的 5 个关键工具实现**（read / edit / grep / skill / bash）：4 个完全相同，1 个仅工具描述文案微调（read 工具描述第 74 行，"not shell commands like cat, sed, head, or tail" → "like cat"），**参数 schema 无任何差异**。故本文结论对最新 release 成立。

**上游相关 schema 的历史稳定性**（逐版本核对上游 fs 工具包与 skill 工具包的实现文件）：

| 上游版本 | `grep` 参数 | `skill` 参数 | `read.offset` |
|---------|------------|-------------|--------------|
| `v0.1.1-rc.1` | `pattern`/`path`/`include` | `name` | 1-based first line |
| `v0.2.0-rc.1` | `pattern`/`path`/`include` | `name` | 1-based first line |
| `v0.2.1-rc.1` | `pattern`/`path`/`include` | `name` | 1-based first line |
| `v0.2.2-rc.1`（最新） | `pattern`/`path`/`include` | `name` | 1-based first line |

**这三个关键 schema 自 v0.1.1 起从未变更。** 因此 peri 的 dsh 内核**不可能对应任何更早的上游版本**——它描述的就是当前最新的 schema；差异来自 peri 侧的实现选择，不是「版本落后」。

## 三、症状详情

### 现象 1：`skill` 硬失败（上游参数名 `name`）

模型按上游 dsh 习惯调用 `skill({"name": "pdf"})`，在 peri dsh 内核下被 Strict 校验拒绝：

```
The required parameter 'skill' is missing (expected string);
Unexpected parameter 'name' was provided (allowed parameters: ['args', 'skill'])
```

上游 `skill` 工具定义（`packages/skill/tool-skill/src/index.ts:85`）：

```ts
parameters: {
  name: { type: 'string', required: true, description: 'The exact skill name from the available skills list.' },
}
```

peri 侧（`cc-middlewares/src/tools/skill.rs:145-158`）为 `skill`（必填）+ `args`（可选），且 `SkillToolInput.skill` 是**必填无默认值**（`cc-middlewares/src/tools/skill.rs:25-29`）。

### 现象 2：`bash` 硬失败（上游 `timeoutMs` / `workdir`）

模型按上游习惯调用 `bash({"description": "...", "command": "ls", "timeoutMs": 5000})`：

```
Unexpected parameter 'timeoutMs' was provided
(allowed parameters: ['command', 'description', 'execution_timeout', 'run_in_background', 'timeout'])
```

`bash({"command": "ls", "workdir": "/tmp"})` 同样被拒：

```
Unexpected parameter 'workdir' was provided
```

上游 `bash` 参数（`packages/shell/tool-bash/src/index.ts:369`）：`description`（**必填**）、`command`（必填）、`timeoutMs`、`workdir`。

peri `bash` 参数（`cc-tools/src/bash/mod.rs:211-236`）：`command`（必填）、`timeout`、`execution_timeout`、`description`（可选）、`run_in_background`。

### 现象 3：`ask_user_question` 硬失败（上游 `multi_select`）

模型按上游习惯调用 `ask_user_question({"questions": [{"id": "q1", "question": "...", "header": "...", "options": [{"label": "A"}], "multi_select": false}]})`：

```
Invalid nested arguments at 'questions[0]':
  Unexpected parameter 'id' was provided (allowed parameters: ['header', 'multiSelect', 'options', 'question']);
  Unexpected parameter 'multi_select' was provided (allowed parameters: ['header', 'multiSelect', 'options', 'question'])
```

上游 `ask_user_question`（`packages/interaction/tool-ask-user/src/index.ts`）的 `questions[]` 项字段为 `id`（**必填**）、`question`（必填）、`header`（可选）、`options`（可选）、`multi_select`（可选）。

peri 的 `InputQuestion`（`cc-middlewares/src/ask_user/mod.rs:17-24`）为 `question`、`header`（**必填**）、`multiSelect`（camelCase）、`options`（**必填**），**无 `id`**。

注意 `multiSelect` 是 camelCase，而 dsh 内核整体是 snake_case 方言——**该字段破坏了 dsh 内核自身的方言一致性**。

### 现象 4：`read.offset` 静默错行（**本单最高危项**）

| | 上游 dsh | peri |
|---|---|---|
| 描述 | `1-based first line to return. Defaults to 1.` | `Number of text lines to skip, 0-based (default 0)` |
| 实现 | `startLine = offset ? Math.max(0, offset - 1) : 0` | `let start = offset;`（`cc-tools/src/tools/filesystem/read.rs:162,222`） |

模型按上游语义传 `offset=1`（意图读第 1 行）：

```
文件内容:  L1 L2 L3 L4 L5
peri   实际读到: ['L2','L3','L4']   ← 少读第 1 行
上游 实际读到: ['L1','L2','L3']
```

**不报错、不告警**，模型拿到的是从第 2 行开始的内容。

### 现象 5：`upstream:` 归因头事实错误（**仅 dsh 一处**）

`cc-agent/src/kernel/prompts/dsh.md:1` 与 `cc-agent/src/kernel/spec/dsh.rs:1`：

```
<!-- peri adaptation summary (NOT a verbatim upstream snapshot); upstream: deepseek-harness @ main, checked 2026-06-01 -->
```

```rust
// <!-- upstream: deepseek-harness (dsh) @ main, checked 2026-06-01 -->
```

两处错误：

1. **`@ main` 分支不存在**。上游仅有 `master` 分支，`/branches/main` 返回 HTTP 404。对照：pi 的上游 `earendil-works/pi` 的 default_branch **确实是** `main`——此行系照 pi 抄写而未核实 dsh。
2. **`checked 2026-06-01` 早于上游仓库创建时间（2026-08-13）**。上游 2026-06-02 之前无任何 commit。

git 追溯：`e9737be3`（2026-11-05，引入 dsh 内核）时 `dsh.md` 首行为**空**，无 upstream 标注；`aed9b8a0`（同日）补上该行，提交说明写的是「标注为 peri 适配摘要而非上游逐字快照」——本意是免责声明，但填入的分支与日期均不属实。

`cc-agent/src/kernel/spec/cc.rs:1` 同样标注 `checked 2026-06-01`（cc 为 peri 自制，`peri 内置呈现` 的措辞尚可，日期仍属杜撰）。

### 现象 6：pi 内核的同类问题（上游 `earendil-works/pi`）

pi 内核的偏差类型与 dsh **不同**：dsh 是「参数名对不上 + offset 基准相反」，pi 是「参数名对不上 + **timeout 单位相反**」。

上游 pi 的工具参数（`packages/coding-agent/src/core/tools/*.ts`，取 2026-06-01 commit `8216cca5a7` 核对，与当前 `main` 一致）：

| 工具 | 上游 pi 参数 | peri pi 内核暴露 | 差异 |
|------|-------------|-----------------|------|
| `read` | `path`/`offset`/`limit` | `path`/`offset`/`limit`/`pages` | peri 多 `pages` |
| `write` | `path`/`content` | `path`/`content`/`append` | peri 多 `append` |
| `edit` | `path`/`edits[]{oldText,newText}` | `path`/`edits[]{oldText,newText}` | **完全一致** ✅ |
| `find` | `pattern`/`path`/**`limit`** | `pattern`/`path` | **缺 `limit`** ❌ |
| `grep` | `pattern`/`path`/`glob`/**`ignoreCase`**/**`literal`**/`context`/**`limit`** | `pattern`/`path`/`glob` + peri 自有 14 个 | **缺 3 个** ❌ |
| `bash` | `command`/**`timeout`（秒）** | `command`/`timeout`（**毫秒**）/`execution_timeout`/`description`/`run_in_background` | **单位相反** ❌ |

**pi 的 `edit` 是正确的**——pi 用 `MutationForm::BatchReplace`（`spec/pi.rs:25`），经 `RENAME_EDIT` 出站为 `path` + `edits[]{oldText,newText}`，与上游逐字一致。（对照 dsh/cc 用 `SingleReplace`，暴露 `file_path` + `old_string`/`new_string`。）

#### pi 现象 6-1：`find` 缺 `limit`、`grep` 缺 `ignoreCase`/`literal`/`limit` → 硬失败

```
[FAIL] find({"pattern": "**/*.ts", "limit": 100})
       Unexpected parameter 'limit' was provided (allowed parameters: ['path', 'pattern'])

[FAIL] grep({"pattern": "foo", "glob": "*.ts", "ignoreCase": true, "literal": false, "context": 2, "limit": 50})
       Unexpected parameter 'ignoreCase' was provided (...);
       Unexpected parameter 'literal' was provided (...);
       Unexpected parameter 'limit' was provided (...)
```

peri 的 `grep` 自有参数 `-i`（忽略大小写）、`fixed_strings`（字面量）、`head_limit`（结果上限）与上游 pi 的 `ignoreCase`/`literal`/`limit` **功能等价但命名不同**——模型按上游习惯传全部落空。

#### pi 现象 6-2：`bash.timeout` 单位相反 → **静默误杀**（pi 侧最高危项）

| | 上游 pi | peri |
|---|---|---|
| 单位 | **秒**（`pi/bash.ts:41` "Timeout in seconds"；`resolveTimeoutMs` 内 `timeout * 1000`） | **毫秒**（`cc-tools/src/bash/mod.rs:219` "Optional timeout in milliseconds"） |

模型按 pi 习惯传 `timeout=30`（意图 30 秒）→ peri 实际按 30 毫秒执行：

```
timeout=30  → 意图 30 秒（30000 ms）→ 实际 30 ms，命令在 0.03 秒后被杀（早 1000 倍）
timeout=60  → 意图 60 秒           → 实际 60 ms
timeout=300 → 意图 300 秒          → 实际 300 ms
```

**不报错**——schema 校验通过（类型都是 number），但命令被提前 1000 倍杀掉，模型只会看到「命令超时」的表象，**不会知道单位错了**。这是本单危害最大的一项：dsh 的 `read.offset` 少读 1 行，pi 的 `bash.timeout` 直接让命令执行失败。

#### pi 现象 6-3：pi 的 `read.offset` 已正确处理（对照组）

上游 pi `read.ts:16` 确为 `Line number to start reading from (1-indexed)`，`read.ts:297` 为 `offset ? Math.max(0, offset - 1) : 0`——**1-indexed**。

peri pi 内核**已在描述中显式标注差异**（`spec/pi.rs:21-23`）：

```rust
// 注意：offset 沿用 peri 语义（从 0 开始跳过的行数），与上游 pi 的 1-indexed 行号不同；
// 已在 description 中显式说明，避免模型按上游文档产生 off-by-one。
tool("Read", "read", "Read a file. Use path (not file_path). offset is 0-based: number of lines to skip.", ...)
```

**这证明作者知道该怎么处理这类问题——dsh 内核只是漏做了。** 且 pi 内核的 `bash.timeout` 单位问题**同样漏做了**（`spec/pi.rs:28` 的 Bash 描述为 `"Execute a shell command."`，无任何单位说明）。

#### pi 现象 6-4：pi 的 `upstream:` 归因头**属实**（与 dsh 形成对照）

```
// <!-- upstream: earendil-works/pi @ main, checked 2026-06-01 -->
```

核实结果：

- `earendil-works/pi` 的 default_branch **确为 `main`** ✅
- 仓库创建于 2025-08-09，**2026-06-01 当天有 commit**（`8216cca5a7` / `e56521e323`）✅
- 该 commit 的 `read`/`find`/`grep`/`bash` 四个工具 schema 与当前 `main` **逐字一致** ✅

**故 pi 的归因头是真实核对记录，问题仅在 dsh 一处。** 这也反证了 dsh 那行是「照 pi 抄格式、未核实 dsh」。

### 现象 7：pi 内核未暴露上游 pi 的 `skill` 工具（**非缺陷**）

上游 pi 在 `packages/coding-agent/src/core/tools/` 下**没有 skill 工具**（已核实 2026-06-01 与当前 `main` 的工具清单）。peri pi 内核保留 `tool("Skill", "skill", ...)` 并附注释「Skills are user-configured shared capabilities, independent of the kernel.」（`spec/pi.rs:29-30`），符合 `docs/acp/agent-kernel-switch.md` 记录的「三种内核均保留 Skill」决策。**非偏差，不改。**

## 四、复现条件

- **复现频率**：现象 1/2/3/6-1 为**必现**（模型按上游习惯调用即触发）；现象 4/6-2 为**必现且静默**；现象 5 为静态事实错误。
- **前置条件**：`agent_kernel` 配置为 `dsh` 或 `pi`（`~/.cc-code/settings.json` 的 `config.agent_kernel`）。
- **触发步骤**：
  1. 切到 dsh 内核；
  2. 让模型调用 `skill`（传 `name`）、`bash`（传 `timeoutMs` 或 `workdir`）、`ask_user_question`（传 `multi_select` 或 `id`）→ 均被 Strict 校验拒绝；
  3. 让模型 `read` 一个多行文件并显式传 `offset=1` → 内容从第 2 行开始，**无任何报错**。
  4. 切到 pi 内核，令模型调用 `find`（传 `limit`）、`grep`（传 `ignoreCase`/`literal`/`limit`）→ 均被拒绝；
  5. pi 内核下令模型执行 `bash({"command":"...","timeout":30})` → 命令在 **0.03 秒**后被杀（模型意图 30 秒），**无任何报错**。
- **环境**：peri 本仓库；Windows；dsh / pi 内核会话。

## 五、失败机制

`cc-agent/src/tools/schema.rs:10-15` —— 内置工具走 `ValidationMode::Strict`：

```rust
pub fn validate_against_schema(input: &Value, schema: &Value) -> Result<(), String> {
    validate_arguments(input, schema, ValidationMode::Strict)
}
```

`schema.rs:256-268`：Strict 模式下**未声明在 `properties` 中的键直接报错**，不是忽略。

**自动改道救不回来**：`cc-agent/src/agent/executor/tool_dispatch.rs:221` 的 `AUTO_RESCUE_WHITELIST = ["Grep", "Glob", "Read"]`，改道条件是「参数 100% 符合唯一白名单工具」。`skill({"name":"pdf"})`、`bash({"command":"ls","timeoutMs":5000})`、`ask_user_question(...)` 均不符合任何白名单工具 → **无法改道**，直接返回参数错误。

## 六、已验证的事实

1. **全量参数表比对**：提取上游 11 个工具的完整 `parameters`（含嵌套 `required`）与 peri 侧对应 `json!` schema 做机械 diff，结果见第一节表格。`glob` 与 `edit` 完全一致（peri `edit` 按 `MutationForm` 选 schema，dsh 用 `SingleReplace`，与上游逐字相同）；`grep` 的 `include` 已由 `RENAME_GREP` 对齐上游。
2. **失败场景实测**：将 `cc-agent/src/tools/schema.rs` 的校验逻辑逐行移植为等价实现，对 11 个「模型按上游 schema 调用」场景跑真实校验：**4 个硬失败**（`bash`×2、`skill`、`ask_user_question`），7 个通过。`read` 通过校验但语义错位（现象 4 独立成立）。
3. **`read.offset` 错位实测**：按 peri 的 `start = offset` 与上游的 `startLine = max(0, offset-1)` 分别求值，`offset=1` 时两者相差整 1 行（现象 4 数据来源）。
4. **`grep` 的 `include` 对齐已核实**：`spec/dsh.rs:8` `RENAME_GREP = ParamSpec::Rename(&[("include", "glob")])`，方向为 dialect→canonical，出入站双向生效，与上游 dsh 的 `include` 一致。
5. **pi 内核的同类问题已正确处理，可作模板**：`spec/pi.rs:21-23` 注释写明「offset 沿用 peri 语义（从 0 开始跳过的行数），与上游 pi 的 1-indexed 行号不同；已在 description 中显式说明，避免模型按上游文档产生 off-by-one」，并在 Read 的 description 中落实。已核对上游 pi（`earendil-works/pi`）`packages/coding-agent/src/core/tools/read.ts:16` 确为 `Line number to start reading from (1-indexed)`，`:297` 为 `offset ? Math.max(0, offset - 1) : 0`——**该注释准确，dsh 内核缺少同样处理**。
6. **`workdir` 属能力缺口而非改名问题**：peri `BashTool` 的工作目录在构造期固定（`cc-tools/src/bash/mod.rs:70` `pub cwd: String`、`:82` `new(cwd)`、`:87` `with_executor(cwd, ...)`），无 per-call 覆盖路径。
7. **上游 `schedule_create` 用 RRULE 而非五段 cron 的既有结论核实无误**：`packages/schedule/tool-schedule/src/index.ts` 的 `schedule_create` 参数为 `title`/`prompt`/`rrule`/`target`，`rrule` 为 RFC 5545 语义；peri 的 `CronScheduler` 只接受五段 cron，`spec/dsh.rs:109-115` 按「能力缺口不映射」处理、Cron 工具保持规范名直接可见，**该处判断正确，本单不涉及**。
8. **dsh 内核参数名无回归保护**：`cc-acp/src/kernel_acceptance_test.rs:140-160` 的 `test_dsh_tool_names` 只断言 11 个**工具名**，**未断言任何参数名**——这是本类偏差能长期存在的原因。
9. **pi 内核参数名同样无回归保护**：现有验收测试未断言 pi 的参数名（`cc-acp/src/kernel_acceptance_test.rs` 内 pi 相关用例仅覆盖 `should_hide`、`dialect_name`）。
10. **pi 的 `edit` 与上游逐字一致**：pi 用 `MutationForm::BatchReplace`（`spec/pi.rs:25`），经 `RENAME_EDIT` 出站为 `path` + `edits[]{oldText,newText}`；用真实 batch schema 跑校验通过。（初版脚本误用 `single_schema` 得到 FAIL，已纠正。）
11. **pi `bash.timeout` 单位差 1000 倍**：上游 `pi/bash.ts:26-36` 的 `resolveTimeoutMs` 做 `timeout * 1000`（秒→毫秒）；peri `cc-tools/src/bash/mod.rs:248-250` 直接把 `input["timeout"]` 当毫秒用。两侧 schema 均为 `number`，**Strict 校验无法察觉**。
12. **pi 的 `upstream:` 归因头经核实属实**：`earendil-works/pi` default_branch 确为 `main`；2026-06-01 当天有 commit（`8216cca5a7`）；该 commit 的 `read`/`find`/`grep`/`bash` schema 与当前 `main` 逐字一致。
13. **上游 pi 无 skill 工具**：已核实 2026-06-01 与当前 `main` 的 `packages/coding-agent/src/core/tools/` 清单，均无 skill 工具；peri pi 内核保留 Skill 属既有设计决策（`docs/acp/agent-kernel-switch.md`），非偏差。

## 七、涉及文件

- `cc-agent/src/kernel/spec/dsh.rs` —— dsh 内核的 `ToolSpec` 定义（工具名/参数改名/描述）。第 1 行为错误的 upstream 归因头；`skill`/`bash`/`ask_user_question`/`read` 四条 `tool(...)` 需调整。
- `cc-agent/src/kernel/spec/pi.rs` —— pi 内核的 `ToolSpec` 定义。`:28` 的 `bash` 描述未标注 timeout 单位；`find`/`grep` 两条需补上游参数名；`:21-23` 是 offset 语义标注的**正确模板**。
- `cc-agent/src/kernel/prompts/pi.md` —— pi 系统提示词；工具清单需与补参后的 schema 同步。
- `cc-agent/src/kernel/prompts/dsh.md` —— dsh 系统提示词。第 1 行为错误的 upstream 归因头；第 8 行 `read` 的参数说明未标注 offset 语义；工具清单需与改名后的 schema 同步。
- `cc-agent/src/kernel/spec/pi.rs:21-23` —— pi 内核处理同类问题的**正确模板**（显式在 description 中记录 offset 语义差异）。
- `cc-agent/src/kernel/mod.rs:74-85` —— `ParamSpec` 定义（`Rename` / `RenameWithArrayFields`），方向统一为 dialect→canonical。
- `cc-agent/src/kernel/mod.rs:410-427`、`:307-322` —— `normalize_input`（入站归一化）与 `translate_schema`（出站 schema 改名），改名双向生效点。
- `cc-agent/src/tools/schema.rs:10-15`、`:256-268` —— Strict 校验，未知参数即报错。
- `cc-agent/src/agent/executor/tool_dispatch.rs:221`、`:245-300` —— 自动改道白名单与唯一匹配判定。
- `cc-middlewares/src/tools/skill.rs:25-29`、`:145-158` —— `SkillToolInput` 与 `skill` 工具 schema（`skill` 必填）。
- `cc-tools/src/bash/mod.rs:70`、`:82`、`:87`、`:211-236`、`:248-250` —— BashTool 的固定 cwd、`bash` schema（毫秒单位）、以及 `timeout` 直接当毫秒使用的读取点（pi 单位偏差的另一半）。
- `cc-tools/src/tools/filesystem/read.rs:61`、`:137-141`、`:162`、`:222` —— `read` 的 0-based offset 语义（描述、schema、实现三处）。
- `cc-tools/src/tools/filesystem/glob.rs`、`grep.rs` —— `find`/`grep` 对应的共享工具实现；`grep` 已有 `-i`/`fixed_strings`/`head_limit`，与上游 pi 的 `ignoreCase`/`literal`/`limit` 功能等价但命名不同。
- `cc-middlewares/src/ask_user/mod.rs:17-24`、`:94`、`:117`、`:124`、`:128` —— `InputQuestion`（`multiSelect` camelCase、无 `id`）与 `ask_user_question` schema。
- `cc-acp/src/kernel_acceptance_test.rs:140-160` —— `test_dsh_tool_names`，缺参数名断言；pi 侧亦无参数名断言。

## 八、解决建议

### 建议修复（内核层完成，不触碰共享工具实现，cc 内核零影响）

1. **修正 upstream 归因头**（`prompts/dsh.md:1`、`spec/dsh.rs:1`）：

   ```
   <!-- peri adaptation summary (NOT a verbatim upstream snapshot); upstream: deepseek-harness @ master (dsh-v0.2.2-rc.1), checked 2026-11-10 -->
   ```

2. **`skill` 参数改名**（`spec/dsh.rs`）：

   ```rust
   /// 上游 dsh 的 skill 工具参数名为 `name`；peri 内部为 `skill`。
   const RENAME_SKILL: ParamSpec = ParamSpec::Rename(&[("name", "skill")]);
   ```

   挂到 `tool("Skill", "skill", ..., RENAME_SKILL, &[], None)`。`rename_keys` 方向为 dialect→canonical，出站 schema 自动显示 `name`、入站自动还原为 `skill`，**工具实现零改动**。peri 多出的 `args` 保留（上游模型不会传）。

3. **`bash` 超时参数改名**（`spec/dsh.rs`）：

   ```rust
   const RENAME_BASH: ParamSpec = ParamSpec::Rename(&[("timeoutMs", "timeout")]);
   ```

   语义自动对齐：上游 `timeoutMs` 与 peri `timeout` 的描述在「超时转后台」上完全一致（`cc-tools/src/bash/mod.rs:217`）。

4. **`ask_user_question` 嵌套字段改名**（`spec/dsh.rs`）：

   ```rust
   const RENAME_ASK_USER: ParamSpec = ParamSpec::RenameWithArrayFields {
       top: &[],
       array_key: "questions",
       fields: &[("multi_select", "multiSelect")],
   };
   ```

   `ParamSpec::RenameWithArrayFields` 已支持嵌套数组字段改名（pi 的 Edit 即用此变体），`normalize_input` 与 `translate_schema` 均有对应分支。同时修正 dsh 内核的方言一致性（camelCase → snake_case）。

5. **`read` 描述补 offset 语义**（`spec/dsh.rs`，照 `spec/pi.rs:23` 先例）：

   ```rust
   tool("Read", "read",
       "Read a file from the local filesystem. Use read before edit. offset is 0-based: number of lines to skip.",
       "read: read a file", RENAME_NONE, &[], None),
   ```

   **不改 `read.rs` 的共享实现**：`read.rs:61` 的工具描述与 TUI 提示均已写死 0-based，且 cc 内核是逐字节等价回归基线，改动面远大于收益。

6. **同步 `prompts/dsh.md`**：改名后 schema 显示 `name`/`timeoutMs`/`multi_select`，提示词的参数清单须一致，避免「提示词说 A、schema 说 B」。

7. **补参数名回归断言**（`cc-acp/src/kernel_acceptance_test.rs`）：`test_dsh_tool_names` 现仅断言工具名，需增加 `dsh().dialect_name` 之外的参数改名往返断言，并补 `spec/dsh.rs` 单元测试。

### 建议修复（pi 内核侧）

8. **`find` 补 `limit`、`grep` 补 `limit`**（`cc-tools/src/tools/filesystem/{glob,grep}.rs` → 内核层暴露）：
   上游 pi 的 `find`/`grep`/`read` 共用 `limit` 语义（结果/行数上限）。peri 的 `glob` 无 `limit`、`grep` 只有 `head_limit`。**注意**：这两个参数位于**共享工具实现**，直接加会同时改变 cc 内核的 schema（破坏逐字节等价基线）——需评估是走 `ParamSpec` 改名（把 `head_limit` 暴露为 `limit`，零共享改动）还是新增参数。

9. **`grep` 的 `ignoreCase`/`literal` 命名对齐**（`spec/pi.rs`）：
   peri 已有功能等价的 `-i`/`fixed_strings`，可用 `ParamSpec::Rename` 在 pi 内核下暴露为上游名：

   ```rust
   /// pi 内核下用上游命名暴露 grep 的大小写/字面量开关。
   const RENAME_GREP_PI: ParamSpec = ParamSpec::Rename(&[
       ("ignoreCase", "-i"),
       ("literal", "fixed_strings"),
       ("limit", "head_limit"),
   ]);
   ```

   纯内核层改名，**共享实现零改动**、cc 内核不受影响。是否采用取决于对「peri 自有参数名」的取舍。

10. **`bash.timeout` 单位差异需在描述中显式标注**（`spec/pi.rs:28`，照 `spec/pi.rs:21-23` 的 offset 先例）：

    ```rust
    tool("Bash", "bash",
        "Execute a shell command. timeout is in MILLISECONDS (upstream pi uses seconds).",
        "bash: run a command", RENAME_NONE, &[HINT_BASH], None),
    ```

    **不改 `bash` 的共享实现**（毫秒语义已被 cc 内核、TUI 提示与既有测试依赖）。同类处理与 pi 内核 `read.offset` 一致：在 description 中显式说明。**这一条建议与 P0 同级**——单位错配是静默误杀，比参数名被拒更隐蔽。

11. **同步 `prompts/pi.md`** 的工具清单，与补参后的 schema 一致。

### 建议不修复（附理由）

- **`bash.workdir`**：属**能力缺口**而非改名问题。peri `BashTool` 工作目录在构造期固定，无 per-call 覆盖路径，需改 Bash 工具实现（跨层）。且上游 `workdir` 非必填（`required: ["description","command"]`），模型不传即不触发。
- **`ask_user_question` 的 `id`**：上游必填、peri 无此字段（`InputQuestion` 无 `id`，服务端生成 `ask_user_question_{i}`）。支持需改 `InputQuestion` + serde，并改变 peri 的交互契约。
- **`ask_user_question` 的 `header`/`options` 必填性**：上游可选、peri 必填，方向对 peri 不利，但模型以 schema 为准时不会触发。
- **`grep` 多出的 10 个参数、`todo_write.activeForm`、`write.append`、`web_fetch.prompt`**：均为 peri 的**能力增强**（上游把这些固定在配置层 `caps.maxMatches`/`caps.maxLineBytes`，模型不可调）。peri 有专门的 `spec/features/20260514_F002_grep-capability-enhancement/`。删除会直接损失能力，且上游模型不会传这些键。
- **`subagent` 参数集**：上游用 `agent_preset`/`provider`/`model`，peri 用 `subagent_type` + `action: spawn|stop` + `fork`。上游为 subagent 提供了多个独立预设实现（分属不同的编码代理与 SDK 形态），说明 subagent 在上游高度可定制，peri 形态不构成「错」。
- **`bash.description` 必填性**：上游必填、peri 可选，peri 更宽松，无失败风险。
- **pi 的 `read.pages` / `write.append` / `bash.execution_timeout` 等多余参数**：同 dsh 侧，均为 peri 的能力增强，上游模型不会传。
- **pi 的 `grep` 多出 14 个参数**：同 dsh 侧 `grep`，是超集能力，不删。

## 九、状态变更记录

| 日期 | 从 | 到 | 操作人 | 说明 |
|------|-----|-----|--------|------|
| 2026-11-10 | — | Open | agent | 创建（上游 dsh v0.2.2-rc.1 全量参数表比对 + 移植 peri Strict 校验器实测 11 场景，4 硬失败） |
| 2026-11-10 | Open | Open | agent | 扩展覆盖 pi 内核：核对上游 `earendil-works/pi`（2026-06-01 commit `8216cca5a7` 与当前 `main`）全部工具 schema，实测 6 场景得 2 硬失败；发现 `bash.timeout` 单位相反（秒 vs 毫秒）为 pi 侧最高危项；确认 pi 的 `upstream:` 归因头属实、`edit` 与上游逐字一致 |
| 2026-11-11 | Open | Fixed | agent | 核对 `69dc0f74`（fix(kernel): 对齐 dsh/pi 工具参数并明确执行语义，2026-11-10 21:51:40 +0800）已合入 `main`；静态复核 `spec/dsh.rs`、`spec/pi.rs` 中参数映射与描述标注均已就位，状态回填为 Fixed |

## 十、修复记录

### 修复 commit

`69dc0f74` — fix(kernel): 对齐 dsh/pi 工具参数并明确执行语义
（2026-11-10 21:51:40 +0800，作者 wismyzhizi2018；已合入 `main`）

改动落点：`cc-agent/src/kernel/spec/{dsh,pi}.rs`、`cc-agent/src/kernel/prompts/{dsh,pi}.md`、
`cc-agent/src/kernel/find.rs`、`cc-agent/src/kernel/mod.rs`、`cc-agent/src/agent/executor/mod.rs`，
新增测试 `cc-agent/src/kernel/find_test.rs`、`cc-acp/src/kernel_tool_params_test.rs`（并于 `cc-acp/src/lib.rs` 注册）。

### 修复内容对照

| 问题 | 修复方式 | 落点 |
|------|---------|------|
| `skill` 参数名 | 新增 `RENAME_SKILL`（dialect `name` → canonical `skill`），出站 schema 显示 `name`、入站自动还原 | `cc-agent/src/kernel/spec/dsh.rs` |
| `bash` 超时参数名 | 新增 `RENAME_BASH`（dialect `timeoutMs` → canonical `timeout`） | 同上 |
| `ask_user_question` 嵌套字段 | 新增 `RENAME_QUESTIONS`（`questions[].multi_select` → `multiSelect`） | 同上 |
| pi 的 `grep` 参数名 | `RENAME_GREP` 扩展为 `ignoreCase` → `-i`、`literal` → `fixed_strings`、`limit` → `head_limit` | `cc-agent/src/kernel/spec/pi.rs` |
| pi 的 `find` 缺 `limit` | 新增**仅 pi 生效**的 `find.limit` 适配，不改共享 `Glob` 实现 | `cc-agent/src/kernel/find.rs`、`cc-agent/src/agent/executor/mod.rs` |
| `read.offset` 基准相反 | 在 dsh/pi 的 Read 描述中显式标注 0-based（`0` 读第一行）、非上游 1-based 行号 | `spec/{dsh,pi}.rs`、`prompts/{dsh,pi}.md` |
| `bash.timeout` 单位相反 | 在 pi 的 Bash 描述中显式标注毫秒（30 秒应传 `30000`），并说明 `execution_timeout` 同为毫秒 | `spec/pi.rs`、`prompts/pi.md` |
| `bash.workdir` 能力缺口 | 描述中明示不支持，需在 `command` 内显式 `cd` | `prompts/dsh.md` |
| `ask_user_question` 的 `id` | 描述中明示不支持 per-question id | `prompts/dsh.md` |
| upstream 归因头事实错误 | 改为 `dsh-v0.2.2-rc.1 (66491b05), branch master, checked 2026-11-10` | `cc-agent/src/kernel/spec/dsh.rs:1` |

### 未修复项（维持「能力缺口不映射」，改在描述中明示边界）

- `bash.workdir`：peri `BashTool` 的工作目录在构造期固定，无 per-call 覆盖路径，属能力缺口而非改名问题。
- `ask_user_question` 的 `id`：上游必填、peri 的 `InputQuestion` 无此字段（服务端生成），支持需改交互契约。
- 上游 `schedule_create` 的 RRULE 与 peri 五段 cron 不同构，Cron 工具保持规范名直接可见。
- peri 自有增强参数（`grep` 多余参数、`todo_write.activeForm`、`write.append`、`web_fetch.prompt`、`read.pages` 等）不删。

### 回归保护

- `cc-agent/src/kernel/find_test.rs` —— `find.limit` 仅 pi 生效的行为与底层警告保留。
- `cc-acp/src/kernel_tool_params_test.rs` —— 校验真实 schema、搜索与交互参数、执行器限额，以及 **cc 内核逐字节等价**（确保方言层改动不外溢到共享工具）。

### 验证情况

- commit message 记录：内核测试 37 项、跨 crate 回归 6 项通过。
- 2026-11-11 静态复核：`cc-agent/src/kernel/spec/dsh.rs` 中 `RENAME_BASH` / `RENAME_SKILL` / `RENAME_QUESTIONS` 与修正后的 upstream 归因头均已就位；`cc-agent/src/kernel/spec/pi.rs` 的 `RENAME_GREP` 已含 `ignoreCase` / `literal` / `limit` 映射，Read 描述含 0-based 说明、Bash 描述含毫秒说明。
- 2026-11-11 本机复跑 `cargo test -p cc-agent -p cc-acp --lib -- kernel` **未取得结论**：rustc 连续以 `STATUS_STACK_BUFFER_OVERRUN`(0xc0000409) 退出，伴 `页面文件太小，无法完成操作`(os error 1455) 与 `rustc-LLVM ERROR: out of memory`；复跑时本机物理内存仅余 0.71 GB、虚拟内存仅余 0.25 GB。属本机内存/页面文件耗尽的环境问题，非代码问题，故本机无测试结论可引用（修复本身的测试结论见 commit message，代码已过 CI）。

