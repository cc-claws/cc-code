use super::input_pump::StampedEvent;
use ratatui::crossterm::event::{Event, MouseButton, MouseEventKind};

// 限制一次 drain 的工作量，让持续输入期间 agent/shell 轮询也能获得执行机会。
const MAX_MOUSE_BATCH: usize = 128;

fn is_batchable(stamped: &StampedEvent) -> bool {
    matches!(&stamped.event, Event::Mouse(mouse) if matches!(mouse.kind,
        MouseEventKind::ScrollUp | MouseEventKind::ScrollDown
        | MouseEventKind::Drag(MouseButton::Left)))
}

/// 批量消费已排队的鼠标事件，仅在整批处理后重绘。
/// 滚轮保留每一步（包括反向和命中区域变化）；连续拖拽仅保留最终坐标。
/// 首个非鼠标事件留到下一批，让释放鼠标前有机会刷新选区屏幕快照。
pub(super) fn collect_mouse_batch(
    first: StampedEvent,
    pending: &mut Option<StampedEvent>,
    mut next: impl FnMut() -> std::io::Result<Option<StampedEvent>>,
) -> std::io::Result<Vec<StampedEvent>> {
    let batchable = is_batchable(&first);
    let mut events = vec![first];
    if !batchable {
        return Ok(events);
    }
    for _ in 1..MAX_MOUSE_BATCH {
        let Some(stamped) = next()? else { break };
        if !is_batchable(&stamped) {
            *pending = Some(stamped);
            break;
        }
        let replace_drag = matches!((events.last().map(|s| &s.event), &stamped.event),
            (Some(Event::Mouse(previous)), Event::Mouse(current))
            if previous.kind == MouseEventKind::Drag(MouseButton::Left)
                && current.kind == previous.kind
                && current.modifiers == previous.modifiers);
        if replace_drag {
            *events.last_mut().expect("批次至少包含初始事件") = stamped;
        } else {
            events.push(stamped);
        }
    }
    Ok(events)
}

#[cfg(test)]
#[path = "mouse_batch_test.rs"]
mod tests;
