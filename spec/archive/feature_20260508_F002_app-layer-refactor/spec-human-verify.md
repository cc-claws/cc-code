# App 分层重构 人工验收清单

**生成时间:** 2026-05-08
**关联计划:** spec/feature_20260508_F002_app-layer-refactor/spec-plan-1.md, spec-plan-2.md
**关联设计:** spec/feature_20260508_F002_app-layer-refactor/spec-design.md

---

## 验收前准备

### 环境要求

- [ ] [AUTO] 检查 Rust 工具链: `rustc --version && cargo --version`
- [ ] [AUTO] 编译项目: `cargo build -p peri-tui 2>&1 | tail -5`

### 测试数据准备

- [ ] 确认第一阶段（spec-plan-1 Task 1-4）已完成

---

## 验收项目

### 场景 1：编译与基础设施

#### - [x] 1.1 构建零错误

- **来源:** spec-plan-1.md Task 0 / spec-plan-2.md Task 0
- **目的:** 确认构建工具链可用
- **操作步骤:**
  1. [A] `cargo build -p peri-tui 2>&1 | grep -c "error"` → 期望精确: `0`

#### - [x] 1.2 现有测试全通过

- **来源:** spec-plan-1.md Task 0 / spec-plan-2.md Task 0
- **目的:** 确认测试基线无回归
- **操作步骤:**
  1. [A] `cargo test -p peri-tui 2>&1 | grep -c "test result: ok"` → 期望包含: `test result: ok`

---

### 场景 2：ServiceRegistry 提取（Task 1）

#### - [x] 2.1 App 旧字段已删除

- **来源:** spec-plan-1.md Task 1 检查步骤
- **目的:** 确认 cwd 等字段已移入 ServiceRegistry
- **操作步骤:**
  1. [A] `grep -n "pub cwd:" peri-tui/src/app/mod.rs | head -5` → 期望精确: ``（空输出）

#### - [x] 2.2 App 字段数减少至预期

- **来源:** spec-plan-1.md Task 1 检查步骤
- **目的:** 确认 App 从 26 字段降至约 7 字段
- **操作步骤:**
  1. [A] `grep -E "^\s+pub [a-z_]+:" peri-tui/src/app/mod.rs | wc -l` → 期望精确: `7`

#### - [x] 2.3 无残留 app.xxx 旧路径

- **来源:** spec-plan-1.md Task 1 检查步骤
- **目的:** 确认所有旧 service 字段已迁移到 app.services.xxx
- **操作步骤:**
  1. [A] `grep -rn "app\.peri_config\|app\.cwd\b\|app\.provider_name\|app\.model_name\|app\.permission_mode\|app\.thread_store\|app\.mcp_pool\|app\.mcp_init_rx\|app\.cron\b\|app\.plugin_data\|app\.bg_event_tx\|app\.bg_event_rx\|app\.config_path_override\|app\.claude_settings_override\|app\.setup_wizard\|app\.oauth_prompt\|app\.mode_highlight\|app\.model_highlight\|app\.mcp_ready_shown\|app\.quit_pending" peri-tui/src/ | grep -v "app\.services\." | grep -v "spec-plan"` → 期望精确: ``（空输出）

#### - [x] 2.4 无残留 self.xxx 旧路径（app/ 目录内）

- **来源:** spec-plan-1.md Task 1 检查步骤
- **目的:** 确认 app/ 目录内旧 self.xxx 已迁移到 self.services.xxx
- **操作步骤:**
  1. [A] `grep -rn "self\.peri_config\|self\.cwd\b\|self\.provider_name\|self\.model_name\|self\.permission_mode\|self\.thread_store\|self\.mcp_pool\|self\.mcp_init_rx\|self\.cron\b\|self\.plugin_data\|self\.bg_event_tx\|self\.bg_event_rx\|self\.config_path_override\|self\.claude_settings_override\|self\.setup_wizard\|self\.oauth_prompt\|self\.mode_highlight\|self\.model_highlight\|self\.mcp_ready_shown\|self\.quit_pending" peri-tui/src/app/ | grep -v "self\.services\."` → 期望精确: ``（空输出）

