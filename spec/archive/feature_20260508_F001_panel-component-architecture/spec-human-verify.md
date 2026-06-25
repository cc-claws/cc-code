# 面板组件化架构重构 人工验收清单

**生成时间:** 2026-05-08
**关联计划:** spec-plan-1.md, spec-plan-2.md
**关联设计:** spec-design.md

---

## 验收前准备

### 环境要求
- [x] [AUTO] 检查 Rust 工具链: `rustc --version && cargo --version` → rustc 1.95.0, cargo 1.95.0
- [x] [AUTO] 全量编译: `cargo build -p peri-tui 2>&1 | tail -3` → Finished
- [x] [AUTO] 运行全量测试: `cargo test -p peri-tui 2>&1 | tail -10` → test result: ok (369+1)

---

## 验收项目

### 场景 1：基础设施完整性

验证 PanelManager + PanelComponent 核心类型已定义并正确导出。

#### - [x] 1.1 PanelManager 和 PanelComponent 文件存在
- **来源:** spec-plan-1.md Task 1 / spec-design.md §核心抽象
- **目的:** 确认核心类型文件已创建
- **操作步骤:**
  1. [A] `ls peri-tui/src/app/panel_manager.rs peri-tui/src/app/panel_component.rs 2>&1` → 期望包含: panel_manager.rs 和 panel_component.rs 两个文件路径

#### - [x] 1.2 PanelKind 枚举覆盖 11 个面板
- **来源:** spec-plan-1.md Task 1 检查步骤 / spec-design.md §PanelKind 枚举
- **目的:** 确认所有面板类型在枚举中穷举
- **操作步骤:**
  1. [A] `grep -c "Model\|Login\|Agent\|Hooks\|Config\|ThreadBrowser\|Mcp\|Plugin\|Cron\|Status\|Memory" peri-tui/src/app/panel_manager.rs` → 期望包含: 数值 ≥ 22（枚举定义 + impl match 分支）

#### - [x] 1.3 PanelState 枚举包含 11 个变体
- **来源:** spec-plan-1.md Task 1 检查步骤 / spec-design.md §PanelState 枚举
- **目的:** 确认穷举式枚举存储面板实例
- **操作步骤:**
  1. [A] `grep "PanelState::" peri-tui/src/app/panel_manager.rs | grep -v "//" | wc -l` → 期望包含: ≥ 33（kind + as_any_ref + as_any_mut 各 11）

#### - [x] 1.4 模块声明和 re-export 完整
- **来源:** spec-plan-1.md Task 1
- **目的:** 确认其他文件可引用新类型
- **操作步骤:**
  1. [A] `grep "pub use panel" peri-tui/src/app/mod.rs` → 期望包含: PanelComponent, EventResult, PanelContext, PanelKind, PanelManager, PanelScope, PanelState

---

### 场景 2：PanelManager 实例和跨作用域互斥

验证双 PanelManager 实例已注入 AppCore/App，且跨作用域互斥正确工作。

#### - [x] 2.1 AppCore 包含 session_panels 字段
- **来源:** spec-plan-1.md Task 2 / spec-design.md §面板作用域
- **目的:** 确认 session-scoped 面板管理器已注入
- **操作步骤:**
  1. [A] `grep "session_panels" peri-tui/src/app/core.rs` → 期望包含: PanelManager 相关声明

#### - [x] 2.2 App 包含 global_panels 字段
- **来源:** spec-plan-1.md Task 2 / spec-design.md §面板作用域
- **目的:** 确认 global-scoped 面板管理器已注入
- **操作步骤:**
  1. [A] `grep "global_panels" peri-tui/src/app/mod.rs | head -5` → 期望包含: PanelManager 相关声明和初始化

#### - [x] 2.3 open_panel 和 close_all_panels 方法存在
- **来源:** spec-plan-1.md Task 2
- **目的:** 确认统一面板操作入口
- **操作步骤:**
  1. [A] `grep -n "pub fn open_panel\|pub fn close_all_panels" peri-tui/src/app/mod.rs` → 期望包含: 两个方法声明

#### - [x] 2.4 CronPanel 从 CronState 迁移到 global_panels
- **来源:** spec-plan-1.md Task 2
- **目的:** 确认 CronPanel 不再在 CronState 中
- **操作步骤:**
  1. [A] `grep "cron_panel" peri-tui/src/app/cron_state.rs` → 期望包含: 无匹配（或仅函数名引用）

#### - [x] 2.5 cron_ops.rs 不再引用 self.cron.cron_panel
- **来源:** spec-plan-1.md Task 2
- **目的:** 确认 cron_ops 全部迁移到 global_panels
- **操作步骤:**
  1. [A] `grep "self.cron.cron_panel" peri-tui/src/app/cron_ops.rs` → 期望精确: （无输出）

