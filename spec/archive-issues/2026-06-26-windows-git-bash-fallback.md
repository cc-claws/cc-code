# Windows Git Bash Fallback

**Status**: Done（2026-06-27，v0.6.21 发版，PR #84，commit c162dfe1 / c3afbfe7）
**Date**: 2026-06-26
**Category**: Feature
**Priority**: Medium

## 问题

Windows 下 `shell_command()` 使用 `cmd /C` 执行命令。Agent 频繁使用 Linux 命令（`grep`、`ls`、`find`、`sed`、`awk`、`cat` 等），`cmd.exe` 不认识这些命令，直接报错：

```
'grep' is not recognized as an internal or external command
```

导致 Agent 需要自行重试（改用 `findstr`、`dir` 等），浪费一轮 LLM 调用，体验差。

## 方案

**失败后 fallback 到 Git Bash**（仅 Windows）。

### 核心逻辑

1. 启动时检测 Git Bash 路径，`OnceLock<Option<PathBuf>>` 缓存
2. `shell_command()` 仍返回 `cmd /C` 的 Command（不改签名）
3. **仅 BashTool::invoke** 在收到输出后判断是否需要 fallback：
   - exit code ≠ 0 **且** stderr 匹配命令未识别模式
   - 检查 Git Bash 是否可用
   - 可用则用 `bash -c` 重试同一命令，**继承原命令的剩余超时**
   - 输出末尾追加 `[Retried with Git Bash]` 提示
4. Git Bash 不可用则保持原行为不变

### 改动范围决策：仅 BashTool，不改 shell_command()

**理由**：fallback 需要读取 stderr + exit code 来判断，`shell_command()` 只负责构造 `Command` 对象，不执行。将 fallback 放在 `shell_command()` 会导致：
- 函数签名从返回 `Command` 变成返回 `Command` + fallback 闭包，所有 5 个调用方都要改
- hooks/MCP 的 stderr 语义不同（插件脚本的 "not found" 不应该触发 bash retry）

**结论**：fallback 逻辑仅放在 `BashTool::invoke()` 中。其他调用点（hooks/executor、mcp/client、shell_exec、update）不受影响。

### Git Bash 检测（`git_bash_path()`）

`OnceLock<Option<PathBuf>>` 缓存，按优先级依次尝试：

1. `C:\Program Files\Git\bin\bash.exe`（官方安装器默认路径）
2. `C:\Program Files (x86)\Git\bin\bash.exe`（32 位安装）
3. `where bash`（PATH 中有 bash 时——scoop、choco、MSYS2 等）
4. 环境变量 `GIT_BASH_PATH`（用户显式指定，优先级最高）

检测到的路径做 `Command::new(path).arg("--version").output()` 验证，确认可执行。

### stderr 匹配模式

cmd.exe 的"命令未识别"错误因系统语言不同：

| 系统语言 | stderr 关键词 |
|----------|--------------|
| English  | `is not recognized as an internal or external command` |
| 中文     | `不是内部或外部命令，也不是可运行的程序` |
| 其他     | exit code ≠ 0 + 无 stdout + stderr 长度 < 200 bytes（兜底） |

匹配策略：exit code ≠ 0 **且** stderr 包含以下任一关键词：
- `"is not recognized"`
- `"不是内部或外部命令"`
- `"n'est pas reconnu"`（法语）
- `"nicht als Befehl erkannt"`（德语）

或者：exit code ≠ 0 **且** stdout 为空 **且** stderr 长度 < 200 bytes（兜底，避免对真正的脚本错误误触发）。

### MSYS_NO_PATHCONV — 关键陷阱

Git Bash 基于 MSYS2/MinGW，会自动将以 `/` 开头的参数转换为 Windows 路径：

```
bash -c "grep /pattern file"  →  MSYS 把 /pattern 转成 C:\pattern
```

retry 时必须设置 `MSYS_NO_PATHCONV=1` 禁用此行为，否则命令语义被篡改。

