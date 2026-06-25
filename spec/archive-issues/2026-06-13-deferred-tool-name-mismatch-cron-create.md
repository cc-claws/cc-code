> 归档于 2026-06-24，原路径 spec/issues/2026-06-13-deferred-tool-name-mismatch-cron-create.md

# Deferred tool 名称不一致导致 ExecuteExtraTool 找不到 CronCreate

**状态**：Fixed
**优先级**：中
**创建日期**：2026-06-13

## 问题描述

LLM 通过 `ExecuteExtraTool` 调用 `CronCreate` 时报错 `tool 'CronCreate' not found or not registered as a deferred tool`。实际注册的工具名是 `cron_register`（snake_case），deferred tools 列表中显示为 `CronRegister`（CamelCase）。`CronCreate` 在系统中不存在——LLM 自行臆造了这个名称。

## 症状详情

LLM 调用链：
1. LLM 先调用 `SearchExtraTools` 搜索 cron 相关工具
2. 搜索结果返回工具名（CamelCase 格式，如 `CronRegister`）
3. LLM 调用 `ExecuteExtraTool({"tool_name": "CronCreate", ...})` — 名称既不是 snake_case 也不是正确的 CamelCase
4. 报错：`tool 'CronCreate' not found or not registered as a deferred tool`

系统中存在**三处命名不一致**，共同导致 LLM 混乱：

| 来源 | 工具名格式 | 示例 |
|------|-----------|------|
| 工具实际注册名（`tools.rs:22`） | snake_case | `cron_register` |
| 系统提示词段落（`12_cron.md`） | snake_case | `cron_register` |
| Deferred tools 列表（`format_deferred_list()`） | CamelCase | `CronRegister` |
| ExecuteExtraTool 参数描述示例 | CamelCase | `CronCreate`（错误示例） |

## 复现条件

- **复现频率**：偶发（取决于 LLM 是否臆造工具名）
- **触发步骤**：
  1. 在会话中要求 LLM 创建定时任务
  2. LLM 调用 `ExecuteExtraTool` 时使用了不存在的工具名 `CronCreate`
- **环境**：任意模型，deferred tool 注册了 cron 工具时

## 涉及文件

- `peri-middlewares/src/cron/tools.rs:22` — 工具实际注册名为 `cron_register`（snake_case）
- `peri-tui/prompts/sections/12_cron.md:3` — 系统提示词用 snake_case 引用工具名
- `peri-middlewares/src/tool_search/tool_index.rs` — `format_deferred_list()` 输出 CamelCase 格式
- `peri-middlewares/src/tool_search/execute_tool.rs:43` — 参数描述示例写的是 `CronCreate`（错误示例，应为 `CronRegister`）

## 状态变更记录

| 日期 | 从 | 到 | 操作人 | 说明 |
|------|-----|-----|--------|------|
| 2026-06-13 | — | Open | agent | 创建 |
| 2026-06-13 | Open | Fixed | agent | 修复：ExecuteExtraTool 增加三级模糊匹配（精确→大小写不敏感+规范化→首词前缀），CronCreate 可自动解析到 cron_register |

## 修复记录

### 修复 #1（2026-06-13）

- **操作人**：agent
- **用户原意**：LLM 使用 CronCreate 能兼容找到 cron_register 工具
- **修复内容**：`execute_tool.rs` 新增 `resolve_tool()` 模糊查找函数，支持三级回退：精确匹配 → 大小写不敏感 + CamelCase↔snake_case 规范化匹配 → 首词前缀匹配。新增 4 个测试用例覆盖各场景。
- **涉及 commit**：待提交
- **验证状态**：已验证（11/11 测试通过）
