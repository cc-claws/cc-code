# Git worktree 工作目录

在 Git 仓库中输入 `/worktree`（别名 `/worktrees`）打开目录列表。方向键选择、Enter 确认、Esc 关闭；第一项创建新的独立 checkout，其余项切换到已有目录。面板显示分支、路径、当前目录标记，以及 locked / unavailable 状态。长列表支持滚动。

| 操作 | 命令 |
| --- | --- |
| 列出并选择 | `/worktree` 或 `/worktree list` |
| 从当前 HEAD 创建 detached worktree | `/worktree create` |
| 创建新分支与 worktree | `/worktree create feat/example` |
| 指定起点 | `/worktree create feat/example main` |
| 按分支切换 | `/worktree switch feat/example` |
| 按面板编号切换 | `/worktree switch 2` |
| 按完整路径切换 | `/worktree switch "D:/my projects/example"` |

目录保存在全局配置文件同级的 `worktrees/<uuid>`，默认是 `~/.cc-code/worktrees/<uuid>`。目标目录必须位于现有 checkout 之外。创建默认从当前提交开始，不包含来源目录的未提交内容；已有分支名称会被拒绝，不强制覆盖。没有提交的仓库会报告 Git 错误。

配置了 `core.worktree` 的仓库暂不支持创建，会在分配目录前明确拒绝，避免新 checkout 的 Git 操作指向原目录。查询及创建时会清除影响仓库定位和索引的继承 Git 环境变量。

切换进入所选 worktree 根目录，恢复这个目录最近的可见主会话；没有历史时新建对话。创建总是开启新对话。不会把原会话历史复制到新目录，也不改变进程的全局 cwd。Agent、Shell、项目配置与状态栏沿用现有会话上下文切换流程；状态栏使用实际 cwd，长路径按终端列宽居中省略。

创建和切换要求当前会话空闲，无排队输入、待确认交互、子 agent、前台或后台 Shell。异步请求期间输入暂存；如果期间出现新任务或输入，完成后会保留 checkout 并提示稍后重试。失败时不自动删除目录或分支，错误中包含可恢复路径。该功能不提供删除、清理、stash 或重置操作。

## ACP 扩展

Git 操作在 ACP Server 的阻塞任务中执行，TUI 通过同一传输调用；不要求先创建 Agent session。没有修改标准会话协议字段。

| 方法 | 参数 | 返回 |
| --- | --- | --- |
| `workspace/worktrees` | `{ "cwd": "/absolute/project" }` | `{ "repositoryRoot": "...", "worktrees": [...] }` |
| `workspace/worktree/create` | `{ "cwd": "...", "branch": "feat/example", "baseRef": "main" }` | 单个 worktree 对象；`branch` / `baseRef` 可省略或为 null |

每个 worktree 对象包含 `path`、`head`、`branch`（detached 时为 null）、`locked`、`prunable`、`bare`。路径来自 Git 登记列表，编号仅用于 TUI 当前列表。不可用目录或 bare 仓库不能作为会话切换目标。

错误码：参数错误 `-32602`，Git 操作失败 `-32000`，后台任务或序列化失败 `-32603`。列表优先使用 NUL 分隔 porcelain，旧 Git 不支持该选项时使用带路径解码的兼容格式。