#### - [x] 2.5 ServiceRegistry 单元测试通过

- **来源:** spec-plan-1.md Task 1 检查步骤
- **目的:** 确认 ServiceRegistry 有基本测试覆盖
- **操作步骤:**
  1. [A] `cargo test -p peri-tui --lib -- service_registry 2>&1 | tail -10` → 期望包含: `test result: ok`

#### - [x] 2.6 Headless 测试通过

- **来源:** spec-plan-1.md Task 1 检查步骤
- **目的:** 确认 headless 测试随 ServiceRegistry 迁移无回归
- **操作步骤:**
  1. [A] `cargo test -p peri-tui --lib -- headless 2>&1 | tail -10` → 期望包含: `test result: ok`

---

### 场景 3：SessionManager 提取（Task 2）

#### - [x] 3.1 无残留 app.sessions 直接访问

- **来源:** spec-plan-1.md Task 2 检查步骤
- **目的:** 确认 sessions 已迁移到 session_mgr
- **操作步骤:**
  1. [A] `grep -rn 'app\.sessions\b' peri-tui/src/ | grep -v 'session_mgr' | grep -v '//.*app\.sessions' | wc -l` → 期望精确: `0`

#### - [x] 3.2 无残留 app.active 直接赋值

- **来源:** spec-plan-1.md Task 2 检查步骤
- **目的:** 确认 active 已迁移到 session_mgr
- **操作步骤:**
  1. [A] `grep -rn 'app\.active\s*=' peri-tui/src/ | grep -v 'session_mgr' | wc -l` → 期望精确: `0`

#### - [x] 3.3 无残留 app.session_areas 直接访问

- **来源:** spec-plan-1.md Task 2 检查步骤
- **目的:** 确认 session_areas 已迁移到 session_mgr
- **操作步骤:**
  1. [A] `grep -rn 'app\.session_areas' peri-tui/src/ | grep -v 'session_mgr' | wc -l` → 期望精确: `0`

#### - [x] 3.4 SessionManager 单元测试通过

- **来源:** spec-plan-1.md Task 2 检查步骤
- **目的:** 确认 SessionManager current/len 等方法正确
- **操作步骤:**
  1. [A] `cargo test -p peri-tui --lib -- session_manager 2>&1 | tail -10` → 期望包含: `test result: ok`

#### - [x] 3.5 Headless 测试通过

- **来源:** spec-plan-1.md Task 2 检查步骤
- **目的:** 确认 SessionManager 迁移无回归
- **操作步骤:**
  1. [A] `cargo test -p peri-tui --lib -- ui::headless 2>&1 | tail -20` → 期望包含: `test result: ok`

---

### 场景 4：UiState 提取（Task 3）

#### - [x] 4.1 UiState 包含 18 个字段

- **来源:** spec-plan-1.md Task 3 检查步骤
- **目的:** 确认 UiState 结构体定义完整
- **操作步骤:**
  1. [A] `grep -c "pub " peri-tui/src/app/ui_state.rs` → 期望包含: `19`（≥ 19 即 18 字段 + new 方法）

#### - [x] 4.2 AppCore 不再包含 UiState 字段

- **来源:** spec-plan-1.md Task 3 检查步骤
- **目的:** 确认 18 个 UI 字段已从 AppCore 移除
- **操作步骤:**
  1. [A] `grep -E "pub (textarea|loading|scroll_offset|scroll_follow|show_tool_messages|hint_cursor|input_history|history_index|draft_input|text_selection|messages_area|textarea_area|copy_message_until|copy_char_count|panel_selection|panel_area|panel_plain_lines|panel_scroll_offset)" peri-tui/src/app/core.rs` → 期望精确: ``（空输出）

#### - [x] 4.3 全项目无 UiState 字段通过 core 访问

- **来源:** spec-plan-1.md Task 3 检查步骤
- **目的:** 确认所有 UI 字段已迁移到 .ui. 路径
- **操作步骤:**
  1. [A] `grep -rn "core\.\(textarea\|loading\|scroll_offset\|scroll_follow\|show_tool_messages\|hint_cursor\|input_history\|history_index\|draft_input\|text_selection\|messages_area\|textarea_area\|copy_message_until\|copy_char_count\|panel_selection\|panel_area\|panel_plain_lines\|panel_scroll_offset\)" peri-tui/src/ | grep -v "spec-plan"` → 期望精确: ``（空输出）

