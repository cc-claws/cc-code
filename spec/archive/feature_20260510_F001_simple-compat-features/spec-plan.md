# 简单兼容特性批次 执行计划

**目标:** 实现 4 项配置系统补全（C1/C2/C4/C6）和 3 项 TUI 命令（T1/T2/T5），总改动量 ~300 行

**技术栈:** Rust 2021, tokio, serde, ratatui, sqlx

**设计文档:** spec-design.md

## 改动总览

- 配置层（Task 1-3）全部修改 `peri-middlewares/src/agents_md.rs` 和 `peri-tui/src/config/types.rs`，按 C2+C6 → C1 → C4 顺序实施
- TUI 命令层（Task 4-5）在 `peri-tui/src/command/` 下新增 3 个命令文件，修改 `mod.rs` 注册
- Task 1 扩展 PeriConfig 字段并传入 excludes 到 AgentsMdMiddleware；Task 2/3 在 AgentsMdMiddleware 中依次叠加 CLAUDE.local.md 读取和 @import 解析
- 关键决策：@import 不引入 regex 依赖，手动解析 `<!-- @import path -->`；excludes 通过 builder 方法传入，不破坏 workspace 分层

---

### Task 0: 环境准备

**背景:**
确保构建和测试工具链在当前开发环境中可用。

**执行步骤:**
- [x] 验证构建可用
  - `cargo build`
- [x] 验证测试框架可用
  - `cargo test --no-run`

**检查步骤:**
- [x] 构建成功
  - `cargo build 2>&1 | tail -3`
  - 预期: 输出包含 `Finished` 且无 error
- [x] 测试编译成功
  - `cargo test --no-run 2>&1 | tail -3`
  - 预期: 输出包含 `Finished` 且无 error

---

### Task 1: PeriConfig 字段扩展（C2 + C6）

**背景:**
对齐 Claude Code 的 settings.json 格式，新增 `$schema`（passthrough）和 `claudeMdExcludes`（CLAUDE.md 排除 glob）。excludes 需从 PeriConfig 传入 AgentsMdMiddleware，通过 builder 方法避免 workspace 分层违规。

**涉及文件:**
- 修改: `peri-tui/src/config/types.rs`
- 修改: `peri-middlewares/src/agents_md.rs`
- 修改: `peri-tui/src/app/agent.rs`

**执行步骤:**
- [x] 在 PeriConfig 新增 `$schema` 字段 — passthrough，不影响逻辑
  - 位置: `peri-tui/src/config/types.rs` PeriConfig struct（~L7）
  - 在 `pub config: AppConfig,` 之前插入:
    ```rust
    #[serde(rename = "$schema", skip_serializing_if = "Option::is_none")]
    pub schema: Option<String>,
    ```
- [x] 在 AppConfig 新增 `claude_md_excludes` 字段
  - 位置: `peri-tui/src/config/types.rs` AppConfig struct（~L131，`extra` 字段之前）
  - 插入:
    ```rust
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub claude_md_excludes: Option<Vec<String>>,
    ```
- [x] 在 AgentsMdMiddleware 新增 `excludes` 字段和 `with_excludes()` builder
  - 位置: `peri-middlewares/src/agents_md.rs` struct 定义（~L18-20）
  - 在 `extra_search_paths` 后新增 `excludes: Vec<String>`
  - 在 `with_extra_paths()` 后新增:
    ```rust
    pub fn with_excludes(mut self, patterns: Vec<String>) -> Self {
        self.excludes = patterns;
        self
    }
    ```
  - 在 `new()` 中初始化: `excludes: Vec::new(),`
- [x] 修改 `find_file()` 添加排除逻辑
  - 位置: `agents_md.rs` `find_file()` 方法（~L54-56）
  - 将方法体改为:
    ```rust
    fn find_file(&self, cwd: &str) -> Option<PathBuf> {
        self.candidate_paths(cwd).into_iter().find(|p| {
            if !p.is_file() { return false; }
            if self.excludes.is_empty() { return true; }
            let path_str = p.to_string_lossy();
            !self.excludes.iter().any(|pat| {
                glob::Pattern::new(pat).map(|g| g.matches(&path_str)).unwrap_or(false)
            })
        })
    }
    ```
