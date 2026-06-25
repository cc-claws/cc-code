# Setup Wizard 语言选择步骤 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 在 Setup 向导的 Choose 和 Form 步骤之间增加 Language 步骤，用户选择 `en` 或 `zh-CN`，结果写入 `AppConfig.language`。

**Architecture:** 三步扩充为四步：Choose → Language → Form → Done。Language 步骤用方向键导航两个语言选项（English / 中文），Enter 确认后进入 Form，Esc 返回 Choose。`SetupWizardPanel` 新增 `language` 和 `language_cursor` 字段，`save_setup_to()` 写入 `cfg.config.language`。

**Tech Stack:** Rust + ratatui + tui-textarea + Fluent i18n

---

### Task 1: FTL 翻译键

**Files:**
- Modify: `peri-tui/locales/en/main.ftl` (追加 ~5 行)
- Modify: `peri-tui/locales/zh-CN/main.ftl` (追加 ~5 行)

新增 Language 步骤需要的翻译键。语言选项名称（English / 中文）硬编码显示，不经过翻译。

- [ ] **Step 1: 在英文 FTL 末尾追加翻译键**

在 `peri-tui/locales/en/main.ftl` 末尾追加：

```ftl
# Setup Language step
setup-language-title = ── Peri Setup ── Language
setup-language-prompt = Choose your interface language:
setup-language-press-enter = Press Enter to confirm
```

- [ ] **Step 2: 在中文 FTL 末尾追加翻译键**

在 `peri-tui/locales/zh-CN/main.ftl` 末尾追加：

```ftl
# Setup Language step
setup-language-title = ── Peri 设置 ── 语言
setup-language-prompt = 选择界面语言：
setup-language-press-enter = 按 Enter 确认
```

- [ ] **Step 3: 验证 FTL 文件解析无错误**

```bash
cargo build -p peri-tui 2>&1 | head -5
```

Expected: 编译成功，无 FTL parse error。

---

### Task 2: SetupWizardPanel 状态扩展

**Files:**
- Modify: `peri-tui/src/app/setup_wizard.rs`

为 `SetupStep` 枚举添加 `Language` 变体，为 `SetupWizardPanel` 添加 `language`/`language_cursor` 字段，定义语言选项常量。

- [ ] **Step 1: 在 `SetupStep` 枚举中插入 `Language` 变体**

修改 `peri-tui/src/app/setup_wizard.rs` 第 3-10 行，在 `Choose` 和 `Form` 之间插入 `Language`：

```rust
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SetupStep {
    /// 选择来源
    Choose,
    /// 选择语言
    Language,
    /// 合并表单：多 Provider + API Key + Model Aliases
    Form,
    /// 确认完成
    Done,
}
```

- [ ] **Step 2: 添加语言选项常量**

在 `SetupSource::ALL` 定义之后（第 22 行附近），插入：

```rust
/// 支持的语言选项：(code, display_name)
pub const LANGUAGE_OPTIONS: [(&str, &str); 2] = [("en", "English"), ("zh-CN", "中文")];
```

- [ ] **Step 3: 在 `SetupWizardPanel` 中添加 `language` 和 `language_cursor` 字段**

修改 `SetupWizardPanel` 结构体（第 221-238 行），添加两个新字段：

```rust
pub struct SetupWizardPanel {
    pub step: SetupStep,
    /// Step 1: 来源选择
    pub source: SetupSource,
    pub choose_cursor: usize,
    /// Step 2: 语言选择
    pub language: String,
    pub language_cursor: usize,
    /// Step 3: 多 provider 列表
    pub providers: Vec<MigratedProvider>,
    // ... 其余字段不变
```

- [ ] **Step 4: 在 `new()` 中初始化新字段**

修改 `SetupWizardPanel::new()`（第 247-259 行），在 `choose_cursor: 0,` 之后插入：

```rust
language: "en".to_string(),
language_cursor: 0,
```

- [ ] **Step 5: 构建验证**

```bash
cargo build -p peri-tui 2>&1 | tail -20
```

