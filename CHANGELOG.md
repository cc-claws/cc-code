# Changelog

Perihelion Agent 版本变更记录。

---

## v0.6.43 — 2026-07-02

### Refactor

- **状态栏工具名映射统一**：删除 `status_bar.rs` 中重复的 `format_tool_display_name` 函数，复用 `tool_display::format_tool_name`，修复 `FolderOperations` 在状态栏未简写为 "Folder" 的问题

---

## v0.6.42 — 2026-07-02

### Bug Fixes

- **后台面板输出自适应终端高度**（#110）：后台任务面板 output 区域根据终端高度动态调整，避免内容溢出
- **Ctrl+B 显示已运行时间**（#110）：后台 shell 任务在面板中展示已运行时长
- **Bash Ctrl+B 提示计时修正**：`Ctrl+B` 提示计时起始点修正，message pipeline 新增 transform/reconcile 阶段支持后台状态注入

---

## v0.6.41 — 2026-07-02

### Bug Fixes

- **MultiplexBroker 竞速跳过全 Reject**：ChannelBroker 无授权时不再抢先返回 Reject，MultiplexBroker 继续等待 TUI broker 的 Approve，消除「no authorized channels」误报
- **Ctrl+B 竞态**：`background_agent_foreground` 入口处 drain pending agent shell 注册，避免延迟注册未到达时 Ctrl+B 找不到前台 shell
- **/model 命令动态 alias**：改为从当前 provider 动态收集非空 alias 匹配，替代硬编码 opus/sonnet/haiku
- **Command 模式光标偏移**：draw_bar_cursor 改用 display_textarea 作为光标源，修正 ! 前缀移除后的位置偏移
- **tip 文案同步**：tip-2 改为「当前 Provider 的可用模型间切换」，tip-6 改为 Ctrl+U/D 滚动

---

## v0.6.40 — 2026-07-02

### Features

- **状态栏权限模式 cycle 提示**：权限模式标签后追加 `(Shift+Tab to cycle)` 灰色提示，方便用户发现快捷键

---

## v0.6.39 — 2026-07-02

### Features

- **跨 provider 模型切换**（#107）：model selection 值使用 `provider_id::alias` 格式，支持同名 alias 跨 provider 正确切换；模型列表展示所有 provider 的可用模型

### Bug Fixes

- **状态栏工具历史行对齐**（#105）：修复第二行在无内容时缺少前导空格导致与其他行左对齐不一致
- **Bash 输出预览窗口缩小**（#109）：Bash 输出截断阈值从 2000 行/100KB 降为 50 行/20KB，完整内容落盘供 Read 按需查看，避免大输出撑爆 context window

---

## v0.6.38 — 2026-07-01

### Bug Fixes

- **模型名上下文窗口标记过滤**：配置中 `mimo-v2.5-pro[1M]` 等含 `[...]` 后缀的模型名，传 API 时自动过滤为 `mimo-v2.5-pro`

---

## v0.6.37 — 2026-07-01

### Bug Fixes

- **状态栏第二行占位恢复**：修复工具执行前默认占位丢失的问题

---

## v0.6.36 — 2026-07-01

### Bug Fixes

- **状态栏第二行快捷键调整**：默认 hints 从 `Tab ::切换模式` 改为 `Ctrl+O ::详情`；详细模式新增 `● Verbose` 标识 + `Ctrl+O ::退出详细`；format_hints 支持空描述跳过

---

## v0.6.35 — 2026-07-01

### Features

- **PermissionMode 循环跳过 DontAsk**：Shift+Tab 循环切换权限模式时跳过 DontAsk，顺序变为 Default → AcceptEdit → AutoMode → Bypass
- **shell_command_with_shell 新增**：支持显式指定 shell（powershell/pwsh/bash），Hook executor 现在正确传递 shell 参数

### Bug Fixes

- **status_bar DontAsk 渲染修复**：补回 DontAsk match 分支防止 non-exhaustive 编译错误，删除残留 hint 引用

---

## v0.6.34 — 2026-07-01

### Features

- **滚动条和状态栏优化**（#104）：优化 TUI 滚动条和状态栏的渲染效果

### Bug Fixes

- **Windows CI 流式输出测试超时**：修复 Windows CI 环境下 Python 流式输出测试超时的问题
- **滚动条测试修复**：修复滚动条相关测试用例

### Documentation

- 完善项目所有 crate 的文档

---

## v0.6.33 — 2026-06-30

### Bug Fixes

- **控制字符渲染异常修复**（#102）：修复控制字符和 ANSI 转义序列导致 TUI 渲染异常的问题
- **Clippy 警告修复**：`map_or(true, ...)` 改为 `is_none_or(...)`，适配 Rust 1.95 新增的 `unnecessary_map_or` lint