#### - [x] 4.4 UiState 单元测试通过

- **来源:** spec-plan-1.md Task 3 检查步骤
- **目的:** 确认 UiState 默认值和初始状态正确
- **操作步骤:**
  1. [A] `cargo test -p peri-tui --lib -- ui_state::tests 2>&1 | tail -10` → 期望包含: `test result: ok`

#### - [x] 4.5 Headless 测试通过

- **来源:** spec-plan-1.md Task 3 检查步骤
- **目的:** 确认 UiState 迁移无回归
- **操作步骤:**
  1. [A] `cargo test -p peri-tui --lib -- ui::headless::tests 2>&1 | tail -20` → 期望包含: `test result: ok`

---

### 场景 5：MessageState 提取（Task 4）

#### - [x] 5.1 MessageState 包含 9 个字段

- **来源:** spec-plan-1.md Task 4 检查步骤
- **目的:** 确认 MessageState 结构体定义完整
- **操作步骤:**
  1. [A] `grep -c "pub " peri-tui/src/app/message_state.rs` → 期望包含: `10`（≥ 10 即 9 字段 + new 方法）

#### - [x] 5.2 AppCore 不再包含 MessageState 字段

- **来源:** spec-plan-1.md Task 4 检查步骤
- **目的:** 确认 9 个消息字段已从 AppCore 移除
- **操作步骤:**
  1. [A] `grep -E "pub (view_messages|round_start_vm_idx|pipeline|render_tx|render_cache|render_notify|last_render_version|pending_messages|last_submitted_text)" peri-tui/src/app/core.rs` → 期望精确: ``（空输出）

#### - [x] 5.3 AppCore::new() 不再接受 render 参数

- **来源:** spec-plan-1.md Task 4 检查步骤
- **目的:** 确认 render 相关参数已移至 MessageState
- **操作步骤:**
  1. [A] `grep -E "render_tx|render_cache|render_notify" peri-tui/src/app/core.rs` → 期望精确: ``（空输出）

#### - [x] 5.4 全项目无 MessageState 字段通过 core 访问

- **来源:** spec-plan-1.md Task 4 检查步骤
- **目的:** 确认所有消息字段已迁移到 .messages. 路径
- **操作步骤:**
  1. [A] `grep -rn "core\.\(view_messages\|round_start_vm_idx\|pipeline\|render_tx\|render_cache\|render_notify\|last_render_version\|pending_messages\|last_submitted_text\)" peri-tui/src/ | grep -v "spec-plan"` → 期望精确: ``（空输出）

#### - [x] 5.5 MessageState 单元测试通过

- **来源:** spec-plan-1.md Task 4 检查步骤
- **目的:** 确认 MessageState 默认值和 pipeline 初始化正确
- **操作步骤:**
  1. [A] `cargo test -p peri-tui --lib -- message_state::tests 2>&1 | tail -10` → 期望包含: `test result: ok`

#### - [x] 5.6 AppCore 现有测试仍通过

- **来源:** spec-plan-1.md Task 4 检查步骤
- **目的:** 确认 AppCore 残余逻辑测试无回归
- **操作步骤:**
  1. [A] `cargo test -p peri-tui --lib -- core::tests 2>&1 | tail -10` → 期望包含: `test result: ok`

#### - [x] 5.7 Headless 测试通过

- **来源:** spec-plan-1.md Task 4 检查步骤
- **目的:** 确认 MessageState 迁移无回归
- **操作步骤:**
  1. [A] `cargo test -p peri-tui --lib -- ui::headless::tests 2>&1 | tail -20` → 期望包含: `test result: ok`

---

### 场景 6：CommandSystem + SessionMetadata 提取（Task 5）

#### - [x] 6.1 AppCore 不再包含 CommandSystem 字段

- **来源:** spec-plan-2.md Task 5 检查步骤
- **目的:** 确认命令相关字段已从 AppCore 移除
- **操作步骤:**
  1. [A] `grep -E "pub (command_registry|command_help_list|skills)" peri-tui/src/app/core.rs` → 期望精确: ``（空输出）