Expected: 大部分编译通过，仅渲染分发缺少 `SetupStep::Language` 分支导致编译错误（预期行为，Task 3 处理）。

---

### Task 3: Language 步骤按键处理

**Files:**
- Modify: `peri-tui/src/app/setup_wizard.rs`

实现 `handle_step_language` 函数，并在 `handle_setup_wizard_key` 分发和步骤间跳转逻辑中插入 Language 步骤。

- [ ] **Step 1: 在 `handle_setup_wizard_key` 中添加 Language 分发**

修改 `handle_setup_wizard_key` 函数（第 481-490 行），在 `SetupStep::Choose` 和 `SetupStep::Form` 之间插入：

```rust
SetupStep::Language => handle_step_language(wizard, input),
```

- [ ] **Step 2: 修改 `handle_step_choose` Enter 逻辑跳转到 Language**

修改第 509-536 行的 Enter/Space 处理，将跳转到 `Form` 改为跳转到 `Language`：

```rust
} | tui_textarea::Input {
    key: Key::Char(' '),
    ..
} => {
    if wizard.source == SetupSource::MigrateClaudeCode {
        if !wizard.migrate_from_claude_code() {
            wizard.source = SetupSource::CustomApi;
            wizard.choose_cursor = 0;
            return Some(SetupWizardAction::Redraw);
        }
    } else {
        wizard.providers = vec![MigratedProvider::new(ProviderType::Anthropic)];
        wizard.active_provider = 0;
    }
    wizard.step = SetupStep::Language;
    wizard.language_cursor = 0;
    Some(SetupWizardAction::Redraw)
}
```

- [ ] **Step 3: 修改 `handle_browse` 的 Esc 跳转到 Language**

修改第 604-608 行，将 Esc 跳转目标从 `Choose` 改为 `Language`：

```rust
tui_textarea::Input { key: Key::Esc, .. } => {
    wizard.step = SetupStep::Language;
    Some(SetupWizardAction::Redraw)
}
```

- [ ] **Step 4: 实现 `handle_step_language` 函数**

在 `handle_step_choose` 函数之后（第 537 行之后），插入新函数：

```rust
fn handle_step_language(
    wizard: &mut SetupWizardPanel,
    input: tui_textarea::Input,
) -> Option<SetupWizardAction> {
    use tui_textarea::Key;
    match input {
        tui_textarea::Input { key: Key::Up, .. } => {
            wizard.language_cursor =
                (wizard.language_cursor + LANGUAGE_OPTIONS.len() - 1) % LANGUAGE_OPTIONS.len();
            Some(SetupWizardAction::Redraw)
        }
        tui_textarea::Input { key: Key::Down, .. } => {
            wizard.language_cursor = (wizard.language_cursor + 1) % LANGUAGE_OPTIONS.len();
            Some(SetupWizardAction::Redraw)
        }
        tui_textarea::Input {
            key: Key::Enter, ..
        }
        | tui_textarea::Input {
            key: Key::Char(' '),
            ..
        } => {
            wizard.language = LANGUAGE_OPTIONS[wizard.language_cursor].0.to_string();
            wizard.step = SetupStep::Form;
            wizard.form_mode = FormMode::Browse;
            wizard.browse_cursor = 0;
            wizard.form_focus = FormField::ProviderType;
            Some(SetupWizardAction::Redraw)
        }
        tui_textarea::Input { key: Key::Esc, .. } => {
            wizard.step = SetupStep::Choose;
            Some(SetupWizardAction::Redraw)
        }
        _ => None,
    }
}
```

- [ ] **Step 5: 编译验证（渲染分发仍缺）**

```bash
cargo build -p peri-tui 2>&1 | grep "error" | head -10
```

Expected: 仅有 `render_setup_wizard` 缺少 `SetupStep::Language` 分支的错误（Task 4 修复）。

---

### Task 4: Language 步骤 UI 渲染

**Files:**
- Modify: `peri-tui/src/ui/main_ui/popups/setup_wizard.rs`

添加 `render_step_language` 函数，在 `render_setup_wizard` 分发中添加 Language 分支。

- [ ] **Step 1: 在 `render_setup_wizard` 中添加 Language 分发**

