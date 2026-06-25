use crate::config::{PeriConfig, ProviderConfig, ProviderModels};

use super::{panel_list::PanelList, App};

// ─── 默认模型名常量表 ─────────────────────────────────────────────────────────

/// (provider_type, opus, sonnet, haiku)
const DEFAULT_MODELS: &[(&str, &str, &str, &str)] = &[
    (
        "anthropic",
        "claude-opus-4-7",
        "claude-sonnet-4-6",
        "claude-haiku-4-5",
    ),
    ("openai", "gpt-4o", "gpt-4o-mini", "gpt-3.5-turbo"),
];

/// provider_type 循环切换列表
const PROVIDER_TYPES: &[&str] = &["openai", "anthropic"];

// ─── 枚举 ─────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub enum LoginPanelMode {
    Browse,
    Edit,
    New,
    ConfirmDelete,
}

#[derive(Debug, Clone, PartialEq)]
pub enum LoginEditField {
    Name,
    Type,
    BaseUrl,
    ApiKey,
    OpusModel,
    SonnetModel,
    HaikuModel,
}

impl LoginEditField {
    pub fn next(&self) -> Self {
        match self {
            Self::Name => Self::Type,
            Self::Type => Self::BaseUrl,
            Self::BaseUrl => Self::ApiKey,
            Self::ApiKey => Self::OpusModel,
            Self::OpusModel => Self::SonnetModel,
            Self::SonnetModel => Self::HaikuModel,
            Self::HaikuModel => Self::Name,
        }
    }

    pub fn prev(&self) -> Self {
        match self {
            Self::Name => Self::HaikuModel,
            Self::Type => Self::Name,
            Self::BaseUrl => Self::Type,
            Self::ApiKey => Self::BaseUrl,
            Self::OpusModel => Self::ApiKey,
            Self::SonnetModel => Self::OpusModel,
            Self::HaikuModel => Self::SonnetModel,
        }
    }

    pub fn label(&self) -> &str {
        match self {
            Self::Name => "Name        ",
            Self::Type => "Type        ",
            Self::BaseUrl => "Base URL    ",
            Self::ApiKey => "API Key     ",
            Self::OpusModel => "Opus Model  ",
            Self::SonnetModel => "Sonnet Model",
            Self::HaikuModel => "Haiku Model ",
        }
    }
}

// ─── LoginPanel ─────────────────────────────────────────────────────────────────

#[derive(Clone)]
pub struct LoginPanel {
    /// provider 列表快照（从 PeriConfig 获取）
    pub providers: Vec<ProviderConfig>,
    /// 当前模式
    pub mode: LoginPanelMode,
    /// Browse 模式光标管理
    pub(crate) browse_list: PanelList<()>,
    /// 正在编辑的字段（Edit/New 模式下）
    pub edit_field: LoginEditField,
    /// 编辑缓冲区
    pub buf_name: String,
    pub buf_type: String,
    pub buf_base_url: String,
    pub buf_api_key: String,
    pub buf_opus_model: String,
    pub buf_sonnet_model: String,
    pub buf_haiku_model: String,
    /// 各字段的编辑光标（char-based index）
    pub cur_name: usize,
    pub cur_base_url: usize,
    pub cur_api_key: usize,
    pub cur_opus_model: usize,
    pub cur_sonnet_model: usize,
    pub cur_haiku_model: usize,
}

impl LoginPanel {
    /// 从 PeriConfig 初始化面板（Browse 模式，光标定位到 active_provider_id 对应的 Provider）
    pub fn from_config(cfg: &PeriConfig) -> Self {
        let providers = cfg.config.providers.clone();
        let cursor = providers
            .iter()
            .position(|p| p.id == cfg.config.active_provider_id)
            .unwrap_or(0);
        let mut browse_list = PanelList::new();
        browse_list.set_items(vec![(); providers.len()]);
        browse_list.move_cursor_to(cursor);
        Self {
            providers,
            mode: LoginPanelMode::Browse,
            browse_list,
            edit_field: LoginEditField::Name,
            buf_name: String::new(),
            buf_type: String::new(),
            buf_base_url: String::new(),
            buf_api_key: String::new(),
            buf_opus_model: String::new(),
            buf_sonnet_model: String::new(),
            buf_haiku_model: String::new(),
            cur_name: 0,
            cur_base_url: 0,
            cur_api_key: 0,
            cur_opus_model: 0,
            cur_sonnet_model: 0,
            cur_haiku_model: 0,
        }
    }

    // ── Browse 模式操作 ──────────────────────────────────────────────────────

    /// 当前光标位置（Browse 模式下标记选中行）
    pub fn cursor(&self) -> usize {
        self.browse_list.cursor()
    }