#### - [x] 6.2 AppCore 不再包含 SessionMetadata 字段

- **来源:** spec-plan-2.md Task 5 检查步骤
- **目的:** 确认会话元数据字段已从 AppCore 移除
- **操作步骤:**
  1. [A] `grep -E "pub (pending_attachments|last_human_message|pre_submit_state_len)" peri-tui/src/app/core.rs` → 期望精确: ``（空输出）

#### - [x] 6.3 event.rs 中 std::mem::take(command_registry) 已消除

- **来源:** spec-plan-2.md Task 5 检查步骤
- **目的:** 确认核心 workaround 已消除
- **操作步骤:**
  1. [A] `grep -n "std::mem::take.*command_registry" peri-tui/src/event.rs` → 期望精确: ``（空输出）

#### - [x] 6.4 headless.rs 中 std::mem::take(command_registry) 已消除

- **来源:** spec-plan-2.md Task 5 检查步骤
- **目的:** 确认测试镜像 workaround 同步消除
- **操作步骤:**
  1. [A] `grep -n "std::mem::take.*command_registry" peri-tui/src/ui/headless.rs` → 期望精确: ``（空输出）

#### - [x] 6.5 无残留 .core. 前缀的 CommandSystem 字段

- **来源:** spec-plan-2.md Task 5 检查步骤
- **目的:** 确认命令字段已迁移到 .commands. 路径
- **操作步骤:**
  1. [A] `grep -rn "core\.\(command_registry\|command_help_list\|skills\)" peri-tui/src/ | grep -v "spec-plan"` → 期望精确: ``（空输出）

#### - [x] 6.6 无残留 .core. 前缀的 SessionMetadata 字段

- **来源:** spec-plan-2.md Task 5 检查步骤
- **目的:** 确认元数据字段已迁移到 .metadata. 路径
- **操作步骤:**
  1. [A] `grep -rn "core\.\(pending_attachments\|last_human_message\|pre_submit_state_len\)" peri-tui/src/ | grep -v "spec-plan"` → 期望精确: ``（空输出）

#### - [x] 6.7 CommandSystem + SessionMetadata 单元测试通过

- **来源:** spec-plan-2.md Task 5 检查步骤
- **目的:** 确认新结构体有基本测试覆盖
- **操作步骤:**
  1. [A] `cargo test -p peri-tui --lib -- "command_system\|session_metadata" 2>&1 | tail -10` → 期望包含: `test result: ok`

#### - [x] 6.8 Headless 测试通过

- **来源:** spec-plan-2.md Task 5 检查步骤
- **目的:** 确认 Task 5 迁移无回归
- **操作步骤:**
  1. [A] `cargo test -p peri-tui --lib -- ui::headless::tests 2>&1 | tail -20` → 期望包含: `test result: ok`

---

### 场景 7：AppCore 消除（Task 6）

#### - [x] 7.1 AppCore 结构体已删除

- **来源:** spec-plan-2.md Task 6 检查步骤
- **目的:** 确认 AppCore 不再存在
- **操作步骤:**
  1. [A] `grep -rn "pub struct AppCore" peri-tui/src/` → 期望精确: ``（空输出）

#### - [x] 7.2 core.rs 文件已删除

- **来源:** spec-plan-2.md Task 6 检查步骤
- **目的:** 确认 AppCore 定义文件已移除
- **操作步骤:**
  1. [A] `ls peri-tui/src/app/core.rs 2>&1` → 期望包含: `No such file or directory`

#### - [x] 7.3 无残留 .core. 路径（不含注释和 spec-plan）

- **来源:** spec-plan-2.md Task 6 检查步骤
- **目的:** 确认所有 .core. 中间路径已消除
- **操作步骤:**
  1. [A] `grep -rn '\.core\.' peri-tui/src/ | grep -v 'spec-plan' | grep -v '//.*\.core\.' | grep -v 'AppCore' | wc -l` → 期望精确: `0`

#### - [x] 7.4 ChatSession 包含 session_panels 直接字段