修改第 20-24 行的 match：

```rust
match wizard.step {
    SetupStep::Choose => render_step_choose(f, wizard, lc, area),
    SetupStep::Language => render_step_language(f, wizard, lc, area),
    SetupStep::Form => render_step_form(f, wizard, lc, area),
    SetupStep::Done => render_step_done(f, wizard, lc, area),
}
```

- [ ] **Step 2: 实现 `render_step_language` 函数**

在 `render_step_choose` 函数之后（第 86 行之前），插入新函数：

```rust
fn render_step_language(
    f: &mut Frame,
    wizard: &SetupWizardPanel,
    lc: &crate::i18n::LcRegistry,
    area: Rect,
) {
    let inner = BorderedPanel::new(Span::styled(
        lc.tr("setup-language-title"),
        Style::default()
            .fg(theme::ACCENT)
            .add_modifier(Modifier::BOLD),
    ))
    .border_style(Style::default().fg(theme::ACCENT))
    .render(f, area);

    let mut lines: Vec<Line> = vec![
        Line::from(""),
        Line::from(Span::styled(
            lc.tr("setup-language-prompt"),
            Style::default().fg(theme::MUTED),
        )),
        Line::from(""),
    ];

    for (i, (_code, name)) in LANGUAGE_OPTIONS.iter().enumerate() {
        let is_cursor = i == wizard.language_cursor;
        let cursor_char = if is_cursor { "❯" } else { " " };
        let cursor_style = Style::default().fg(theme::THINKING);
        let name_style = if is_cursor {
            Style::default()
                .fg(theme::THINKING)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(theme::TEXT)
        };
        lines.push(Line::from(vec![
            Span::styled(format!("{} ", cursor_char), cursor_style),
            Span::styled(*name, name_style),
        ]));
    }

    lines.push(Line::from(""));
    lines.push(make_hint_line(vec![
        ("Enter".to_string(), lc.tr("setup-key-confirm")),
        ("↑/↓".to_string(), lc.tr("setup-key-select")),
        ("Esc".to_string(), lc.tr("setup-key-back")),
    ]));

    f.render_widget(Paragraph::new(Text::from(lines)), inner);
}
```

Note: 需要确认 `LANGUAGE_OPTIONS` 的 import。检查文件顶部的 imports，当前已有 `use crate::app::setup_wizard::{FormField, FormMode, SetupSource, SetupStep, SetupWizardPanel};`，需要添加 `LANGUAGE_OPTIONS`。

- [ ] **Step 3: 添加 `LANGUAGE_OPTIONS` 的 import**

修改第 11 行的 import：

```rust
use crate::app::setup_wizard::{FormField, FormMode, LANGUAGE_OPTIONS, SetupSource, SetupStep, SetupWizardPanel};
```

- [ ] **Step 4: 编译验证**

```bash
cargo build -p peri-tui 2>&1 | tail -5
```

Expected: 编译成功。

---

### Task 5: save_setup_to 写入 language 字段

**Files:**
- Modify: `peri-tui/src/app/setup_wizard.rs`

- [ ] **Step 1: 在 `save_setup_to` 中写入 `language`**

在 `save_setup_to` 函数中（`cfg.config.active_provider_id = first_id;` 之后，第 778 行附近），添加：

```rust
cfg.config.language = Some(wizard.language.clone());
```

最终 `save_setup_to` 的相关部分变为：

```rust
if !first_id.is_empty() {
    cfg.config.active_alias = "opus".to_string();
    cfg.config.active_provider_id = first_id;
}

cfg.config.language = Some(wizard.language.clone());

let content = serde_json::to_string_pretty(&cfg)?;
```

- [ ] **Step 2: 编译验证**

```bash
cargo build -p peri-tui 2>&1 | tail -5
```

Expected: 编译成功。

---

### Task 6: 单元测试

**Files:**
- Modify: `peri-tui/src/app/setup_wizard_test.rs`

- [ ] **Step 1: 添加 Language 步骤导航测试**

在 Choose 步骤测试之后（第 216 行后）、Form 步骤测试之前，插入：

