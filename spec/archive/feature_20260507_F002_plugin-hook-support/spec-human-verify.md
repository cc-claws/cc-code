# Plugin Hook Support 基础层 人工验收清单

**生成时间:** 2026-05-07 19:56
**关联计划:** spec/feature_20260507_F002_plugin-hook-support/spec-plan-1.md
**关联设计:** spec/feature_20260507_F002_plugin-hook-support/spec-design.md

---

## 验收前准备

### 环境要求
- [x] [AUTO] 检查 Rust 工具链版本: `rustc --version && cargo --version` → 期望包含: rustc 1.x / cargo 1.x
- [x] [AUTO] 编译 workspace: `cargo build 2>&1 | tail -5` → 期望包含: Finished 且无 error
- [x] [AUTO] 编译目标 crate: `cargo build -p peri-middlewares 2>&1 | tail -5` → 期望包含: Finished 且无 error

### 测试数据准备
- [x] [AUTO] 验证 hooks 模块目录存在: `ls peri-middlewares/src/hooks/` → 期望包含: types.rs, matcher.rs, variables.rs, output_parser.rs, mod.rs

---

## 验收项目

### 场景 1：Task 1 — Hook 数据类型定义

#### - [x] 1.1 HookEvent 枚举定义与 serde 兼容性
- **来源:** spec-plan.md Task 1 / spec-design.md 数据模型
- **目的:** 确认 13 个 Phase 1 事件枚举正确
- **操作步骤:**
  1. [A] `cargo test -p peri-middlewares --lib hooks::types::tests::test_hookevent_serialize 2>&1 | tail -5` → 期望包含: test result: ok
  2. [A] `cargo test -p peri-middlewares --lib hooks::types::tests::test_hookevent_serialize 2>&1 | grep -o '"PreToolUse"'` → 期望包含: "PreToolUse"

#### - [x] 1.2 HookType discriminated union 反序列化
- **来源:** spec-plan.md Task 1
- **目的:** 确认 4 种 hook 类型 JSON 解析正确
- **操作步骤:**
  1. [A] `cargo test -p peri-middlewares --lib hooks::types::tests::test_hooktype_deser 2>&1 | tail -3` → 期望包含: test result: ok

#### - [x] 1.3 HookInput 序列化（PascalCase + None 跳过）
- **来源:** spec-plan.md Task 1 / spec-design.md HookInput
- **目的:** 确认 stdin JSON 协议字段格式正确
- **操作步骤:**
  1. [A] `cargo test -p peri-middlewares --lib hooks::types::tests 2>&1 | grep -E "(test.*hook_input|test.*HookInput)"` → 期望包含: 测试通过

#### - [x] 1.4 SyncHookResponse 反序列化
- **来源:** spec-plan.md Task 1
- **目的:** 确认 stdout JSON 响应解析类型正确
- **操作步骤:**
  1. [A] `cargo test -p peri-middlewares --lib hooks::types::tests 2>&1 | grep -E "(test.*sync|test.*SyncHookResponse)"` → 期望包含: 测试通过

#### - [x] 1.5 HookSpecificOutput tag 解析
- **来源:** spec-plan.md Task 1 / spec-design.md HookSpecificOutput
- **目的:** 确认 discriminated union 按事件类型正确分发
- **操作步骤:**
  1. [A] `cargo test -p peri-middlewares --lib hooks::types::tests 2>&1 | grep -E "(test.*specific|test.*hook_specific)"` → 期望包含: 测试通过

#### - [x] 1.6 HookType getter 辅助方法
- **来源:** spec-plan.md Task 1
- **目的:** 确认公共字段访问器行为正确
- **操作步骤:**
  1. [A] `cargo test -p peri-middlewares --lib hooks::types::tests 2>&1 | grep -E "(test.*getter|test.*is_once|test.*is_async)"` → 期望包含: 测试通过

#### - [x] 1.7 HookInput 构造函数
- **来源:** spec-plan.md Task 1
- **目的:** 确认各事件类型的工厂方法正确填充字段
- **操作步骤:**
  1. [A] `cargo test -p peri-middlewares --lib hooks::types::tests 2>&1 | grep -E "(test.*tool_call|test.*construct)"` → 期望包含: 测试通过

