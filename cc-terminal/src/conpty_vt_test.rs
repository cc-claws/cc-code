use std::{io, mem::zeroed, ptr};

use ratatui::{
    crossterm::{execute, terminal::EnterAlternateScreen},
    layout::Rect,
    style::{Color, Style},
    widgets::Paragraph,
    Terminal, TerminalOptions, Viewport,
};
use windows_sys::Win32::System::Console::{
    GetConsoleMode, GetStdHandle, ReadConsoleOutputW, SetConsoleCursorPosition, SetConsoleMode,
    SetConsoleOutputCP, WriteConsoleW, CHAR_INFO, COORD, ENABLE_PROCESSED_OUTPUT,
    ENABLE_VIRTUAL_TERMINAL_PROCESSING, ENABLE_WRAP_AT_EOL_OUTPUT, SMALL_RECT, STD_OUTPUT_HANDLE,
};

use super::{enable_vt_processing, take_vt_mode_recovery};
use crate::terminal_backend::TuiBackend;

fn read_console_row(y: i16) -> io::Result<String> {
    // SAFETY: 测试运行在专属隐藏控制台；输出数组和区域大小均为 64 列。
    unsafe {
        let mut cells: [CHAR_INFO; 64] = zeroed();
        let mut region = SMALL_RECT {
            Left: 0,
            Top: y,
            Right: 63,
            Bottom: y,
        };
        if ReadConsoleOutputW(
            GetStdHandle(STD_OUTPUT_HANDLE),
            cells.as_mut_ptr(),
            COORD { X: 64, Y: 1 },
            COORD { X: 0, Y: 0 },
            &mut region,
        ) == 0
        {
            return Err(io::Error::last_os_error());
        }
        let wide: Vec<_> = cells.iter().map(|cell| cell.Char.UnicodeChar).collect();
        Ok(String::from_utf16_lossy(&wide))
    }
}

#[test]
#[ignore = "需由 scripts/test-cmd-vt.ps1 在独立隐藏控制台运行"]
fn test_windows_console_vt_recovery() -> anyhow::Result<()> {
    anyhow::ensure!(std::env::var("PERI_ISOLATED_CONSOLE_TEST").as_deref() == Ok("1"));
    let evidence = std::env::var("PERI_CONSOLE_TEST_RESULT")?;
    enable_vt_processing()?;
    execute!(io::stdout(), EnterAlternateScreen)?;
    let mut terminal = Terminal::with_options(
        TuiBackend::new(io::stdout()),
        TerminalOptions {
            viewport: Viewport::Fixed(Rect::new(0, 0, 64, 4)),
        },
    )?;
    let output = unsafe { GetStdHandle(STD_OUTPUT_HANDLE) };
    let required = ENABLE_PROCESSED_OUTPUT | ENABLE_VIRTUAL_TERMINAL_PROCESSING;
    let mut cases = Vec::new();
    for code_page in [936, 65001] {
        assert_ne!(
            unsafe { SetConsoleOutputCP(code_page) },
            0,
            "设置专属控制台代码页"
        );
        enable_vt_processing()?;
        terminal.clear()?;
        // 先以旧路径复现：关闭 VT 后，颜色序列确实成为可见文本。
        assert_ne!(
            unsafe { SetConsoleMode(output, ENABLE_PROCESSED_OUTPUT | ENABLE_WRAP_AT_EOL_OUTPUT) },
            0
        );
        assert_ne!(
            unsafe { SetConsoleCursorPosition(output, COORD { X: 0, Y: 0 }) },
            0
        );
        let leaked: Vec<_> = "\x1b[38;2;78;186;101mCMD\x1b[0m".encode_utf16().collect();
        let mut written = 0;
        assert_ne!(
            unsafe {
                WriteConsoleW(
                    output,
                    leaked.as_ptr().cast(),
                    leaked.len() as u32,
                    &mut written,
                    ptr::null(),
                )
            },
            0
        );
        let broken_row = read_console_row(0)?;
        assert!(
            broken_row.contains("38;2;78;186;101m"),
            "必须先复现截图中的控制码明文泄露"
        );
        for mode in [
            0,
            ENABLE_PROCESSED_OUTPUT,
            ENABLE_VIRTUAL_TERMINAL_PROCESSING,
            required,
        ] {
            let mode = mode | ENABLE_WRAP_AT_EOL_OUTPUT;
            take_vt_mode_recovery();
            for frame in 0..24 {
                assert_ne!(unsafe { SetConsoleMode(output, mode) }, 0);
                if mode & required != required {
                    // 污染逻辑内容不变的第二行，证明恢复后仅做 diff 不足以修复屏幕。
                    assert_ne!(
                        unsafe { SetConsoleCursorPosition(output, COORD { X: 0, Y: 1 }) },
                        0
                    );
                    let garbage: Vec<_> = "BAD!".encode_utf16().collect();
                    assert_ne!(
                        unsafe {
                            WriteConsoleW(
                                output,
                                garbage.as_ptr().cast(),
                                garbage.len() as u32,
                                &mut written,
                                ptr::null(),
                            )
                        },
                        0
                    );
                }
                enable_vt_processing()?;
                let mut restored = 0;
                assert_ne!(unsafe { GetConsoleMode(output, &mut restored) }, 0);
                assert_eq!(restored, mode | required, "只补必要模式位，保留原有设置");
                // 模拟标题/鼠标先恢复，绘制前再次检查，恢复信号仍不能丢失。
                enable_vt_processing()?;
                let recovered = take_vt_mode_recovery();
                assert_eq!(recovered, mode & required != required);
                assert!(!take_vt_mode_recovery(), "恢复标记只消费一次");
                if recovered {
                    terminal.clear()?;
                }
                let text = format!("CMD frame {frame}");
                let contents = format!("{text}\nCMD static line");
                terminal.draw(|f| {
                    f.render_widget(
                        Paragraph::new(contents.as_str())
                            .style(Style::default().fg(Color::Rgb(78, 186, 101))),
                        f.area(),
                    )
                })?;
                assert_eq!(
                    read_console_row(0)?.trim_end(),
                    text,
                    "颜色和光标控制序列不能泄露到物理屏幕"
                );
                assert_eq!(
                    read_console_row(1)?.trim_end(),
                    "CMD static line",
                    "逻辑内容不变的行也必须完整恢复"
                );
            }
            cases.push(serde_json::json!({
                "code_page": code_page, "disabled_mode": mode,
                "restored_mode": mode | required, "frames": 24,
                "baseline_row": broken_row, "recovered_row": read_console_row(0)?,
                "static_row": read_console_row(1)?,
            }));
            std::fs::write(&evidence, serde_json::to_string_pretty(&cases)?)?;
        }
    }
    Ok(())
}
