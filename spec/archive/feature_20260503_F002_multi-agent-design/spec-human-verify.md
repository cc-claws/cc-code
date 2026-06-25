# Multi-Agent Design (Fork + Prompt 优化) 人工验收清单

**生成时间:** 2026-05-03
**关联计划:** spec/feature_20260503_F002_multi-agent-design/spec-plan.md
**关联设计:** spec/feature_20260503_F002_multi-agent-design/spec-design.md

---

## 验收前准备

### 环境要求
- [ ] [AUTO] 编译项目: `cargo build -p peri-middlewares -p peri-tui 2>&1 | tail -5`
- [ ] [AUTO] 验证测试框架可用: `cargo test -p peri-middlewares --lib -- subagent 2>&1 | tail -5`

### 测试数据准备
- 无额外测试数据准备，所有测试通过单元测试和临时目录自包含

---

## 验收项目

### 场景 1：Fork 路径——消息继承与工具集

#### - [x] 1.1 Fork 子 agent 继承父消息历史
- **来源:** spec-plan.md Task 3 §2 / spec-design.md §1.2
- **目的:** 验证消息深拷贝正确传递
- **操作步骤:**
  1. [A] `cargo test -p peri-middlewares --lib -- test_fork_inherits_parent_messages 2>&1 | tail -5` → 期望包含: `ok`

#### - [x] 1.2 Fork 子 agent 工具集包含 Agent（未硬编码排除）
- **来源:** spec-plan.md Task 3 §3 / spec-design.md §1.4
- **目的:** 验证全量工具继承保持 cache 命中
- **操作步骤:**
  1. [A] `cargo test -p peri-middlewares --lib -- test_fork_registers_all_tools_including_agent 2>&1 | tail -5` → 期望包含: `ok`

#### - [x] 1.3 Fork directive 包含规则约束
- **来源:** spec-plan.md Task 1 §test_fork_directive_includes_rules / spec-design.md §1.3
- **目的:** 验证防递归指令模板正确注入
- **操作步骤:**
  1. [A] `cargo test -p peri-middlewares --lib -- test_fork_directive_includes_rules 2>&1 | tail -5` → 期望包含: `ok`

#### - [x] 1.4 Fork 路径无 parent_messages 时返回错误
- **来源:** spec-plan.md Task 1 §test_fork_without_parent_messages_returns_error
- **目的:** 验证边界条件处理
- **操作步骤:**
  1. [A] `cargo test -p peri-middlewares --lib -- test_fork_without_parent_messages_returns_error 2>&1 | tail -5` → 期望包含: `ok`

#### - [x] 1.5 Fork 子 agent system prompt 与父一致
- **来源:** spec-plan.md Task 1 §test_fork_system_prompt_consistent / spec-design.md §1.5
- **目的:** 验证 cache 命中前提
- **操作步骤:**
  1. [A] `cargo test -p peri-middlewares --lib -- test_fork_system_prompt_consistent 2>&1 | tail -5` → 期望包含: `ok`

---

### 场景 2：Fork 路径——中间件消息传递

#### - [x] 2.1 before_agent 正确快照父消息
- **来源:** spec-plan.md Task 1 §test_before_agent_snapshots_messages
- **目的:** 验证快照时机在 prepend 之前
- **操作步骤:**
  1. [A] `cargo test -p peri-middlewares --lib -- test_before_agent_snapshots_messages 2>&1 | tail -5` → 期望包含: `ok`

#### - [x] 2.2 build_tool 正确传递 parent_messages
- **来源:** spec-plan.md Task 1 §test_build_tool_receives_parent_messages
- **目的:** 验证共享引用传递链完整
- **操作步骤:**
  1. [A] `cargo test -p peri-middlewares --lib -- test_build_tool_receives_parent_messages 2>&1 | tail -5` → 期望包含: `ok`

---

### 场景 3：System Prompt Agent 指导优化

#### - [x] 3.1 `{{available_agents}}` 占位符被替换为 agent 列表
- **来源:** spec-plan.md Task 2 §test_available_agents_placeholder_replaced / spec-design.md §3.3
- **目的:** 验证动态注入机制生效
- **操作步骤:**
  1. [A] `cargo test -p peri-tui --lib -- test_available_agents_placeholder_replaced 2>&1 | tail -5` → 期望包含: `ok`

#### - [x] 3.2 无 agent 文件时显示提示信息
- **来源:** spec-plan.md Task 2 §test_available_agents_placeholder_empty_dir
- **目的:** 验证空目录边界处理
- **操作步骤:**
  1. [A] `cargo test -p peri-tui --lib -- test_available_agents_placeholder_empty_dir 2>&1 | tail -5` → 期望包含: `ok`