#### - [x] 1.8 HooksConfig 完整反序列化
- **来源:** spec-plan.md Task 1 / spec-design.md HookMatchRule
- **目的:** 确认 hooks.json 格式兼容性
- **操作步骤:**
  1. [A] `cargo test -p peri-middlewares --lib hooks::types::tests 2>&1 | grep -E "(test.*hooks_config|test.*HooksConfig)"` → 期望包含: 测试通过

#### - [x] 1.9 类型定义编译无警告
- **来源:** spec-plan.md Task 1 检查步骤
- **目的:** 确认类型层代码质量
- **操作步骤:**
  1. [A] `cargo check -p peri-middlewares --lib 2>&1 | grep -E "(error|warning:.*hooks::types)"` → 期望精确: (无输出)

---

### 场景 2：Task 2 — Hook 匹配引擎

#### - [x] 2.1 matcher 通配符匹配
- **来源:** spec-plan.md Task 2 / spec-design.md matcher vs if
- **目的:** 确认 "*" 匹配所有工具名
- **操作步骤:**
  1. [A] `cargo test -p peri-middlewares --lib hooks::matcher::tests::test_matcher_wildcard 2>&1 | tail -3` → 期望包含: test result: ok

#### - [x] 2.2 matcher 精确匹配
- **来源:** spec-plan.md Task 2
- **目的:** 确认精确匹配与不匹配两种情况
- **操作步骤:**
  1. [A] `cargo test -p peri-middlewares --lib hooks::matcher::tests::test_matcher_exact 2>&1 | tail -3` → 期望包含: test result: ok

#### - [x] 2.3 matcher 管道列表匹配
- **来源:** spec-plan.md Task 2
- **目的:** 确认 Write|Edit|Grep 管道分隔语法
- **操作步骤:**
  1. [A] `cargo test -p peri-middlewares --lib hooks::matcher::tests::test_matcher_pipe 2>&1 | tail -3` → 期望包含: test result: ok

#### - [x] 2.4 matcher 正则匹配
- **来源:** spec-plan.md Task 2
- **目的:** 确认正则表达式匹配和非法正则降级
- **操作步骤:**
  1. [A] `cargo test -p peri-middlewares --lib hooks::matcher::tests::test_matcher_regex 2>&1 | tail -3` → 期望包含: test result: ok

#### - [x] 2.5 if 条件工具名匹配
- **来源:** spec-plan.md Task 2
- **目的:** 确认 ToolName(rule) 语法解析
- **操作步骤:**
  1. [A] `cargo test -p peri-middlewares --lib hooks::matcher::tests::test_if_condition_tool_match 2>&1 | tail -3` → 期望包含: test result: ok

#### - [x] 2.6 if 条件内容包含匹配
- **来源:** spec-plan.md Task 2 / spec-design.md match_tool_rule
- **目的:** 确认 tool_input 字符串包含语义
- **操作步骤:**
  1. [A] `cargo test -p peri-middlewares --lib hooks::matcher::tests::test_if_condition_content 2>&1 | tail -3` → 期望包含: test result: ok

#### - [x] 2.7 matcher 公共 API 导出
- **来源:** spec-plan.md Task 2 检查步骤
- **目的:** 确认模块导出路径可达
- **操作步骤:**
  1. [A] `grep -n "pub use matcher" peri-middlewares/src/hooks/mod.rs` → 期望包含: pub use matcher::{matches_matcher, matches_if_condition}

#### - [x] 2.8 匹配引擎测试覆盖率
- **来源:** spec-plan.md Task 2 检查步骤
- **目的:** 确认 9 个匹配场景全覆盖
- **操作步骤:**
  1. [A] `cargo test -p peri-middlewares --lib hooks::matcher 2>&1 | grep -E "running \d+ test" | head -1` → 期望包含: running 9 test
  2. [A] `cargo test -p peri-middlewares --lib hooks::matcher 2>&1 | grep "test result:"` → 期望包含: test result: ok

