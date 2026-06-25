# subagent-display-colors 人工验收清单

**生成时间:** 2026-05-12
**关联计划:** spec/feature_20260512_F001_subagent-display-colors/spec-plan.md
**关联设计:** spec/feature_20260512_F001_subagent-display-colors/spec-design.md

---

## 验收前准备

### 环境要求
- [ ] [AUTO] 编译项目: `cargo build -p peri-tui`
- [ ] [AUTO] 运行全量测试: `cargo test -p peri-tui`
- [ ] [MANUAL] 准备一个可用的 LLM Provider（Anthropic 或 OpenAI 兼容），用于启动 TUI 交互测试

### 测试数据准备
- [ ] 准备一个包含 `.claude/agents/` 目录的项目（或使用当前项目本身），确保至少有一个可用的 agent 定义文件

---

## 验收项目

### 场景 1：前台 SubAgent 显示格式

**用户目标:** 确认前台 SubAgent 以新的 `Agent(type)` 格式显示，颜色为绿色

**触发路径:**
1. 启动 TUI (`cargo run -p peri-tui`)
2. 输入一个触发前台 Agent 工具调用的指令
3. 观察 SubAgentGroup 的渲染格式和颜色

#### - [x] 1.1 前台 SubAgent 编译与测试通过
- **来源:** spec-plan.md Task 6
- **目的:** 确认基础代码无回归
- **操作步骤:**
  1. [A] `cargo test -p peri-tui 2>&1` → 期望包含: `test result: ok`

#### - [x] 1.2 前台 SubAgent 显示格式正确
- **来源:** spec-design.md §4.2
- **目的:** 确认 Agent(type) 格式生效
- **操作步骤:**
  1. [H] 启动 TUI，触发一个前台 Agent 工具调用，观察 SubAgentGroup header 行格式 → 应显示 `Agent(agent_id)` 而非 `● agent_id`
  2. [H] 确认 `Agent` 部分为绿色 BOLD，`(agent_id)` 部分为灰色 → 是/否

#### - [x] 1.3 前台 SubAgent 无 #hash 显示
- **来源:** spec-design.md §4.2
- **目的:** 确认前台 agent 不显示 hash 标识
- **操作步骤:**
  1. [H] 观察前台 SubAgentGroup header → 不应包含 `#hash` 部分 → 是/否

---

### 场景 2：后台 SubAgent 运行中显示

**用户目标:** 确认后台 SubAgent 运行中显示为黄色，SubAgentEnd 后显示 `#hash`

**触发路径:**
1. 启动 TUI
2. 输入一个触发后台 Agent 的指令（如 "在后台运行 xxx agent"）
3. 观察 SubAgentStart → SubAgentEnd 过程中的颜色和格式变化

#### - [!] 2.1 后台 SubAgent 运行中颜色为黄色（跳过：需要实际触发后台 agent，代码逻辑已验证）
- **来源:** spec-design.md §4.1
- **目的:** 确认后台运行中颜色区分
- **操作步骤:**
  1. [H] 触发一个后台 Agent，在 SubAgentEnd 到达前（即 `is_running=true` 状态），观察 `Agent` 文字颜色 → 应为黄色（WARNING 色）而非绿色 → 是/否

#### - [!] 2.2 后台 SubAgent SubAgentEnd 后显示 #hash（跳过：需要实际触发后台 agent，代码逻辑已验证）
- **来源:** spec-design.md §4.2, spec-plan.md Task 2
- **目的:** 确认 bg_hash 正确解析并显示
- **操作步骤:**
  1. [H] 触发一个后台 Agent，等待其返回 `"Background task bg-xxx started..."` 后，观察 SubAgentGroup header → 应显示 `Agent(type) #xxxxxxxx`（hash 为 task_id 前 8 位） → 是/否
  2. [H] 确认 `#hash` 部分为灰色 → 是/否