    /// 列表上下移动光标（clamp 模式，不循环）
    pub fn move_cursor(&mut self, delta: isize) {
        self.browse_list.move_cursor(delta);
    }

    /// 进入编辑模式（编辑光标处的 provider）
    pub fn enter_edit(&mut self) {
        if let Some(p) = self.providers.get(self.cursor()) {
            self.buf_name = p.display_name().to_string();
            self.buf_type = p.provider_type.clone();
            self.buf_base_url = p.base_url.clone();
            self.buf_api_key = p.api_key.clone();
            self.buf_opus_model = p.models.opus.clone();
            self.buf_sonnet_model = p.models.sonnet.clone();
            self.buf_haiku_model = p.models.haiku.clone();
            self.cur_name = self.buf_name.chars().count();
            self.cur_base_url = self.buf_base_url.chars().count();
            self.cur_api_key = self.buf_api_key.chars().count();
            self.cur_opus_model = self.buf_opus_model.chars().count();
            self.cur_sonnet_model = self.buf_sonnet_model.chars().count();
            self.cur_haiku_model = self.buf_haiku_model.chars().count();
            self.edit_field = LoginEditField::Name;
            self.mode = LoginPanelMode::Edit;
        }
    }

    /// 进入新建模式（清空所有缓冲，type 默认 "openai"，模型名按 type 自动填充）
    pub fn enter_new(&mut self) {
        self.buf_name = String::new();
        self.buf_type = "openai".to_string();
        self.buf_base_url = String::new();
        self.buf_api_key = String::new();
        self.buf_opus_model = String::new();
        self.buf_sonnet_model = String::new();
        self.buf_haiku_model = String::new();
        self.auto_fill_models_for_type();
        self.edit_field = LoginEditField::Name;
        self.mode = LoginPanelMode::New;
    }

    /// 进入删除确认模式
    pub fn request_delete(&mut self) {
        if !self.providers.is_empty() {
            self.mode = LoginPanelMode::ConfirmDelete;
        }
    }

    /// 选中（激活）光标处的 Provider，写入 cfg
    pub fn select_provider(&mut self, cfg: &mut PeriConfig) {
        if let Some(p) = self.providers.get(self.cursor()) {
            cfg.config.active_provider_id = p.id.clone();
        }
    }

    /// 取消删除确认，回到浏览模式
    pub fn cancel_delete(&mut self) {
        self.mode = LoginPanelMode::Browse;
    }

    // ── Edit/New 模式操作 ────────────────────────────────────────────────────

    /// 字段导航：下一个字段
    pub fn field_next(&mut self) {
        self.edit_field = self.edit_field.next();
    }

    /// 字段导航：上一个字段
    pub fn field_prev(&mut self) {
        self.edit_field = self.edit_field.prev();
    }

    /// 循环切换 provider_type（Space 键，仅在 edit_field == Type 时生效）
    /// 切换后自动调用 auto_fill_models_for_type 更新模型名默认值
    pub fn cycle_type(&mut self) {
        if self.edit_field == LoginEditField::Type {
            let cur = PROVIDER_TYPES
                .iter()
                .position(|&t| t == self.buf_type)
                .unwrap_or(0);
            self.buf_type = PROVIDER_TYPES[(cur + 1) % PROVIDER_TYPES.len()].to_string();
            self.auto_fill_models_for_type();
        }
    }

    /// 获取当前编辑字段的 (buf, cursor) 可变引用
    pub fn active_field(&mut self) -> Option<(&mut String, &mut usize)> {
        match self.edit_field {
            LoginEditField::Name => Some((&mut self.buf_name, &mut self.cur_name)),
            LoginEditField::Type => None,
            LoginEditField::BaseUrl => Some((&mut self.buf_base_url, &mut self.cur_base_url)),
            LoginEditField::ApiKey => Some((&mut self.buf_api_key, &mut self.cur_api_key)),
            LoginEditField::OpusModel => Some((&mut self.buf_opus_model, &mut self.cur_opus_model)),
            LoginEditField::SonnetModel => {
                Some((&mut self.buf_sonnet_model, &mut self.cur_sonnet_model))
            }
            LoginEditField::HaikuModel => {
                Some((&mut self.buf_haiku_model, &mut self.cur_haiku_model))
            }
        }
    }

    /// 粘贴文本到当前活动字段（过滤换行符，Type 字段忽略粘贴）
    pub fn paste_text(&mut self, text: &str) {
        let text: String = text.chars().filter(|&c| c != '\n' && c != '\r').collect();
        if let Some((buf, cursor)) = self.active_field() {
            let char_count = buf.chars().count();
            if *cursor > char_count {
                *cursor = char_count;
            }
            let byte_pos = buf
                .char_indices()
                .nth(*cursor)
                .map(|(i, _)| i)
                .unwrap_or(buf.len());
            buf.insert_str(byte_pos, &text);
            *cursor += text.chars().count();
        }
    }