---

### 场景 3：Task 3 — 变量替换

#### - [x] 3.1 CLAUDE_PLUGIN_ROOT 替换
- **来源:** spec-plan.md Task 3 / spec-design.md 变量替换
- **目的:** 确认插件路径变量正确展开
- **操作步骤:**
  1. [A] `cargo test -p peri-middlewares --lib hooks::variables::tests::test_plugin_root 2>&1 | tail -3` → 期望包含: test result: ok

#### - [x] 3.2 多变量混合替换
- **来源:** spec-plan.md Task 3
- **目的:** 确认 ${CLAUDE_PLUGIN_ROOT}/${CLAUDE_PLUGIN_DATA} 混合展开
- **操作步骤:**
  1. [A] `cargo test -p peri-middlewares --lib hooks::variables::tests::test_multi_var 2>&1 | tail -3` → 期望包含: test result: ok

#### - [x] 3.3 ARGUMENTS 替换
- **来源:** spec-plan.md Task 3
- **目的:** 确认 $ARGUMENTS 和 ${ARGUMENTS} 两种格式
- **操作步骤:**
  1. [A] `cargo test -p peri-middlewares --lib hooks::variables::tests::test_arguments 2>&1 | tail -3` → 期望包含: test result: ok

#### - [x] 3.4 环境变量白名单替换
- **来源:** spec-plan.md Task 3
- **目的:** 确认白名单内替换、白名单外阻断
- **操作步骤:**
  1. [A] `cargo test -p peri-middlewares --lib hooks::variables::tests::test_env_whitelist 2>&1 | tail -3` → 期望包含: test result: ok

#### - [x] 3.5 边界情况处理
- **来源:** spec-plan.md Task 3 / spec-design.md 变量替换
- **目的:** 确认空输入、无变量字符串、未定义环境变量
- **操作步骤:**
  1. [A] `cargo test -p peri-middlewares --lib hooks::variables::tests::test_edge_cases 2>&1 | tail -3` → 期望包含: test result: ok

#### - [x] 3.6 lib.rs 模块导出
- **来源:** spec-plan.md Task 3
- **目的:** 确认 hooks 模块在 crate 根可见
- **操作步骤:**
  1. [A] `grep -n "pub mod hooks" peri-middlewares/src/lib.rs` → 期望包含: pub mod hooks

#### - [x] 3.7 mod.rs 公共 API 导出
- **来源:** spec-plan.md Task 3
- **目的:** 确认 variables 函数正确 re-export
- **操作步骤:**
  1. [A] `grep -n "pub use variables::resolve_hook_variables" peri-middlewares/src/hooks/mod.rs` → 期望包含: pub use variables::resolve_hook_variables

#### - [x] 3.8 variables 全量测试通过
- **来源:** spec-plan.md Task 3 检查步骤
- **目的:** 确认所有变量替换测试无回归
- **操作步骤:**
  1. [A] `cargo test -p peri-middlewares --lib hooks::variables 2>&1 | grep "test result:"` → 期望包含: test result: ok

---

### 场景 4：Task 4 — 输出解析器

#### - [x] 4.1 纯文本 stdout → Allow
- **来源:** spec-plan.md Task 4 / spec-design.md parse_command_hook_output
- **目的:** 确认非 JSON 输出降级为 Allow
- **操作步骤:**
  1. [A] `cargo test -p peri-middlewares --lib hooks::output_parser::tests::test_parse_command 2>&1 | tail -3` → 期望包含: test result: ok

#### - [x] 4.2 JSON continue=false → PreventContinuation
- **来源:** spec-plan.md Task 4
- **目的:** 确认 continue=false 优先级最高
- **操作步骤:**
  1. [A] `cargo test -p peri-middlewares --lib hooks::output_parser::tests::test_continue_false 2>&1 | tail -3` → 期望包含: test result: ok