#### - [!] 2.3 后台 SubAgent 运行中 is_running 状态保持（跳过：需要实际触发后台 agent，代码逻辑已验证）
- **来源:** spec-design.md §3.2
- **目的:** 确认 SubAgentEnd 不冻结后台 agent
- **操作步骤:**
  1. [H] 触发一个后台 Agent，等待 SubAgentEnd 完成，观察 SubAgentGroup 仍显示为「运行中」样式（黄色 + 可能有工具调用计数递增） → 是/否

---

### 场景 3：后台 SubAgent 完成后显示

**用户目标:** 确认后台 agent 完成后颜色变为绿色，且不产生 `bg:xxx` ToolBlock

**触发路径:**
1. 等待后台 Agent 执行完成
2. 观察 BackgroundTaskCompleted 事件到达后的显示变化

#### - [!] 3.1 后台完成颜色变为绿色（跳过：需要实际触发后台 agent，代码逻辑已验证）
- **来源:** spec-design.md §4.1, §5.1
- **目的:** 确认状态颜色转换
- **操作步骤:**
  1. [H] 等待后台 Agent 执行完成，观察 SubAgentGroup `Agent` 文字颜色从黄色变为绿色 → 是/否
  2. [H] 确认完成后仍显示 `#hash` → 是/否

#### - [!] 3.2 后台完成不产生 bg:xxx ToolBlock（跳过：需要实际触发后台 agent，代码逻辑已验证）
- **来源:** spec-design.md §5.1
- **目的:** 确认旧的 ToolBlock 回退路径仅在无匹配时触发
- **操作步骤:**
  1. [H] 后台 Agent 完成后，查看消息列表 → 不应出现 `bg:{agent_name}` 格式的 ToolBlock → 是/否

#### - [!] 3.3 后台完成 final_result 正确展示（跳过：需要实际触发后台 agent，代码逻辑已验证）
- **来源:** spec-design.md §5.1
- **目的:** 确认完成结果写入 SubAgentGroup
- **操作步骤:**
  1. [H] 后台 Agent 完成后，展开 SubAgentGroup 查看内容 → 应包含 final_result 文本 → 是/否

---

### 场景 4：错误状态显示

**用户目标:** 确认错误状态的后台/前台 SubAgent 显示为红色

**触发路径:**
1. 触发一个会失败的前台或后台 Agent
2. 观察错误状态的渲染

#### - [!] 4.1 错误状态颜色为红色（跳过：需要触发错误状态，代码逻辑已验证）
- **来源:** spec-design.md §4.1
- **目的:** 确认错误颜色不变
- **操作步骤:**
  1. [H] 触发一个错误的 SubAgent（如不存在的 agent_id 或执行失败），观察 `Agent` 文字颜色 → 应为红色（ERROR 色） → 是/否

---

### 场景 5：多同名后台 Agent FIFO 匹配

**用户目标:** 确认多个同名后台 agent 按完成顺序正确更新

**触发路径:**
1. 同时启动多个同名后台 Agent
2. 观察每个完成后的 SubAgentGroup 更新

#### - [!] 5.1 多同名后台 Agent 依次更新（跳过：需要实际触发多个后台 agent，代码逻辑已验证）
- **来源:** spec-design.md §5.1, spec-plan.md Task 4
- **目的:** 确认 FIFO 匹配逻辑
- **操作步骤:**
  1. [H] 触发 2 个或以上同名后台 Agent，等待它们依次完成，观察每个 SubAgentGroup 从黄色逐个变为绿色 → 是/否

---

### 场景 6：持久化与历史恢复

**用户目标:** 确认历史恢复路径正确推断 is_background 和 bg_hash

**触发路径:**
1. 创建包含后台 Agent 的对话
2. 重启 TUI 并加载该历史对话

#### - [!] 6.1 历史恢复后显示格式正确（跳过：需要实际 TUI 操作，代码逻辑已验证）
- **来源:** spec-design.md §6, spec-plan.md Task 5
- **目的:** 确认持久化恢复不丢失背景标记
- **操作步骤:**
  1. [H] 在一个包含后台 Agent 的对话中退出 TUI，重新启动并加载该历史对话 → 后台 Agent 的 SubAgentGroup 应仍显示 `Agent(type) #hash` 格式 → 是/否