### 与 rewrite_git_commit_for_windows 的交互

`terminal.rs` 已有 `rewrite_git_commit_for_windows()` 把 `git commit -m "msg"` 改写为 `git commit -F tempfile`。

**交互逻辑**：rewrite 发生在 cmd 执行之前。若 cmd 执行后 fallback 到 bash，retry 用的是已改写的命令（`-F tempfile`），语义仍然正确——无需特殊处理。但 temp 文件清理必须在 retry 完成后才做，当前清理时机（cmd 执行后）需要调整到整个 invoke 结束。

### 超时策略

```
总超时 = 原始 timeout（默认 120s）
cmd 耗时 = T_cmd
retry 超时 = 总超时 - T_cmd（至少 10s）
```

retry 不额外增加总时长，用户体验上"最多和以前一样慢"。

## 涉及文件

| 文件 | 改动 |
|------|------|
| `peri-middlewares/src/process/mod.rs` | 新增 `pub fn git_bash_path() -> Option<&'static Path>`（OnceLock 缓存 + 检测逻辑） |
| `peri-middlewares/src/middleware/terminal.rs` | `invoke()` 中 cmd 失败后 fallback 逻辑（`#[cfg(windows)]`）；调整 temp 文件清理时机 |
| `peri-middlewares/src/process/process_test.rs` | `git_bash_path()` 检测测试；`is_cmd_not_found()` 模式匹配测试 |

**不改动的文件**：
- `hooks/executor.rs` — 插件脚本不应触发 bash retry
- `mcp/client.rs` — MCP server 启动不应触发 bash retry
- `tui/src/shell_exec.rs` — `!` 命令用 cmd 即可，用户自己知道在 Windows 上

## 边界情况

| 场景 | 行为 |
|------|------|
| `grep foo file` | cmd 失败 → retry bash → 成功 |
| `dir C:\` | cmd 成功 → 无 retry |
| `cargo build` | cmd 成功 → 无 retry |
| `git commit -m "fix"` | rewrite 为 `-F tempfile` → cmd 执行 → 如失败则 retry bash（用 `-F tempfile`） → 清理 temp |
| `grep /pattern file` | cmd 失败 → retry bash（`MSYS_NO_PATHCONV=1`）→ 正确匹配 |
| Git Bash 未安装 | cmd 失败 → 检测到 `None` → 返回原 cmd 错误，行为不变 |
| cmd 成功但有 stderr | 不触发 retry（exit code = 0） |
| 脚本内部错误（非命令缺失）| exit code ≠ 0 但 stderr 不匹配模式 → 不触发 retry |
| 中文 Windows | stderr 匹配 `"不是内部或外部命令"` → 触发 retry |

## 验收标准

- [x] Windows 下 `grep`/`ls`/`find`/`sed`/`awk`/`cat` 等命令通过 Git Bash 自动执行，Agent 无需重试
- [x] cmd 能正常执行的命令不受影响（仍走 cmd）
- [x] Git Bash 未安装时行为与当前完全一致
- [x] 输出中包含 `[Retried with Git Bash]` 标记
- [x] Linux/macOS 构建不受影响（`#[cfg(windows)]`）
- [x] 中文 Windows 下 `grep` 命令也能自动 fallback
- [x] `grep /pattern file` 在 Git Bash retry 中不被 MSYS 路径转换破坏
- [x] `git commit -m "msg"` 在 fallback 场景下 temp 文件被正确清理
- [x] retry 不超过原始 timeout 上限

## 性能影响

- cmd 成功的命令：**零开销**（不触发检测）
- cmd 失败 + Git Bash 未安装：一次 `OnceLock` 读取（已缓存 `None`），**< 1μs**
- cmd 失败 + Git Bash 可用：一次 `bash -c` spawn + 执行，**与直接用 bash 相同**
- 首次检测（`OnceLock::get_or_init`）：最多 4 次文件系统检查 + 1 次 `bash --version`，**约 50-100ms**，仅一次
