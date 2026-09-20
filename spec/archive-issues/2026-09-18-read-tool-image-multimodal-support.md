# Read 工具缺少多模态图片读取支持，导致多模态模型无法通过工具识别图片

**状态**: Fixed (已归档)
**优先级**: 中  
**创建日期**: 2026-09-18  
**模块**: peri-middlewares / peri-agent  

## 问题描述

在使用 Peri Code 搭配多模态模型（如 Claude 3.5/3.7 Sonnet、GPT-4o 等）时，Agent 调用内置 `Read` 工具读取本地图片文件（如 `.png`, `.jpg` 等）会被直接拒绝，Agent 向用户回复“该文件为二进制文件，无法显示为文本/无法识别图片”，导致多模态视觉能力在工具读取链路中无法发挥作用。

## 症状详情

用户在交互中引导 Agent 查看、分析本地图片时，观察到如下现象：
1. Agent 发起 `Read(file_path: "xxx.png")` 工具调用。
2. 工具返回如下文本：
   ```text
   [BINARY FILE DETECTED]

   File type: .png
   File path: D:\path\to\xxx.png

   This is a binary file and cannot be displayed as text.
   ```
3. Agent 收到该返回后，终止对图片的视觉理解，向用户回复当前无法识别/读取图片。

## 现状与根因分析

### 1. `ReadFileTool` 将图片后缀硬编码为不可显示的二进制文件（直接诱因）
- 文件位置：`peri-middlewares/src/tools/filesystem/read.rs:41-79, 142-150`
- `is_binary_extension` 函数将 `png`、`jpg`、`jpeg`、`gif`、`bmp`、`webp`、`tiff`、`ico` 等全部归入二进制扩展名列表中。
- 当文件扩展名匹配时，函数直接短路并返回 `[BINARY FILE DETECTED]` 纯文本错误提示，未尝试读取图片字节流。

### 2. 工具接口 `BaseTool::invoke` 返回类型受限于纯文本 `String`
- 文件位置：`peri-agent/src/tools/mod.rs:31-34`
- 当前签名：
  ```rust
  async fn invoke(&self, input: serde_json::Value) -> Result<String, Box<dyn std::error::Error + Send + Sync>>;
  ```
- 工具执行结果被直接打包为纯文本类型的 `BaseMessage::tool_result`，底层未提供让工具返回结构化 `ContentBlock::Image` 的通道。

### 3. 底层多模态链路已完备，两端存在断层
- `peri-agent` 的消息模型（`MessageContent::Blocks`、`ContentBlock::Image`）以及适配器（`AnthropicAdapter`、`OpenAiAdapter`）已完整支持多模态图片格式。
- TUI 粘贴图片（`agent_submit.rs`）已能正常发送图片到多模态 LLM。
- 仅有中间的“工具调用读取本地图片”这一通道尚未打通。

## 期望实现方案

### 方案设计

1. **扩展 `BaseTool` 接口（保持向前兼容）**：
   - 在 `BaseTool` 中增加默认实现的 `invoke_content` 方法：
     ```rust
     async fn invoke_content(
         &self,
         input: serde_json::Value,
     ) -> Result<MessageContent, Box<dyn std::error::Error + Send + Sync>> {
         self.invoke(input).await.map(MessageContent::text)
     }
     ```
   - 原有所有返回 `String` 的工具完全无需修改，保持 100% 兼容。

2. **升级 `ToolResult` 与调度链路**：
   - 在 `peri-agent/src/agent/react.rs` 的 `ToolResult` 中增加 `content: Option<MessageContent>` 字段，保留 `output: String` 用于 TUI 事件展示（例如 `[Image: image/png, 1024x768, 56KB]`）。
   - 在 `peri-agent/src/agent/executor/tool_dispatch.rs` 中：
     - 调用 `tool.invoke_content(input)`；
     - 写入 `state` 时，优先使用结构化的 `content` 构造 `BaseMessage::tool_result`。

3. **改造 `ReadFileTool` 支持图片读取**：
   - 从 `is_binary_extension` 中移除 `png`、`jpg`、`jpeg`、`gif`、`webp`、`bmp`。
   - 新增图片识别逻辑：读取图片字节、做大小上限保护（如 10MB/20MB 防 OOM）、计算 Base64 编码。
   - 重写 `invoke_content`：返回包含 `ContentBlock::Image { source: ImageSource::Base64 { media_type, data } }` 的 `MessageContent`。
   - 同步更新 `READ_FILE_DESCRIPTION` 与相关单元测试。

4. **适配器防御性处理**：
   - `AnthropicAdapter` 原生支持 `tool_result` 包含 `image` 块，直接透传。
   - `OpenAiAdapter` 需检查针对 `role: "tool"` 不支持复杂 content 数组的端点做兼容兜底。

## 涉及文件

- `peri-middlewares/src/tools/filesystem/read.rs`：移除二进制扩展名拦截，实现图片 Base64 读取与 `invoke_content`。
- `peri-middlewares/src/tools/filesystem/read_test.rs`：补充图片读取的测试用例。
- `peri-agent/src/tools/mod.rs`：在 `BaseTool` 中引入 `invoke_content`。
- `peri-agent/src/agent/react.rs`：`ToolResult` 增加结构化内容字段。
- `peri-agent/src/agent/executor/tool_dispatch.rs`：调度 `invoke_content` 并将多模态 blocks 写入 state。

## 状态变更记录

| 日期 | 从 | 到 | 操作人 | 说明 |
|------|-----|-----|--------|------|
| 2026-09-18 | — | Open | Claude | 创建 Issue 文档 |

## 修复记录

（待修复验证）
