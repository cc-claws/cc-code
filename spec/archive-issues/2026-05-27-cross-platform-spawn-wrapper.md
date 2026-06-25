> 归档于 2026-05-27，原路径 spec/issues/2026-05-27-cross-platform-spawn-wrapper.md

# Windows 下 `Command::new()` 无法执行 .cmd 脚本（npx 等），���统一跨平台 spawn 封装

**状态**：Fixed
**优先级**：高
**创建日期**：2026-05-27
**修复日期**：2026-05-27

## 问题描述

Windows 上 MCP 配置 `"command": "npx"` 无法被 `tokio::process::Command::new("npx")` 直接执行——因为 `npx` 实质是 `.cmd` 批处理脚本，必须手动改为 `"command": "cmd", "args": ["/c", "npx", ...]` 才能运行。项目中存在 3 处 spawn 调用点，各自用不同方式处理跨平台，没有统一封装，导致 Windows 用户配置心智负担大且容易出错。

## 症状详情

| 场景 | Windows 表现 | 预期行为 |
|------|-------------|---------|
| MCP 配置 `"command": "npx"` | 启动失败，找不到可执行文件 | 应自动通过 shell 包裹执行 |
| Hooks 执行 shell 命令 | `shell` 来源未确认是否 Windows 兼容 | 应统一使用 `cmd /C` |
| Bash 工具执行命令 | 已用 `cfg!` 切换 `cmd /C` / `bash -c` ✅ | — |

## 涉及文件

| 文件 | 行号 | 现状 | Windows 兼容 |
|------|------|------|-------------|
| `peri-middlewares/src/mcp/client.rs` | 345-382 | `spawn_stdio_transport()` 直接 `Command::new(command)` | ❌ |
| `peri-middlewares/src/middleware/terminal.rs` | 160-174 | `cfg!` 切换 `cmd /C` vs `bash -c` | ✅ |
| `peri-middlewares/src/hooks/executor.rs` | 66-84 | `Command::new(shell)` + `-c`，shell 来源未确认 | ❓ |

## 解决方向

封装统一跨平台 spawn 工具函数：

- **Windows**：`cmd /C <command> <args>`
- **Unix**：`bash -c <command> <args>`

已调研并排除以下第三方 crate：
- `xshell`：同步 API，不适配 tokio 异步场景
- `system`：过于简单基础
- `command_runner`：不成熟
- `which`：仅做路径解析，不解决 spawn 问题

**决策**：自己封装工具函数，统一 3 处调用点（MCP transport + Hooks executor + Bash 工具）。

## 修复记录

**方案**：新增 `peri-middlewares/src/process/` 模块，提供两层 API：

| 函数 | 用途 |
|------|------|
| `shell_command(command, args) -> Command` | 底层 builder，返回 `tokio::process::Command` 供调用者自定义配置 |
| `spawn_shell(command, args) -> io::Result<Child>` | 快捷 spawn，预置 piped stdio + kill_on_drop + process_group |
| `spawn_shell_with_env(command, args, env) -> io::Result<Child>` | 同上 + 环境变量注入（MCP 场景） |

**行为策略**：统一 shell 包裹（用户访谈确认）——所有调用点始终走 shell 包裹，不做自动检测或配置字段区分。

| 平台 | 包裹方式 |
|------|---------|
| Unix | `bash -c "<command> <args...>"`，含特殊字符参数用单引号转义 |
| Windows | `cmd /C <command> <args...>` |

**变更文件**：

| 文件 | 变更 |
|------|------|
| `peri-middlewares/src/process/mod.rs` | 新增：`shell_command` + `spawn_shell` + `spawn_shell_with_env` |
| `peri-middlewares/src/process/process_test.rs` | 新增：3 个单元测试 |
| `peri-middlewares/src/lib.rs` | 新增 `pub mod process;` |
| `peri-middlewares/src/mcp/client.rs:345` | `Command::new(command)` → `shell_command(command, &arg_strs)` |
| `peri-middlewares/src/middleware/terminal.rs:160` | 内联 `cfg!` 块 → `shell_command(command, &[])` |
| `peri-middlewares/src/hooks/executor.rs:62` | `Command::new(&shell).arg("-c")` → `shell_command(&command, &[])`，`shell` 字段不再用于 spawn |

**Commits**：
- `b85e26e` feat(middlewares): add cross-platform process spawn module
- `992f80b` refactor(mcp): use cross-platform shell_command for spawn_stdio_transport
- `97f971e` refactor(terminal): use cross-platform shell_command in Bash tool

**验证**：809 tests pass / build clean / clippy clean

**设计文档**：`docs/superpowers/specs/2026-05-27-cross-platform-spawn-design.md`
**实现计划**：`docs/superpowers/plans/2026-05-27-cross-platform-spawn.md`