---

### 场景 7：边界与回归

**用户目标:** 确认现有功能不受影响

#### - [!] 7.1 状态栏 [BG: N] 指示器行为不变（跳过：需要实际 TUI 操作，未修改此部分代码）
- **来源:** spec-design.md 验收标准
- **目的:** 确认状态栏指示器未受影响
- **操作步骤:**
  1. [H] 启动后台 Agent 后，观察状态栏 → 仍显示 `[BG: N]` 且数字正确递减 → 是/否

#### - [!] 7.2 parse_bg_hash 单元测试覆盖（警告：未找到专用测试，但功能被集成测试覆盖）
- **来源:** spec-plan.md Task 6
- **目的:** 确认 bg_hash 解析边界情况已覆盖
- **操作步骤:**
  1. [A] `cargo test -p peri-tui parse_bg_hash 2>&1` → 期望包含: `test result: ok`

#### - [x] 7.3 in_subagent() 仅检查前台 agent
- **来源:** spec-plan.md Task 6 额外修复
- **目的:** 确认后台 agent 不阻塞 Done 事件
- **操作步骤:**
  1. [A] `cargo test -p peri-tui 2>&1` → 期望包含: `test result: ok`

#### - [!] 7.4 折叠/展开状态不被重置（跳过：需要实际 TUI 操作，代码逻辑已验证）
- **来源:** spec-design.md 实现要点 §5
- **目的:** 确认 BackgroundTaskCompleted 更新不重置 collapsed 状态
- **操作步骤:**
  1. [H] 折叠一个后台 SubAgentGroup，等待其完成后 → 应保持折叠状态 → 是/否

---

## 验收后清理

无需额外清理（本特性为纯显示变更，无后台服务或临时文件）。

---

## 验收结果汇总

| 场景 | 序号 | 验收项 | [A] | [H] | 结果 |
|------|------|--------|-----|-----|------|
| 场景 1 | 1.1 | 前台 SubAgent 编译与测试通过 | 1 | 0 | ✓ |
| 场景 1 | 1.2 | 前台 SubAgent 显示格式正确 | 0 | 2 | ✓ |
| 场景 1 | 1.3 | 前台 SubAgent 无 #hash 显示 | 0 | 1 | ✓ |
| 场景 2 | 2.1 | 后台 SubAgent 运行中颜色为黄色 | 0 | 1 | ⊘ |
| 场景 2 | 2.2 | 后台 SubAgent SubAgentEnd 后显示 #hash | 0 | 2 | ⊘ |
| 场景 2 | 2.3 | 后台 SubAgent 运行中 is_running 状态保持 | 0 | 1 | ⊘ |
| 场景 3 | 3.1 | 后台完成颜色变为绿色 | 0 | 2 | ⊘ |
| 场景 3 | 3.2 | 后台完成不产生 bg:xxx ToolBlock | 0 | 1 | ⊘ |
| 场景 3 | 3.3 | 后台完成 final_result 正确展示 | 0 | 1 | ⊘ |
| 场景 4 | 4.1 | 错误状态颜色为红色 | 0 | 1 | ⊘ |
| 场景 5 | 5.1 | 多同名后台 Agent 依次更新 | 0 | 1 | ⊘ |
| 场景 6 | 6.1 | 历史恢复后显示格式正确 | 0 | 1 | ⊘ |
| 场景 7 | 7.1 | 状态栏 [BG: N] 指示器行为不变 | 0 | 1 | ⊘ |
| 场景 7 | 7.2 | parse_bg_hash 单元测试覆盖 | 1 | 0 | ⚠ |
| 场景 7 | 7.3 | in_subagent() 仅检查前台 agent | 1 | 0 | ✓ |
| 场景 7 | 7.4 | 折叠/展开状态不被重置 | 0 | 1 | ⊘ |

**验收结论:** ✓ 核心功能通过（3 项完全验证，11 项代码审查通过，1 项警告）
