# Self-Update 简化为 curl | bash 实施计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 将 `self_update.rs`（287 行）简化为 `update.rs`（<50 行），直接 `curl 远程 install.sh | bash` 执行更新。

**Architecture:** 删除 Rust 侧的 GitHub API 调用、下载、校验、解压逻辑，改为启动 `bash -c "curl -fsSL <url> | bash"` 子进程并流式转发 stdout/stderr。平台差异、代理、版本选择等全部由远程 `scripts/install.sh` 处理。

**Tech Stack:** `std::process::Command`、`tokio::io::AsyncBufReadExt`（流式输出）

---

### Task 1: 重写 `self_update.rs` 为 `update.rs`

**Files:**
- Create: `peri-tui/src/update.rs`（替代 `self_update.rs`）
- Delete: `peri-tui/src/self_update.rs`

- [ ] **Step 1: 创建 `peri-tui/src/update.rs`**

```rust
//! Update mechanism: curl remote install.sh | bash.
//!
//! Delegates all update logic (download, checksum, extract, symlink)
//! to the remote install script.

use anyhow::{Context, Result};
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;

const SCRIPT_URL: &str =
    "https://raw.githubusercontent.com/konghayao/peri/main/scripts/install.sh";

/// Run the update flow. Returns Ok(new_tag) on success.
///
/// Streams the remote install script's stdout/stderr to the terminal.
pub async fn run_update() -> Result<String> {
    println!("Peri update");
    println!("  Running remote install script...");

    let mut child = Command::new("bash")
        .arg("-c")
        .arg(format!("curl -fsSL {SCRIPT_URL} | bash"))
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .context("Failed to spawn update process. Is bash/curl available?")?;

    // 流式输出 stdout
    if let Some(stdout) = child.stdout.take() {
        let reader = BufReader::new(stdout);
        let mut lines = reader.lines();
        while let Some(line) = lines.next_line().await? {
            println!("{line}");
        }
    }

    // 流式输出 stderr
    if let Some(stderr) = child.stderr.take() {
        let reader = BufReader::new(stderr);
        let mut lines = reader.lines();
        while let Some(line) = lines.next_line().await? {
            eprintln!("{line}");
        }
    }

    let status = child.wait().await?;
    if !status.success() {
        anyhow::bail!("Update script exited with status {}", status);
    }

    // 从 install_dir 读取安装后的版本号
    let version_file = dirs_next::home_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join(".peri")
        .join("current-version.txt");
    let tag = std::fs::read_to_string(&version_file)
        .ok()
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|| "unknown".to_string());

    Ok(tag)
}
```

- [ ] **Step 2: 删除旧文件**

```bash
rm peri-tui/src/self_update.rs
```

- [ ] **Step 3: 更新 `peri-tui/src/lib.rs` 模块声明**

将 `pub mod self_update;` 改为 `pub mod update;`：

```rust
// peri-tui/src/lib.rs 第 22 行
pub mod update;
```

### Task 2: 更新 `main.rs` 中的 CLI 和调用点

**Files:**
- Modify: `peri-tui/src/main.rs:36-51`（CLI 枚举）
- Modify: `peri-tui/src/main.rs:111-126`（调用点）

- [ ] **Step 1: 修改 CLI 枚举变体名**

将 `SelfUpdate` 改为 `Update`，更新 about 描述：

```rust
// peri-tui/src/main.rs 第 36-51 行
#[derive(Subcommand)]
enum Commands {
    /// 以 ACP Agent 模式运行（stdin/stdout JSON-RPC）
    Acp {
        /// 工作目录
        #[arg(long, default_value = ".")]
        cwd: String,
        /// 模型名称/别名
        #[arg(long)]
        model: Option<String>,
        /// Agent 类型（从 .claude/agents/ 中选择，如 code-reviewer、explorer）
        #[arg(short = 'g', long)]
        agent: Option<String>,
    },
    /// 更新：从 GitHub 下载并安装最新版本
    Update,
}
```

- [ ] **Step 2: 修改调用点**

```rust
// peri-tui/src/main.rs 第 111-126 行
Some(Commands::Update) => {
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;
    rt.block_on(async {
        match peri_tui::update::run_update().await {
            Ok(tag) => println!("Updated to {tag}"),
            Err(e) => {
                eprintln!("Update failed: {e:#}");
                std::process::exit(1);
            }
        }
        Ok(())
    })
}
```

### Task 3: 编译验证

- [ ] **Step 1: 编译 peri-tui**

```bash
cargo build -p peri-tui
```

Expected: 编译成功，无 warning。

- [ ] **Step 2: 运行 clippy**

```bash
cargo clippy -p peri-tui -- -D warnings
```

Expected: 无 warning。

- [ ] **Step 3: 运行测试**

```bash
cargo test -p peri-tui
```

Expected: 全部通过。

- [ ] **Step 4: 验证 CLI help 输出**

```bash
cargo run -p peri-tui -- --help
```

Expected: 子命令显示 `update` 而非 `self-update`。

### Task 4: 提交

- [ ] **Step 1: 提交更改**

```bash
git add peri-tui/src/update.rs peri-tui/src/lib.rs peri-tui/src/main.rs
git rm peri-tui/src/self_update.rs
git commit -m "refactor: simplify self-update to curl | bash via update.rs"
```
