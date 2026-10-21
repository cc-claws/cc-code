//! 靠左的紧凑欢迎卡；高度测量与实际渲染共用同一份内容。

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Padding, Paragraph, Widget},
    Frame,
};

use crate::{
    app::{tool_display::sanitize_display_text, App, UpdateStatus},
    ui::{message_render::truncate_to_display_width, theme},
};

/// 卡片保持适中的宽度，宽终端也不拉伸为整屏。
const CARD_MAX_WIDTH: u16 = 64;
/// 在内容右侧仍能保留足够列宽时才显示装饰。
const MASCOT_MIN_WIDTH: u16 = 56;
const MASCOT_WIDTH: u16 = 12;

/// 吉祥物像素稿：12 列 × 10 像素行（`C` = 左半青，`O` = 右半橙，`.` = 空）。
///
/// 渲染时两两像素行压成一个字符格（`▀` + 前景 / 背景），得到 12 列 × 5 行。
/// 眼睛是**挖空**而不是涂色：露出终端底色，因此 light / dark / daltonized /
/// `*-ansi` 各主题下都有对比度，吉祥物只需两种颜色。
const MASCOT: [&str; 10] = [
    "...C....O...",
    "...C....O...",
    "..CCCCOOOO..",
    ".CCCCCOOOOO.",
    "CCCCCCOOOOOO",
    "CC..CCOO..OO",
    "CCCCCCOOOOOO",
    ".CCCCCOOOOO.",
    "..CC....OO..",
    "..CC....OO..",
];

/// 吉祥物在终端里占用的行数（每两个像素行压成一个字符行）。
const MASCOT_ROWS: usize = MASCOT.len() / 2;

/// 像素字符 → 颜色（空像素返回 `None`，透出终端底色）。
fn mascot_pixel(ch: u8) -> Option<Color> {
    match ch {
        b'C' => Some(theme::cur().cyan),
        b'O' => Some(theme::cur().claude),
        _ => None,
    }
}

/// 把 [`MASCOT`] 压成 5 行半块字符。
///
/// 每格上下两个像素分别走前景 / 背景，因此一列即可表达纵向两色；
/// 上下都空时输出空格且不设背景，保持透明。
fn mascot_lines() -> Vec<Line<'static>> {
    let mut lines = Vec::with_capacity(MASCOT_ROWS);
    for pair in MASCOT.as_chunks::<2>().0 {
        let (top, bottom) = (pair[0].as_bytes(), pair[1].as_bytes());
        let mut spans: Vec<Span<'static>> = Vec::with_capacity(top.len());
        for col in 0..top.len() {
            let upper = mascot_pixel(top[col]);
            let lower = mascot_pixel(bottom[col]);
            spans.push(match (upper, lower) {
                (Some(u), Some(l)) => Span::styled("▀", Style::default().fg(u).bg(l)),
                (Some(u), None) => Span::styled("▀", Style::default().fg(u)),
                (None, Some(l)) => Span::styled("▄", Style::default().fg(l)),
                (None, None) => Span::raw(" "),
            });
        }
        lines.push(Line::from(spans));
    }
    lines
}

/// 欢迎卡副标题里的模型与推理档位。
///
/// 只取当前实际生效的两个值：模型名来自 `services.model_name`（`/model` 切换后
/// 会同步更新），effort 取自配置的 `thinking.effort`。任一项缺失都返回 `None`，
/// 由调用方整行省略——不编造默认档位。
fn model_effort_label(app: &App) -> Option<String> {
    let model = app.services.model_name.trim();
    if model.is_empty() {
        return None;
    }
    let effort = app
        .services
        .peri_config
        .as_ref()
        .and_then(|config| config.config.thinking.as_ref())
        .map(|thinking| thinking.effort.trim())
        .filter(|effort| !effort.is_empty())?;
    Some(format!("{model} · ({effort} effort)"))
}

