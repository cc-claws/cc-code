# App 分层重构 执行计划（第二阶段）

**目标:** 完成 AppCore 消除和 God Object 消除，App 最终仅 3 字段

**技术栈:** Rust 2021, tokio async/await, ratatui, 字段投影拆分借用策略

**设计文档:** spec-design.md

## 改动总览

- 涉及 peri-tui/src/app/core.rs 删除，18 个文件的 `session.core.*` 路径替换，event.rs 重构为主控分发
- Task 5 提取最后 2 个小组件（CommandSystem + SessionMetadata），Task 6 消除 AppCore，Task 7 消除 God Object
- 依赖链：Task 5（依赖 spec-plan-1 全部完成）→ Task 6 → Task 7
- 关键决策：AppCore 消除后 `session.core.xxx` 路径全部替换为 `session.ui.xxx` / `session.messages.xxx` 等

---

### Task 0: 环境准备

**背景:**
确认第一阶段（spec-plan-1.md）全部完成，构建和测试基线正常。

**执行步骤:**

- [x] 验证第一阶段产出
  - `ls peri-tui/src/app/service_registry.rs peri-tui/src/app/session_manager.rs peri-tui/src/app/ui_state.rs peri-tui/src/app/message_state.rs`
  - 预期: 4 个文件均存在
- [x] 验证构建和测试通过
  - `cargo test -p peri-tui 2>&1 | tail -5`
  - 预期: 全部测试通过

**检查步骤:**

- [x] 第一阶段文件完整
  - 上面的 ls 命令返回 0
- [x] 测试通过
  - `cargo test -p peri-tui 2>&1 | grep -c "test result: ok"`
  - 预期: ≥ 1

---

### Task 5: 提取 CommandSystem + SessionMetadata

**背景:**
[业务语境] — 将 AppCore 中最后 6 个字段拆分为 CommandSystem（命令注册表 + 帮助列表 + Skills 元数据）和 SessionMetadata（附件 + 最近消息 + 提交前长度），完成 AppCore 的全部字段剥离
[修改原因] — CommandSystem 的 3 个字段是 command dispatch 的核心数据，当前因 `CommandRegistry::dispatch(&self, app: &mut App, ...)` 的签名要求 `&self` + `&mut App`，导致 `command_registry` 和 `app` 产生借用冲突，event.rs:595 和 headless.rs 中共 5 处使用 `std::mem::take` 临时交换 workaround。提取 CommandSystem 后可通过字段投影拆分同时持有 `&mut CommandSystem` 和 `&mut App` 其余字段
[上下游影响] — 依赖 Task 1-4 全部完成（UiState/MessageState 已从 AppCore 提取）。本 Task 的输出被 Task 6（消除 AppCore）直接依赖

**涉及文件:**

- 新建: `peri-tui/src/app/command_system.rs`, `peri-tui/src/app/session_metadata.rs`
- 修改: `peri-tui/src/app/core.rs`, `peri-tui/src/app/chat_session.rs`, `peri-tui/src/app/mod.rs`, `peri-tui/src/event.rs`, `peri-tui/src/app/agent_ops.rs`, `peri-tui/src/app/hint_ops.rs`, `peri-tui/src/app/thread_ops.rs`, `peri-tui/src/app/panel_ops.rs`, `peri-tui/src/command/help.rs`, `peri-tui/src/command/agents.rs`, `peri-tui/src/ui/main_ui.rs`, `peri-tui/src/ui/main_ui/popups/hints.rs`, `peri-tui/src/ui/main_ui/sticky_header.rs`, `peri-tui/src/ui/main_ui/status_bar.rs`, `peri-tui/src/ui/headless.rs`

**执行步骤:**

- [x] 创建 `peri-tui/src/app/command_system.rs`，定义 CommandSystem 结构体
  - 位置: 新建 `peri-tui/src/app/command_system.rs`
  - 定义 3 个 pub 字段:

    ```rust
    use peri_middlewares::prelude::SkillMetadata;
    use crate::command::CommandRegistry;

    pub struct CommandSystem {
        pub command_registry: CommandRegistry,
        pub command_help_list: Vec<(String, String, Vec<String>)>,
        pub skills: Vec<SkillMetadata>,
    }

    impl CommandSystem {
        pub fn new(command_registry: CommandRegistry, skills: Vec<SkillMetadata>) -> Self {
            let command_help_list: Vec<(String, String, Vec<String>)> = command_registry
                .list()
                .into_iter()
                .map(|(n, d, a)| (n.to_string(), d.to_string(), a.into_iter().map(String::from).collect()))
                .collect();
            Self { command_registry, command_help_list, skills }
        }
    }
    ```

  - 原因: 将命令注册表、帮助列表、Skills 元数据聚合为独立结构体，消除 event.rs 中的 `std::mem::take` workaround

