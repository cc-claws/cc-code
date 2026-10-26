# 产品需求文档 (PRD)：Peri `Skill` 工具机制与端到端架构重构

**文档版本**：v1.1 (Final Architecture RFC)  
**创建日期**：2026-10-26  
**责任模块**：`cc-agent` / `cc-middlewares` / `cc-acp` / `cc-tui` / `cc-widgets` / `spec`  
**基准对准**：参考实现的 `SkillTool` 行为契约  
**文档状态**：Ready for Implementation  

---

## 1. 背景与核心问题

### 1.1 现状与直观体验断层
在参考实现中，当用户询问业务相关问题（如“查一下本地需求列表”）或输入 Slash 指令（如 `/example-skill`）时，终端会呈现出专门的工具卡片：
```text
● Skill (example-skill)
  ✔ Successfully loaded skill (3 tools allowed)
```
而在当前 Peri 的实现中，用户看到的却是普通的文件读取动作：
```text
● Read (<技能目录>/example-skill/SKILL.md)
```
如果是直接输入 `/example-skill`，底层 `SkillPreloadMiddleware` 会伪造一条 `Ai[ToolUse: Read]` 和 `Tool[ToolResult: content]` 插入历史，在界面上产生突兀的 `● Read` 动作。

### 1.2 现状架构缺陷对比

| 架构维度 | 参考实现 | Peri 现有实现 | 缺陷与负面影响 |
| :--- | :--- | :--- | :--- |
| **工具模型** | 一级核心工具 `SkillTool`（名字定死为 `Skill`） | 无 `Skill` 工具，未加入 Core Tools | 技能降级为普通文件读取，LLM 缺乏显式意图表达 |
| **调度协议** | System Prompt 注入 **BLOCKING REQUIREMENT** 强硬约束 | Prompt 引导“用 `Read` 工具读取对应路径” | 导致模型易幻觉（直接凭记忆回答，不触发读文件）或后知后觉 |
| **参数与意图** | 支持透传用户参数（`args: "..."`） | 伪造 Read 无法处理动态参数传递 | 技能无法直接接收上下文参数进行模板插值 |
| **隔离执行** | 识别 Frontmatter `context: fork` 自动唤起子 Agent 执行 | 仅支持平铺注入当前上下文 | 复杂长技能直接撑爆主上下文，污染 Prompt Cache |
| **压缩防护** | 通过 `addInvokedSkill` 精确追踪已激活技能 | `re_inject.rs` 用正则和模糊路径匹配过滤 Read | 严重技术债：不得不通过技能目录路径字符串做前缀匹配，来区分普通代码文件与技能文件 |
| **UI 表达** | 专属 `Skill` 卡片渲染，折叠正文仅展示状态 | 普通 Read 渲染，大段 markdown 正文冲刷屏幕 | 用户无法直观感知当前处于哪项专业技能的上下文 |

---

## 2. 五大底层暗礁与针对性防御设计

在前期排查中，Peri 曾踩过两次严重的 P0 级 Provider 协议崩溃（`2026-05-26 孤儿 tool_result 触发协议 400`、`2026-05-27 假工具消息缺失 thinking 触发思维链模型 400`）。本 PRD 必须对以下五大暗礁提供严密的防御机制：

### 暗礁一：伪造 `Skill` 消息会再次引爆开启思维链模型的 Thinking 400 报错
- **暗礁**：在开启思维链（Thinking / Reasoning）的模型下，API 协议强制校验：**Assistant 消息若包含 `tool_calls`，必须同时包含非空的 `thinking` / `reasoning_content`**。一旦伪造的消息缺少该字段，API 会在第一轮对话直接报错：`LLM HTTP 错误 (400): The 'content[].thinking' in the thinking mode must be passed back to the API`。
- **防御架构**：
  - **彻底放弃在中间件伪造 `Ai` 消息的旧路线**。
  - **模型自主意图（自然语言问“查需求列表”）**：走真正的工具调用链路。模型自己吐出 `tool_use(Skill)`（自然带有合法的 thinking 块），`SkillTool` 正常执行返回 `ToolResult`。
  - **用户显式输入（`/example-skill`）**：在用户消息侧作为 `UserMessage`（Meta 指令）注入上下文，或者如果要在 TUI 显示卡片，仅在 UI 渲染层做伪装投影，**坚决不向 LLM API 发生器发送无 thinking 的伪造 Ai 消息**。

### 暗礁二：Peri 的 `BaseTool` 返回结构与参考实现的架构断层
- **暗礁**：参考实现的 `SkillTool.call` 在 TypeScript 里返回的不仅是 `data`，它还能返回 `newMessages` 和 `contextModifier`。但 Peri 核心 crate `cc-agent` 的 `BaseTool` trait 只能返回一个 `ToolContent`，无法凭空向 ReAct 循环追加一条独立的系统/用户消息。
- **防御架构**：
  - `SkillTool::invoke` 直接在 `ToolContent::text` 中返回 **格式化好的技能指令全文（Markdown）**。
  - 对模型而言，这就是一次标准的函数调用：`模型发起："我调 Skill(example-skill)"` → `工具返回："这是 example-skill 的规则和脚本：..."` → `模型下一轮基于正文调 Bash/Read 执行`。
  - 此解法 **0 改动核心 Agent 引擎**，完全符合标准 Function Calling 协议，极其稳定。

