//! scripts/test-hover-conpty.py 启动专属 ConPTY，发送真实 SGR 输入。
use super::*;
use ratatui::crossterm::{
    event::{DisableMouseCapture, EnableMouseCapture},
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use std::io::Write;

#[test]
#[ignore = "必须由 scripts/test-hover-conpty.py 启动专属 ConPTY"]
fn test_hover_conpty_flood_does_not_become_keyboard_input() -> anyhow::Result<()> {
    anyhow::ensure!(std::env::var("PERI_HOVER_CONPTY_TEST").as_deref() == Ok("1"));
    let evidence = std::env::var("PERI_HOVER_TEST_RESULT")?;
    // Python/CI 父进程可能带有重定向句柄，明确绑定本子进程所属的伪控制台。
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::System::Console::{
        SetStdHandle, STD_ERROR_HANDLE, STD_INPUT_HANDLE, STD_OUTPUT_HANDLE,
    };
    let input = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open("CONIN$")?;
    let output = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open("CONOUT$")?;
    unsafe {
        anyhow::ensure!(SetStdHandle(STD_INPUT_HANDLE, input.as_raw_handle()) != 0);
        anyhow::ensure!(SetStdHandle(STD_OUTPUT_HANDLE, output.as_raw_handle()) != 0);
        anyhow::ensure!(SetStdHandle(STD_ERROR_HANDLE, output.as_raw_handle()) != 0);
    }
    enable_raw_mode()?;
    anyhow::ensure!(std::env::var("RUST_LOG_FILE")? == format!("{evidence}.log"));
    let _tracing = peri_agent::telemetry::init_tracing("hover-conpty-test");
    let pump = input_pump::InputPump::start()?;
    let mut stdout = std::io::stdout();
    ratatui::crossterm::execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    crate::conpty::enable_mouse_tracking()?;
    let hover_available = crate::conpty::hover_available();
    writeln!(stdout, "HOVER_READY:{}", u8::from(hover_available))?;
    stdout.flush()?;
    // 模拟 UI 停顿及流式绘制：消费端暂停，读取线程必须持续排空。
    std::thread::sleep(Duration::from_millis(500));
    for _ in 0..100 {
        stdout.write_all(b"\x1b[Hstreaming output while mouse input is queued\r\n")?;
        stdout.flush()?;
    }
    let deadline = std::time::Instant::now() + Duration::from_secs(15);
    let mut text = String::new();
    let mut moves = 0;
    let mut last_position = None;
    let mut buttons = Vec::new();
    let mut finished = false;
    while std::time::Instant::now() < deadline {
        match pump.next(Duration::from_millis(50))? {
            Some(Event::Mouse(mouse)) if mouse.kind == MouseEventKind::Moved => {
                moves += 1;
                last_position = Some((mouse.column, mouse.row));
            }
            Some(Event::Mouse(mouse)) => buttons.push(format!("{:?}", mouse.kind)),
            Some(Event::Key(key)) if key.kind != KeyEventKind::Release => match key.code {
                KeyCode::Char(c) => text.push(c),
                KeyCode::Enter => {
                    finished = true;
                    break;
                }
                _ => {}
            },
            _ => {}
        }
    }
    crate::conpty::disable_mouse_tracking()?;
    ratatui::crossterm::execute!(stdout, DisableMouseCapture, LeaveAlternateScreen)?;
    drop(pump);
    disable_raw_mode()?;
    std::fs::write(
        evidence,
        serde_json::to_string_pretty(&serde_json::json!({
            "keyboard_text": text, "delivered_moves": moves, "last_position": last_position,
            "finished": finished, "consumer_pause_ms": 500, "output_frames": 100,
            "hover_available": hover_available,
            "buttons": buttons,
        }))?,
    )?;
    assert!(finished, "必须收到结束按键");
    assert_eq!(text, "beforeafter", "鼠标 SGR 不能泄漏为键盘文字");
    let expected_buttons = ["Down(Left)", "Drag(Left)", "Up(Left)", "ScrollUp"];
    if hover_available {
        assert_eq!(
            buttons, expected_buttons,
            "兼容宿主必须保留点击、拖拽、释放和滚轮顺序"
        );
        assert!(moves > 0, "必须实际收到悬停事件");
        assert_eq!(last_position, Some((79, 9)), "最终鼠标位置必须保留");
    } else {
        // Win10 系统 ConPTY 连 SGR 按键也不产生 INPUT_RECORD（原始开关序列同样如此）。
        // 此分支只验证安全降级/键盘无乱码；空 buttons 不能算鼠标链路验证通过。
        assert!(
            buttons.is_empty() || buttons == expected_buttons,
            "若宿主支持按键，必须保持完整顺序"
        );
        assert_eq!(moves, 0, "旧宿主不能要求终端报告悬停");
    }
    Ok(())
}
