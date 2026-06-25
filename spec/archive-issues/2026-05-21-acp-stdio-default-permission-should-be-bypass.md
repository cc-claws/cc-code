> 归档于 2026-05-24，原路径 spec/issues/2026-05-21-acp-stdio-default-permission-should-be-bypass.md

# ACP 命令默认权限模式应为 Bypass

**状态**：Fixed
**优先级**：中
**创建日期**：2026-05-21
**修复日期**：2026-05-21

## 问题描述

ACP stdio 模式的默认权限模式硬编码为 `AutoMode`，与 TUI 模式和 `-p` 模式的默认行为不一致。用户期望 ACP 命令也默认使用 `Bypass`。

## 当前行为对比

| 模式 | 默认权限 | 代码位置 |
|------|---------|----------|
| TUI | `Bypass` | `main.rs:467` — YOLO_MODE 默认 true |
| `-p` 模式 | `Bypass` | `cli_print.rs:109` — 直接硬编码 fallback |
| ACP stdio | `AutoMode` | `acp_stdio.rs:184-185` — 硬编码 |

## 症状

ACP stdio 客户端连接后，默认处于 `AutoMode`，与 TUI 的默认 `Bypass` 行为不一致。`AutoMode` 会对部分工具操作进行审批拦截，用户如果不显式切换模式，会感受到行为差异。

## 根因

`acp_stdio.rs:184-185` 直接硬编码：

```rust
let permission_mode = peri_middlewares::prelude::SharedPermissionMode::new(
    peri_middlewares::prelude::PermissionMode::AutoMode,
);
```

未考虑 TUI 默认行为的一致性。

## 涉及文件

- `peri-tui/src/acp_stdio.rs:184-185` — 硬编码默认值
- `peri-tui/src/main.rs:441-467` — TUI 默认逻辑（参考）
- `peri-tui/src/cli_print.rs:96-109` — `-p` 模式默认逻辑（参考）

## 修复方向

将 `acp_stdio.rs:184` 的默认值改为 `PermissionMode::Bypass`，与其他模式保持一致。