---

## v0.6.32 — 2026-06-29

### Bug Fixes

- **后台 shell 通知显示优化**（#100）：后台 shell 完成/等待输入的 XML 通知在 TUI 聊天区显示为可读的中文提示（SystemNote），而非原始 XML 标签。前台小命令快速结束不再打断对话流，仅后台化（Ctrl+B）命令注入通知
- **`/review` skill fallback 测试修复**：mock skill 名称从 `review` 改为 `deploy`，避免与内置 `/review` 命令冲突
- **`Stdio` import 条件编译**：`peri-middlewares/terminal.rs` 的 `std::process::Stdio` 加 `#[cfg(windows)]` 修复非 Windows 平台 Clippy unused-import 错误

---

## v0.6.29 — 2026-06-28

### Features

- **Ctrl+B 后台 Shell**（#99）：Shell 命令支持 Ctrl+B 转为后台运行，输出写入磁盘，支持后台任务面板查看
- **`/commit` 命令**（#93）：一键 git commit，自动生成 commit message
- **`/review` 命令**（#95）：PR 代码审查
- **`/export` 命令**（#95）：对话导出
- **Read 工具行范围显示**（#91）：Read tool header 显示 offset/limit 行范围
- **全局屏幕选区（ScreenSelection）**（#94）：新增基于渲染 Buffer 的全局选区，覆盖面板、状态栏、sticky header、bg agent bar、空白区域。与消息区域现有 TextSelection 跨区域衔接，松开鼠标自动复制到剪贴板，蓝色高亮显示。详见 [spec/features/screen-selection-prd.md](spec/features/screen-selection-prd.md)
- **消息区域 TextSelection 内容锚定**：选区以消息内容为锚（而非屏幕坐标），滚动后选区跟随内容，复制纯文本不受 buffer 渲染影响
- **双击选整行**：消息区双击用 TextSelection 选整行（纯文本），其他非 textarea 区域双击用 ScreenSelection 选整屏行
- **spinner / 总结行可选可复制**：`✻ Brewed for...` + 进度条等位于 messages_area 底部但不在 wrap_map 内的行，现可通过 ScreenSelection 选中复制
- **选区 auto-scroll 改进**：auto-scroll 仅在鼠标移出消息区域外时触发，区域内首末行可正常选中（修复"最后 1 行难选中"）
- **复制 toast UX**：复制成功后状态栏显示 "已复制 N 个字符" toast

### Bug Fixes

- **拖选溢出 panic**：`visual_row + scroll_offset` 计算改用 `saturating_add`，修复 `scroll_offset=usize::MAX`（初始/提交/scroll_to_bottom 状态）时拖选导致 exit code 101 的崩溃
- **安装脚本 tag 前缀**：`install.sh` / `install.ps1` 匹配 `npm-v*` release tag 前缀
- **Clippy warnings**：`ShellCommandPool` Default derive + `map_or` → `is_none_or`

---

## v0.6.22 — 2026-06-27

### Security

- **MCP OAuth CSRF 防御**（#82）：回调服务器注入 rmcp 生成的 state 参数，纵深防御 CSRF 攻击
- **Session UUID 校验**（#74）：`session/load` + `session/resume` 增加 UUID 格式校验和存在性校验，防止路径穿越
- **文件权限加固**（#81）：history_persistence 文件权限 0600，grandparent 目录权限校验
- **At-mention 目录注入防护**（#77）：防止 `@path` 引用越权访问
- **工具输出截断加固**（#80）：防止超长输出绕过截断机制

### Bug Fixes

- **输入框鼠标乱码**（#88）：移除 `?1003h`（any-event tracking），防止 ConPTY 缓冲区溢出导致 SGR 鼠标转义序列泄漏为文本
- **Windows Ctrl+C 双击退出**（#86）：100ms debounce 防止 ConPTY 重复事件
- **AskUser 弹窗高度**（#76）：修复弹窗高度计算错误
- **Tab 缩进编辑**（#76）：修复 tab 缩进文件的 Edit 工具匹配问题
- **Grep offset 测试**（#86）：兼容 `persist_truncated_output` 附加行

### Chore

- npm 版本 bump 到 0.6.22

---

## v0.6.21 — 2026-06-27

### Features