- [x] 修改 agent.rs 传入 excludes
  - 位置: `peri-tui/src/app/agent.rs`（~L284）
  - 将 `Box::new(AgentsMdMiddleware::new())` 改为:
    ```rust
    Box::new(AgentsMdMiddleware::new().with_excludes(
        peri_config.as_ref()
            .and_then(|c| c.config.claude_md_excludes.clone())
            .unwrap_or_default()
    ))
    ```
  - 注: `peri_config` 是 `Arc<PeriConfig>` 类型，在此处已在作用域内
- [x] 为新增字段和排除逻辑编写单元测试
  - 测试文件: `peri-tui/src/config/types.rs` tests 模块 + `peri-middlewares/src/agents_md.rs` tests 模块
  - 场景:
    - `$schema` 序列化/反序列化 roundtrip → 保留且不影响逻辑
    - `claude_md_excludes` 为 None 时序列化不输出该字段
    - excludes 匹配时 CLAUDE.md 被跳过
    - excludes 不匹配时正常加载
  - 运行: `cargo test -p peri-middlewares -- agents_md && cargo test -p peri-tui -- config::types`
  - 预期: 全部通过

**检查步骤:**
- [x] PeriConfig 新字段编译通过
  - `cargo build -p peri-tui 2>&1 | tail -3`
  - 预期: `Finished` 无 error
- [x] 排除逻辑测试通过
  - `cargo test -p peri-middlewares -- agents_md 2>&1 | tail -5`
  - 预期: 所有 test ok

---

### Task 2: CLAUDE.local.md 支持（C1）

**背景:**
Claude Code 支持 `./CLAUDE.local.md` 作为不入库的个人项目配置。当前 AgentsMdMiddleware 的 `find_file()` 只返回第一个存在的文件，需要额外读取 CLAUDE.local.md 并追加到主文件内容末尾。

**涉及文件:**
- 修改: `peri-middlewares/src/agents_md.rs`

**执行步骤:**
- [x] 在 `before_agent()` 中追加 CLAUDE.local.md 读取逻辑
  - 位置: `agents_md.rs` `before_agent()` 方法（~L71-96）
  - 在读取主文件内容之后、`state.prepend_message()` 之前，插入 CLAUDE.local.md 读取:
    ```rust
    // 追加 CLAUDE.local.md（个人项目级，不入库）
    let local_path = Path::new(state.cwd()).join("CLAUDE.local.md");
    let content = if local_path.is_file() {
        let local_content = tokio::task::spawn_blocking(move || std::fs::read_to_string(&local_path))
            .await
            .map_err(|e| AgentError::MiddlewareError {
                middleware: "AgentsMdMiddleware".to_string(),
                reason: format!("spawn_blocking 失败: {e}"),
            })?
            .map_err(|e| AgentError::MiddlewareError {
                middleware: "AgentsMdMiddleware".to_string(),
                reason: format!("读取 CLAUDE.local.md 失败: {e}"),
            })?;
        if local_content.trim().is_empty() {
            content
        } else {
            format!("{content}\n\n{local_content}")
        }
    } else {
        content
    };
    ```
  - 原因: CLAUDE.local.md 追加到已有内容末尾，不单独生成 system block
- [x] 为 CLAUDE.local.md 读取编写单元测试
  - 测试文件: `peri-middlewares/src/agents_md.rs` tests 模块
  - 场景:
    - 只有 CLAUDE.md，无 CLAUDE.local.md → 只加载 CLAUDE.md
    - 同时存在 CLAUDE.md 和 CLAUDE.local.md → 两者内容合并
    - 只有 CLAUDE.local.md（无 CLAUDE.md）→ 加载 CLAUDE.local.md
    - CLAUDE.local.md 内容为空 → 不追加
  - 运行: `cargo test -p peri-middlewares -- agents_md`
  - 预期: 全部通过

**检查步骤:**
- [x] CLAUDE.local.md 合并逻辑测试通过
  - `cargo test -p peri-middlewares -- agents_md 2>&1 | tail -5`
  - 预期: 所有 test ok