- [x] 创建 `peri-tui/src/app/session_metadata.rs`，定义 SessionMetadata 结构体
  - 位置: 新建 `peri-tui/src/app/session_metadata.rs`
  - 定义 3 个 pub 字段:

    ```rust
    use super::hitl_prompt::PendingAttachment;

    pub struct SessionMetadata {
        pub pending_attachments: Vec<PendingAttachment>,
        pub last_human_message: Option<String>,
        pub pre_submit_state_len: usize,
    }

    impl SessionMetadata {
        pub fn new() -> Self {
            Self {
                pending_attachments: Vec::new(),
                last_human_message: None,
                pre_submit_state_len: 0,
            }
        }
    }
    ```

  - 原因: 将低频访问的会话元数据聚合为独立结构体

- [x] 在 `app/mod.rs` 中注册两个新模块
  - 位置: `peri-tui/src/app/mod.rs` 模块声明区（~L18-36），追加:

    ```rust
    mod command_system;
    mod session_metadata;
    pub use command_system::CommandSystem;
    pub use session_metadata::SessionMetadata;
    ```

  - 原因: 新模块需作为 app 子模块可见

- [x] 在 ChatSession 中新增 `commands: CommandSystem` 和 `metadata: SessionMetadata` 字段（双写阶段）
  - 位置: `peri-tui/src/app/chat_session.rs` ChatSession 结构体（~L11-19）
  - 在 `pub core: AppCore,` 之前添加:

    ```rust
    pub commands: CommandSystem,
    pub metadata: SessionMetadata,
    ```

  - 位置: `ChatSession::new()` 方法（~L23-44），将 `command_registry` 和 `skills` 参数改为先构建 CommandSystem，再传给 AppCore:

    ```rust
    let commands = CommandSystem::new(command_registry, skills.clone());
    Self {
        commands,
        metadata: SessionMetadata::new(),
        core: AppCore::new(cwd, render_tx, render_cache, render_notify, commands.command_registry.clone(), skills),
        // ... 其余不变
    }
    ```

  - 注意: `CommandRegistry` 必须实现 `Clone`（或改为 `Arc<CommandRegistry>`）。检查 `CommandRegistry` 是否已实现 Clone，若未实现则在 `CommandSystem` 中持有 `Arc<CommandRegistry>` 并在 AppCore 中持有同一 Arc 的 clone
  - 原因: 双写过渡——新旧路径共存，确保编译不中断

- [x] 在 `panel_ops.rs` 的 `new_headless()` 中初始化 `commands` 和 `metadata` 字段
  - 位置: `peri-tui/src/app/panel_ops.rs` ChatSession 构造处（~L1076）
  - 在 `core,` 之前添加:

    ```rust
    commands: super::CommandSystem::new(/* 同 headless 中已有的 command_registry */, skills.clone()),
    metadata: super::SessionMetadata::new(),
    ```

  - 原因: headless 测试工厂必须同步创建新字段

- [x] 迁移 `event.rs` 中 CommandSystem 字段访问并消除 `std::mem::take` workaround
  - 位置: `peri-tui/src/event.rs` ~L594-621
  - 将:

    ```rust
    let registry = std::mem::take(&mut app.sessions[app.active].core.command_registry);
    let known = registry.dispatch(app, &text);
    app.sessions[app.active].core.command_registry = registry;
    ```

  - 替换为（通过字段投影拆分同时借用 commands 和 app 其余字段）:

    ```rust
    let known = app.session_mgr.current_mut().commands.command_registry.dispatch(app, &text);
    ```

  - 位置: `event.rs` ~L607-611 `.core.skills.iter().find(...)` 替换为 `.commands.skills.iter().find(...)`
  - 位置: `event.rs` ~L618-621 `.core.command_registry.match_prefix(...)` 替换为 `.commands.command_registry.match_prefix(...)`
  - 原因: 消除核心 workaround——`CommandSystem` 提取后 `commands` 和 `app` 是不同路径，Rust 借用检查器可同时持有

- [x] 迁移 `event.rs` 中 SessionMetadata 字段访问
  - 位置: `event.rs` ~L516 `.core.pending_attachments.len()` → `.metadata.pending_attachments.len()`
  - 位置: `event.rs` ~L673 `.core.pending_attachments.is_empty()` → `.metadata.pending_attachments.is_empty()`
  - 原因: SessionMetadata 字段路径迁移

