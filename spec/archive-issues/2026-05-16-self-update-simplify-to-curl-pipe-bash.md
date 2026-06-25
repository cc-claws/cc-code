> 归档于 2026-05-16，原路径 spec/issues/2026-05-16-self-update-simplify-to-curl-pipe-bash.md

# update.rs 应简化为 curl 远程脚本 | bash

**状态**：Fixed
**优先级**：中
**创建日期**：2026-05-16

## 问题描述

`self_update.rs` 在 Rust 中重新实现了一套完整的更新流程（GitHub API 调用、下载 tarball、SHA256 校验、解压、符号链接管理），共 287 行。而 `scripts/install.sh` 已经用 shell 完成了同样的工作。两份代码逻辑重复，Rust 侧的实现对平台差异的处理（sha256sum vs shasum、curl fallback 到 reqwest 等）增加了维护负担。

期望：`self_update.rs` 简化为直接 `curl 远程 install.sh | bash`，把进程的 stdout/stderr 流式输出给用户。

## 现状

- `peri-tui/src/self_update.rs`（287 行）：完整实现了平台检测、GitHub Releases API 查询、下载、校验、解压、符号链接创建、版本文件写入
- `scripts/install.sh`（235 行）：功能等价的 shell 脚本，支持环境变量控制（`PERI_INSTALL_VERSION`、`GITHUB_PROXY`、`GITHUB_TOKEN` 等）
- 两份代码存在逻辑重复，修改安装流程时需要同步两处

## 期望改进方向

`update.rs`（从 `self_update.rs` 改名）简化为：

1. 拼接远程脚本 URL（`https://raw.githubusercontent.com/konghayao/peri/main/scripts/install.sh`）
2. 用 `Command` 启动 `bash -c "curl -fsSL <url> | bash"` 子进程
3. 流式输出子进程的 stdout/stderr
4. 删除所有 Rust 侧的下载、校验、解压、API 调用逻辑

## 涉及文件

- `peri-tui/src/self_update.rs`（287 行）—— 改名为 `update.rs`，删除冗余逻辑后预计 <50 行
- `scripts/install.sh`（235 行）—— 不修改，作为 self-update 的实际执行者
