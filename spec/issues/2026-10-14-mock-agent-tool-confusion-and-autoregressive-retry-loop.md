# 试验/Mock Agent 会话中工具混淆与自回归复读死循环（WebFetch与Bash错配、路径盲猜）

**状态**：Open
**优先级**：中
**创建日期**：2026-10-14
**关联样本**：试验/Mock Agent 会话导出数据（原 `conversation-2026-10-14-170818.md`；**样本文件已清理**）

---

## 一、问题背景与现场

在针对 Agent 的试验/Mock 数据运行与多轮会话日志（试验/Mock Agent 会话导出，全为 Assistant 与 Tool 轮次；**样本文件已清理**）分析中，暴露出 Agent 在特定场景下存在严重的**工具交叉混淆**与**错误参数死循环复读**现象：

### 现场 1：双目标下工具名与参数错配（第 8~29 行）
模型开场第一句话声明：
> *"I'll start by fetching that CSV and looking at the local workspace to understand the context."*

然而紧接着发出的工具调用却将两个动作杂糅错位：
```json
Tool: WebFetch
{
  "command": "pwd && ls -la",
  "description": "Show working directory and contents"
}
```
工具名选了网络抓取的 `WebFetch`，但参数内容却是终端执行的 `Bash` 参数（`command` 与 `description`）。

### 现场 2：自回归死循环复读与“口头纠偏”脱节（第 33~138 行）
系统校验工具参数失败，并明确在 `Tool Result` 中提示：
```text
Invalid arguments for tool WebFetch:
The required parameter 'url' is missing (expected string)
Unexpected parameter 'command' was provided (allowed parameters: ["prompt", "url"])
Unexpected parameter 'description' was provided (allowed parameters: ["prompt", "url"])
💡 Did you mean to use 'Bash'? The parameters ["command"] are characteristic of the Bash tool.
```
但在接下来的 ReAct 循环中：
1. **第 2、3 轮**：模型在无前置思考文本的情况下，**连续两次原样输出完全相同的错误参数**；
2. **第 4 轮**：模型在文本中承诺：`Let me fix those calls.`，但紧随其后的工具调用**依然原样输出了 `WebFetch + command`**；
3. **第 5 轮**：再次原样重发；
4. **第 6 轮**：模型在文本中承认 `I keep mis-emitting those calls. Let me issue properly-formed calls now.`，才终于切到 `Bash`。

整整耗费 **5 轮无效 Token 与工具往返**。

### 现场 3：路径格式盲猜（第 211~293 行）
后续尝试通过 `Read` 工具读取本地文件 `COMPATIBLE.csv` 时：
1. 先传 MSYS 格式绝对路径：`{"file_path": "/d/code/peri/COMPATIBLE.csv"}` → 报错 `File not found`；
2. 随后盲猜错误盘符：`{"file_path": "C:/code/peri/COMPATIBLE.csv"}` → 再次报错 `File not found`；
3. 第三次才修正为相对路径：`{"file_path": "COMPATIBLE.csv"}` 成功。

---

## 二、根因分析

1. **注意力跨目标漂移（Attention Bleed-through）**：
   当规划包含抓取（fetching）与查看本地（looking at workspace）两个子动作时，模型注意力先在动作 A 选定了工具名（`WebFetch`），随后在生成 JSON 参数阶段跳转到了动作 B 的具体命令（`pwd && ls -la`），造成**工具签名与参数内容的分裂**。
2. **上下文样本自强化与缺乏思考缓冲（Lack of CoT Barrier）**：
   错误的工具调用一旦落入对话上下文，在没有 CoT 思考打断的情况下，自回归解码（Next-token prediction）会被历史记录中的近邻 Token 强行牵引，导致模型哪怕收到了 Tool 的纠正提示，依然顺着惯性复读旧参数。
3. **口头承诺与动作推导脱节**：
   模型输出空泛的“让我修复”并不包含对“当前是 WebFetch、参数是 Bash、应当修改哪一个”的具体推导，导致解码器到工具调用层依然输出高频复读项。
4. **运行时缺乏连续同参数错误熔断机制**：
   当前 `cc-agent` 对连续相同工具+相同报错缺乏拦截与硬惩罚机制，完全依赖模型自觉跳出循环。

---

## 三、涉及模块与文件

- `cc-agent/src/agent/action_loop_detector.rs`（或相关循环检测中间件）—— 缺乏针对工具调用连续相同错误参数的硬阻断/强惩罚机制。
- `cc-middlewares/src/tool_search/core_tools.rs` —— 核心工具定义层。
- 系统提示词通用规范 —— 缺少针对“工具报错后必须显式写出 1 行参数比对思考，严禁裸发重试”的负向约束。

---

## 四、解决与治理建议

1. **运行时机制防线（循环熔断）**：
   在 ReAct 循环或 `ActionLoopDetector` 中加入规则：当检测到同一个工具连续 2 次以完全相同的参数触发 `Invalid arguments` 报错时，强制拦截并注入系统级警告阻断，禁止该轮继续向模型提交相同动作。
2. **提示词防御规范落地**：
   在系统提示词的通用规则（如 `CLAUDE.md` / `Working Style`）中固化约束：
   - 收到工具参数确定性报错（`Missing required parameter` / `invalid param`）时，**严禁原样重试**；
   - 必须先在回复中显式核对工具名称与参数定义，不得在报错后直接裸发下一次工具调用。
3. **路径规范强化**：
   针对 Windows 跨平台环境，强调禁止向 `Read`/`Edit` 传递 MSYS 虚拟路径（`/d/...`），统一优先使用相对路径或 Windows 原生盘符路径。

---

## 五、状态变更记录

| 日期 | 从 | 到 | 操作人 | 说明 |
|------|-----|-----|--------|------|
| 2026-10-14 | — | Open | agent | 基于试验/Mock Agent 会话数据（**样本文件已清理**）提取并落库工具混淆与死循环复读分析 |
