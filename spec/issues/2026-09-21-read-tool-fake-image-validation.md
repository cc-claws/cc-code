# Read 工具读取伪图片文件（如 HTML 错误页）不校验内容，直接 base64 发给模型导致 400

**状态**：Open  
**优先级**：高  
**创建日期**：2026-09-21  
**类型**：缺陷（健壮性防御缺失导致 LLM API 400 崩溃）  
**GitHub Issue**：[cc-claws/cc-code#186](https://github.com/cc-claws/cc-code/issues/186)  

## 问题描述

`Read` 工具在读取本地图片文件（如 `.png`, `.jpg`, `.jpeg`, `.gif`, `.webp`, `.bmp`）时，**仅依据文件扩展名**判定媒体类型，未对文件真实内容或头部魔数（Magic Bytes）进行校验。

当目标文件为“伪图片”（即扩展名是图片后缀，但实际内容为纯文本或 HTML，例如脚本下载远程图片时因 403 Forbidden、404 Not Found 或 502 网关错误而将返回的 HTML/JSON 错误页面写入了 `.png` 文件；或是 Git LFS 纯文本指针文件）时，`Read` 工具仍会将其作为合法图片读取并进行 Base64 编码，标记为 `image/png` 等类型发送给多模态大模型。

Anthropic API / OpenAI API 接收到非法图片 Base64 数据后，在服务端图像解码校验阶段直接报错拒绝，返回 `400 Bad Request`（如 `invalid_request_error: unable to decode image`），导致当前 Agent 对话轮次直接崩溃中断。

## 症状详情

### 复现链路

1. 某自动化脚本或测试下载头像/商品图到本地，由于服务端拦截，保存的文件名为 `avatar.png`，实际内容为 HTML 文本：
   ```html
   <html>
   <head><title>403 Forbidden</title></head>
   <body><center><h1>403 Forbidden</h1></center></body>
   </html>
   ```
2. Agent 在任务中调用 `Read(file_path: "avatar.png")` 试图分析图片。
3. `Read` 工具检查后缀名为 `.png`，命中 `is_image_extension`，读取全部字节并 Base64 编码，构造 `ToolContent::image("image/png", base64_data, ...)` 返回。
4. 模型适配层将结构化 `ContentBlock::Image` 打包进请求发往模型接口（例如 Anthropic `/v1/messages`）。
5. 模型接口响应：
   ```text
   LLM HTTP 错误 (400): API 错误 400 Bad Request: {"type":"error","error":{"type":"invalid_request_error","message":"Could not process image: invalid image format"}}
   ```
6. Agent 执行直接以错误退出，无法继续自主修复。

### 期望表现

当遇到伪图片或损坏文件时，`Read` 工具应具备前置内容嗅探与自适应防御能力：
- 若内容实际上是可读文本（如 HTML 错误页、JSON 报错、Git LFS 指针），应**降级为文本展示**，并附带格式异常警示（如 `[IMAGE CONTENT MISMATCH] File has .png extension but contains HTML text:`），使大模型能够直接看懂“这是一个 403 页面，下载失败了”，进而驱动正确的后续修复行动；
- 若内容是未知损坏二进制，应返回显式工具错误（`Error: Corrupted image file or unsupported image format`），绝不向模型回传虚假 Base64 数据触发下游 400 崩溃。

## 现状与根因分析

代码定位：`peri-middlewares/src/tools/filesystem/read.rs`

### 1. 仅凭扩展名路由到 `read_image`

在 `ReadFileTool::invoke_content` 中：
```rust
if let Some(ext) = resolved.extension().and_then(|e| e.to_str()) {
    let ext_lower = ext.to_lowercase();
    if is_image_extension(&ext_lower) {
        return self.read_image(&resolved, &ext_lower, file_path);
    }
}
```

### 2. `read_image` 缺少文件签名校验

在 `read_image` 中，只检查了文件存在性与 `MAX_IMAGE_SIZE`（20MB）大小保护：
```rust
let bytes = std::fs::read(resolved)?;
let base64_data = base64_encode(&bytes);
let media_type = image_media_type(ext);
...
Ok(ToolContent::image(media_type, base64_data, summary))
```
只要读取成功，便盲目假设文件字节流与 `ext` 对应的 MIME 完全契合，直接编码发送。

## 期望修复方案

### 1. 图片格式魔数（Magic Bytes）校验

在读取字节后，增加文件签名快速嗅探（首部若干字节）：

| 格式 | 扩展名 | 真实魔数特征（Magic Bytes） |
|---|---|---|
| PNG | `.png` | 前 8 字节必须为 `89 50 4E 47 0D 0A 1A 0A` (`\x89PNG\r\n\x1a\n`) |
| JPEG | `.jpg` / `.jpeg` | 前 3 字节必须以 `FF D8 FF` 开头 |
| GIF | `.gif` | 前 6 字节为 `GIF87a` 或 `GIF89a` |
| WebP | `.webp` | 前 4 字节为 `RIFF`，偏移 8~12 字节为 `WEBP` |
| BMP | `.bmp` | 前 2 字节为 `42 4D` (`BM`) |

### 2. 伪图片自适应降级逻辑

若魔数与后缀声明的图片格式不符：
1. **尝试按 UTF-8 文本解析**：
   - 若可成功解析为 UTF-8 文本，且包含文本特征（或检测到 `<html`、`<!DOCTYPE`、`{"` 等常见错误响应）：
     - 优雅降级为 `ToolContent::text(...)` 返回文本内容；
     - 头部附加清晰的提示信息：
       ```text
       [WARNING: Image format mismatch] File 'avatar.png' is not a valid PNG image.
       The content appears to be plain text / HTML (possibly a failed download or error page):

       1  <!DOCTYPE html>
       2  <html><head><title>403 Forbidden</title></head>
       ...
       ```
2. **非文本且非有效图片**：
   - 返回清晰的工具报错文本：
     ```text
     Error: File 'xxx.png' does not match the expected image signature (corrupted or unrecognized format).
     ```
   - 坚决不构造 `ToolContent::image`，杜绝将脏数据塞给模型接口。

## 涉及文件

- `peri-middlewares/src/tools/filesystem/read.rs`：新增图片魔数检查与文本降级防御逻辑。
- `peri-middlewares/src/tools/filesystem/read_test.rs`：新增针对伪图片（HTML 内容写入 `.png`、空文件、截断文件）的测试用例。

## 验收标准

1. 读取真实合规图片（PNG/JPG/GIF/WebP/BMP）时，仍正常返回 Base64 结构化 `ToolContent::image`。
2. 读取内容为 HTML 的 `.png` 文件时，不向模型发送图片 Base64，而是降级以文本形式展示 HTML 内容并附带告警提示。
3. 模型接口不再因此类文件触发 HTTP 400 错误。
4. 相关的单元测试与集成测试全部通过，clippy 零告警。

## 状态变更记录

| 日期 | 从 | 到 | 操作人 | 说明 |
|------|-----|-----|--------|------|
| 2026-09-21 | — | Open | agent | 本地补充完整 Issue 规约文档，并同步至 GitHub Issue #186 |

## 修复记录

（待修复验证）