- [x] 迁移 `agent_ops.rs` 中 CommandSystem + SessionMetadata 字段访问
  - 位置: `agent_ops.rs` ~L28-29 `.core.pre_submit_state_len` → `.metadata.pre_submit_state_len`（2 处: ~L28, ~L915）
  - 位置: `agent_ops.rs` ~L34 `std::mem::take(&mut self.sessions[self.active].core.pending_attachments)` → `std::mem::take(&mut self.session_mgr.current_mut().metadata.pending_attachments)`
  - 位置: `agent_ops.rs` ~L46 `.core.last_human_message` → `.metadata.last_human_message`
  - 位置: `agent_ops.rs` ~L927 `.core.last_human_message` → `.metadata.last_human_message`
  - 原因: agent_ops.rs 是 submit_message 和中断恢复的核心逻辑

- [x] 迁移 `hint_ops.rs` 中 CommandSystem 字段访问
  - 位置: `hint_ops.rs` ~L32-34 `.core.command_registry.match_prefix(...)` → `.commands.command_registry.match_prefix(...)`
  - 位置: `hint_ops.rs` ~L36-38 `.core.skills.iter()` → `.commands.skills.iter()`
  - 位置: `hint_ops.rs` ~L109 `.core.command_registry.match_prefix(...)` → `.commands.command_registry.match_prefix(...)`
  - 原因: hint_ops.rs 构建补全候选时需要 command_registry 和 skills

- [x] 迁移 `thread_ops.rs` 中 SessionMetadata + CommandSystem 字段访问
  - 位置: `thread_ops.rs` ~L56 `.pending_attachments` → `.metadata.pending_attachments`
  - 位置: `thread_ops.rs` ~L62 `.pending_attachments.pop()` → `.metadata.pending_attachments.pop()`
  - 位置: `thread_ops.rs` ~L118 `.pending_attachments.clear()` → `.metadata.pending_attachments.clear()`
  - 位置: `thread_ops.rs` ~L125 `.last_human_message` → `.metadata.last_human_message`
  - 位置: `thread_ops.rs` ~L192 `.pending_attachments.clear()` → `.metadata.pending_attachments.clear()`
  - 位置: `thread_ops.rs` ~L198 `.last_human_message` → `.metadata.last_human_message`
  - 位置: `thread_ops.rs` ~L200 `.pre_submit_state_len` → `.metadata.pre_submit_state_len`
  - 原因: thread_ops.rs 中线程加载/保存涉及 metadata 字段

- [x] 迁移 `mod.rs` 中 SessionMetadata 字段访问
  - 位置: `mod.rs` ~L410 `.core.pre_submit_state_len` → `.metadata.pre_submit_state_len`
  - 位置: `mod.rs` ~L429 `.core.last_human_message` → `.metadata.last_human_message`
  - 原因: interrupt() 方法中引用 metadata 字段

- [x] 迁移 UI 层文件中 CommandSystem + SessionMetadata 字段访问
  - 位置: `main_ui.rs` ~L106 `.pending_attachments` → `.metadata.pending_attachments`
  - 位置: `main_ui.rs` ~L120 `.last_human_message` → `.metadata.last_human_message`
  - 位置: `main_ui.rs` ~L580 `.pending_attachments` → `.metadata.pending_attachments`
  - 位置: `sticky_header.rs` ~L21 `.core.last_human_message` → `.metadata.last_human_message`
  - 位置: `status_bar.rs` ~L215 `.session_panels` → `.session_panels`（Task 6 处理）
  - 位置: `popups/hints.rs` ~L38 `.command_registry` → `.commands.command_registry`
  - 位置: `command/help.rs` ~L18 `.core.command_help_list` → `.commands.command_help_list`
  - 原因: UI 渲染层通过 session 引用访问命令和元数据

- [x] 迁移 `headless.rs` 中 CommandSystem + SessionMetadata 字段访问并消除 4 处 `std::mem::take`
  - 位置: `headless.rs` ~L884,919,941,953,988,990,1032 `.core.last_human_message` → `.metadata.last_human_message`
  - 位置: `headless.rs` ~L2095-2097: 将 `std::mem::take(&mut app.sessions[app.active].core.command_registry)` 替换为直接使用 `app.session_mgr.current_mut().commands.command_registry`（消除 take + put_back 模式）
  - 位置: `headless.rs` ~L2128-2130: 同上
  - 位置: `headless.rs` ~L2160-2162: 同上
  - 位置: `headless.rs` ~L2208-2214: 同上（~L2208 是只读引用，直接改路径；~L2212 是 take + put_back，消除）
  - 原因: headless.rs 中 4 处 `std::mem::take` 是生产代码 workaround 的测试镜像，必须同步消除

