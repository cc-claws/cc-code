use ratatui::crossterm::event::{Event, MouseButton, MouseEventKind};

// 限制一次 drain 的工作量，让持续输入期间 agent/shell 轮询也能获得执行机会。
const MAX_MOUSE_BATCH: usize = 128;

fn is_batchable(event: &Event) -> bool {
    matches!(event, Event::Mouse(mouse) if matches!(mouse.kind,
        MouseEventKind::ScrollUp | MouseEventKind::ScrollDown
        | MouseEventKind::Drag(MouseButton::Left)))
}

/// 批量消费已排队的鼠标事件，仅在整批处理后重绘。
/// 滚轮保留每一步（包括反向和命中区域变化）；连续拖拽仅保留最终坐标。
/// 首个非鼠标事件留到下一批，让释放鼠标前有机会刷新选区屏幕快照。
pub(super) fn collect_mouse_batch(
    first: Event,
    pending: &mut Option<Event>,
    mut next: impl FnMut() -> std::io::Result<Option<Event>>,
) -> std::io::Result<Vec<Event>> {
    let batchable = is_batchable(&first);
    let mut events = vec![first];
    if !batchable {
        return Ok(events);
    }
    for _ in 1..MAX_MOUSE_BATCH {
        let Some(event) = next()? else { break };
        if !is_batchable(&event) {
            *pending = Some(event);
            break;
        }
        let replace_drag = matches!((events.last(), &event),
            (Some(Event::Mouse(previous)), Event::Mouse(current))
            if previous.kind == MouseEventKind::Drag(MouseButton::Left)
                && current.kind == previous.kind
                && current.modifiers == previous.modifiers);
        if replace_drag {
            *events.last_mut().expect("批次至少包含初始事件") = event;
        } else {
            events.push(event);
        }
    }
    Ok(events)
}

#[cfg(test)]
#[path = "mouse_batch_test.rs"]
mod tests;