- **来源:** spec-plan-2.md Task 6 检查步骤
- **目的:** 确认 session_panels 已提升为 ChatSession 一级字段
- **操作步骤:**
  1. [A] `grep "pub session_panels:" peri-tui/src/app/chat_session.rs` → 期望包含: `pub session_panels:`

#### - [x] 7.5 Headless 测试通过

- **来源:** spec-plan-2.md Task 6 检查步骤
- **目的:** 确认 AppCore 消除无回归
- **操作步骤:**
  1. [A] `cargo test -p peri-tui --lib -- ui::headless::tests 2>&1 | tail -20` → 期望包含: `test result: ok`

#### - [x] 7.6 Clippy 无新增警告

- **来源:** spec-plan-2.md Task 6 检查步骤
- **目的:** 确认代码质量达标
- **操作步骤:**
  1. [A] `cargo clippy -p peri-tui 2>&1 | grep -E 'warning|error' | head -10` → 期望精确: ``（空输出）

---

### 场景 8：God Object 消除（Task 7）

#### - [ ] 8.1 App 结构体仅 3 字段

- **来源:** spec-plan-2.md Task 7 检查步骤 / spec-design.md §验收标准
- **目的:** 确认 App 从 26 字段降至 services/session_mgr/global_panels
- **操作步骤:**
  1. [A] `grep -A 10 "pub struct App" peri-tui/src/app/mod.rs | grep "pub " | wc -l` → 期望精确: `3`

#### - [!] 8.2 event.rs 中无 std::mem::take workaround

- **来源:** spec-plan-2.md Task 7 检查步骤 / spec-design.md §验收标准
- **目的:** 确认所有 God Object workaround 已消除
- **操作步骤:**
  1. [A] `grep -n "std::mem::take" peri-tui/src/event.rs | grep -v "//" | wc -l` → 期望精确: `0`

#### - [ ] 8.3 PanelContext 仅 2 字段

- **来源:** spec-plan-2.md Task 7 检查步骤
- **目的:** 确认 PanelContext 已精简
- **操作步骤:**
  1. [A] `grep -A 5 "pub struct PanelContext" peri-tui/src/app/panel_manager.rs | grep "pub " | wc -l` → 期望精确: `2`

#### - [ ] 8.4 无残留 app.sessions / app.active 直接访问

- **来源:** spec-plan-2.md Task 7 检查步骤
- **目的:** 确认所有 session 访问通过 session_mgr
- **操作步骤:**
  1. [A] `grep -rn 'app\.sessions\b\|app\.active\b' peri-tui/src/ | grep -v 'session_mgr\|spec-plan\|//.*app\.' | wc -l` → 期望精确: `0`

#### - [ ] 8.5 Headless 测试通过

- **来源:** spec-plan-2.md Task 7 检查步骤
- **目的:** 确认 God Object 消除无回归
- **操作步骤:**
  1. [A] `cargo test -p peri-tui --lib -- ui::headless::tests 2>&1 | tail -20` → 期望包含: `test result: ok`

#### - [ ] 8.6 Clippy 无警告

- **来源:** spec-plan-2.md Task 7 检查步骤
- **目的:** 确认代码质量达标
- **操作步骤:**
  1. [A] `cargo clippy -p peri-tui 2>&1 | grep -E 'warning|error' | head -10` → 期望精确: ``（空输出）

---

### 场景 9：完整端到端回归验证

#### - [ ] 9.1 完整测试套件通过

- **来源:** spec-plan-2.md Task 验收 / spec-design.md §验收标准
- **目的:** 确认 Task 1-7 整体无回归
- **操作步骤:**
  1. [A] `cargo test -p peri-tui 2>&1 | tail -30` → 期望包含: `test result: ok`

#### - [ ] 9.2 App 最终仅 3 字段

- **来源:** spec-plan-2.md Task 验收 / spec-design.md §验收标准
- **目的:** 确认 God Object 彻底消除
- **操作步骤:**
  1. [A] `grep -A 10 "pub struct App" peri-tui/src/app/mod.rs | grep "pub " | wc -l` → 期望精确: `3`

#### - [ ] 9.3 AppCore 完全消除