- [x] 删除 AppCore 中的 6 个已迁移字段
  - 位置: `peri-tui/src/app/core.rs` AppCore 结构体定义（~L16-64）
  - 删除字段: `command_registry`, `command_help_list`, `skills`, `pending_attachments`, `last_human_message`, `pre_submit_state_len`
  - 位置: `AppCore::new()` — 从参数列表中删除 `command_registry: CommandRegistry, skills: Vec<SkillMetadata>`，从 `Self { ... }` 中删除对应 6 行，删除 `command_help_list` 的构建逻辑（~L76-86）
  - 位置: `AppCore::new()` — 从参数列表中保留 `cwd: String`（pipeline 仍需使用）
  - 原因: 双写阶段结束后清理旧字段

- [x] 为 CommandSystem 编写单元测试
  - 测试文件: `peri-tui/src/app/command_system.rs`（模块内 `#[cfg(test)] mod tests`）
  - 测试场景:
    - `test_command_system_new`: 使用 `default_registry()` + 空 skills 构建 CommandSystem → 验证 `command_help_list` 非空（至少包含 /help, /model 等内置命令）、`skills` 为空
    - `test_command_system_with_skills`: 传入 skills 列表 → 验证 `skills.len()` 与传入一致
    - `test_std_mem_take_eliminated`: 编译期验证——尝试 `std::mem::take(&mut commands.command_registry)` 后 `commands.command_registry.dispatch()` 无需 put_back 也可工作（通过字段投影拆分）
  - 运行命令: `cargo test -p peri-tui --lib -- command_system`
  - 预期: 所有测试通过

- [x] 为 SessionMetadata 编写单元测试
  - 测试文件: `peri-tui/src/app/session_metadata.rs`（模块内 `#[cfg(test)] mod tests`）
  - 测试场景:
    - `test_session_metadata_defaults`: `SessionMetadata::new()` → `pending_attachments` 为空、`last_human_message` 为 None、`pre_submit_state_len` 为 0
    - `test_session_metadata_mutate`: 设置 `last_human_message = Some("hello")`、`pre_submit_state_len = 5` → 验证可读取修改后的值
  - 运行命令: `cargo test -p peri-tui --lib -- session_metadata`
  - 预期: 所有测试通过

**检查步骤:**

- [x] 验证 AppCore 不再包含 CommandSystem 字段
  - `grep -E "pub (command_registry|command_help_list|skills)" peri-tui/src/app/core.rs`
  - 预期: 0 行输出
- [x] 验证 AppCore 不再包含 SessionMetadata 字段
  - `grep -E "pub (pending_attachments|last_human_message|pre_submit_state_len)" peri-tui/src/app/core.rs`
  - 预期: 0 行输出
- [x] 验证 event.rs 中 `std::mem::take` 消除（command_registry 相关）
  - `grep -n "std::mem::take.*command_registry" peri-tui/src/event.rs`
  - 预期: 0 行输出
- [x] 验证 headless.rs 中 `std::mem::take` 消除（command_registry 相关）
  - `grep -n "std::mem::take.*command_registry" peri-tui/src/ui/headless.rs`
  - 预期: 0 行输出
- [x] 验证全项目无残留 `.core.` 前缀的 CommandSystem 字段
  - `grep -rn "core\.\(command_registry\|command_help_list\|skills\)" peri-tui/src/ | grep -v "spec-plan"`
  - 预期: 0 行输出
- [x] 验证全项目无残留 `.core.` 前缀的 SessionMetadata 字段
  - `grep -rn "core\.\(pending_attachments\|last_human_message\|pre_submit_state_len\)" peri-tui/src/ | grep -v "spec-plan"`
  - 预期: 0 行输出
- [x] 验证编译通过
  - `cargo build -p peri-tui 2>&1 | tail -5`
  - 预期: 构建成功，无错误
- [x] 验证 headless 测试通过
  - `cargo test -p peri-tui --lib -- ui::headless::tests 2>&1 | tail -20`
  - 预期: 所有测试通过
- [x] 验证 CommandSystem + SessionMetadata 单元测试通过
  - `cargo test -p peri-tui --lib -- "command_system\|session_metadata" 2>&1 | tail -10`
  - 预期: 所有测试通过

---

### Task 6: 消除 AppCore