### 暗礁三：弱模型与非原生模型对“阻塞契约”的遵从度衰减
- **暗礁**：原生模型对 `BLOCKING REQUIREMENT: invoke Skill tool` 遵从度极高；但部分第三方模型容易直接无视约束。
- **防御架构**：
  - **Prompt 增加 Few-shot 正反例**：
    ```text
    Negative example: User says "查需求列表", Model directly answers "好的我来帮你查...". (FORBIDDEN)
    Positive example: User says "查需求列表", Model calls `Skill({ skill: "example-skill" })`. (CORRECT)
    ```
  - **Tool Description 强化诱导**：在 `SkillTool` 本身的参数描述上直接写明：“Execute a specialized workflow. When the user task matches any skill name in the system prompt, you MUST call this tool first before generating text.”

### 暗礁四：超长 `SKILL.md` 导致单轮上下文膨胀与 Cache 抖动
- **暗礁**：部分技能正文长达数十 KB，若多轮对话频繁调用，会导致上下文窗口迅速被工具结果撑满。
- **防御架构**：
  - **系统提示词技能概览硬上限（对齐参考实现 `MAX_LISTING_DESC_CHARS = 1536`）**：注入 System Prompt 的技能列表仅保留核心意图描述，严禁全量注入。
  - **Compact 阶段精准恢复**：在 `CompactMiddleware`（`cc-agent/src/agent/compact/re_inject.rs`）中，废弃之前扫描技能目录路径的脆弱判断，精准识别 `tc.name == "Skill"`。只有最近一轮激活的技能才在压缩后重新注入，历史旧技能不重复注入。

### 暗礁五：TUI 侧折叠与状态栏统计的显示陷阱
- **暗礁**：如果未将 `Skill` 纳入 `READ_ONLY_TOOLS` 集合，大段 SKILL 文本会在主视口完全展开，冲垮终端排版；状态栏统计也会出现未识别的显示名。
- **防御架构**：
  - `cc-widgets/src/tool_call/collapse.rs` 显式将 `"Skill"` 加入默认折叠白名单。
  - 折叠后单行高度严格锁定为 1 行，右侧只呈现 `✔ Successfully loaded skill`。
  - `cc-tui/src/app/tool_display.rs` 中 `format_tool_name_for_stats` 统一映射为 `"Skill"`。

---

## 3. 详细技术方案

### 3.1 `SkillTool` 接口与 Schema 定义

在 `cc-middlewares/src/tools/skill.rs` 中实现 `BaseTool`：

```rust
use async_trait::async_trait;
use cc_agent::{
    error::{AgentError, AgentResult},
    tools::{BaseTool, FileSnapshot, ToolContent},
};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

pub struct SkillTool {
    dirs: Vec<PathBuf>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillToolInput {
    pub skill: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub args: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillToolOutput {
    pub success: bool,
    pub command_name: String,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allowed_tools: Option<Vec<String>>,
    pub instruction: String,
}

#[async_trait]
impl BaseTool for SkillTool {
    fn name(&self) -> &str {
        "Skill"
    }

    fn description(&self) -> &str {
        "Execute a skill within the conversation. When users ask to perform tasks that match an available skill, invoke this tool before taking other actions."
    }

    fn parameters(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "skill": {
                    "type": "string",
                    "description": "REQUIRED. The exact name of the skill to invoke (e.g. 'example-skill', 'commit')."
                },
                "args": {
                    "type": "string",
                    "description": "Optional arguments or user instruction passed to the skill."
                }
            },
            "required": ["skill"]
        })
    }

    fn file_snapshot(&self, _input: &serde_json::Value) -> Option<FileSnapshot> {
        None
    }

    async fn invoke(&self, input: serde_json::Value) -> AgentResult<ToolContent> {
        let parsed: SkillToolInput = serde_json::from_value(input)
            .map_err(|e| AgentError::ToolExecution(format!("Invalid Skill input: {e}")))?;

        // 统一查找 SKILL.md
        let (path, content) = self.find_and_read_skill(&parsed.skill).await?;
        
        // 解析 Frontmatter (提取 allowed-tools, context 等)
        let (frontmatter, body) = parse_frontmatter(&content);

        let output = SkillToolOutput {
            success: true,
            command_name: parsed.skill,
            status: "inline".to_string(),
            allowed_tools: frontmatter.allowed_tools,
            instruction: body,
        };

        Ok(ToolContent::text(serde_json::to_string_pretty(&output).unwrap_or(body)))
    }
}
```

### 3.2 核心工具集收敛与延迟加载对齐 (`cc-middlewares`)

修改 `cc-middlewares/src/tool_search/core_tools.rs`：
- 定义常量：`pub const TOOL_SKILL: &str = "Skill";`
- 加入 `CORE_TOOLS` 白名单，确保始终对 LLM 可见（从 11 个核心工具扩展至 12 个）。