- **Windows Git Bash fallback**：`cmd /C` 执行 Linux 命令（`grep`/`ls`/`find` 等）失败时自动 fallback 到 Git Bash，Agent 无需自行重试
  - 多语言 stderr 匹配（English/中文/法语/德语）+ 兜底模式
  - `GIT_BASH_PATH` 环境变量支持，`bash --version` 验证
  - `MSYS_NO_PATHCONV=1` 防止 MSYS 路径转换
  - 剩余超时继承（总超时 - cmd 耗时，至少 10s）
  - `git commit -m` 重写与 fallback 的 temp 文件清理时序修复

---

## v0.6.19 — 2026-06-26

### Bug Fixes

- **Windows GBK 编码修复**：`shell_exec.rs`（TUI `!command`）和 `executor.rs`（hook）的 subprocess stdout/stderr 在 Windows 中文环境下正确解码 GBK→UTF-8，不再显示乱码
- 共享 `decode_output_bytes()` 提取到 `peri-agent/encoding.rs`，消除重复代码
- `shell_exec.rs` 中文 anyhow context 消息改为英文

---

## v0.6.18 — 2026-06-26

### i18n

- **peri-lsp 硬编码中文改英文**：error.rs 12 处、transport.rs 8 处、client.rs 4 处、pool.rs 8 处，共 32 处 `#[error()]` / tracing / 错误字符串
- **peri-agent 硬编码中文改英文**：sqlite_store.rs 2 处、filesystem.rs 4 处，共 6 处 anyhow context 消息

---

## v0.6.17 — 2026-06-26

### i18n

- **spinner 和 thinking 块跟随语言设置**：peri-widgets 新增 `set_mode_with_label()` / `pick_verb_from()` 接口，TUI 调用方通过 `lc.tr()` 传入翻译后的 label。用户 `/lang en` 后 spinner 显示 "Thinking…"，`/lang zh-CN` 显示 "思考中…"
- 新增 `spinner-thinking` / `spinner-tool-use` / `spinner-responding` / `spinner-thinking-header` 翻译 key（en + zh-CN）

---

## v0.6.15 — 2026-06-25

### Bug Fixes

- **TUI 模型切换快捷键收敛**：删 Ctrl+Shift+T / Alt+Shift+M（与 Ctrl+P 命令面板重叠），统一 Ctrl+P 作为 Provider/Model/Effort 完整选择入口
- **Ctrl+T 硬编码 alias bug**：原硬编码 `[opus, sonnet, haiku]` 三选一，未按当前 Provider 实际配置过滤——切到只配 1 个 alias 的 Provider 时无法切换。改为从激活 Provider 的 `ProviderModels` 动态收集非空 alias

### Documentation

- CLAUDE.md 新增「PR / Issue 流程」+「分支命名规则」段落
- 分支名禁用 `#` 字符（会让 GitHub Actions `pull_request` trigger 静默失效）
- spec/issues/ 补模型切换快捷键收敛 issue 详细分析文档

### Chore

- 清理误传的 `.claude/CLAUDE.md`（adim 钉钉/PowerShell/PHP 那套无关内容），加入 `.gitignore`

---

## v0.6.13 — 2026-06-25

### npm 包

- npm 包二进制命名统一为 `cc-code-*`（原 `peri-*`），与 CI workflow 对齐
- `install.js` 下载文件名、解压目标、Windows wrapper 全部改为 `cc-code`
- `bin/cc-code` wrapper 查找 `cc-code-bin` / `cc-code.exe`
- `npm/README.md` 命令示例更新为 `cc-code`
- 删除 `mimo-code-vs-peri-analysis.md`

---

## v0.99.14 — 2026-06-02

### Performance

- 全局分配器从 mimalloc 切换到 jemalloc，碎片管理更优
- tokio worker_threads 限制为 4，18 核机器节省约 56 MB 栈空间
- list_threads 排除 cached_context 大字段，每线程内存从约 1 MB 降至约 1 KB
- LlmCallStart.messages 改为 Arc\<Vec\>，消除每次 LLM 调用的全量 clone
- history_for_cancel 用 Option\<MessageId\> 替代完整消息 clone

### Features

- **Rewind 对话回滚**：双击 ESC 弹窗选择回滚点，支持 /rewind 命令
- **/gc 命令**：手动内存回收 + RSS/jemalloc breakdown 诊断

### Bug Fixes

- PermissionRequest hook 在 Bypass/AutoMode 下不应触发
- 从 ~/.claude/settings.json 加载全局 hooks + TUI 退出时 fire SessionEnd
- /clear 时关闭旧 session 防内存泄漏
- 过滤 ACP 下发命令中与本地注册表重复的条目
- AgentResult invoke 消息优化，防止 LLM 轮询循环

### Refactoring

- CLAUDE.md 拆分为子模块文件
- 提取 ACP 共享逻辑，消除 TUI/Stdio 重复代码
- 移除 /split 命令