- **来源:** spec-plan-2.md Task 验收 / spec-design.md §验收标准
- **目的:** 确认无 AppCore 残留引用
- **操作步骤:**
  1. [A] `grep -rn "AppCore\|app\.core\b\|session\.core\b\|\.core\." peri-tui/src/ | grep -v 'spec-plan' | wc -l` → 期望精确: `0`

#### - [ ] 9.4 无 std::mem::take workaround

- **来源:** spec-plan-2.md Task 验收 / spec-design.md §验收标准
- **目的:** 确认 event.rs 无 workaround 残留
- **操作步骤:**
  1. [A] `grep -rn "std::mem::take" peri-tui/src/event.rs | grep -v "//" | wc -l` → 期望精确: `0`

#### - [ ] 9.5 ChatSession 包含 6 个子模块字段

- **来源:** spec-plan-2.md Task 验收 / spec-design.md §验收标准
- **目的:** 确认 ChatSession 结构化完成
- **操作步骤:**
  1. [A] `grep "pub " peri-tui/src/app/chat_session.rs | grep -E "(ui|messages|session_panels|commands|metadata|agent):" | wc -l` → 期望精确: `6`

#### - [ ] 9.6 Clippy 零警告零错误

- **来源:** spec-plan-2.md Task 验收 / spec-design.md §验收标准
- **目的:** 确认最终代码质量达标
- **操作步骤:**
  1. [A] `cargo clippy -p peri-tui 2>&1 | grep -E "warning\[|error\[" | head -10` → 期望精确: ``（空输出）

#### - [ ] 9.7 Headless 全量测试通过

- **来源:** spec-design.md §验收标准
- **目的:** 确认 TUI 核心逻辑无回归
- **操作步骤:**
  1. [A] `cargo test -p peri-tui --lib -- headless 2>&1 | grep "test result"` → 期望包含: `test result: ok`

#### - [ ] 9.8 event.rs 不含 app.sessions[app.active].core.xxx 直接访问

- **来源:** spec-design.md §验收标准
- **目的:** 确认旧路径模式彻底消除
- **操作步骤:**
  1. [A] `grep -n "sessions\[.*\.active\]\.core\." peri-tui/src/event.rs | wc -l` → 期望精确: `0`

---

## 验收后清理

本验收清单无后台服务需要清理。

---

## 验收结果汇总

