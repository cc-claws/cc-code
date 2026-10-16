pub mod ask_user;
pub mod bash;
pub mod glob;
pub mod read;

#[derive(Debug, PartialEq, Eq, Clone)]
pub struct ToolDisplayLine<'a> {
    pub prefix: &'static str,
    pub text: &'a str,
    pub is_error_header: bool,
}
