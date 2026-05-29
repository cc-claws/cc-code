//! 有状态 SSE（Server-Sent Events）解析器。
//!
//! 配合 reqwest `bytes_stream()` 使用，解析 OpenAI/Anthropic 流式响应。
//!
//! # 用法
//!
//! ```ignore
//! let mut parser = SseParser::new();
//! while let Some(chunk) = stream.next().await {
//!     for (event_type, data) in parser.push(&chunk?) {
//!         // 按协议分发事件
//!     }
//!     if parser.is_done() { break; }
//! }
//! ```

/// 有状态 SSE 解析器
pub struct SseParser {
    /// 跨 chunk 不完整行缓冲区（上一个 chunk 末尾未以 \n 结尾的原始字节）
    pending_bytes: Vec<u8>,
    /// 当前累积的 event type（Anthropic 格式：`event: content_block_delta`）
    event_type: Option<String>,
    /// 当前累积的 data 文本（`data:` 行内容拼接）
    data: String,
    /// [DONE] 或流终止标志
    done: bool,
}

impl SseParser {
    pub fn new() -> Self {
        Self {
            pending_bytes: Vec::new(),
            event_type: None,
            data: String::new(),
            done: false,
        }
    }

    /// Push 新到达的字节块，返回此次推入后解析出的所有完整事件。
    /// 返回空 Vec 表示当前 chunk 内无完整事件（仍在累积中）。
    pub fn push(&mut self, bytes: &[u8]) -> Vec<(Option<String>, String)> {
        let mut events = Vec::new();

        // 字节级拼接：保留原始字节，避免 from_utf8_lossy 截断多字节 UTF-8 序列
        self.pending_bytes.extend_from_slice(bytes);

        // 找最后一个 \n，分离完整行和残留字节
        // SSE 是行协议，行边界一定是 UTF-8 安全的
        let complete_end = self
            .pending_bytes
            .iter()
            .rposition(|&b| b == b'\n')
            .map(|i| i + 1)
            .unwrap_or(0);

        if complete_end == 0 {
            return events; // 无完整行，继续累积
        }

        // 先拆分再解码，避免借用冲突
        let remaining = self.pending_bytes[complete_end..].to_vec();
        self.pending_bytes.truncate(complete_end);
        // into_owned() 断开对 self.pending_bytes 的借用
        let text = String::from_utf8_lossy(&self.pending_bytes).into_owned();
        self.pending_bytes = remaining;

        for mut line in text.lines() {
            // 处理 \r\n: lines() 分割时可能残留 \r 后缀
            if line.ends_with('\r') {
                line = &line[..line.len() - 1];
            }
            // trim 掉 \r 后为空的行
            let trimmed = line.trim_end_matches('\r');

            if trimmed.is_empty() {
                // 事件边界：空行触发 commit
                if !self.data.is_empty() || self.event_type.is_some() {
                    let event = (self.event_type.take(), std::mem::take(&mut self.data));
                    events.push(event);
                }
            } else if let Some(data) = trimmed.strip_prefix("data: ") {
                if data == "[DONE]" {
                    self.done = true;
                    // [DONE] 不产出事件
                    return events;
                }
                self.data.push_str(data);
            } else if let Some(data) = trimmed.strip_prefix("data:") {
                // data: 后无空格
                if data.trim() == "[DONE]" {
                    self.done = true;
                    return events;
                }
                if !data.is_empty() {
                    self.data.push_str(data.trim_start());
                }
                // 空 data:（无内容）跳过
            } else if let Some(et) = trimmed.strip_prefix("event: ") {
                self.event_type = Some(et.to_string());
            } else if let Some(et) = trimmed.strip_prefix("event:") {
                self.event_type = Some(et.trim_start().to_string());
            }
            // 其他行（如 id:, retry:）忽略
        }

        events
    }

    /// 流是否已终止（收到 [DONE]）
    pub fn is_done(&self) -> bool {
        self.done
    }
}

impl Default for SseParser {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
#[path = "sse_test.rs"]
mod tests;