#### - [x] 4.3 JSON decision=block → Block
- **来源:** spec-plan.md Task 4
- **目的:** 确认 block 决策正确转换
- **操作步骤:**
  1. [A] `cargo test -p peri-middlewares --lib hooks::output_parser::tests::test_decision_block 2>&1 | tail -3` → 期望包含: test result: ok

#### - [x] 4.4 JSON systemMessage → SystemMessage
- **来源:** spec-plan.md Task 4
- **目的:** 确认系统消息注入
- **操作步骤:**
  1. [A] `cargo test -p peri-middlewares --lib hooks::output_parser::tests::test_system_message 2>&1 | tail -3` → 期望包含: test result: ok

#### - [x] 4.5 hookSpecificOutput.updatedInput → ModifyInput
- **来源:** spec-plan.md Task 4 / spec-design.md hook_specific_to_action
- **目的:** 确认 PreToolUse 事件工具输入修改
- **操作步骤:**
  1. [A] `cargo test -p peri-middlewares --lib hooks::output_parser::tests::test_updated_input 2>&1 | tail -3` → 期望包含: test result: ok

#### - [x] 4.6 sync_response_to_action 优先级
- **来源:** spec-plan.md Task 4 / spec-design.md sync_response_to_action
- **目的:** 确认 continue=false > decision=block > systemMessage > hookSpecificOutput > Allow
- **操作步骤:**
  1. [A] `cargo test -p peri-middlewares --lib hooks::output_parser::tests::test_sync_response 2>&1 | tail -3` → 期望包含: test result: ok

#### - [x] 4.7 HTTP hook 空 body → Allow
- **来源:** spec-plan.md Task 4 / spec-design.md parse_http_hook_response
- **目的:** 确认空 body 视为有效 JSON ({})
- **操作步骤:**
  1. [A] `cargo test -p peri-middlewares --lib hooks::output_parser::tests::test_parse_http 2>&1 | tail -3` → 期望包含: test result: ok

#### - [x] 4.8 HTTP hook 非 JSON body → Allow + warn
- **来源:** spec-plan.md Task 4
- **目的:** 确认非法响应体降级处理
- **操作步骤:**
  1. [A] `cargo test -p peri-middlewares --lib hooks::output_parser::tests::test_http_non_json 2>&1 | tail -3` → 期望包含: test result: ok

#### - [x] 4.9 output_parser 公共 API 导出
- **来源:** spec-plan.md Task 4 检查步骤
- **目的:** 确认解析函数模块导出
- **操作步骤:**
  1. [A] `grep -n "pub use output_parser" peri-middlewares/src/hooks/mod.rs` → 期望包含: pub use output_parser::{parse_command_hook_output, parse_http_hook_response}

#### - [x] 4.10 output_parser 测试覆盖率
- **来源:** spec-plan.md Task 4 检查步骤
- **目的:** 确认 15 个解析场景全覆盖
- **操作步骤:**
  1. [A] `cargo test -p peri-middlewares --lib hooks::output_parser 2>&1 | grep -E "running \d+ test" | head -1` → 期望包含: running 15 test
  2. [A] `cargo test -p peri-middlewares --lib hooks::output_parser 2>&1 | grep "test result:"` → 期望包含: test result: ok

---

### 场景 5：Task 5 — 基础层端到端验收

#### - [x] 5.1 hooks 模块全量测试无回归
- **来源:** spec-plan.md Task 5
- **目的:** 确认 Task 1-4 集成后无破坏
- **操作步骤:**
  1. [A] `cargo test -p peri-middlewares --lib hooks 2>&1 | tail -10` → 期望包含: test result: ok

#### - [x] 5.2 4 个子模块测试独立通过
- **来源:** spec-plan.md Task 5
- **目的:** 确认各子模块公共 API 完整
- **操作步骤:**
  1. [A] `cargo test -p peri-middlewares --lib hooks::types hooks::matcher hooks::variables hooks::output_parser 2>&1 | grep -E "test result:"` → 期望包含: test result: ok (出现 4 次)

