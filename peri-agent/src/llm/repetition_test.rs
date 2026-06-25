use super::RepetitionDetector;

/// 连续重复 10 次以上，检测到
#[test]
fn test_repetition_detected_at_threshold() {
    let sentence = "Actually, let me try to use `cargo doc` to generate the docs.";
    let text = sentence.repeat(10);
    let mut detector = RepetitionDetector::new();
    assert!(detector.check(&text));
}

/// 连续重复 9 次（未达阈值），不检测
#[test]
fn test_repetition_below_threshold() {
    let sentence = "Actually, let me try to use `cargo doc` to generate the docs.";
    let text = sentence.repeat(9);
    let mut detector = RepetitionDetector::new();
    assert!(!detector.check(&text));
}

/// 正常文本不误检
#[test]
fn test_normal_text_not_detected() {
    let text = "I need to analyze this codebase. Let me start by reading the main file. \
                The structure looks reasonable. I'll check the test coverage next. \
                After that I'll run the benchmarks and compare with baseline.";
    let mut detector = RepetitionDetector::new();
    assert!(!detector.check(text));
}

/// 文本太短不检测
#[test]
fn test_too_short_not_checked() {
    let text = "Short text. Short text. Short text.";
    let mut detector = RepetitionDetector::new();
    assert!(!detector.check(text));
}

/// 不会在短增量上重复检测
#[test]
fn test_no_recheck_within_interval() {
    let sentence = "This is a repeated sentence for testing purposes here. ";
    let mut detector = RepetitionDetector::new();
    let base = sentence.repeat(20);

    // 第一次检测
    assert!(detector.check(&base));
    // 已检测过 last_check_len 被更新，短增量不重检
    let extended = format!("{base}extra padding");
    assert!(!detector.check(&extended));
}

/// 交错不同句子不算连续重复
#[test]
fn test_alternating_sentences_not_detected() {
    let text = "Step one: read the file carefully. Step two: check the tests. \
                Step one: read the file carefully. Step two: check the tests. \
                Step one: read the file carefully. Step two: check the tests.";
    let mut detector = RepetitionDetector::new();
    assert!(!detector.check(text));
}

/// 真实退化场景模拟：thinking 内容连续重复同一句话 20 次
#[test]
fn test_realistic_degenerate_output() {
    let unit = "Actually, let me try to use `cargo doc` to generate the docs for tui-textarea-2, which will show me the source code.";
    let text = unit.repeat(20);
    let mut detector = RepetitionDetector::new();
    assert!(detector.check(&text));
}

/// 换行分隔的连续重复也检测
#[test]
fn test_newline_separated_repetition() {
    let line = "I should try a completely different approach to solve this problem now\n";
    let text = line.repeat(12);
    let mut detector = RepetitionDetector::new();
    assert!(detector.check(&text));
}