**背景:**
[业务语境] — Task 3-5 已将 AppCore 的全部字段提取到 UiState（18 字段）、MessageState（9 字段）、CommandSystem（3 字段）、SessionMetadata（3 字段），AppCore 仅剩 `session_panels: PanelManager` 一个字段
[修改原因] — 保留仅 1 字段的 AppCore 结构体毫无意义，`session.core.session_panels` 的冗余路径增加认知负担和代码长度。将 `session_panels` 直接提升为 ChatSession 字段后删除 AppCore，可消除所有 `.core.` 中间路径
[上下游影响] — 依赖 Task 5 完成。本 Task 的输出被 Task 7（消除 God Object）直接依赖

**涉及文件:**

- 删除: `peri-tui/src/app/core.rs`
- 修改: `peri-tui/src/app/chat_session.rs`, `peri-tui/src/app/mod.rs`, `peri-tui/src/event.rs`, `peri-tui/src/app/agent_ops.rs`, `peri-tui/src/app/panel_ops.rs`, `peri-tui/src/app/thread_ops.rs`, `peri-tui/src/app/hint_ops.rs`, `peri-tui/src/ui/main_ui.rs`, `peri-tui/src/ui/main_ui/status_bar.rs`, `peri-tui/src/ui/main_ui/panels/hooks.rs`, `peri-tui/src/ui/main_ui/panels/model.rs`, `peri-tui/src/ui/main_ui/panels/agent.rs`, `peri-tui/src/ui/main_ui/panels/login.rs`, `peri-tui/src/ui/main_ui/panels/mcp.rs`, `peri-tui/src/ui/main_ui/panels/cron.rs`, `peri-tui/src/ui/main_ui/panels/plugin.rs`, `peri-tui/src/ui/main_ui/panels/thread_browser.rs`, `peri-tui/src/ui/main_ui/panels/memory.rs`, `peri-tui/src/ui/headless.rs`

**执行步骤:**

- [x] 确认 AppCore 仅剩 `session_panels` 字段
  - 位置: `peri-tui/src/app/core.rs` AppCore 结构体
  - 验证 AppCore 中仅包含 `session_panels` 和 MessageState 字段（`view_messages`, `round_start_vm_idx`, `pipeline`, `render_tx`, `render_cache`, `render_notify`, `last_render_version`, `pending_messages`, `last_submitted_text`）。若 MessageState 字段仍在 AppCore 中（说明 Task 4 未迁移完成），先完成 MessageState 迁移再继续
  - 若 MessageState 字段已迁移到 ChatSession.messages，则 AppCore 仅剩 `session_panels`
  - 原因: 确认前置条件满足，避免部分迁移状态

- [x] 将 `session_panels` 从 AppCore 移到 ChatSession 直接字段
  - 位置: `peri-tui/src/app/chat_session.rs` ChatSession 结构体（~L11-19）
  - 添加 `pub session_panels: panel_manager::PanelManager,` 字段
  - 位置: `ChatSession::new()` 方法，在 `Self { ... }` 中添加 `session_panels: panel_manager::PanelManager::new(),`
  - 原因: `session_panels` 提升为 ChatSession 一级字段，不再经过 AppCore 中转

- [x] 在 `panel_ops.rs` 的 `new_headless()` 中初始化 `session_panels`
  - 位置: `peri-tui/src/app/panel_ops.rs` ChatSession 构造处
  - 添加 `session_panels: super::panel_manager::PanelManager::new(),`
  - 原因: headless 测试工厂同步创建

- [x] 全项目替换 `.core.session_panels` → `.session_panels`（约 55 处）
  - 执行全局替换，涉及文件:
    - `event.rs`（8 处: ~L228, 241, 271, 278, 741, 753, 772, 以及可能的隐藏处）
    - `main_ui.rs`（4 处: ~L166, 169, 175, 280）
    - `status_bar.rs`（2 处: ~L215, 271, 274）
    - `headless.rs`（约 13 处: ~L1103, 2562, 2569, 2590, 2624, 2632, 2653, 2943, 2948, 2959, 2967, 2992, 2996）
    - `panel_ops.rs`（约 16 处: ~L17, 28, 67, 84, 92, 132, 183, 231, 239, 261, 873, 881, 894, 909, 947, 956, 981, 989, 1002）
    - `thread_ops.rs`（2 处: ~L116, 195）
    - `mod.rs`（3 处: ~L516, 517, 522, 533）
    - `panels/hooks.rs`（3 处: ~L164, 168, 220, 223）
    - `panels/model.rs`（2 处: ~L146）
    - `panels/agent.rs`（2 处: ~L178, 182）
    - `panels/login.rs`（4 处: ~L312, 316, 365, 369）
    - `panels/mcp.rs`, `panels/cron.rs`, `panels/plugin.rs`, `panels/thread_browser.rs`, `panels/memory.rs`（各 1-2 处）
  - 替换命令: `sed -i '' 's/\.core\.session_panels/.session_panels/g' <file>`
  - 原因: `.core.` 中间路径全部消除