---

### 场景 3：PanelComponent 完整实现

验证所有 11 个面板已实现 PanelComponent trait。

#### - [x] 3.1 PanelComponent 实现数量为 11
- **来源:** spec-plan-2.md Acceptance / spec-design.md §PanelComponent Trait
- **目的:** 确认所有面板都实现了统一接口
- **操作步骤:**
  1. [A] `grep -rl "impl PanelComponent for" peri-tui/src/ | wc -l` → 期望包含: 11

#### - [x] 3.2 列出所有 PanelComponent 实现文件
- **来源:** spec-plan-2.md Acceptance
- **目的:** 确认覆盖 Model/Login/Agent/Hooks/Config/ThreadBrowser/Mcp/Plugin/Cron/Status/Memory
- **操作步骤:**
  1. [A] `grep -rl "impl PanelComponent for" peri-tui/src/` → 期望包含: model_panel.rs, login_panel.rs, agent_panel.rs, hooks_panel.rs, config_panel.rs, browser.rs (ThreadBrowser), mcp_panel.rs, plugin_panel.rs, cron_state.rs, status_panel.rs, memory_panel.rs

---

### 场景 4：事件分发简化

验证 event.rs 中面板分发逻辑已从 15 层 if-else 链简化为 PanelManager 分发。

#### - [x] 4.1 event.rs 中面板 is_some() 检查数量大幅减少
- **来源:** spec-plan-2.md Acceptance / spec-design.md §P1
- **目的:** 确认面板分发走 PanelManager 路径
- **操作步骤:**
  1. [A] `grep -c "if app.*\.is_some()" peri-tui/src/event.rs` → 期望包含: ≤ 5（仅剩 Setup Wizard / OAuth / Interaction Prompt）

#### - [x] 4.2 PanelManager dispatch_key 调用存在
- **来源:** spec-plan-2.md Task 4-5
- **目的:** 确认事件分发通过 PanelManager
- **操作步骤:**
  1. [A] `grep -c "dispatch_key\|dispatch_paste\|dispatch_scroll" peri-tui/src/event.rs` → 期望包含: ≥ 2

#### - [x] 4.3 event.rs 无迁移残留注释
- **来源:** spec-plan-2.md Task 7
- **目的:** 确认清理完成
- **操作步骤:**
  1. [A] `grep -c "Task [345].*已迁移\|panel_ops\|旧面板分发" peri-tui/src/event.rs` → 期望包含: 0

---

### 场景 5：旧字段迁移完成

验证旧 Option<XxxPanel> 字段已全部从 AppCore 和 App 中移除。

#### - [x] 5.1 core.rs 中无旧 session 面板字段
- **来源:** spec-plan-2.md Task 7 / Acceptance
- **目的:** 确认 6 个旧 Option 字段已移除
- **操作步骤:**
  1. [A] `grep -c "model_panel:\|login_panel:\|agent_panel:\|hooks_panel:\|config_panel:\|thread_browser:" peri-tui/src/app/core.rs` → 期望包含: 0（注释中的引用可忽略）

#### - [x] 5.2 App 中无旧 global 面板字段
- **来源:** spec-plan-2.md Task 7 / Acceptance
- **目的:** 确认 4 个旧 Option 字段已移除
- **操作步骤:**
  1. [A] `grep -E "Option<.*Panel>" peri-tui/src/app/mod.rs` → 期望包含: 无匹配（或仅 setup_wizard 等特殊面板）

---

### 场景 6：unwrap 消除

验证 PanelComponent impl 块中无 unwrap 调用。

#### - [x] 6.1 LoginPanel PanelComponent 无 unwrap
- **来源:** spec-plan-2.md Acceptance / spec-design.md §P3
- **目的:** 确认最复杂的面板消除了 unwrap
- **操作步骤:**
  1. [A] `grep -rn "\.unwrap()" peri-tui/src/app/login_panel.rs | grep -v "#\[cfg(test)\]" | grep -v "mod tests" | grep -v "fn test_"` → 期望包含: 无匹配

#### - [x] 6.2 ModelPanel PanelComponent 无 unwrap
- **来源:** spec-plan-2.md Acceptance / spec-design.md §P3
- **目的:** 确认 Model 面板消除了 unwrap
- **操作步骤:**
  1. [A] `grep "\.unwrap()" peri-tui/src/app/model_panel.rs | grep -v "test"` → 期望包含: 无匹配

---

### 场景 7：状态栏解耦

验证 status_bar.rs 不再硬编码各面板名称。

