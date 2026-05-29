# P3: ACP Slash Commands 统一命令系统

## 设计决策（grill-me 已确认）

| 决策点 | 选择 |
|--------|------|
| 命令通道 | ACP Slash Commands（`session/prompt` 发 `/xxx`） |
| 拦截点 | executor 入口，`build_agent()` 之前 |
| 上下文传递 | 新建 `CommandContext` 结构体 |
| 命令发现 | Agent 广播 `available_commands`，TUI 学习 |
| 广播时机 | `session/new` response + `available_commands_update` 增量 |
| 命令分类 | Immediate / Passthrough / Transform(reserved) |
| 废弃策略 | 全部更换，删除旧 RPC，TUI 不直连 |
| P2 `peri/agent_event_done` | 保留（TUI 性能优化） |
| P4 compact 绕过 | 随 P3 解决——compact 走 executor 入口拦截 |

## Phase 1: `peri-acp` 命令基础设施

### 1.1 新建 `peri-acp/src/session/command.rs`

```rust
/// 命令执行方式
enum CommandKind {
    /// 直接执行，不构建 agent（compact、clear）
    Immediate,
    /// 透传给 agent，由 middleware 处理（skill）
    Passthrough,
    /// [reserved] 变换 prompt 后传给 agent
    Transform,
}

/// Agent 侧命令 trait
#[async_trait]
trait AgentCommand: Send + Sync {
    fn name(&self) -> &str;
    fn aliases(&self) -> Vec<&str> { vec![] }
    fn description(&self) -> &str;
    fn kind(&self) -> CommandKind;
    async fn execute(&self, ctx: CommandContext) -> CommandResult;
}

struct CommandContext {
    session_id: String,
    history: Vec<BaseMessage>,
    provider: Arc<dyn BaseModel>,
    peri_config: PeriConfig,
    cwd: String,
    event_sink: Arc<dyn EventSink>,
    args: String,
    // compact_config, thread_store 等按需扩展
}

struct CommandResult {
    messages: Vec<BaseMessage>,
    stop_reason: PromptStopReason,
}

struct CommandRegistry {
    commands: Vec<Box<dyn AgentCommand>>,
}
```

### 1.2 实现内置命令

- `CompactCommand`（Immediate）—— 从 `acp_server/compact.rs` 移植逻辑
- `ClearCommand`（Immediate）—— 清空 history，返回空 messages

### 1.3 修改 `executor.rs` 入口

在 `execute_prompt()` 最前面添加命令拦截：

```rust
pub async fn execute_prompt(...) -> PromptResult {
    // 命令拦截
    if let Some(text) = extract_command_text(&content) {
        if let Some(cmd) = command_registry.find(text) {
            match cmd.kind() {
                CommandKind::Immediate => {
                    let ctx = CommandContext::from_executor_params(...);
                    return cmd.execute(ctx).await.into_prompt_result();
                }
                CommandKind::Passthrough => {
                    // 不拦截，继续正常 agent 流程
                }
                // Transform: reserved
            }
        }
    }

    // 正常 prompt 流程（build_agent + execute）
    ...
}
```

### 1.4 `available_commands` 广播

- `session/new` response 中包含 `availableCommands` 字段
- 后续通过 `available_commands_update` 通知增量更新
- 格式：
  ```json
  {
    "availableCommands": [
      { "name": "compact", "description": "...", "args": {} },
      { "name": "clear", "description": "...", "args": {} }
    ]
  }
  ```

### 1.5 文件变更

| 文件 | 操作 |
|------|------|
| `peri-acp/src/session/command.rs` | 新增 |
| `peri-acp/src/session/mod.rs` | 添加 `mod command` |
| `peri-acp/src/session/executor.rs` | 入口添加命令拦截 |
| `peri-acp/src/session/command/compact.rs` | 新增（从 TUI 移植） |
| `peri-acp/src/session/command/clear.rs` | 新增 |