- [x] 全项目替换 `.core.view_messages` → `.messages.view_messages` 等剩余 `.core.` 路径
  - 若 MessageState 字段（`view_messages`, `round_start_vm_idx`, `pipeline`, `render_tx`, `render_cache`, `render_notify`, `last_render_version`, `pending_messages`, `last_submitted_text`）仍在 AppCore 中（Task 4 未完成），先执行 MessageState 迁移
  - 若已迁移到 `ChatSession.messages`，则替换 `.core.xxx` → `.messages.xxx`
  - 若已迁移到 `ChatSession.ui`（Task 3 已完成），则替换 `.core.xxx` → `.ui.xxx`
  - 执行后确认: `grep -rn '\.core\.' peri-tui/src/ | grep -v 'spec-plan' | grep -v 'app_core\|AppCore\|core.rs'` 返回 0 结果
  - 原因: 彻底清除所有 `.core.` 路径

- [x] 删除 AppCore 结构体定义和 `app/core.rs` 文件
  - 位置: `peri-tui/src/app/core.rs` — 整个文件删除
  - 位置: `peri-tui/src/app/mod.rs` — 删除 `mod core;` 和 `pub use core::AppCore;`
  - 位置: `peri-tui/src/app/chat_session.rs` — 删除 `use super::AppCore;` 和 `pub core: AppCore,`
  - 位置: 全项目搜索 `AppCore` 引用，确认无残留
  - 原因: AppCore 完全消除，不再需要

- [x] 重新组织 ChatSession 结构体，确认 6 个一级字段
  - 位置: `peri-tui/src/app/chat_session.rs`
  - 确认 ChatSession 最终结构:

    ```rust
    pub struct ChatSession {
        pub ui: UiState,
        pub messages: MessageState,
        pub session_panels: PanelManager,
        pub commands: CommandSystem,
        pub metadata: SessionMetadata,
        pub agent: AgentComm,
        pub current_thread_id: Option<ThreadId>,
        pub langfuse: LangfuseState,
        pub todo_items: Vec<TodoItem>,
        pub background_task_count: usize,
        pub spinner_state: peri_widgets::SpinnerState,
    }
    ```

  - 原因: ChatSession 成为清晰的多模块容器

- [x] 为 AppCore 消除编写回归测试
  - 测试文件: `peri-tui/src/ui/headless.rs`（追加到现有测试模块）
  - 测试场景:
    - `test_no_appcore_path`: 创建 headless app，验证 `app.session_mgr.current().session_panels.is_any_open() == false`（通过新路径访问 session_panels）
    - `test_session_fields_independent`: 验证 `session.ui.textarea`、`session.commands.command_registry`、`session.metadata.pending_attachments` 均可独立访问
  - 运行命令: `cargo test -p peri-tui --lib -- "no_appcore\|session_fields_independent"`
  - 预期: 所有测试通过

**检查步骤:**

- [x] 验证 AppCore 结构体已删除
  - `grep -rn "pub struct AppCore" peri-tui/src/`
  - 预期: 0 行输出
- [x] 验证 core.rs 文件已删除
  - `ls peri-tui/src/app/core.rs 2>&1`
  - 预期: "No such file or directory"
- [x] 验证无残留 `.core.` 路径（不含注释和 spec-plan）
  - `grep -rn '\.core\.' peri-tui/src/ | grep -v 'spec-plan' | grep -v '//.*\.core\.' | grep -v 'AppCore' | wc -l`
  - 预期: 输出 0
- [x] 验证 ChatSession 包含 session_panels 直接字段
  - `grep "pub session_panels:" peri-tui/src/app/chat_session.rs`
  - 预期: 1 行匹配
- [x] 验证编译通过
  - `cargo build -p peri-tui 2>&1 | tail -5`
  - 预期: 构建成功，无错误
- [x] 验证 headless 测试通过
  - `cargo test -p peri-tui --lib -- ui::headless::tests 2>&1 | tail -20`
  - 预期: 所有测试通过
- [x] 验证 clippy 无警告
  - `cargo clippy -p peri-tui 2>&1 | grep -E 'warning|error' | head -10`
  - 预期: 无新增 warning

---

### Task 7: 消除 God Object

