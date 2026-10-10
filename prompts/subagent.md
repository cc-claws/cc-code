# SubAgent 手工验证提示词

> 用途：手工验证 `Agent` 工具的前台 / 后台 / fork 三条执行路径，以及后台 Agent 的
> 消息回调、树状聚合与并发上限行为。
> 最后校对：2026-10-10（与 `cc-middlewares/src/subagent/tool/define.rs` 对齐）

---

## 一、真实工具契约（先读这里）

以下字段来自 `Agent` 工具的真实参数定义（`tool/define.rs`），**提示词里请让主 Agent 使用这些字段**，
不要再使用历史文档里的 `subagent` / `bg` / `bg fork subagent` 等简写——那些不是工具接受的参数。

| 字段 | 类型 | 必填 | 真实语义 |
|------|------|------|----------|
| `prompt` | string | ✅ | 任务目标、上下文、约束、期望产出 |
| `subagent_type` | string | ⚠️ | Agent ID；`fork: true` 时可省略 |
| `description` | string | ❌ | schema 中声明为 UI 展示用，但当前实现接收后未使用（`_description`） |
| `name` | string | ❌ | schema 中声明为 UI 别名，但当前实现接收后未使用（`_name`） |
| `run_in_background` | boolean | ❌ | `true` 时主 Agent 立即继续，完成后收到通知；**最多 3 个并发** |
| `fork` | boolean | ❌ | `true` 时继承父会话快照，而非加载 Agent 定义；此模式下省略 `subagent_type` |
| `cwd` | string | ❌ | 子 Agent 工作目录，缺省继承父 cwd |
| `isolation` | string | ❌ | 保留字段，当前**不**提供文件系统 / worktree 隔离 |

> 后台任务在 UI 上显示的任务预览取自 `prompt` 的前 100 字符
> （`execute_bg.rs` 中 `prompt.chars().take(100)`），**不是** `description`。

### 可用的 `subagent_type` 取值

- **内置 Agent**（`cc-middlewares/src/subagent/built-in/`）：
  `explore`、`plan`、`general-purpose`、`verification`
- **项目级 Agent**（`.claude/agents/`，优先级高于内置同名 Agent）：
  本仓当前为 `hello-agent`、`code-reviewer`、`web-researcher`

> 若 `subagent_type` 既不在项目目录、也不是内置 ID，工具会直接报错
> （`cannot find agent definition ...`），这正是历史提示词 `subagent` 会失败的原因。

### 并发上限的真实来源

后台并发上限为 **3**，硬编码于 `subagent/background.rs`（`max_concurrent: 3`）与
`subagent/tool/execute_bg.rs`（`active_count() >= 3` 即拒绝）。第 4 个 `run_in_background: true`
调用会返回 `maximum 3 concurrent background tasks reached`。

---

## 二、前台同步执行

> 验证：前台子 Agent 独立消息状态、结果回传、树状聚合展示。

请使用 Agent 工具，`subagent_type: "hello-agent"`，`prompt: "say hello"`，同步执行（不要后台）。

> 验证：前台并发多路（树状聚合）

请在一次响应里并发派出三个同步（非后台）`hello-agent`，各自 `prompt: "say hello"`，观察三条子 Agent 分组是否正确并列聚合。

---

## 三、后台执行（`run_in_background: true`）

> 验证：后台 Agent 启动卡片出现，主 Agent 立即返回，完成后消息回调触发会话

请使用 Agent 工具，`subagent_type: "hello-agent"`，`prompt: "先 sleep 10 秒，然后 say hello"`，`run_in_background: true`。

> 验证：后台并发回调（3 路）

请在一次响应里并发派出三个后台 Agent，均为 `subagent_type: "hello-agent"`、`run_in_background: true`、`prompt: "say hello"`，观察三个完成通知是否都送回会话。

> 验证：后台并发上限（第 4 个应被拒绝）

请连续派出四个后台 Agent（`run_in_background: true`，`subagent_type: "hello-agent"`），确认第 4 个返回并发超限错误。

> 验证：后台 Agent 的多步工具能力

派出一个后台 Agent（`subagent_type: "general-purpose"`，`run_in_background: true`），prompt 为：`说一句 1，然后调用两次 read，再说 2，两次 read，重复直到 4`。

---

## 四、Fork 执行（`fork: true`）

> 验证：fork 继承父会话快照，且 fork 模式下无需 `subagent_type`

请使用 Agent 工具，设置 `fork: true`（不要传 `subagent_type`），`prompt: "say hello"`，同步执行。

> 验证：后台 fork 的消息回调触发会话

请使用 Agent 工具，`fork: true`，`run_in_background: true`，`prompt: "先 sleep 10 秒，然后 say hello"`。

> 验证：后台 fork 长时间运行时的分 Agent 观察（Ctrl+B 聚焦后台栏）

请使用 Agent 工具，`fork: true`，`run_in_background: true`，`prompt: "先 sleep 60 秒，然后 say hello"`。运行期间按 `Ctrl+B` 聚焦后台 Agent 栏（最多展示 4 个），确认可查看该 fork Agent 的实时状态。

---

## 五、人工检查清单

- [ ] 前台同步 Agent 结果正常回传，多路并发能树状聚合
- [ ] 后台 Agent 启动后主 Agent 立即继续，完成通知能触发新会话轮次
- [ ] 后台并发 3 路完成通知全部送达，第 4 路被明确拒绝
- [ ] fork 模式省略 `subagent_type` 也能执行，且继承父会话上下文
- [ ] 后台栏（`Ctrl+B`）能列出运行中的后台 Agent 并切换查看
