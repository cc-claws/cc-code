# [BUG] Windows git commit -m 被 rewrite 后 cmd /C 引号嵌套导致失败

**状态**: Open
**优先级**: P1
**模块**: middlewares/terminal
**创建时间**: 2026-06-25
**发现方式**: 开发过程

## 现象

通过 agent 执行 `git commit -m "msg"` 时，报错：

```
fatal: could not read log file '"C:/Users/wismy/AppData/Local/Temp/peri-commit-msg-xxx.txt"': No such file or directory
```

注意路径被包裹了**多余引号**：`'"C:/...txt"'`（外层多了一对引号）。

## 根因

`peri-middlewares/src/middleware/terminal.rs` 的 `rewrite_git_commit_for_windows` 将：

```
git commit -m "feat: hello"
```

重写为：

```
git commit -F "C:/Users/.../peri-commit-msg-xxx.txt"
```

然后通过 `shell_command` 传递给 `cmd /C`：

```
cmd /C git commit -F "C:/Users/.../peri-commit-msg-xxx.txt"
```

`cmd /C` 对嵌套引号的处理与 bash 不同，会将 `-F` 后的引号路径解析为带字面引号的文件名，导致 git 读取 `'"C:/...txt"'`（含引号）而非 `C:/...txt`。

### 问题链路

```
用户: git commit -m "msg"
  → rewrite_git_commit_for_windows()
    → git commit -F "tempfile"
      → shell_command() → cmd /C git commit -F "tempfile"
        → cmd.exe 解析: -F 的参数为 "tempfile"（含字面引号）
          → git 尝试读取 '"tempfile"' → 文件不存在
```

### 对比：正常路径

手动写文件 + 直接调用（不经过 rewrite）：

```bash
echo msg > tempfile && git commit -F tempfile
```

绕过 rewrite（用 `&&` 触发 chained 命令跳过）可以正常工作。

## 影响

- **所有** Windows 上通过 agent 执行的 `git commit -m` 命令都会失败
- 开发者无法通过 agent 提交代码
- 必须手动绕过（写文件 + `-F`，或用 `&&` 跳过 rewrite）

## 修复方向

### 方案 1：修复 shell_command 引号处理（推荐）

在 `shell_command` 中对 Windows `cmd /C` 的引号做转义：

```rust
// cmd /C 需要双引号包裹整个命令，内部引号用 ^ 转义
let escaped = command.replace('"', "^\"");
cmd.arg("/C").arg(format!("{escaped}"));
```

### 方案 2：rewrite 时不加引号

`rewrite_git_commit_for_windows` 中 `-F` 路径不加引号（路径无空格时安全）：

```rust
let mut new_cmd = format!("{commit_prefix} -F {temp_path_str}");
```

### 方案 3：完全移除 rewrite，改用 PowerShell

Windows 10+ 默认有 PowerShell，可直接处理 `-m` 引号：

```rust
cmd.arg("-Command").arg(command);
```

## 相关文件

- `peri-middlewares/src/middleware/terminal.rs:17-94` — `rewrite_git_commit_for_windows`
- `peri-middlewares/src/process/mod.rs:14-36` — `shell_command` Windows 分支

## 验证标准

Windows 上通过 agent 执行 `git commit -m "test message"` 能正常提交，不再报 `could not read log file` 错误。