**背景:**
[业务语境] — Task 1-6 完成后，App 应仅保留 3 个字段（services: ServiceRegistry, session_mgr: SessionManager, global_panels: PanelManager），从原始的 26 字段 God Object 彻底瘦身
[修改原因] — event.rs 中的 `handle_event()` 函数当前接收 `&mut App` 并直接访问所有子字段，未利用字段投影拆分。PanelContext 构造在 3 处重复（~L244, ~L296, ~L757），每次手动解构 App 字段传入。本 Task 将 event.rs 重构为顶部统一解构，各分支操作子结构体引用
[上下游影响] — 依赖 Task 6 完成。本 Task 是整个重构链的最终 Task，完成后 App 结构体从 64 字段（26+38）降至 3 字段

**涉及文件:**

- 修改: `peri-tui/src/app/mod.rs`, `peri-tui/src/event.rs`, `peri-tui/src/ui/main_ui.rs`, `peri-tui/src/app/panel_manager.rs`, `peri-tui/src/app/panel_ops.rs`

**执行步骤:**

- [ ] 确认 App 仅含 3 个字段（services + session_mgr + global_panels）
  - 位置: `peri-tui/src/app/mod.rs` App 结构体（~L93-134）
  - 验证 Task 1（ServiceRegistry）和 Task 2（SessionManager）已完成迁移，App 中无残留旧字段
  - 若有残留字段（如 `setup_wizard`, `oauth_prompt`, `mode_highlight_until` 等未被 Task 1 迁移的字段），将其移入 ServiceRegistry
  - 原因: 确认前置条件

- [ ] 精简 PanelContext 结构体
  - 位置: `peri-tui/src/app/panel_manager.rs` PanelContext 定义（~L265-279）
  - 当前 PanelContext 有 11 个字段，每个都是 App 字段的解构投影。Task 1-2 完成后可简化为:

    ```rust
    pub struct PanelContext<'a> {
        pub services: &'a mut ServiceRegistry,
        pub session_mgr: &'a mut SessionManager,
    }
    ```

  - 同步修改所有构造 PanelContext 的位置（event.rs ~L244, ~L296, ~L757）和 PanelComponent::handle_key() 的 impl
  - 修改所有面板的 `ctx.cwd` → `ctx.services.cwd`、`ctx.peri_config` → `ctx.services.peri_config` 等访问路径
  - 原因: PanelContext 从 11 字段简化为 2 字段引用，面板代码通过 `ctx.services.xxx` 访问

- [ ] 重构 event.rs 为字段投影分发模式
  - 位置: `peri-tui/src/event.rs` — `handle_event()` 函数（全文件 2486 行）
  - 在函数入口处（关键分支之前）添加统一解构:

    ```rust
    let App { services, session_mgr, global_panels } = app;
    ```

  - 注意: 此解构仅在不需整体 `&mut App` 的分支中使用。对于需要整体 `App` 的分支（如 `update_textarea_hint()` 等 `impl App` 方法调用），使用 `app` 原始引用
  - 逐步将各分支中的 `app.services.xxx` 替换为 `services.xxx`，`app.session_mgr.xxx` 替换为 `session_mgr.xxx`，`app.global_panels.xxx` 替换为 `global_panels.xxx`
  - 原因: 统一解构后，Rust 借用检查器可验证各子结构体的独立可变性

- [ ] 消除 event.rs 中 PanelContext 构造的 3 处重复
  - 位置: `event.rs` ~L244-258, ~L296-308, ~L757-802
  - 将 3 处重复的 PanelContext 构造统一为:

    ```rust
    let ctx = PanelContext { services, session_mgr };
    global_panels.dispatch_key(input, &mut ctx);
    ```

  - 消除重复的 `let cwd = app.services.cwd.clone();` 等 11 行字段投影代码
  - 原因: DRY 原则——PanelContext 构造从每处 15 行降为 2 行

- [ ] 重构 main_ui.rs 渲染函数签名
  - 位置: `peri-tui/src/ui/main_ui.rs` — `render()` 函数（~L19）
  - 将 `render(f: &mut Frame, app: &mut App)` 签名改为接收子结构体引用:

    ```rust
    pub fn render(f: &mut Frame, services: &ServiceRegistry, session_mgr: &SessionManager, global_panels: &PanelManager)
    ```

  - 同步修改 `render_session_column()`、`render_messages()`、`render_attachment_bar()` 等子函数签名
  - 原因: 渲染函数不再需要 `&mut App` 全访问，仅需读取子结构体