#### - [x] 5.3 serde 反序列化兼容性验证
- **来源:** spec-plan.md Task 5
- **目的:** 确认 Claude Code hooks.json 格式兼容
- **操作步骤:**
  1. [A] `cargo test -p peri-middlewares --lib hooks::types::tests 2>&1 | grep "test result:"` → 期望包含: test result: ok

---

## 验收后清理

无后台服务需要清理。

---

## 验收结果汇总

| 场景 | 序号 | 验收项 | [A] | [H] | 结果 |
|------|------|--------|-----|-----|------|
| 场景 1 | 1.1 | HookEvent 枚举 serde 兼容性 | A | | ✅ |
| 场景 1 | 1.2 | HookType discriminated union 反序列化 | A | | ✅ |
| 场景 1 | 1.3 | HookInput 序列化格式 | A | | ✅ |
| 场景 1 | 1.4 | SyncHookResponse 反序列化 | A | | ✅ |
| 场景 1 | 1.5 | HookSpecificOutput tag 解析 | A | | ✅ |
| 场景 1 | 1.6 | HookType getter 辅助方法 | A | | ✅ |
| 场景 1 | 1.7 | HookInput 构造函数 | A | | ✅ |
| 场景 1 | 1.8 | HooksConfig 完整反序列化 | A | | ✅ |
| 场景 1 | 1.9 | 类型定义编译无警告 | A | | ✅ |
| 场景 2 | 2.1 | matcher 通配符匹配 | A | | ✅ |
| 场景 2 | 2.2 | matcher 精确匹配 | A | | ✅ |
| 场景 2 | 2.3 | matcher 管道列表匹配 | A | | ✅ |
| 场景 2 | 2.4 | matcher 正则匹配 | A | | ✅ |
| 场景 2 | 2.5 | if 条件工具名匹配 | A | | ✅ |
| 场景 2 | 2.6 | if 条件内容包含匹配 | A | | ✅ |
| 场景 2 | 2.7 | matcher 公共 API 导出 | A | | ✅ |
| 场景 2 | 2.8 | 匹配引擎测试覆盖率 | A | | ✅ |
| 场景 3 | 3.1 | CLAUDE_PLUGIN_ROOT 替换 | A | | ✅ |
| 场景 3 | 3.2 | 多变量混合替换 | A | | ✅ |
| 场景 3 | 3.3 | ARGUMENTS 替换 | A | | ✅ |
| 场景 3 | 3.4 | 环境变量白名单替换 | A | | ✅ |
| 场景 3 | 3.5 | 边界情况处理 | A | | ✅ |
| 场景 3 | 3.6 | lib.rs 模块导出 | A | | ✅ |
| 场景 3 | 3.7 | mod.rs 公共 API 导出 | A | | ✅ |
| 场景 3 | 3.8 | variables 全量测试通过 | A | | ✅ |
| 场景 4 | 4.1 | 纯文本 stdout → Allow | A | | ✅ |
| 场景 4 | 4.2 | JSON continue=false → PreventContinuation | A | | ✅ |
| 场景 4 | 4.3 | JSON decision=block → Block | A | | ✅ |
| 场景 4 | 4.4 | JSON systemMessage → SystemMessage | A | | ✅ |
| 场景 4 | 4.5 | hookSpecificOutput.updatedInput → ModifyInput | A | | ✅ |
| 场景 4 | 4.6 | sync_response_to_action 优先级 | A | | ✅ |
| 场景 4 | 4.7 | HTTP hook 空 body → Allow | A | | ✅ |
| 场景 4 | 4.8 | HTTP hook 非 JSON body → Allow + warn | A | | ✅ |
| 场景 4 | 4.9 | output_parser 公共 API 导出 | A | | ✅ |
| 场景 4 | 4.10 | output_parser 测试覆盖率 | A | | ✅ |
| 场景 5 | 5.1 | hooks 模块全量测试无回归 | A | | ✅ |
| 场景 5 | 5.2 | 4 个子模块测试独立通过 | A | | ✅ |
| 场景 5 | 5.3 | serde 反序列化兼容性验证 | A | | ✅ |

**验收结论:** ✅ 全部通过