- [x] 编译通过
  - `cargo build -p peri-middlewares 2>&1 | tail -3`
  - 预期: `Finished` 无 error

---

### Task 3: @import 外部文件引用（C4）

**背景:**
CLAUDE.md 中 `<!-- @import path -->` 语法允许引用外部文件内容。需递归解析，深度上限 3，循环检测。仅对 CLAUDE.md 文件生效，不对 AGENTS.md 生效。不引入 regex 依赖，手动解析。

**涉及文件:**
- 修改: `peri-middlewares/src/agents_md.rs`

**执行步骤:**
- [x] 新增 `std::collections::HashSet` import
  - 位置: `agents_md.rs` 顶部 imports（~L1）
  - 添加: `use std::collections::HashSet;`
- [x] 新增 `resolve_imports()` 函数
  - 位置: `agents_md.rs`，在 `impl AgentsMdMiddleware` 之前
  - 实现:
    ```rust
    /// 递归解析 `<!-- @import path -->` 引用，替换为引用文件内容。
    /// `base_dir` 为包含 @import 的文件所在目录。
    /// `depth` 递归深度上限 3，`visited` 防循环。
    fn resolve_imports(
        content: &str,
        base_dir: &Path,
        depth: u32,
        visited: &mut HashSet<PathBuf>,
    ) -> String {
        if depth == 0 {
            return content.to_string();
        }
        let mut result = String::with_capacity(content.len());
        let mut pos = 0;
        while pos < content.len() {
            if let Some(offset) = content[pos..].find("<!-- @import ") {
                let abs_pos = pos + offset;
                result.push_str(&content[pos..abs_pos]);
                // 提取 path：从 "<!-- @import " 之后到 " -->"
                let after = &content[abs_pos + 13..]; // 13 = "<!-- @import ".len()
                if let Some(end) = after.find(" -->") {
                    let import_path = after[..end].trim();
                    let resolved = base_dir.join(import_path).canonicalize().unwrap_or_else(|_| base_dir.join(import_path));
                    if visited.contains(&resolved) || !resolved.is_file() {
                        // 循环引用或文件不存在，保留原始占位符
                        result.push_str(&content[abs_pos..abs_pos + 13 + end + 4]);
                    } else {
                        visited.insert(resolved.clone());
                        let imported_content = std::fs::read_to_string(&resolved)
                            .unwrap_or_default();
                        let import_dir = resolved.parent().unwrap_or(base_dir);
                        let resolved_content = resolve_imports(
                            &imported_content, import_dir, depth - 1, visited,
                        );
                        result.push_str(&resolved_content);
                        // 跳过 " -->" 结尾
                    }
                    pos = abs_pos + 13 + end + 4; // 4 = " -->".len()
                } else {
                    // 没找到 " -->"，不是有效的 @import，原样保留
                    result.push_str("<!-- @import ");
                    pos = abs_pos + 13;
                }
            } else {
                result.push_str(&content[pos..]);
                break;
            }
        }
        result
    }
    ```
- [x] 在 `before_agent()` 中调用 `resolve_imports()`
  - 位置: `agents_md.rs` `before_agent()` 方法，在内容读取之后、`state.prepend_message()` 之前
  - 判断当前加载的文件名是否为 CLAUDE.md（通过 `path` 的文件名判断）:
    ```rust
    // 仅对 CLAUDE.md 系列文件解析 @import（AGENTS.md 不处理）
    let is_claude_md = path.file_name()
        .map(|n| n.to_string_lossy().starts_with("CLAUDE"))
        .unwrap_or(false);
    let content = if is_claude_md {
        let dir = path.parent().unwrap_or_else(|| Path::new(state.cwd()));
        let mut visited = HashSet::new();
        visited.push(path.to_path_buf());
        resolve_imports(&content, dir, 3, &mut visited)
    } else {
        content
    };
    ```
  - 注: 此段在 Task 2 的 CLAUDE.local.md 追加逻辑之后插入，对合并后的内容做 @import 解析
  - 但 CLAUDE.local.md 的 @import 也需要解析。将 `is_claude_md` 检查改为始终对合并内容执行 @import 解析（如果主文件或 local 文件任一是 CLAUDE.md 系列）