#### - [x] 3.3 subagent 禁用时整个段落不注入
- **来源:** spec-plan.md Task 2 §test_available_agents_not_replaced_when_subagent_disabled
- **目的:** 验证 Feature-gated 条件注入
- **操作步骤:**
  1. [A] `cargo test -p peri-tui --lib -- test_available_agents_not_replaced_when_subagent_disabled 2>&1 | tail -5` → 期望包含: `ok`

#### - [x] 3.4 before_agent 不再注入 agent 摘要
- **来源:** spec-plan.md Task 2 §test_before_agent_no_longer_injects_summary
- **目的:** 验证旧注入逻辑已移除
- **操作步骤:**
  1. [A] `cargo test -p peri-middlewares --lib -- test_before_agent_no_longer_injects_summary 2>&1 | tail -5` → 期望包含: `ok`

#### - [x] 3.5 `scan_agents` 公开可见性
- **来源:** spec-plan.md Task 2 检查步骤
- **目的:** 确认跨 crate 调用可行
- **操作步骤:**
  1. [A] `grep -n 'pub fn scan_agents' peri-middlewares/src/subagent/mod.rs` → 期望包含: `pub fn scan_agents`

#### - [x] 3.6 `build_agents_summary` 已移除
- **来源:** spec-plan.md Task 2 检查步骤
- **目的:** 确认死代码已清理
- **操作步骤:**
  1. [A] `grep -c 'build_agents_summary' peri-middlewares/src/subagent/mod.rs` → 期望精确: `0`

#### - [x] 3.7 `{{available_agents}}` 占位符替换代码存在
- **来源:** spec-plan.md Task 2 检查步骤
- **目的:** 确认 replace 链已追加
- **操作步骤:**
  1. [A] `grep -n 'available_agents' peri-tui/src/prompt.rs` → 期望包含: `format_available_agents`

---

### 场景 4：回归与全量验证

#### - [x] 4.1 全量测试套件通过
- **来源:** spec-plan.md Task 3 §1
- **目的:** 确认无回归
- **操作步骤:**
  1. [A] `cargo test -p peri-middlewares -p peri-tui 2>&1 | tail -20` → 期望包含: `0 failed`

#### - [x] 4.2 Normal 路径行为不变
- **来源:** spec-plan.md Task 3 §7 / spec-design.md §验收标准
- **目的:** 确认现有功能无回归
- **操作步骤:**
  1. [A] `cargo test -p peri-middlewares --lib -- subagent::tool::tests::test_tool_executes_with_valid_agent_file 2>&1 | tail -5` → 期望包含: `ok`

#### - [x] 4.3 编译无新增 warning
- **来源:** spec-plan.md Task 3 §8
- **目的:** 确认无 dead_code / unused_import
- **操作步骤:**
  1. [A] `cargo build -p peri-middlewares -p peri-tui 2>&1 | grep -i warning` → 期望包含: 无输出（空结果）

---

## 验收后清理

- 无需清理（无后台服务启动）

---

## 验收结果汇总

| 场景 | 序号 | 验收项 | [A] | [H] | 结果 |
|------|------|--------|-----|-----|------|
| 场景 1 | 1.1 | Fork 继承父消息历史 | 1 | 0 | ✅ |
| 场景 1 | 1.2 | Fork 工具集包含 Agent | 1 | 0 | ✅ |
| 场景 1 | 1.3 | Fork directive 规则约束 | 1 | 0 | ✅ |
| 场景 1 | 1.4 | Fork 无 parent_messages 返回错误 | 1 | 0 | ✅ |
| 场景 1 | 1.5 | Fork system prompt 一致 | 1 | 0 | ✅ |
| 场景 2 | 2.1 | before_agent 快照父消息 | 1 | 0 | ✅ |
| 场景 2 | 2.2 | build_tool 传递 parent_messages | 1 | 0 | ✅ |
| 场景 3 | 3.1 | available_agents 占位符替换 | 1 | 0 | ✅ |
| 场景 3 | 3.2 | 空 agent 目录提示信息 | 1 | 0 | ✅ |
| 场景 3 | 3.3 | subagent 禁用时段落不注入 | 1 | 0 | ✅ |
| 场景 3 | 3.4 | before_agent 不再注入摘要 | 1 | 0 | ✅ |
| 场景 3 | 3.5 | scan_agents 公开可见性 | 1 | 0 | ✅ |
| 场景 3 | 3.6 | build_agents_summary 已移除 | 1 | 0 | ✅ |
| 场景 3 | 3.7 | 占位符替换代码存在 | 1 | 0 | ✅ |
| 场景 4 | 4.1 | 全量测试套件通过 | 1 | 0 | ✅ |
| 场景 4 | 4.2 | Normal 路径行为不变 | 1 | 0 | ✅ |
| 场景 4 | 4.3 | 编译无新增 warning | 1 | 0 | ✅ |

**验收结论:** ✅ 全部通过
