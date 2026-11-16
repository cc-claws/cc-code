//! 会话标题提炼的请求构造与响应取舍测试。
//!
//! 锁死一类回归：标题请求一旦压了 `max_tokens`，推理模型的思考会吃光预算，
//! `content` 只剩思考残片（「我们需要回答用户。用户要求是…」）并被当成标题落库。

use super::*;

// ─── 请求构造 ───────────────────────────────────────────────────────────────

/// 标题请求 **不得** 设置 `max_tokens`：留给 provider 上限，避免思考吃光预算。
#[test]
fn test_build_title_request_leaves_max_tokens_unset() {
    // Arrange / Act
    let request = build_title_request("看看我的勾股需求列表");

    // Assert
    assert_eq!(
        request.max_tokens, None,
        "标题请求不得自带 max_tokens（思考会吃光预算，正文退化成思考残片）"
    );
}

/// 标题请求携带固定 system 指令，且输入被包进「用户任务输入」。
#[test]
fn test_build_title_request_carries_system_instruction_and_input() {
    // Arrange / Act
    let request = build_title_request("看看我的勾股需求列表");

    // Assert
    let system = request
        .system
        .as_deref()
        .expect("标题请求必须带 system 指令");
    assert!(
        system.contains("会话主题生成工具"),
        "system 指令应说明标题生成角色: {system}"
    );
    assert_eq!(request.messages.len(), 1, "标题请求应只有一条用户消息");
    let content = request.messages[0].content();
    assert!(
        content.contains("看看我的勾股需求列表"),
        "用户消息应包含原始输入: {content}"
    );
}

// ─── 响应取舍 ───────────────────────────────────────────────────────────────

/// 命中输出上限的回复必须丢弃：此时 `content` 是思考残片，不是标题。
#[test]
fn test_extract_title_from_response_discards_max_tokens_truncation() {
    // Arrange：模拟被截断的真实响应（思考残片）
    let truncated =
        "我们需要回答用户。用户任务输入：我们需要回答用户。用户要求是作为会话主模型生成工具";

    // Act
    let title = extract_title_from_response(&StopReason::MaxTokens, truncated);

    // Assert
    assert_eq!(
        title, None,
        "被 max_tokens 截断的回复是思考残片，必须回退本地保底标题"
    );
}

/// 正常结束的回复按原样清洗后作为标题。
#[test]
fn test_extract_title_from_response_accepts_normal_end_turn() {
    // Arrange / Act
    let title = extract_title_from_response(&StopReason::EndTurn, "会话主题生成");

    // Assert
    assert_eq!(title.as_deref(), Some("会话主题生成"));
}

/// 模型输出的包裹符号与换行被剥离，空白被折叠。
#[test]
fn test_extract_title_from_response_strips_wrappers_and_collapses_whitespace() {
    // Arrange：模型常见的包裹形态
    let raw = "  “终端标题修复。”\n";

    // Act
    let title = extract_title_from_response(&StopReason::EndTurn, raw);

    // Assert
    assert_eq!(title.as_deref(), Some("终端标题修复"));
}

/// 空白回复返回 None，不产生空标题。
#[test]
fn test_extract_title_from_response_empty_returns_none() {
    // Arrange / Act / Assert
    assert_eq!(
        extract_title_from_response(&StopReason::EndTurn, "   "),
        None
    );
    assert_eq!(
        extract_title_from_response(&StopReason::EndTurn, "\n\t"),
        None
    );
}

/// 工具调用结束的回复同样按正文处理（标题提炼不应出现工具调用，但语义上不能 panic）。
#[test]
fn test_extract_title_from_response_tool_use_reason_still_yields_text() {
    // Arrange / Act
    let title = extract_title_from_response(&StopReason::ToolUse, "会话标题生成");

    // Assert
    assert_eq!(title.as_deref(), Some("会话标题生成"));
}

/// 超长标题按字形簇截断并以 `...` 闭合。
#[test]
fn test_extract_title_from_response_truncates_long_title() {
    // Arrange：80 个中文字符，远超 48 字上限
    let raw: String = "标".repeat(80);

    // Act
    let title = extract_title_from_response(&StopReason::EndTurn, &raw).expect("应返回标题");

    // Assert
    assert!(title.ends_with("..."), "超长标题必须以省略号闭合: {title}");
    assert_eq!(
        title.chars().count(),
        TITLE_MAX_CHARS,
        "截断后应恰好为上限字符数"
    );
}