```rust
// ── Step: Language ──

#[test]
fn test_language_arrow_navigates() {
    let mut wizard = SetupWizardPanel::new();
    wizard.step = SetupStep::Language;
    assert_eq!(wizard.language_cursor, 0);
    let _ = handle_setup_wizard_key(&mut wizard, make_key(Key::Down));
    assert_eq!(wizard.language_cursor, 1);
    let _ = handle_setup_wizard_key(&mut wizard, make_key(Key::Down));
    assert_eq!(wizard.language_cursor, 0); // wraps around
    let _ = handle_setup_wizard_key(&mut wizard, make_key(Key::Up));
    assert_eq!(wizard.language_cursor, 1); // wraps around
}

#[test]
fn test_language_enter_selects_and_advances_to_form() {
    let mut wizard = SetupWizardPanel::new();
    wizard.step = SetupStep::Language;
    wizard.language_cursor = 1; // zh-CN
    let _ = handle_setup_wizard_key(&mut wizard, make_key(Key::Enter));
    assert_eq!(wizard.language, "zh-CN");
    assert_eq!(wizard.step, SetupStep::Form);
    assert_eq!(wizard.form_mode, FormMode::Browse);
}

#[test]
fn test_language_space_selects_and_advances() {
    let mut wizard = SetupWizardPanel::new();
    wizard.step = SetupStep::Language;
    let _ = handle_setup_wizard_key(&mut wizard, make_char(' '));
    assert_eq!(wizard.language, "en");
    assert_eq!(wizard.step, SetupStep::Form);
}

#[test]
fn test_language_esc_back_to_choose() {
    let mut wizard = SetupWizardPanel::new();
    wizard.step = SetupStep::Language;
    let _ = handle_setup_wizard_key(&mut wizard, make_key(Key::Esc));
    assert_eq!(wizard.step, SetupStep::Choose);
}

#[test]
fn test_language_default_is_en() {
    let wizard = SetupWizardPanel::new();
    assert_eq!(wizard.language, "en");
}
```

- [ ] **Step 2: 更新 Choose 步骤 Enter 测试**

修改 `test_choose_enter_custom_advances_to_form` (第 203-209 行)：

```rust
#[test]
fn test_choose_enter_custom_advances_to_language() {
    let mut wizard = SetupWizardPanel::new();
    let _ = handle_setup_wizard_key(&mut wizard, make_key(Key::Enter));
    assert_eq!(wizard.step, SetupStep::Language);
}
```

- [ ] **Step 3: 更新 Browse Esc 测试**

修改 `test_browse_esc_back_to_choose` (第 272-278 行)：

```rust
#[test]
fn test_browse_esc_back_to_language() {
    let mut wizard = SetupWizardPanel::new();
    wizard.step = SetupStep::Form;
    wizard.form_mode = FormMode::Browse;
    let _ = handle_setup_wizard_key(&mut wizard, make_key(Key::Esc));
    assert_eq!(wizard.step, SetupStep::Language);
}
```

- [ ] **Step 4: 添加 `save_setup_to` 写入 language 测试**

在 `test_save_setup_skips_unselected` 之后（第 371 行后），添加：

```rust
#[test]
fn test_save_setup_writes_language() {
    let mut wizard = SetupWizardPanel::new();
    wizard.providers[0].api_key = "sk-test".to_string();
    wizard.language = "zh-CN".to_string();
    let temp_dir = std::env::temp_dir().join(format!("zen-lang-setup-{}", uuid::Uuid::now_v7()));
    let config_path = temp_dir.join("settings.json");
    let cfg = save_setup_to(&wizard, &config_path).expect("save should succeed");
    assert_eq!(cfg.config.language.as_deref(), Some("zh-CN"));
    let _ = std::fs::remove_dir_all(&temp_dir);
}
```

- [ ] **Step 5: 更新 `test_setup_wizard_new_defaults`**

修改 `test_setup_wizard_new_defaults` (第 24-32 行)，添加语言默认值断言：

