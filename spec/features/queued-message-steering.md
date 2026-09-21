# 执行中补充消息与队列管理

Agent 执行期间允许在聊天输入框使用 Alt+V（兼容 Ctrl+V）粘贴图片。按 Enter 后文字和仍有占位符的图片一起排队，多行粘贴内容在入队时展开，后续草稿与队列附件互相独立。

输入框上方每条排队消息提供：

- `立即补充`：选择该条消息加入当前执行；界面显示“插入中…”直到服务端确认。
- `×`：删除该条未发送的消息及其图片，不影响其他消息和草稿。
- 超过三条时使用分页按钮浏览全部消息。

未主动补充的消息仍按原顺序在前一轮结束后发送。已请求补充的消息在确认前保留且禁用重复提交和删除。插入失败时保留原消息，继续正常排队。

“立即补充”表示无需等待整个任务结束；当前模型请求或工具执行完成后，在下一次模型调用前写入用户消息，不取消正在运行的工具，不修改会话冻结的系统提示词。最终回答期间收到补充也会继续下一次模型调用。执行已结束或提前失败时拒绝尚未消费的请求。

## 内部 ACP 扩展

TUI 的 Mpsc ACP 使用 `peri/session/steer`，请求包含 `sessionId` 和 `message.content`（现有 MessageContent，支持文字及图片）。响应 `{ "consumed": true }` 仅在消息写入 Agent state 且发出 StateSnapshot 后返回。此方法不是标准 ACP 接口，当前不在 Stdio/SDK 路径中暴露；这两条路径显式不启用 steering。

队列使用稳定消息 ID，确认返回时只移除对应消息。StateSnapshot 在同一轮内可包含多条 Human 消息，界面重建保留本轮之前的回答与工具结果。

## 验证范围

- Agent mock：工具执行中补充、最终回答期间补充、图片保留、工具结果配对、结束竞态和异常关闭。
- 内存 ACP：图片消息传输、消费确认、执行结束后拒绝、非法参数。
- TUI headless：队列图片隔离、草稿保留、逐条删除与点击分发、插入失败保留、确认前防重复、分页及窄屏、执行中补充后的历史重建。

上述测试不调用真实模型或外部 API。真实终端验收：Agent 运行中截图后 Alt+V，Enter 入队，点击指定消息的“立即补充”或 `×`，核对未选中队列和草稿仍在。

### 本地验证记录（2026-09-21）

- `cargo test -p peri-agent --lib steering -j 1`：5 项通过。
- `cargo test -p peri-acp --lib steering -j 1`：3 项通过。
- TUI 的 `queued`、`message_pipeline`、`event::keyboard` 过滤测试：110 项通过。
- `cargo check -p peri-tui --bins -j 1`：通过。
- `git diff --check`：通过。

Windows 编译曾遇到 rustc 栈/页面文件资源不足，测试命令设置 `RUST_MIN_STACK=16777216`（Agent 测试为 `33554432`）；TUI 测试同时使用 `-j 1 --config 'profile.test.package.peri-tui.debug=0' --config 'profile.test.package.peri-tui.codegen-units=8'`。未修改系统配置或项目构建配置。以上为局部回归，不代表 workspace 全量测试或真实模型/终端手工验收；未覆盖本机已安装的 cc-code 二进制。