#### - [x] 7.1 status_bar.rs 无面板名硬编码
- **来源:** spec-plan-2.md Acceptance / spec-design.md §P4
- **目的:** 确认状态栏通过 PanelManager 查询快捷键
- **操作步骤:**
  1. [A] `grep -c "login_panel\|model_panel\|config_panel\|agent_panel\|hooks_panel\|mcp_panel\|status_panel\|memory_panel\|plugin_panel\|thread_browser\|cron_panel" peri-tui/src/ui/main_ui/status_bar.rs` → 期望包含: 0

---

### 场景 8：渲染分发迁移

验证 main_ui.rs 渲染逻辑通过 PanelManager 查询。

#### - [x] 8.1 渲染分发使用 active_kind
- **来源:** spec-plan-2.md Task 6
- **目的:** 确认渲染通过 PanelManager 查询面板类型
- **操作步骤:**
  1. [A] `grep -c "active_kind\|dispatch_desired_height" peri-tui/src/ui/main_ui.rs` → 期望包含: ≥ 2

---

### 场景 9：CLAUDE.md 更新

验证项目文档反映了新架构。

#### - [x] 9.1 CLAUDE.md 包含面板组件化架构说明
- **来源:** spec-plan-2.md Task 7
- **目的:** 确认文档已更新
- **操作步骤:**
  1. [A] `grep -c "面板组件化架构\|PanelManager\|PanelComponent\|PanelState" CLAUDE.md` → 期望包含: ≥ 3

#### - [x] 9.2 状态栏说明已更新为 PanelManager 驱动
- **来源:** spec-plan-2.md Task 7
- **目的:** 确认快捷键说明反映新架构
- **操作步骤:**
  1. [A] `grep "status_bar_hints" CLAUDE.md` → 期望包含: status_bar_hints 方法引用

---

### 场景 10：编译和测试质量

验证代码质量和测试覆盖。

#### - [x] 10.1 全量编译通过
- **来源:** spec-plan-2.md Acceptance
- **目的:** 确认编译无错误
- **操作步骤:**
  1. [A] `cargo build -p peri-tui 2>&1 | tail -3` → 期望包含: Finished

#### - [x] 10.2 全量测试通过
- **来源:** spec-plan-1.md + spec-plan-2.md Acceptance
- **目的:** 确认所有测试通过
- **操作步骤:**
  1. [A] `cargo test -p peri-tui 2>&1 | tail -5` → 期望包含: test result: ok

#### - [x] 10.3 clippy 无新增警告
- **来源:** spec-plan-1.md + spec-plan-2.md Acceptance / spec-design.md
- **目的:** 确认代码质量
- **操作步骤:**
  1. [A] `cargo clippy -p peri-tui 2>&1 | grep -E "warning|error" | grep -v "generated" | head -5` → 期望包含: 仅预先存在的警告（无来自 panel_manager.rs / panel_component.rs 的警告）

#### - [x] 10.4 PanelManager 单元测试通过
- **来源:** spec-plan-1.md Task 1
- **目的:** 确认核心类型测试覆盖
- **操作步骤:**
  1. [A] `cargo test -p peri-tui --lib -- panel_manager::tests 2>&1 | tail -5` → 期望包含: test result: ok

---

### 场景 11：手动运行时验证

需要手动启动 TUI 应用验证面板交互。

#### - [x] 11.1 Model 面板打开/关闭/选择
- **来源:** spec-design.md §验收标准
- **目的:** 确认 Model 面板交互正常
- **操作步骤:**
  1. [H] 启动 TUI `cargo run -p peri-tui`，输入 `/model` → 面板弹出，显示模型列表
  2. [H] ↑↓ 导航 → 光标移动
  3. [H] Enter 选择 → 面板关闭，状态栏显示新模型
  4. [H] Esc 关闭 → 面板关闭 → 是/否

#### - [x] 11.2 Login 面板 Provider 管理
- **来源:** spec-design.md §验收标准
- **目的:** 确认 Login 面板完整流程
- **操作步骤:**
  1. [H] 输入 `/login` → 面板弹出，显示 Provider 列表
  2. [H] Tab 进入编辑 → 编辑模式正常
  3. [H] Ctrl+N 新建 → 新建模式正常
  4. [H] Enter 保存 → 面板关闭，Provider 已激活 → 是/否

#### - [x] 11.3 MCP 面板浏览和切换
- **来源:** spec-design.md §验收标准
- **目的:** 确认 MCP 面板功能正常
- **操作步骤:**
  1. [H] 输入 `/mcp` → 面板弹出（需有 MCP 配置）
  2. [H] ↑↓ 导航 → 光标移动
  3. [H] Esc 关闭 → 面板关闭 → 是/否

