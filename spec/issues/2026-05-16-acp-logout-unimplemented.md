# ACP 未实现认证方法：`authenticate` + `logout`

**状态**：Open
**优先级**：中
**创建日期**：2026-05-16

## 问题描述

ACP 认证相关方法未实现：

1. **`authenticate`**（稳定，唯一缺失的稳定方法）— Agent 未处理认证请求
2. **`logout`**（unstable）— Agent 未处理登出请求

## 症状详情

### 缺口 1：`authenticate` 未处理

- Client 发送认证请求（指定支持的方法类型），Agent 直接忽略
- ACP 规范中 `authenticate` 用于在初始化后协商认证方式

**ACP 规范中的认证方法**：
- `Agent` — 通过 `/auth` 回调或类似页面的平台认证
- `EnvVar` — 环境变量注入凭据
- `Terminal` — 终端命令获取凭据

**当前状态**：perihelion 通过 `~/.peri/settings.json` 配置 API key，身份验证在配置层面已完成。`authenticate` 可返回"所有方法均已通过"的响应。

### 缺口 2：`logout` 未处理

- Agent 收到 `logout` 请求时直接忽略（未注册 handler）
- 当前无 session 级认证状态，`logout` 可为空操作

## 涉及文件

- `peri-tui/src/acp/dispatch.rs` —— 需添加 `handle_authenticate` + `handle_logout` handler
- `peri-tui/src/acp/main_acp.rs` —— 需注册两个 handler 到 Agent builder