## Phase 2: TUI CommandRegistry 重构

### 2.1 命令分类

`peri-tui/src/command/mod.rs` 改为：

```rust
/// UI 命令——本地执行
trait UICommand: Send + Sync {
    fn name(&self) -> &str;
    fn aliases(&self) -> Vec<&str>;
    fn description(&self, lc: &LcRegistry) -> String;
    fn execute(&self, app: &mut App, args: &str);
}

/// 统一分发器
struct CommandDispatcher {
    ui_commands: Vec<Box<dyn UICommand>>,      // 本地命令
    agent_commands: HashSet<String>,            // 从 ACP available_commands 学习
}
```

### 2.2 分发逻辑

```
用户输入 "/xxx"
  → 先查 ui_commands（精确/别名/前缀匹配）
    → 命中 → 本地执行
  → 再查 agent_commands
    → 命中 → client.prompt("/xxx")（走 session/prompt）
  → 未命中
    → client.prompt("/xxx")（让 agent 判断，可能当普通文本处理）
```

### 2.3 学习 AgentCommand 列表

- `session/new` response 到达时，提取 `availableCommands`
- 存入 `CommandDispatcher.agent_commands`
- 后续 `available_commands_update` 通知到达时，增量更新

### 2.4 文件变更

| 文件 | 操作 |
|------|------|
| `peri-tui/src/command/mod.rs` | 重构为 UICommand + CommandDispatcher |
| `peri-tui/src/command/session/compact.rs` | 删除（逻辑移入 peri-acp） |
| `peri-tui/src/command/core/clear.rs` | 删除（逻辑移入 peri-acp） |
| 所有现有 Command impl | 改为 impl UICommand（不改逻辑） |

## Phase 3: 清理旧 RPC

### 3.1 删除 TUI server 旧 handler

| 方法 | 文件 | 操作 |
|------|------|------|
| `session/compact` | `acp_server/mod.rs` | 删除 handler |
| `session/clear` | `acp_server/requests.rs` | 删除 handler |
| `session/set_thinking` | `acp_server/requests.rs` | 删除 handler（已被 set_config_option 覆盖） |
| `execute_compact()` | `acp_server/compact.rs` | 删除整个文件 |

### 3.2 删除 ACP client 旧方法

| 方法 | 文件 | 操作 |
|------|------|------|
| `client.compact()` | `acp_client/client.rs` | 删除 |
| `client.clear()` | `acp_client/client.rs` | 删除 |
| `client.set_thinking()` | `acp_client/client.rs` | 删除 |

### 3.3 Stdio 路径

- stdio 无需额外改动——命令天然通过 `session/prompt` 到达
- stdio 的 `session/compact` / `session/clear` handler 从未实现，无需删除

## Phase 4: 更新文档

- `acp-improve.md`：P3/P4 标记为完成，合规表更新
- `CLAUDE.md`：更新命令相关描述
- 知识库同步：将 ACP Slash Commands 规范存入本地

## 依赖关系

```
Phase 1（peri-acp 基础设施）
  → Phase 2（TUI 重构）依赖 Phase 1 的 CommandContext/CommandResult 类型
  → Phase 3（清理）依赖 Phase 2 完成后才能安全删除旧代码
  → Phase 4（文档）最后更新
```

## 验证标准

- [ ] `cargo check` 全 workspace 通过
- [ ] `/compact` 通过 `session/prompt` 发送，executor 拦截执行
- [ ] `/clear` 通过 `session/prompt` 发送，executor 拦截执行
- [ ] `/help`、`/model`、`/exit` 等 UICommand 本地执行，不走 ACP
- [ ] stdio 路径 `session/prompt "/compact"` 正确触发 compact
- [ ] `session/new` response 包含 `availableCommands`
- [ ] 旧 RPC 方法（`session/compact`、`session/clear`、`session/set_thinking`）已删除
- [ ] TUI 不直连 acp_server 的 compact/clear 逻辑
