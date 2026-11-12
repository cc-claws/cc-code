# Worktree 详情

按需读取，正常流程不必看。

## EnterWorktree 行为

- 目录落在仓库内托管目录 `.claude/worktrees/<name>`（自动写入 `.git/info/exclude`，不污染 `git status`）；
  不在 git 仓库内时回退到 `~/.cc-code/worktrees`。
- 新建分支 `worktree/<name>`（不是 detached HEAD）。
- 同名复用已有 checkout，消息首行为 `Reusing worktree on branch ...`，不是报错。
- 切换**对下一轮生效**：同一轮内已构建的工具集仍用旧目录。
- 已在托管目录内再调用会报错 —— 先 `ExitWorktree`。
- `name` 省略时生成 `wt-<8 位随机>`；每段只允许字母数字 `.` `_` `-`，总长 ≤64。
- **子 Agent 拿不到**：中间件只挂主 Agent 链，避免子 Agent 把父会话目录一起切走。

## ExitWorktree 行为

- `keep` 保留 checkout 与分支；`remove` 删除两者。
- `remove` 遇未提交文件或新提交会拒绝 → 与用户确认后带 `discard_changes: true`。
- 变更统计失败时**一律拒绝删除**（fail-closed）；由 hook 创建的 checkout 无基线 commit，也不自动删。
- 不调用就退出：提示 `Worktree session still open: <path>`，默认保留。
- 会话登记在 `~/.cc-code/worktree_sessions.json`，进程重启后仍能识别。

## 启动时隔离（可选）

宿主 CLI 若提供 `--worktree [name]` 参数，可在进程启动时创建（或复用）隔离 checkout；不传名称则用随机名。

## git 回退（仅在原生工具不可用时）

纯 shell 环境、非本框架会话时：

```bash
ls -d .claude/worktrees 2>/dev/null     # 已有托管目录优先
git check-ignore -q .claude/worktrees   # 必须确认被忽略，否则先加 .gitignore 再建
git worktree add .claude/worktrees/<name> -b <name>
```

- 分支命名遵循项目既有规范：纯 ASCII kebab-case，**避免 `#`（某些平台上会让 PR CI 静默失效）、避免中文**。
- 回退路径建的 worktree 不受 `ExitWorktree` 管理，收工自己 `git worktree remove`。

## 隔离区里的构建与清理

- **磁盘**：新 worktree 首次构建会重新生成产物目录、可能重复拉取/编译依赖，占用可观空间；全量构建/测试前先看剩余空间。
- **磁盘将满时 `git worktree add` 可能「成功返回但目录没建出来」**：`worktree list` 有条目、`cd` 却报 No such file
  or directory（判据：`cd` 失败但 `git -C <path> status` 正常）→ 先查空间，别怀疑路径写法。
- **别指望跨 worktree 共享产物目录省空间**：构建系统按源码路径索引，共享后仍全量重建，只能避免「多份」；
  也别写进被版本控制共享的配置文件。
- 清理时**只删自己建的 worktree**，别动其他会话在途的目录。