- [ ] 验证所有 `std::mem::take` workaround 已消除
  - 搜索 event.rs 中所有 `std::mem::take` 调用
  - 排除 UI 渲染层中合法的 `std::mem::take(&mut line.spans)`（main_ui.rs:525, panels/*.rs）和 message_pipeline.rs 中的数据转移
  - 确认 command_registry 相关的 `std::mem::take` 已在 Task 5 中消除
  - 确认 pending_attachments 相关的 `std::mem::take` 已在 Task 5 中消除
  - 确认 agent_ops.rs 中 `std::mem::take(&mut self.session_mgr.current_mut().agent.agent_event_queue)` 保留（属于 AgentComm 内部数据转移，非 God Object workaround）
  - 原因: 验收标准要求 0 处 workaround

- [ ] 最终清理 mod.rs 中 App 的 re-export
  - 位置: `peri-tui/src/app/mod.rs` re-export 区域（~L79-89）
  - 删除 `pub use core::AppCore;`（已在 Task 6 删除 core.rs 时处理）
  - 确认 re-export 列表仅包含必要的公共类型
  - 原因: 清理过期 re-export

- [ ] 为 God Object 消除编写最终验证测试
  - 测试文件: `peri-tui/src/app/mod.rs` 或 `peri-tui/src/ui/headless.rs`
  - 测试场景:
    - `test_app_three_fields`: 编译期验证——`std::mem::size_of::<App>()` 的相对大小或通过 `#[cfg(test)]` 代码确认 App 仅 3 字段（services, session_mgr, global_panels）
    - `test_no_mem_take_workaround_in_event`: 搜索 event.rs 中 `std::mem::take` 出现次数（排除注释），预期为 0
  - 运行命令: `cargo test -p peri-tui --lib -- "three_fields\|no_mem_take"`
  - 预期: 所有测试通过

**检查步骤:**

- [ ] 验证 App 结构体仅 3 字段
  - `grep -E "^\s+pub [a-z_]+:" peri-tui/src/app/mod.rs | grep -A 20 "pub struct App" | head -5`
  - 预期: 仅 services, session_mgr, global_panels 3 个字段
- [ ] 验证 event.rs 中无 `std::mem::take` workaround（排除注释和 UI 数据转移）
  - `grep -n "std::mem::take" peri-tui/src/event.rs | grep -v "//"`
  - 预期: 0 行输出
- [ ] 验证 PanelContext 仅 2 字段
  - `grep -A 5 "pub struct PanelContext" peri-tui/src/app/panel_manager.rs`
  - 预期: 仅 services 和 session_mgr 2 个字段
- [ ] 验证无残留 `app.sessions` / `app.active` 直接访问
  - `grep -rn 'app\.sessions\b\|app\.active\b' peri-tui/src/ | grep -v 'session_mgr\|spec-plan\|//.*app\.' | wc -l`
  - 预期: 输出 0
- [ ] 验证编译通过
  - `cargo build -p peri-tui 2>&1 | tail -5`
  - 预期: 构建成功，无错误
- [ ] 验证 clippy 无警告
  - `cargo clippy -p peri-tui 2>&1 | grep -E 'warning|error' | head -10`
  - 预期: 无新增 warning
- [ ] 验证全部 headless 测试通过
  - `cargo test -p peri-tui --lib -- ui::headless::tests 2>&1 | tail -20`
  - 预期: 所有测试通过

---

### Task 验收: App 分层重构 完整验收

**前置条件:**

- spec-plan-1.md Task 1-4 全部完成
- spec-plan-2.md Task 5-7 全部完成

**端到端验证:**

1. 运行完整测试套件确保无回归
   - `cargo test -p peri-tui 2>&1 | tail -30`
   - 预期: 全部测试通过
   - 失败排查: 检查各 Task 的测试步骤

2. 验证 App 结构体仅 3 字段
   - `grep -A 10 "pub struct App" peri-tui/src/app/mod.rs | grep "pub " | wc -l`
   - 预期: 输出 3

3. 验证 AppCore 完全消除
   - `grep -rn "AppCore\|app\.core\b\|session\.core\b\|\.core\." peri-tui/src/ | grep -v 'spec-plan' | wc -l`
   - 预期: 输出 0

4. 验证无 `std::mem::take` workaround
   - `grep -rn "std::mem::take" peri-tui/src/event.rs | grep -v "//" | wc -l`
   - 预期: 输出 0

5. 验证 ChatSession 包含 6 个子模块字段
   - `grep "pub " peri-tui/src/app/chat_session.rs | grep -E "(ui|messages|session_panels|commands|metadata|agent):" | wc -l`
   - 预期: 输出 6

6. 验证 clippy 无新增警告
   - `cargo clippy -p peri-tui 2>&1 | grep -E "warning\[|error\[" | head -10`
   - 预期: 无新增 warning 或 error