- [x] 为 @import 解析编写单元测试
  - 测试文件: `peri-middlewares/src/agents_md.rs` tests 模块
  - 场景:
    - 简单 @import → 内容被替换
    - 嵌套 @import（depth 2）→ 正确展开
    - 超深嵌套（depth > 3）→ 保留原始占位符
    - 循环引用 → 保留原始占位符不 panic
    - 引用不存在文件 → 保留原始占位符
    - 非法格式（缺少 ` -->`）→ 保留原始文本
  - 运行: `cargo test -p peri-middlewares -- agents_md`
  - 预期: 全部通过

**检查步骤:**
- [x] @import 单元测试通过
  - `cargo test -p peri-middlewares -- agents_md 2>&1 | tail -5`
  - 预期: 所有 test ok
- [x] 编译无 warning
  - `cargo build -p peri-middlewares 2>&1 | grep -c warning`
  - 预期: 输出 0

---

### Task 4: /effort + /rename 命令（T1 + T2）

**背景:**
新增两个轻量 TUI 命令：`/effort` 调整推理力度，`/rename` 修改当前会话标题。参考 `/model <alias>` 的即时切换模式。T2 需要在 ThreadStore trait 和 SqliteThreadStore 中新增 `update_title()` 方法。

**涉及文件:**
- 新建: `peri-tui/src/command/effort.rs`
- 新建: `peri-tui/src/command/rename.rs`
- 修改: `peri-tui/src/command/mod.rs`
- 修改: `peri-agent/src/thread/store.rs`
- 修改: `peri-agent/src/thread/sqlite_store.rs`

**执行步骤:**
- [x] 在 ThreadStore trait 新增 `update_title()` 方法
  - 位置: `peri-agent/src/thread/store.rs`（~L34，`delete_thread` 之后）
  - 添加默认方法:
    ```rust
    /// 更新指定 thread 的标题
    async fn update_title(&self, id: &ThreadId, title: &str) -> Result<()> {
        let mut meta = self.load_meta(id).await?;
        meta.title = Some(title.to_string());
        self.update_meta(id, meta).await
    }
    ```
  - 注: 使用默认实现（load_meta → update_meta），SqliteThreadStore 无需额外代码。但为效率可提供优化实现。
- [x] 在 SqliteThreadStore 新增 `update_title()` 优化实现
  - 位置: `peri-agent/src/thread/sqlite_store.rs`（~L249，`update_meta` 之后）
  - 添加:
    ```rust
    async fn update_title(&self, id: &ThreadId, title: &str) -> Result<()> {
        let now = chrono::Utc::now().to_rfc3339();
        sqlx::query("UPDATE threads SET title = ?1, updated_at = ?2 WHERE id = ?3")
            .bind(title)
            .bind(&now)
            .bind(id.as_str())
            .execute(&self.pool)
            .await?;
        Ok(())
    }
    ```
- [x] 新建 `/effort` 命令文件
  - 新建: `peri-tui/src/command/effort.rs`
  - 内容:
    ```rust
    use crate::app::{agent, App, MessageViewModel};
    use crate::command::Command;

    pub struct EffortCommand;

    impl Command for EffortCommand {
        fn name(&self) -> &str {
            "effort"
        }

        fn description(&self) -> &str {
            "查看或设置推理力度（low/medium/high）"
        }

        fn execute(&self, app: &mut App, args: &str) {
            let arg = args.trim().to_lowercase();
            match arg.as_str() {
                "low" | "medium" | "high" => {
                    let cfg = app.services
                        .peri_config
                        .get_or_insert_with(Default::default);
                    cfg.config.thinking.get_or_insert_with(|| Default::default()).effort = arg.clone();
                    if let Err(e) = App::save_config(cfg, app.services.config_path_override.as_deref()) {
                        app.session_mgr.current_mut().messages.view_messages.push(
                            MessageViewModel::system(format!("配置保存失败: {}", e)),
                        );
                        return;
                    }
                    if let Some(p) = agent::LlmProvider::from_config(cfg) {
                        app.services.provider_name = p.display_name().to_string();
                        app.services.model_name = p.model_name().to_string();
                    }
                    let vm = MessageViewModel::system(format!("推理力度已设为 {}", arg));
                    app.session_mgr.current_mut().messages.view_messages.push(vm.clone());
                    let _ = app.session_mgr.current_mut().messages.render_tx
                        .send(crate::ui::render_thread::RenderEvent::AddMessage(vm));
                }
                _ => {
                    let current = app.services.peri_config
                        .as_ref()
                        .and_then(|c| c.config.thinking.as_ref())
                        .map(|t| t.effort.as_str())
                        .unwrap_or("high");
                    let vm = MessageViewModel::system(format!(
                        "当前推理力度: {}\n用法: /effort low|medium|high", current
                    ));
                    app.session_mgr.current_mut().messages.view_messages.push(vm.clone());
                    let _ = app.session_mgr.current_mut().messages.render_tx
                        .send(crate::ui::render_thread::RenderEvent::AddMessage(vm));
                }
            }
        }
    }
    ```
