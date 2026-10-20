pub mod ask_user;
pub mod bash;
pub mod glob;
pub mod output_line;
pub mod read;

/// Bash 结果的一行待渲染内容。
///
/// `text` 用 `Cow` 是为了容纳 JSON 美化产出的 owned 字符串：CC `OutputLine` 的
/// `tryJsonFormatContent` 会把单行 JSON 展开成 2 空格缩进的多行，那部分内容在
/// 原输出里并不存在，只能自己持有。其余行借原 content，零拷贝。
#[derive(Debug, PartialEq, Eq, Clone)]
pub struct ToolDisplayLine<'a> {
    pub prefix: &'static str,
    pub text: std::borrow::Cow<'a, str>,
    pub is_error_header: bool,
}
