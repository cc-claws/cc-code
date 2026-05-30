# ACP InitializeResponse 缺少 prompt_capabilities 声明

**状态**：Fixed
**优先级**：中
**创建日期**：2026-05-19
**更新日期**：2026-05-30

## 问题描述

`acp_server.rs:174` 构造 `AgentCapabilities` 时只声明了 `load_session` 和 `session_capabilities`，缺少 `prompt_capabilities` 声明。ACP 规范要求 Agent 在初始化响应中声明其 prompt 能力（是否支持图片、音频、嵌入上下文等），即使全部不支持也应显式声明为空，而非省略。

## 症状详情

当前代码（`acp_server.rs:174`）：

```rust
let caps = AgentCapabilities::new()
    .load_session(true)
    .session_capabilities(
        SessionCapabilities::new()
            .list(SessionListCapabilities::new())
            .close(SessionCloseCapabilities::new())
            .resume(SessionResumeCapabilities::new())
            .fork(SessionForkCapabilities::new()),
    );
```

缺失的声明：

| 能力 | 我们是否支持 | 应声明 |
|------|-------------|--------|
| `prompt_capabilities.image` | ❌ | 显式声明为空或不支持 |
| `prompt_capabilities.audio` | ❌ | 显式声明为空或不支持 |
| `prompt_capabilities.embedded_refs` | ❌ | 显式声明为空或不支持 |

ACP 客户端在初始化阶段根据 `prompt_capabilities` 决定是否展示图片上传、文件附加等 UI 控件。缺少声明时客户端行为不确定——可能默认启用所有功能（导致用户上传后报错），也可能默认禁用（功能不可见）。

## 涉及文件

- `peri-tui/src/acp_server.rs`（第 174 行）— `AgentCapabilities` 构造