- [x] 新建 `/rename` 命令文件
  - 新建: `peri-tui/src/command/rename.rs`
  - 内容:
    ```rust
    use crate::app::{App, MessageViewModel};
    use crate::command::Command;

    pub struct RenameCommand;

    impl Command for RenameCommand {
        fn name(&self) -> &str {
            "rename"
        }

        fn description(&self) -> &str {
            "查看或修改当前会话标题"
        }

        fn execute(&self, app: &mut App, args: &str) {
            let name = args.trim();
            let session = app.session_mgr.current_mut();

            let Some(thread_id) = session.current_thread_id.clone() else {
                let vm = MessageViewModel::system("当前无活跃会话，无法重命名".to_string());
                session.messages.view_messages.push(vm.clone());
                let _ = session.messages.render_tx
                    .send(crate::ui::render_thread::RenderEvent::AddMessage(vm));
                return;
            };

            if name.is_empty() {
                // 显示当前标题
                let store = &app.services.thread_store;
                let title = tokio::task::block_in_place(|| {
                    store.load_meta(&thread_id).ok().and_then(|m| m.title)
                }).unwrap_or_else(|| "(无标题)".to_string());
                let vm = MessageViewModel::system(format!("当前标题: {}", title));
                session.messages.view_messages.push(vm.clone());
                let _ = session.messages.render_tx
                    .send(crate::ui::render_thread::RenderEvent::AddMessage(vm));
            } else {
                // 更新标题
                let store = &app.services.thread_store;
                let result = tokio::task::block_in_place(|| {
                    store.update_title(&thread_id, name)
                });
                match result {
                    Ok(()) => {
                        let vm = MessageViewModel::system(format!("会话标题已更新为: {}", name));
                        session.messages.view_messages.push(vm.clone());
                        let _ = session.messages.render_tx
                            .send(crate::ui::render_thread::RenderEvent::AddMessage(vm));
                    }
                    Err(e) => {
                        let vm = MessageViewModel::system(format!("重命名失败: {}", e));
                        session.messages.view_messages.push(vm.clone());
                        let _ = session.messages.render_tx
                            .send(crate::ui::render_thread::RenderEvent::AddMessage(vm));
                    }
                }
            }
        }
    }
    ```
  - 注: 使用 `block_in_place` 因为 `execute()` 不是 async，而 `update_title` 是 async。tokio::task::block_in_place 允许在 async runtime 中阻塞当前线程而不影响其他任务。
- [x] 在 `mod.rs` 注册新命令
  - 位置: `peri-tui/src/command/mod.rs`
  - 在 `pub mod split;` 之后添加:
    ```rust
    pub mod effort;
    pub mod rename;
    ```
  - 在 `default_registry()` 函数中，`r.register(Box::new(hooks::HooksCommand));` 之后添加:
    ```rust
    r.register(Box::new(effort::EffortCommand));
    r.register(Box::new(rename::RenameCommand));
    ```
- [x] 为 update_title 编写单元测试
  - 测试文件: `peri-agent/src/thread/sqlite_store.rs` tests 模块
  - 场景:
    - update_title 后 load_meta 返回新标题
    - update_title 后 updated_at 已更新
  - 运行: `cargo test -p peri-agent -- sqlite_store`
  - 预期: 全部通过

