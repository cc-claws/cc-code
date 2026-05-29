use super::{ask_user_prompt::AskUserBatchPrompt, hitl_prompt::HitlBatchPrompt};

/// 统一交互弹窗枚举：同一时刻只允许一种弹窗激活
pub enum InteractionPrompt {
    Approval(HitlBatchPrompt),
    Questions(AskUserBatchPrompt),
}
