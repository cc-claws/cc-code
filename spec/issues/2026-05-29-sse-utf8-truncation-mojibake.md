# SSE 流式解析跨 chunk UTF-8 截断产生乱码（U+FFFD）

**状态**：Open
**优先级**：高
**创建日期**：2026-05-29

## 问题描述

SSE 流式解析器 `SseParser` 在处理 LLM 返回的中文��多字节 UTF-8 文本时，当网络 chunk 边界恰好切割在某个字符的字节序列中间（如中文"描述"的 UTF-8 编码 3 字节被切成 2+1），`from_utf8_lossy` 将不完整字节序列替换为 `�`（U+FFFD Replacement Character）。用户看到的是"问题�述"而非"问题描述"。

## 症状详情

| 现象 | 说明 |
|------|------|
| 乱码字符 | `�`（U+FFFD）出现在中文等多字节文本中 |
| 触发条件 | 流式 chunk 边界恰好切在 UTF-8 多字节序列中间 |
| 复现频率 | 偶发，取决于网络分包时机 |
| 影响 | OpenAI 兼容和 Anthropic 两条流式路径均受影响（共用同一个 `SseParser`） |

## 根因

`SseParser.push()` 将 `pending_line` 存为 `String`，新 chunk 通过 `String::from_utf8_lossy(bytes)` 转为字符串后拼接。`from_utf8_lossy` 遇到尾部不完整 UTF-8 序列时**不可逆地**替换为 U+FFFD，��续 chunk 到达后无法恢复原始字节。

关键代码（`peri-agent/src/llm/sse.rs:44-46`）：

```rust
let mut text = std::mem::take(&mut self.pending_line);
text.push_str(&String::from_utf8_lossy(bytes));
```

问题在于：SSE 协议是行协议（以 `\n` 分隔），而行边界是 UTF-8 安全的（JSON 文本不会在多字节字符中间换行）。因此只需要在**行边界**处做 UTF-8 解码，跨 chunk 拼接应保留原始字节。

## 修复方案

将 `pending_line: String` 改为 `pending_bytes: Vec<u8>`，在 `push()` 中：

1. **字节级拼接**：`pending_bytes.extend_from_slice(bytes)`
2. **字节级行分割**：在 `pending_bytes` 中找最后一个 `b'\n'`，分离完整部分和残留部分
3. **整体验码**：仅对完整部分（到最后一个 `\n`）做一次 `String::from_utf8`（SSE 数据一定是合法 UTF-8）
4. **保留残留字节**：不完整部分（最后一个 `\n` 之后）以 `Vec<u8>` 形式保存，等下一个 chunk 拼接

```rust
pub struct SseParser {
    pending_bytes: Vec<u8>,   // String → Vec<u8>
    event_type: Option<String>,
    data: String,
    done: bool,
}

pub fn push(&mut self, bytes: &[u8]) -> Vec<(Option<String>, String)> {
    let mut events = Vec::new();

    self.pending_bytes.extend_from_slice(bytes);

    // 找最后一个 \n，分离完整行和残留字节
    let complete_end = self.pending_bytes
        .iter()
        .rposition(|&b| b == b'\n')
        .map(|i| i + 1)
        .unwrap_or(0);

    if complete_end == 0 {
        return events; // 无完整行，继续累积
    }

    // 仅对完整部分做 UTF-8 解码
    let text = String::from_utf8_lossy(&self.pending_bytes[..complete_end]);
    self.pending_bytes = self.pending_bytes[complete_end..].to_vec();

    // 后续按行解析逻辑不变...
}
```

### 方案优势

- **最小改动**：仅修改 `SseParser` 内部存储和 `push()` 前 10 行，后续行解析逻辑完全不变
- **零拷贝友好**：`extend_from_slice` 比字符串拼接更高效
- **向后兼容**：`push()` 签名不变（`&[u8]`），所有调用方无需修改
- **测试友好**：现有测试全部通过（SSE 行协议语义不变），新增 UTF-8 截断测试即可覆盖

## 涉及文件

- `peri-agent/src/llm/sse.rs` —— `SseParser` 结构体和 `push()` 方法（**需修改**）
- `peri-agent/src/llm/sse_test.rs` —— 测试文件（需新增跨 chunk UTF-8 截断测试）
- `peri-agent/src/llm/openai/stream.rs` —— OpenAI 流式调用方（无需修改，`push()` 签名不变）
- `peri-agent/src/llm/anthropic/stream.rs` —— Anthropic 流式调用方（无需修改）

## 预期测试用例

```rust
#[test]
fn test_cross_chunk_utf8_cjk() {
    // "描述" 的 UTF-8: E6 8F 8F E8 BF B0
    // 模拟 chunk 在 E6 8F 8F | E8 BF B0 处切割
    let mut parser = SseParser::new();
    let events1 = parser.push(b"data: \xe6\x8f\x8f");      // "描" 完整，无 \n → 无事件
    assert!(events1.is_empty());
    let events2 = parser.push(b"\xe8\xbf\xb0\n\n");        // "述" + 换行
    assert_eq!(events2.len(), 1);
    assert_eq!(events2[0].1, "描述");
}

#[test]
fn test_cross_chunk_utf8_mid_character() {
    // "描述" 切割在 "描" 的最后一个字节之前: E6 8F | 8F E8 BF B0
    let mut parser = SseParser::new();
    let events1 = parser.push(b"data: \xe6\x8f");
    assert!(events1.is_empty());
    let events2 = parser.push(b"\x8f\xe8\xbf\xb0\n\n");
    assert_eq!(events2.len(), 1);
    assert_eq!(events2[0].1, "描述");  // 不应出现 U+FFFD
}
```