**检查步骤:**
- [x] 命令编译通过
  - `cargo build -p peri-tui 2>&1 | tail -3`
  - 预期: `Finished` 无 error
- [x] update_title 测试通过
  - `cargo test -p peri-agent -- sqlite_store 2>&1 | tail -5`
  - 预期: 所有 test ok

---

### Task 5: /doctor 健康检查（T5）

**背景:**
新增 `/doctor` 命令检测配置完整性，帮助用户排查启动问题。检查 5 项：settings.json 存在、API Key 设置、Provider 配置、MCP 配置、Model Alias 配置。结果以 Markdown 表格输出到消息区。

**涉及文件:**
- 新建: `peri-tui/src/command/doctor.rs`
- 修改: `peri-tui/src/command/mod.rs`

**执行步骤:**
- [x] 新建 `/doctor` 命令文件
  - 新建: `peri-tui/src/command/doctor.rs`
  - 内容:
    ```rust
    use crate::app::{App, MessageViewModel};
    use crate::command::Command;

    pub struct DoctorCommand;

    impl Command for DoctorCommand {
        fn name(&self) -> &str {
            "doctor"
        }

        fn description(&self) -> &str {
            "诊断配置完整性"
        }

        fn execute(&self, app: &mut App, _args: &str) {
            let mut lines = vec!["Doctor 检查结果：".to_string(), "".to_string()];

            // 1. Settings 文件
            let settings_path = dirs_next::home_dir()
                .map(|h| h.join(".peri").join("settings.json"));
            let settings_status = match &settings_path {
                Some(p) if p.is_file() => format!("OK  {}", p.display()),
                Some(p) => format!("Missing  {}", p.display()),
                None => "Missing  无法获取 home 目录".to_string(),
            };
            lines.push(format!("| 检查项 | 状态 | 详情 |"));
            lines.push(format!("|--------|------|------|"));
            lines.push(format!("| Settings | {} |", settings_status));

            // 2. API Key
            let has_anthropic = std::env::var("ANTHROPIC_API_KEY").map(|k| !k.is_empty()).unwrap_or(false);
            let has_openai = std::env::var("OPENAI_API_KEY").map(|k| !k.is_empty()).unwrap_or(false);
            let api_status = if has_anthropic || has_openai {
                let keys: Vec<&str> = [
                    has_anthropic.then_some("ANTHROPIC_API_KEY"),
                    has_openai.then_some("OPENAI_API_KEY"),
                ].into_iter().flatten().collect();
                format!("OK  {}", keys.join(" + "))
            } else {
                "Missing  未设置 ANTHROPIC_API_KEY 或 OPENAI_API_KEY".to_string()
            };
            lines.push(format!("| API Key | {} |", api_status));

            // 3. Provider 配置
            let provider_status = match &app.services.peri_config {
                Some(cfg) if !cfg.config.providers.is_empty() => {
                    let active = &cfg.config.active_provider_id;
                    let provider = cfg.config.providers.iter().find(|p| p.id == *active);
                    match provider {
                        Some(p) => format!("OK  {} ({})", p.display_name(),
                            p.models.get_model(&cfg.config.active_alias).unwrap_or("default")),
                        None => format!("No Provider  active_provider_id '{}' 未找到", active),
                    }
                }
                _ => "No Provider  未配置任何 Provider".to_string(),
            };
            lines.push(format!("| Provider | {} |", provider_status));

            // 4. MCP 配置
            let mcp_project = std::path::Path::new(&app.services.cwd).join(".mcp.json");
            let mcp_global = settings_path.as_ref().map(|p| {
                p.parent().unwrap_or_else(|| std::path::Path::new("/")).join("settings.json")
            });
            let mcp_status = if app.services.mcp_pool.is_some() {
                "OK  MCP 连接池已初始化".to_string()
            } else if mcp_project.is_file() {
                "None  .mcp.json 存在但 MCP 未初始化".to_string()
            } else {
                "None  未配置 MCP 服务器".to_string()
            };
            lines.push(format!("| MCP | {} |", mcp_status));

            // 5. Model Alias
            let alias_status = match &app.services.peri_config {
                Some(cfg) => {
                    let p = cfg.config.providers.iter().find(|p| p.id == cfg.config.active_provider_id);
                    match p {
                        Some(p) => {
                            let aliases: Vec<String> = ["opus", "sonnet", "haiku"].iter()
                                .filter(|a| !p.models.get_model(a).unwrap_or("").is_empty())
                                .map(|a| a.to_string())
                                .collect();
                            if aliases.is_empty() {
                                "No Alias  未配置任何模型别名".to_string()
                            } else {
                                format!("OK  {}", aliases.join("/"))
                            }
                        }
                        None => "No Alias  无活跃 Provider".to_string(),
                    }
                }
                _ => "No Alias  未配置".to_string(),
            };
            lines.push(format!("| Model Alias | {} |", alias_status));

            let vm = MessageViewModel::system(lines.join("\n"));
            app.session_mgr.current_mut().messages.view_messages.push(vm.clone());
            let _ = app.session_mgr.current_mut().messages.render_tx
                .send(crate::ui::render_thread::RenderEvent::AddMessage(vm));
        }
    }
    ```