#### - [x] 11.4 面板互斥验证
- **来源:** spec-design.md §验收标准 / spec-plan-1.md Task 2
- **目的:** 确认跨作用域互斥正常
- **操作步骤:**
  1. [H] 打开 `/model`（session） → 再打开 `/mcp`（global） → Model 面板自动关闭，MCP 面板显示 → 是/否
  2. [H] 打开 `/status`（global） → 再打开 `/login`（session） → Status 面板自动关闭 → 是/否

#### - [x] 11.5 状态栏快捷键显示
- **来源:** spec-design.md §P4
- **目的:** 确认快捷键随面板切换更新
- **操作步骤:**
  1. [H] 打开任意面板 → 状态栏第二行显示对应快捷键 → 是/否
  2. [H] 关闭面板 → 快捷键消失 → 是/否

#### - [ ] 11.6 多 session 面板独立性
- **来源:** spec-design.md §验收标准
- **目的:** 确认分屏下 session 面板状态独立
- **操作步骤:**
  1. [H] 多 session 分屏下，在 A session 打开 Model 面板 → B session 不受影响 → 是/否

---

## 验收后清理

无需清理（无后台服务）。

---

## 验收结果汇总

| 场景 | 序号 | 验收项 | [A] | [H] | 结果 |
|------|------|--------|-----|-----|------|
| 场景 1 | 1.1 | PanelManager/PanelComponent 文件存在 | 1 | 0 | ✅ |
| 场景 1 | 1.2 | PanelKind 枚举覆盖 11 面板 | 1 | 0 | ✅ |
| 场景 1 | 1.3 | PanelState 枚举 11 变体 | 1 | 0 | ✅ |
| 场景 1 | 1.4 | 模块声明和 re-export | 1 | 0 | ✅ |
| 场景 2 | 2.1 | AppCore session_panels | 1 | 0 | ✅ |
| 场景 2 | 2.2 | App global_panels | 1 | 0 | ✅ |
| 场景 2 | 2.3 | open_panel/close_all_panels | 1 | 0 | ✅ |
| 场景 2 | 2.4 | CronPanel 迁移到 global_panels | 1 | 0 | ✅ |
| 场景 2 | 2.5 | cron_ops 无旧引用 | 1 | 0 | ✅ |
| 场景 3 | 3.1 | PanelComponent 实现 11 个 | 1 | 0 | ✅ |
| 场景 3 | 3.2 | 所有面板文件覆盖 | 1 | 0 | ✅ |
| 场景 4 | 4.1 | event.rs is_some ≤ 5 | 1 | 0 | ✅ |
| 场景 4 | 4.2 | PanelManager dispatch 调用 | 1 | 0 | ✅ |
| 场景 4 | 4.3 | 无迁移残留注释 | 1 | 0 | ✅ |
| 场景 5 | 5.1 | core.rs 无旧字段 | 1 | 0 | ✅ |
| 场景 5 | 5.2 | App 无旧全局字段 | 1 | 0 | ✅ |
| 场景 6 | 6.1 | LoginPanel 无 unwrap | 1 | 0 | ✅ |
| 场景 6 | 6.2 | ModelPanel 无 unwrap | 1 | 0 | ✅ |
| 场景 7 | 7.1 | status_bar 无硬编码面板名 | 1 | 0 | ✅ |
| 场景 8 | 8.1 | 渲染用 active_kind | 1 | 0 | ✅ |
| 场景 9 | 9.1 | CLAUDE.md 面板架构说明 | 1 | 0 | ✅ |
| 场景 9 | 9.2 | 状态栏说明已更新 | 1 | 0 | ✅ |
| 场景 10 | 10.1 | 编译通过 | 1 | 0 | ✅ |
| 场景 10 | 10.2 | 测试通过 | 1 | 0 | ✅ |
| 场景 10 | 10.3 | clippy 无新警告 | 1 | 0 | ✅ |
| 场景 10 | 10.4 | PanelManager 单元测试 | 1 | 0 | ✅ |
| 场景 11 | 11.1 | Model 面板交互 | 0 | 1 | ✅ |
| 场景 11 | 11.2 | Login 面板交互 | 0 | 1 | ✅ |
| 场景 11 | 11.3 | MCP 面板交互 | 0 | 1 | ✅ |
| 场景 11 | 11.4 | 面板互斥验证 | 0 | 1 | ✅ |
| 场景 11 | 11.5 | 状态栏快捷键 | 0 | 1 | ✅ |
| 场景 11 | 11.6 | 多 session 独立性 | 0 | 1 | ⬜ |

**验收结论:** ⬜ 存在问题（1 项跳过）