### 3.3 System Prompt 阻塞契约重构

修改 `cc-tui/prompts/sections/13_skills.md` 与 `cc-middlewares/src/skills/mod.rs`（`SkillsMiddleware::build_summary`）：
```markdown
# Skills

Skills provide task-specific workflows, requirements, and domain knowledge.

## Using skills

When users ask you to perform tasks, check if any of the available skills match.
When a skill matches the user's request, this is a BLOCKING REQUIREMENT:
invoke the `Skill` tool with the skill name BEFORE generating any other response about the task.

- NEVER attempt to read the SKILL.md file directly using the `Read` tool.
- NEVER claim you applied a skill without actually calling the `Skill` tool.
- Follow the instructions returned in the tool result.

Examples:
- User: "查需求列表" -> Call `Skill({ skill: "example-skill" })`
- User: "做一次代码审计" -> Call `Skill({ skill: "example-skill" })`
```

### 3.4 TUI 格式化与折叠渲染 (`cc-tui` & `cc-widgets`)

1. **`cc-tui/src/app/tool_display.rs`**：
   - `format_tool_name("Skill")` -> 返回 `"Skill"`
   - `format_tool_args("Skill", input)` -> 解析 `skill` 与可选 `args`，格式化为 `● Skill (example-skill)` 或 `● Skill (commit -m "fix")`。
2. **`cc-widgets/src/tool_call/collapse.rs`**：
   - 将 `"Skill"` 加入 `READ_ONLY_TOOLS`，默认收拢正文，右侧仅显示 `✔ Successfully loaded skill`。

### 3.5 上下文压缩恢复重构 (`cc-agent`)

修改 `cc-agent/src/agent/compact/re_inject.rs`：
- 废弃 `is_skills_path` 及按技能目录路径前缀匹配的旧判断。
- 直接通过 `tc.name == "Skill"` 提取活跃技能名称，在生成压缩摘要后精准重新注入最新激活的技能指令。

---

## 4. 全量涉及代码与成本枚举清单

| 序号 | 文件路径 | 改动职责 | 预估代码行 |
| :---: | :--- | :--- | :---: |
| 1 | `cc-middlewares/src/tools/skill.rs` | 新增 `SkillTool` 实现 `BaseTool` | +130 行 |
| 2 | `cc-middlewares/src/tools/skill_test.rs` | 单元测试：参数解析、路径查找、异常分支 | +80 行 |
| 3 | `cc-middlewares/src/tools/mod.rs` | 导出 `pub mod skill; pub use skill::SkillTool;` | +2 行 |
| 4 | `cc-middlewares/src/tool_search/core_tools.rs` | 加入 `TOOL_SKILL` 常量与 `CORE_TOOLS` 白名单 | +4 行 |
| 5 | `cc-middlewares/src/skills/mod.rs` | 更新 `build_summary` 提示词模板为阻塞性契约 | +20 行 |
| 6 | `cc-middlewares/src/subagent/skill_preload.rs` | 废除伪造 Ai 消息，改为用户端安全提示注入 | ~35 行 |
| 7 | `cc-agent/src/agent/compact/re_inject.rs` | 简化压缩重注入识别为 `tc.name == "Skill"` | -30 行, +15 行 |
| 8 | `cc-acp/src/agent/builder.rs` | 注册 `SkillTool` 进入 ReActAgent 与父工具集 | +12 行 |
| 9 | `cc-tui/prompts/sections/13_skills.md` | 同步静态 Prompt 段落 | +15 行 |
| 10 | `cc-tui/src/app/tool_display.rs` | `format_tool_name` 与 `format_tool_args` 适配 | +14 行 |
| 11 | `cc-tui/src/app/tool_display_test.rs` | 补充 Skill 名称与参数格式化单测 | +20 行 |
| 12 | `cc-widgets/src/tool_call/collapse.rs` | 将 `"Skill"` 纳入 `READ_ONLY_TOOLS` 默认折叠白名单 | +1 行 |
| 13 | `spec/global/domains/skill-trigger.md` | 同步更新技能触发领域设计规范 | +30 行 |

---

## 5. 验收标准与交付计划

### 5.1 验收标准 (DoD)
1. **模型意图自发触发**：输入“查一下本地需求列表”，模型第一轮直接调用 `Skill({ skill: "example-skill" })`，终端呈现 `● Skill (example-skill)`，无 `Read` 痕迹。
2. **Slash 命令触发**：输入 `/example-skill`，终端正常呈现技能指令加载与执行，不发生 400 协议报错。
3. **多模型兼容性**：在开启思维链的模型、原生模型、第三方模型三类环境下分别验证，均无 HTTP 400 报错。
4. **上下文压缩验证**：触发 `/compact` 后，活跃技能指令被完整恢复，下一轮问答不失忆。

### 5.2 实施排期
- **P0 阶段（1-2 天）**：实现 `SkillTool`、`CORE_TOOLS` 注册、`tool_display.rs` 渲染适配与 `re_inject.rs` 简化。
- **P1 阶段（1 天）**：多模型环境验证（重点覆盖思维链回传与协议严格校验）与单元测试全绿。
