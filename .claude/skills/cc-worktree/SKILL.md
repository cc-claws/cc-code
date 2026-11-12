---
name: cc-worktree
description: Use when asked to work in a worktree, isolate a workspace, or before editing code in a checkout shared by concurrent sessions.
---

# cc-worktree

在需要隔离的改动前，用宿主原生的 worktree 工具，**不要手敲 `git worktree add`**。

**最高频的坑**：`EnterWorktree` / `ExitWorktree` 是 **deferred tool**，不在默认可见的核心工具列表里。
必须先 `SearchExtraTools` 发现、再 `ExecuteExtraTool` 调用。**看不见 ≠ 不存在。**

## 最小工作流

1. 检测 —— 已在隔离区就别再建：
   ```bash
   git status --short && git worktree list
   echo "GIT_DIR=$(git rev-parse --git-dir)"; echo "GIT_COMMON=$(git rev-parse --git-common-dir)"
   ```
   `GIT_DIR != GIT_COMMON` → 已是 linked worktree，直接开工（先 `git rev-parse --show-superproject-working-tree` 排除子模块）。
2. 有别人的未提交改动 / 多方在用 → **必须隔离**；禁止在主工作区 `switch`/建分支/stash。
3. 建：
   ```
   SearchExtraTools("select:EnterWorktree,ExitWorktree")
   ExecuteExtraTool(tool_name="EnterWorktree", params={"name": "fix-x"})
   ```
   切换**对下一轮生效**；同名复用（`Reusing worktree on branch ...`）。
4. 收工：
   ```
   ExecuteExtraTool(tool_name="ExitWorktree", params={"action": "keep"})   # 或 "remove"
   ```
   `remove` 遇未提交/新提交会拒绝 → 与用户确认后加 `discard_changes: true`。

## Why not `git worktree add`

原生工具还会登记会话、切换会话目录、触发 WorktreeCreate/Remove hook；手敲 git 绕开这些，产生宿主看不见、也清理不掉的幽灵状态。

## 常见错误

| 错误 | 正确做法 |
| :--- | :--- |
| 直接 `Bash` 敲 `git worktree add` | 先用原生 `EnterWorktree` |
| 工具列表没有就认为「没有此工具」 | deferred tool，`SearchExtraTools` 能发现 |
| 在已有 worktree 里再建一层 | Step 1 检测后直接开工 |
| 在共享主工作区 `switch`/`stash` | 一律在隔离区操作 |

## 详情

- 工具行为、CLI 启动入口、git 回退路径、磁盘与清理 → `references/worktree-notes.md`