- [x] 在 `mod.rs` 注册 `/doctor` 命令
  - 位置: `peri-tui/src/command/mod.rs`
  - 在 `pub mod rename;` 之后添加: `pub mod doctor;`
  - 在 `default_registry()` 中添加: `r.register(Box::new(doctor::DoctorCommand));`
- [x] 为 doctor 命令编写 headless 集成测试（可选，简单场景）
  - 测试文件: `peri-tui/src/command/doctor.rs` 底部 `#[cfg(test)] mod tests`
  - 场景:
    - 无配置时输出包含 "No Provider" 和 "Missing"
  - 运行: `cargo test -p peri-tui -- doctor`
  - 预期: 通过

**检查步骤:**
- [x] 命令编译通过
  - `cargo build -p peri-tui 2>&1 | tail -3`
  - 预期: `Finished` 无 error
- [x] doctor 测试通过
  - `cargo test -p peri-tui -- doctor 2>&1 | tail -5`
  - 预期: 所有 test ok

---

### Task 6: 简单兼容特性批次 验收

**前置条件:**
- 启动命令: `cargo build -p peri-tui && cargo test`

**端到端验证:**

1. - [x] 运行完整测试套件确保无回归
   - `cargo test 2>&1 | tail -10`
   - 预期: 全部测试通过
   - 失败排查: 检查各 Task 的测试步骤

2. - [x] C2 验证: `$schema` 字段 passthrough
   - 创建包含 `$schema` 的 settings.json → 加载 → 保存 → 字段保留
   - `cargo test -p peri-tui -- config::types 2>&1 | tail -5`
   - 预期: 所有 test ok
   - 失败排查: 检查 Task 1 PeriConfig 字段定义

3. - [x] C1 验证: CLAUDE.local.md 加载
   - `cargo test -p peri-middlewares -- agents_md 2>&1 | tail -5`
   - 预期: 包含 CLAUDE.local.md 相关测试通过
   - 失败排查: 检查 Task 2 before_agent() 修改

4. - [x] C4 验证: @import 解析
   - `cargo test -p peri-middlewares -- agents_md 2>&1 | tail -5`
   - 预期: 包含 resolve_imports 相关测试通过
   - 失败排查: 检查 Task 3 resolve_imports 函数

5. - [x] T1 验证: /effort 命令切换
   - `cargo test -p peri-tui -- effort 2>&1 | tail -5`
   - 预期: 测试通过
   - 失败排查: 检查 Task 4 EffortCommand

6. - [x] T2 验证: /rename 命令
   - `cargo test -p peri-agent -- sqlite_store 2>&1 | tail -5`
   - 预期: update_title 测试通过
   - 失败排查: 检查 Task 4 ThreadStore 新方法

7. - [x] T5 验证: /doctor 命令
   - `cargo test -p peri-tui -- doctor 2>&1 | tail -5`
   - 预期: 测试通过
   - 失败排查: 检查 Task 5 DoctorCommand

8. - [x] 全量构建无 warning
   - `cargo build 2>&1 | grep -c warning`
   - 预期: 输出 0
   - 失败排查: 定位 warning 来源，修复