```rust
#[test]
fn test_setup_wizard_new_defaults() {
    let wizard = SetupWizardPanel::new();
    assert_eq!(wizard.step, SetupStep::Choose);
    assert_eq!(wizard.source, SetupSource::CustomApi);
    assert_eq!(wizard.language, "en");
    assert_eq!(wizard.language_cursor, 0);
    assert_eq!(wizard.providers.len(), 1);
    assert_eq!(wizard.providers[0].provider_type, ProviderType::Anthropic);
    assert!(wizard.providers[0].api_key.is_empty());
    assert!(wizard.providers[0].selected);
}
```

- [ ] **Step 6: 运行全部 setup wizard 测试**

```bash
cargo test -p peri-tui --lib -- setup_wizard_test 2>&1
```

Expected: 所有测试通过。

---

### Task 7: 集成测试更新

**Files:**
- Modify: `peri-tui/src/ui/headless_test.rs`

Setup wizard 的 headless 集成测试可能也需要适配新的步骤顺序。

- [ ] **Step 1: 搜索 headless 测试中 setup wizard 相关测试**

```bash
grep -n "test_s.*setup\|test_s.*wizard\|test_s.*form\|test_s.*choose" peri-tui/src/ui/headless_test.rs
```

- [ ] **Step 2: 阅读相关测试并更新步骤预期**

检查 `peri-tui/src/ui/headless_test.rs` 中所有直接调用 `handle_setup_wizard_key` 并按步骤状态做断言的测试。如果某测试从 Choose 直接进入 Form（跳过 Language），需要在测试中插入一次 Enter（从 Choose 进 Language）再 Enter（从 Language 进 Form）。

- [ ] **Step 3: 更新 headless_test.rs 的 import**

搜索 `set_wizard` 相关的 import 语句，确保 `LANGUAGE_OPTIONS` 被导入（如需要）。

- [ ] **Step 4: 运行 headless 测试**

```bash
cargo test -p peri-tui --test '*' -- setup 2>&1 | tail -30
```

Expected: 所有测试通过。

---

### Task 8: 完整编译 + 测试验证

- [ ] **Step 1: 完整编译**

```bash
cargo build -p peri-tui 2>&1
```

- [ ] **Step 2: 完整测试**

```bash
cargo test -p peri-tui 2>&1
```

Expected: 编译无错误，所有测试通过。

- [ ] **Step 3: Clippy**

```bash
cargo clippy -p peri-tui -- -D warnings 2>&1
```

Expected: 无 clippy 警告。

- [ ] **Step 4: Commit**

```bash
git add peri-tui/src/app/setup_wizard.rs peri-tui/src/app/setup_wizard_test.rs \
    peri-tui/src/ui/main_ui/popups/setup_wizard.rs peri-tui/locales/en/main.ftl \
    peri-tui/locales/zh-CN/main.ftl peri-tui/src/ui/headless_test.rs
git commit -m "feat(setup): add language selection step between Choose and Form

Closes spec/issues/2026-05-16-i18n-language-not-in-setup.md

Co-Authored-By: deepseek-v4-pro <deepseek-ai@claude-code-best.win>"
```

---

## Self-Review

### 1. Spec coverage
- ✅ 在 Choose 和 Form 之间增加独立的 Language 步骤
- ✅ 用户可选择 English (en) 或 中文 (zh-CN)
- ✅ 选择结果写入 AppConfig.language
- ✅ save_setup_to 写入 language 字段
- ✅ 涉及文件：setup_wizard.rs, setup_wizard.rs (popups), types.rs (已存在 language 字段，无需修改)

### 2. Placeholder scan
- 无占位符

### 3. Type consistency
- `SetupStep::Language` 在所有 match 分支中一致（handle_setup_wizard_key, render_setup_wizard）
- `LANGUAGE_OPTIONS` 数组 `[("en", "English"), ("zh-CN", "中文")]` 索引 0/1 与 `language_cursor` 对应
- `wizard.language` 取值 `"en"` 或 `"zh-CN"`，与 `AppConfig.language: Option<String>` 兼容
- 测试中 `make_char(' ')` 和 `make_key(Key::Enter)` 在 Language 步骤的行为一致
