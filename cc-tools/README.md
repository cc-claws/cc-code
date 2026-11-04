# cc-tools

无状态核心工具与跨平台进程包装的**叶子 crate**（2026-11 拆分新增）。

## 职责

| 模块 | 内容 |
| :--- | :--- |
| `src/tools/filesystem/` | Read / Write / Edit / Glob / Grep 五个文件工具 |
| `src/process/` | 跨平台 shell 命令构造、输出捕获、Windows 作业对象、RTK 改写预测 |
| `src/bash/` | `BashTool` 与 `InlineShellExecutor`（原 `cc-middlewares/src/middleware/terminal.rs`） |
| `src/tools/output_filter.rs` / `output_persist.rs` / `todo.rs` | 输出过滤、截断落盘、Todo |

## 依赖方向

仅依赖 `cc-agent`（`BaseTool`、`ShellDialect` 等）。**不得依赖** `cc-middlewares` / `cc-acp` / `cc-tui` / `cc-tui-core`。

## 垫片：路径 ≠ 物理位置

`cc-middlewares` 侧以同名 `pub use` 重导出，因此 `crate::tools::*`、`crate::process::*`、
`crate::middleware::terminal::BashTool` 等路径仍有效，但源码已不在 `cc-middlewares/src/`。
改工具实现请到本 crate。

保留在 `cc-middlewares` 的复合工具：`SkillTool`、`AskUserTool`、`SubAgentTool`、
`ToolSearch`、`ArcToolWrapper` / `BoxToolWrapper`（它们依赖 skills / subagent / ask_user 状态，
下沉会造成循环依赖）。

## 维护注意

- 新增纯工具优先放这里；依赖中间件状态的工具必须留在 `cc-middlewares`。
- 单测：`cargo test -p cc-tools -j 1`。
- 拆分依据见 `docs/designs/2026-11-04-rust-crate-architecture-split-plan.md` §4.1 / §9.5。
