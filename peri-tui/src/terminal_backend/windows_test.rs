use std::io;

use ratatui::{
    crossterm::{execute, terminal::EnterAlternateScreen},
    layout::Rect,
    widgets::Paragraph,
    Terminal, TerminalOptions, Viewport,
};
use windows_sys::Win32::System::Console::{
    ReadConsoleOutputW, SetConsoleOutputCP, SetCurrentConsoleFontEx, CHAR_INFO, SMALL_RECT,
};

use super::*;
use crate::terminal_backend::TuiBackend;

fn read_row(y: i16) -> io::Result<String> {
    let mut cells: [CHAR_INFO; 64] = unsafe { zeroed() };
    let mut region = SMALL_RECT {
        Left: 0,
        Top: y,
        Right: 63,
        Bottom: y,
    };
    // 仅在测试专属控制台读取物理单元格，保留每一列，避免隐藏右侧残影。
    let ok = unsafe {
        ReadConsoleOutputW(
            GetStdHandle(STD_OUTPUT_HANDLE),
            cells.as_mut_ptr(),
            COORD { X: 64, Y: 1 },
            COORD { X: 0, Y: 0 },
            &mut region,
        )
    };
    if ok == 0 {
        return Err(io::Error::last_os_error());
    }
    let chars: Vec<u16> = cells
        .iter()
        .map(|cell| unsafe { cell.Char.UnicodeChar })
        .collect();
    Ok(String::from_utf16_lossy(&chars))
}

fn set_font(name: &str) -> io::Result<()> {
    // 调用者必须通过 CREATE_NEW_CONSOLE 启动专属测试进程；绝不用于用户活动窗口。
    unsafe {
        let handle = GetStdHandle(STD_OUTPUT_HANDLE);
        let mut font: CONSOLE_FONT_INFOEX = zeroed();
        font.cbSize = size_of::<CONSOLE_FONT_INFOEX>() as u32;
        if GetCurrentConsoleFontEx(handle, 0, &mut font) == 0 {
            return Err(io::Error::last_os_error());
        }
        font.FaceName = [0; 32];
        for (slot, ch) in font.FaceName.iter_mut().zip(name.encode_utf16()) {
            *slot = ch;
        }
        font.FontFamily = 0;
        font.dwFontSize = COORD { X: 0, Y: 16 };
        if SetCurrentConsoleFontEx(handle, 0, &font) == 0 {
            return Err(io::Error::last_os_error());
        }
    }
    Ok(())
}

#[test]
#[ignore = "需由 CREATE_NEW_CONSOLE 隐藏子进程显式运行，不能修改用户活动控制台字体"]
fn test_windows_backend_real_console_ghosting() -> io::Result<()> {
    assert_eq!(
        std::env::var("PERI_ISOLATED_CONSOLE_TEST").as_deref(),
        Ok("1")
    );
    let evidence_path = std::env::var("PERI_CONSOLE_TEST_RESULT").expect("指定探针证据文件");
    execute!(io::stdout(), EnterAlternateScreen)?;
    let mut cases = Vec::new();
    for font in ["新宋体", "Consolas"] {
        let backend = TuiBackend::new(io::stdout());
        let mut terminal = Terminal::with_options(
            backend,
            TerminalOptions {
                viewport: Viewport::Fixed(Rect::new(0, 0, 64, 5)),
            },
        )?;
        for code_page in [936, 65001] {
            assert_ne!(
                unsafe { SetConsoleOutputCP(code_page) },
                0,
                "设置专属控制台代码页"
            );
            set_font(font)?;
            if terminal.backend_mut().refresh_widths() {
                terminal.clear()?;
            }
            let mut probe = ConsoleWidthProbe::new();
            let actual = probe.width("∴").expect("测量实际歧义字符列宽");
            let bullet_width = probe.width("●").expect("测量回复前缀实际列宽");
            let fingerprint = ConsoleFingerprint::read().expect("读取实际字体");
            let actual_font = String::from_utf16_lossy(&fingerprint.face)
                .trim_end_matches('\0')
                .to_owned();
            // 同一缓冲区反复测量，确保旧的双列内容不污染下一次测量。
            for _ in 0..4 {
                assert_eq!(probe.width("中"), Some(2));
                assert_eq!(probe.width("∴"), Some(actual));
                assert_eq!(probe.width("A"), Some(1));
            }
            terminal.clear()?;
            terminal.draw(|f| {
                f.render_widget(
                    Paragraph::new("∴ Thought for 57 chars (ctrl+o to expand)"),
                    f.area(),
                );
            })?;
            let long_row = read_row(0)?;
            let expected = if actual == 1 { "∴" } else { "." };
            assert_eq!(
                long_row.trim_end(),
                format!("{expected} Thought for 57 chars (ctrl+o to expand)")
            );
            for i in 0..24 {
                terminal.draw(|f| {
                    let text = if i % 2 == 0 {
                        "● “answer”—正文\n∴ more text"
                    } else {
                        "● 2"
                    };
                    f.render_widget(Paragraph::new(text), f.area());
                })?;
            }
            let short_row = read_row(0)?;
            let empty_row = read_row(1)?;
            let bullet = if probe.width("●") == Some(1) {
                "●"
            } else {
                "*"
            };
            assert_eq!(
                short_row.trim_end(),
                format!("{bullet} 2"),
                "旧行尾必须完整擦除"
            );
            assert!(
                empty_row.chars().all(|ch| ch == ' '),
                "旧第二行必须完整擦除"
            );
            // 复用后端切换字体，验证真实指纹变化和缓存失效。
            // CP936 下系统可能拒绝 Consolas 并保留新宋体，切 UTF-8 后再切换。
            let other_font = if actual_font == "新宋体" {
                "Consolas"
            } else {
                "新宋体"
            };
            assert_ne!(unsafe { SetConsoleOutputCP(65001) }, 0);
            set_font(other_font)?;
            assert!(
                terminal.backend_mut().refresh_widths(),
                "字体变化必须触发重绘"
            );
            terminal.clear()?;
            terminal.draw(|f| f.render_widget(Paragraph::new("∴ end"), f.area()))?;
            let mut changed_probe = ConsoleWidthProbe::new();
            let changed_width = changed_probe.width("∴").expect("测量新字体");
            let changed_prefix = if changed_width == 1 { "∴" } else { "." };
            assert_eq!(read_row(0)?.trim_end(), format!("{changed_prefix} end"));
            cases.push(serde_json::json!({
                "requested_font": font, "actual_font": actual_font,
                "code_page": code_page, "actual_width": actual,
                "bullet_width": bullet_width,
                "long_row": long_row, "short_row": short_row,
                "empty_row": empty_row, "frames": 24, "font_change_verified": true,
            }));
            std::fs::write(&evidence_path, serde_json::to_string_pretty(&cases)?)?;
        }
    }
    assert!(
        cases.iter().any(|case| case["actual_width"] == 2),
        "必须在真实双列控制台复现前置条件"
    );
    assert!(
        cases.iter().any(|case| case["bullet_width"] == 1),
        "必须覆盖正常单列符号，不能全局切双列模式"
    );
    Ok(())
}