    // ── Type 切换自动填充 ────────────────────────────────────────────────────

    /// Type 切换时自动填充模型名默认值
    /// 规则：检测三个模型名字段是否为空或等于旧 provider_type 的默认值；若是则填入新 type 的默认值
    pub fn auto_fill_models_for_type(&mut self) {
        let new_defaults = DEFAULT_MODELS
            .iter()
            .find(|(t, _, _, _)| *t == self.buf_type);
        let (opus_default, sonnet_default, haiku_default) = match new_defaults {
            Some((_, o, s, h)) => (o.to_string(), s.to_string(), h.to_string()),
            None => return, // 未知 provider_type，不自动填充
        };

        // 收集所有 provider_type 的默认值作为"旧默认值"候选
        let all_defaults: Vec<(String, String, String)> = DEFAULT_MODELS
            .iter()
            .map(|(_, o, s, h)| (o.to_string(), s.to_string(), h.to_string()))
            .collect();

        let is_default_or_empty = |val: &str| -> bool {
            if val.is_empty() {
                return true;
            }
            all_defaults
                .iter()
                .any(|(o, s, h)| val == o || val == s || val == h)
        };

        if is_default_or_empty(&self.buf_opus_model) {
            self.buf_opus_model = opus_default;
        }
        if is_default_or_empty(&self.buf_sonnet_model) {
            self.buf_sonnet_model = sonnet_default;
        }
        if is_default_or_empty(&self.buf_haiku_model) {
            self.buf_haiku_model = haiku_default;
        }
    }

    // ── 保存/删除操作 ──────────────────────────────────────────────────────────

    /// 将编辑/新建的内容保存到 PeriConfig，并更新内部 providers 快照
    /// 返回 true 表示成功
    /// 新建 Provider 后，active_provider_id 为空时自动设置为新建的 Provider ID
    pub fn apply_edit(&mut self, cfg: &mut PeriConfig) -> bool {
        let is_new = self.mode == LoginPanelMode::New;
        let id = if is_new {
            if self.buf_name.trim().is_empty() {
                return false;
            }
            self.buf_name.trim().to_lowercase().replace(' ', "_")
        } else {
            self.providers
                .get(self.cursor())
                .map(|p| p.id.clone())
                .unwrap_or_default()
        };

        if id.is_empty() {
            return false;
        }

        let mut p = ProviderConfig {
            id: id.clone(),
            provider_type: self.buf_type.clone(),
            api_key: self.buf_api_key.clone(),
            base_url: self.buf_base_url.clone(),
            name: if self.buf_name.trim().is_empty() {
                None
            } else {
                Some(self.buf_name.trim().to_string())
            },
            models: ProviderModels {
                opus: self.buf_opus_model.clone(),
                sonnet: self.buf_sonnet_model.clone(),
                haiku: self.buf_haiku_model.clone(),
            },
            thinking: None,
            extra: Default::default(),
        };

        // 编辑模式：保留原有的 extra 字段
        if self.mode == LoginPanelMode::Edit {
            if let Some(orig) = self.providers.get(self.cursor()) {
                p.extra = orig.extra.clone();
            }
        }

        if is_new {
            cfg.config.providers.push(p);
            // active_provider_id 为空时自动设置
            if cfg.config.active_provider_id.is_empty() {
                cfg.config.active_provider_id = id;
            }
        } else if let Some(existing) = cfg.config.providers.iter_mut().find(|x| x.id == id) {
            *existing = p;
        }

        self.providers = cfg.config.providers.clone();
        self.browse_list.set_items(vec![(); self.providers.len()]);
        self.browse_list.clamp_cursor();
        self.mode = LoginPanelMode::Browse;
        true
    }

    /// 确认删除光标处的 provider，写入 cfg
    pub fn confirm_delete(&mut self, cfg: &mut PeriConfig) {
        if let Some(p) = self.providers.get(self.cursor()) {
            let id = p.id.clone();
            cfg.config.providers.retain(|x| x.id != id);
            self.providers = cfg.config.providers.clone();
            self.browse_list.set_items(vec![(); self.providers.len()]);
            self.browse_list.clamp_cursor();
            // 如果删除的是当前激活的 provider，清空 active_provider_id
            if cfg.config.active_provider_id == id {
                cfg.config.active_provider_id.clear();
            }
        }
        self.mode = LoginPanelMode::Browse;
    }
}

mod component;

#[cfg(test)]
#[path = "login_panel_test.rs"]
mod tests;