fn welcome_lines(app: &App, width: u16) -> Vec<Line<'static>> {
    let lc = &app.services.lc;
    let fit = |text: &str| truncate_to_display_width(&sanitize_display_text(text), width as usize);
    // Provider 的有无决定两处内容：副标题是否显示、是否追加 `/login` 引导。
    // 未配置 Provider 时模型名与 effort 都还没有实际意义，整行省略。
    let has_provider = app
        .services
        .peri_config
        .as_ref()
        .is_some_and(|config| !config.config.providers.is_empty());
    // 标题行拆两段：品牌名用 ACCENT 强调，版本号降级为 MUTED 次要信息。
    // 品牌名恒定不截断（窄卡片下优先保住它），只有版本号按剩余宽度收口。
    const BRAND: &str = "CC Code  ";
    let version = format!("v{}", env!("CARGO_PKG_VERSION"));
    let version_width = (width as usize).saturating_sub(BRAND.len());
    let mut lines = vec![Line::from(vec![
        Span::styled(
            BRAND,
            Style::default()
                .fg(theme::cur().claude)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            truncate_to_display_width(&version, version_width),
            Style::default().fg(theme::cur().inactive),
        ),
    ])];

    // 副标题：模型 + 推理档位，缺失或未配置 Provider 时整行省略。
    if has_provider {
        if let Some(label) = model_effort_label(app).filter(|label| !label.is_empty()) {
            let fitted = fit(&label);
            if !fitted.is_empty() {
                lines.push(Line::from(Span::styled(
                    fitted,
                    Style::default().fg(theme::cur().inactive),
                )));
            }
        }
    }

    lines.push(Line::from(Span::styled(
        fit(&app.services.cwd),
        Style::default().fg(theme::cur().inactive),
    )));

    // 最后一行：有更新时用「更新提示 + 升级命令」两行**替换**掉开始提示。
    //
    // 这是刻意替换而非追加——卡片内高由吉祥物的 5 行决定，开始提示占 1 行、
    // 更新提示占 2 行，正好补满那 1 行空位，卡片高度始终不变（内高 5 / 外框 7）。
    // 追加的话会把卡片撑高一行，空会话与有更新时的启动占用就不一致了。
    match app.services.update_checker.lock().status() {
        UpdateStatus::Available { latest } => {
            let current = env!("CARGO_PKG_VERSION");
            lines.push(Line::from(Span::styled(
                fit(&lc.tr_args(
                    "welcome-update-available",
                    &[
                        ("current".into(), current.into()),
                        ("latest".into(), latest.clone().into()),
                    ],
                )),
                Style::default().fg(theme::cur().warning),
            )));
            lines.push(Line::from(Span::styled(
                fit(&lc.tr("welcome-update-command")),
                Style::default().fg(theme::cur().inactive),
            )));
        }
        UpdateStatus::Unknown | UpdateStatus::UpToDate => {
            lines.push(Line::from(Span::styled(
                fit(&lc.tr("welcome-start-hint")),
                Style::default().fg(theme::cur().text),
            )));
        }
    }

    if !has_provider {
        lines.push(Line::from(Span::styled(
            fit(&lc.tr("welcome-login-hint")),
            Style::default().fg(theme::cur().warning),
        )));
    }
    lines
}

/// 自然高度与终端高度无关，空闲刷新不会扩大欢迎区。
pub(crate) fn welcome_height(app: &App, width: u16) -> u16 {
    if width == 0 {
        return 0;
    }
    let content_height = welcome_lines(app, width.saturating_sub(4)).len() as u16;
    let mascot_height = if width >= MASCOT_MIN_WIDTH {
        MASCOT_ROWS as u16
    } else {
        0
    };
    content_height.max(mascot_height).saturating_add(2)
}

/// 卡片从给定区域左上角开始；窄屏或矮区域优先保留文本。
#[cfg(test)]
pub(crate) fn render_welcome(f: &mut Frame, app: &App, area: Rect) {
    render_welcome_buffer(app, area, f.buffer_mut());
}

/// 按完整卡片的视觉行裁切，滚动时保持原有边框、文本与吉祥物布局。
pub(crate) fn render_welcome_slice(f: &mut Frame, app: &App, area: Rect, offset: usize) -> u16 {
    let visible = area.intersection(f.area());
    if visible.is_empty() {
        return 0;
    }
    let height = welcome_height(app, area.width);
    let source_row = offset.saturating_add(usize::from(visible.y - area.y));
    if source_row >= usize::from(height) {
        return 0;
    }
    let source_area = Rect::new(0, 0, area.width, height);
    let mut source = Buffer::empty(source_area);
    render_welcome_buffer(app, source_area, &mut source);
    let rows = visible.height.min(height - source_row as u16);
    let source_column = visible.x - area.x;
    let target = f.buffer_mut();
    for row in 0..rows {
        for column in 0..visible.width {
            target[(visible.x + column, visible.y + row)] =
                source[(source_column + column, source_row as u16 + row)].clone();
        }
    }
    rows
}

fn render_welcome_buffer(app: &App, area: Rect, buffer: &mut Buffer) {
    if area.is_empty() {
        return;
    }
    let card = Rect::new(
        area.x,
        area.y,
        area.width.min(CARD_MAX_WIDTH),
        area.height.min(welcome_height(app, area.width)),
    );
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(ratatui::widgets::BorderType::Rounded)
        .border_style(Style::default().fg(theme::cur().subtle))
        .padding(Padding::horizontal(1));
    let inner = block.inner(card);
    block.render(card, buffer);
    let show_mascot = card.width >= MASCOT_MIN_WIDTH && inner.height >= MASCOT_ROWS as u16;
    let text = if show_mascot {
        Paragraph::new(mascot_lines()).render(
            Rect::new(inner.x, inner.y, MASCOT_WIDTH, MASCOT_ROWS as u16),
            buffer,
        );
        Rect::new(
            inner.x + MASCOT_WIDTH + 2,
            inner.y,
            inner.width.saturating_sub(MASCOT_WIDTH + 2),
            inner.height,
        )
    } else {
        inner
    };
    Paragraph::new(welcome_lines(app, text.width)).render(text, buffer);
}

#[cfg(test)]
#[path = "welcome_test.rs"]
mod tests;