| 场景 | 序号 | 验收项 | [A] | [H] | 结果 |
|------|------|--------|-----|-----|------|
| 场景 1 | 1.1 | 构建零错误 | 1 | 0 | ⬜ |
| 场景 1 | 1.2 | 现有测试全通过 | 1 | 0 | ⬜ |
| 场景 2 | 2.1 | App 旧字段已删除 | 1 | 0 | ⬜ |
| 场景 2 | 2.2 | App 字段数减少至预期 | 1 | 0 | ⬜ |
| 场景 2 | 2.3 | 无残留 app.xxx 旧路径 | 1 | 0 | ⬜ |
| 场景 2 | 2.4 | 无残留 self.xxx 旧路径 | 1 | 0 | ⬜ |
| 场景 2 | 2.5 | ServiceRegistry 单元测试通过 | 1 | 0 | ⬜ |
| 场景 2 | 2.6 | Headless 测试通过 | 1 | 0 | ⬜ |
| 场景 3 | 3.1 | 无残留 app.sessions 直接访问 | 1 | 0 | ⬜ |
| 场景 3 | 3.2 | 无残留 app.active 直接赋值 | 1 | 0 | ⬜ |
| 场景 3 | 3.3 | 无残留 app.session_areas 直接访问 | 1 | 0 | ⬜ |
| 场景 3 | 3.4 | SessionManager 单元测试通过 | 1 | 0 | ⬜ |
| 场景 3 | 3.5 | Headless 测试通过 | 1 | 0 | ⬜ |
| 场景 4 | 4.1 | UiState 包含 18 个字段 | 1 | 0 | ⬜ |
| 场景 4 | 4.2 | AppCore 不再包含 UiState 字段 | 1 | 0 | ⬜ |
| 场景 4 | 4.3 | 全项目无 UiState 字段通过 core 访问 | 1 | 0 | ⬜ |
| 场景 4 | 4.4 | UiState 单元测试通过 | 1 | 0 | ⬜ |
| 场景 4 | 4.5 | Headless 测试通过 | 1 | 0 | ⬜ |
| 场景 5 | 5.1 | MessageState 包含 9 个字段 | 1 | 0 | ⬜ |
| 场景 5 | 5.2 | AppCore 不再包含 MessageState 字段 | 1 | 0 | ⬜ |
| 场景 5 | 5.3 | AppCore::new() 不再接受 render 参数 | 1 | 0 | ⬜ |
| 场景 5 | 5.4 | 全项目无 MessageState 字段通过 core 访问 | 1 | 0 | ⬜ |
| 场景 5 | 5.5 | MessageState 单元测试通过 | 1 | 0 | ⬜ |
| 场景 5 | 5.6 | AppCore 现有测试仍通过 | 1 | 0 | ⬜ |
| 场景 5 | 5.7 | Headless 测试通过 | 1 | 0 | ⬜ |
| 场景 6 | 6.1 | AppCore 不再包含 CommandSystem 字段 | 1 | 0 | ⬜ |
| 场景 6 | 6.2 | AppCore 不再包含 SessionMetadata 字段 | 1 | 0 | ⬜ |
| 场景 6 | 6.3 | event.rs std::mem::take 已消除 | 1 | 0 | ⬜ |
| 场景 6 | 6.4 | headless.rs std::mem::take 已消除 | 1 | 0 | ⬜ |
| 场景 6 | 6.5 | 无残留 .core. CommandSystem 字段 | 1 | 0 | ⬜ |
| 场景 6 | 6.6 | 无残留 .core. SessionMetadata 字段 | 1 | 0 | ⬜ |
| 场景 6 | 6.7 | CommandSystem + SessionMetadata 单元测试通过 | 1 | 0 | ⬜ |
| 场景 6 | 6.8 | Headless 测试通过 | 1 | 0 | ⬜ |
| 场景 7 | 7.1 | AppCore 结构体已删除 | 1 | 0 | ⬜ |
| 场景 7 | 7.2 | core.rs 文件已删除 | 1 | 0 | ⬜ |
| 场景 7 | 7.3 | 无残留 .core. 路径 | 1 | 0 | ⬜ |
| 场景 7 | 7.4 | ChatSession 含 session_panels 直接字段 | 1 | 0 | ⬜ |
| 场景 7 | 7.5 | Headless 测试通过 | 1 | 0 | ⬜ |
| 场景 7 | 7.6 | Clippy 无新增警告 | 1 | 0 | ⬜ |
| 场景 8 | 8.1 | App 结构体仅 3 字段 | 1 | 0 | ⬜ |
| 场景 8 | 8.2 | event.rs 无 std::mem::take workaround | 1 | 0 | ❌ |
| 场景 8 | 8.3 | PanelContext 仅 2 字段 | 1 | 0 | ⬜ |
| 场景 8 | 8.4 | 无残留 app.sessions/active 直接访问 | 1 | 0 | ⬜ |
| 场景 8 | 8.5 | Headless 测试通过 | 1 | 0 | ⬜ |
| 场景 8 | 8.6 | Clippy 无警告 | 1 | 0 | ⬜ |
| 场景 9 | 9.1 | 完整测试套件通过 | 1 | 0 | ⬜ |
| 场景 9 | 9.2 | App 最终仅 3 字段 | 1 | 0 | ⬜ |
| 场景 9 | 9.3 | AppCore 完全消除 | 1 | 0 | ⬜ |
| 场景 9 | 9.4 | 无 std::mem::take workaround | 1 | 0 | ⬜ |
| 场景 9 | 9.5 | ChatSession 含 6 个子模块字段 | 1 | 0 | ⬜ |
| 场景 9 | 9.6 | Clippy 零警告零错误 | 1 | 0 | ⬜ |
| 场景 9 | 9.7 | Headless 全量测试通过 | 1 | 0 | ⬜ |
| 场景 9 | 9.8 | event.rs 无旧路径模式 | 1 | 0 | ⬜ |

**验收结论:** ⬜ 全部通过 / ⬜ 存在问题
