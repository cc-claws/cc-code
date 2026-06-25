> 归档于 2026-05-24，原路径 spec/issues/2026-05-21-skills-not-passed-as-acp-commands.md

# Skills 未作为 ACP AvailableCommands 传递给 IDE 客户端

**状态**：Fixed
**优先级**：中
**创建日期**：2026-05-21
**修复日期**：2026-05-21

## 问题描述

ACP 协议中的 `AvailableCommandsUpdate` 通知用于告知客户端可用的斜杠命令列表。当前 ACP Stdio 和 TUI ACP 路径均只发送硬编码的静态命令列表，未包含动态发现的 Skills 名称。IDE 客户端无法知晓当前上下文中可用的 Skills。

## 当前行为

### ACP Stdio 路径 (acp_stdio.rs)

- `build_stdio_available_commands()` 返回 21 个静态命令（help/clear/compact/model/mode 等）
- 无 Skills 相关信息
- `session/new` 不冻结任何数据（`FrozenSessionData` 为 `None`）
- Skills 通过 `SkillsMiddleware.before_agent()` 每轮扫描注入 system prompt，MODEL 可见

### TUI ACP 路径 (acp_server/notify.rs)

- `build_available_commands()` 同样返回静态命令列表，无 Skills
- TUI 自己的 `CommandRegistry` 也不动态注册 Skills（Skills 通过 `SkillPreloadMiddleware` 的 `#` 前缀机制激活）

## 数据流对比

```
TUI:   skills 发现 → SkillsMiddleware.before_agent() → prepend system 消息 → MODEL 可见
       skills 发现 → (无) → CommandRegistry/AvailableCommands → 客户端不可见

ACP:   skills 发现 → SkillsMiddleware.before_agent() → prepend system 消息 → MODEL 可见
       skills 发现 → (无) → AvailableCommandsUpdate → IDE 客户端不可见
```

## 影响

IDE 客户端（如 VS Code 插件）无法显示可用的 Skills 列表，用户需要从系统提示词中手动查找 skills 名称。

## 涉及文件

- `peri-tui/src/acp_stdio.rs:79-108` — `build_stdio_available_commands()`，静态命令列表
- `peri-tui/src/acp_server/notify.rs:118-146` — `build_available_commands()`，同静态列表
- `peri-tui/src/acp_stdio.rs:362-363` — `session/prompt` 传 `None` 作为 frozen data，无 frozen skill summary
- `peri-middlewares/src/skills/mod.rs:115-122` — `build_frozen_summary()` 已有 build 能力
- `peri-tui/src/acp_server/requests.rs:92-114` — TUI 侧 `session/new` 构建 frozen data 的标准做法（参考）

## 修复方向

在 `session/new` 时：

1. **ACP Stdio 路径**：构建 `FrozenSessionData`（包含 `skill_summary`），传给 `execute_prompt()`；同时扫描 skills 并动态追加到 `AvailableCommands` 列表
2. **TUI ACP 路径**：扫描 skills 并追加到 `build_available_commands()` 返回值

Skills 命令格式建议：`skill:<name>` 或直接使用 skill name 作为 command name（与 TUI 内 `#skill-name` 用法对应）。

## 修复方案

| 文件 | 变更 |
|------|------|
| `acp_stdio.rs:7-15` | `SessionInfo` 新增 5 个 frozen 字段 |
| `acp_stdio.rs:87-94` | `build_stdio_available_commands()` 新增 `skills` 参数，动态追加 `skill:<name>` 命令 |
| `acp_stdio.rs:292-331` | `session/new` 构建 FrozenSessionData，扫描 skills 并传给 AvailableCommands |
| `acp_stdio.rs:384-410` | `session/prompt` 从 SessionInfo 读取 frozen 数据，构建 `FrozenSessionData` 传入 `execute_prompt()` |
| `acp_server/notify.rs:15` | 新增 `use peri_middlewares::skills::SkillMetadata` |
| `acp_server/notify.rs:78-83` | `send_available_commands_update()` 新增 `skills` 参数 |
| `acp_server/notify.rs:119-156` | `build_available_commands()` 新增 `skills` 参数并动态追加 |
| `acp_server/requests.rs:132-137` | `session/new` 扫描 skills 并传入 `send_available_commands_update()` |
