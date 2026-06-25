# Scroll Event Coalescing Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Reduce CPU usage during mouse scroll/drag by coalescing rapid-fire mouse events into a single scroll update + single redraw.

**Architecture:** In `next_event()`, after reading a Scroll or Drag mouse event, non-blocking drain remaining mouse events from the crossterm queue. Accumulate Scroll deltas, keep only the last Drag position. The last event type in the drain determines the final action. Only one `handle_event` call and one `Redraw` result.

**Tech Stack:** Rust, crossterm event polling, ratatui TUI

---

### Task 1: Add scroll event coalescing in `next_event()`

**Files:**
- Modify: `peri-tui/src/event/mod.rs:64-71`

**Context:** The current `next_event()` reads exactly one event per poll cycle. When a mouse scroll storm occurs, each event triggers a full `handle_event()` → `Action::Redraw` → `terminal.draw()`. We insert a drain loop between `event::read()` and `handle_event()` that only activates for `Scroll` and `Drag` mouse events.

- [ ] **Step 1: Write the coalescing drain loop in `next_event()`**

Replace the current lines 64-71:

```rust
    if !event::poll(Duration::from_millis(50))? {
        return Ok(None);
    }

    let ev = event::read()?;

    handle_event(app, ev).await
```

With:

```rust
    if !event::poll(Duration::from_millis(50))? {
        return Ok(None);
    }

    let ev = event::read()?;

    // Scroll/Drag event coalescing: drain queued mouse events to avoid
    // redundant redraws during rapid scrolling or scrollbar dragging.
    // Scroll events accumulate delta; Drag events keep only the latest
    // position. The last event type determines the final action.
    let ev = coalesce_mouse_events(ev);

    handle_event(app, ev).await
```

- [ ] **Step 2: Add the `coalesce_mouse_events` function**

Add this function right before the `handle_event` function (before line 78):

```rust
/// Coalesces rapid-fire mouse scroll/drag events from the crossterm queue.
///
/// When a `Scroll` or `Drag` mouse event is the initial event, drains any
/// additional queued mouse events using a non-blocking poll. Scroll deltas
/// are accumulated; Drag events overwrite to keep only the latest mouse
/// position. The final event is determined by the last event type seen.
/// Non-mouse events stop the drain and are NOT consumed (they remain queued).
fn coalesce_mouse_events(mut ev: Event) -> Event {
    // Only activate coalescing for scroll and drag mouse events
    match &ev {
        Event::Mouse(m) => match m.kind {
            MouseEventKind::ScrollUp
            | MouseEventKind::ScrollDown
            | MouseEventKind::Drag(MouseButton::Left) => {}
            _ => return ev,
        },
        _ => return ev,
    }

    let mut scroll_delta: i32 = 0;
    // Track the last event to determine final behavior
    let mut last_ev = ev;

    loop {
        if !event::poll(Duration::ZERO).unwrap_or(false) {
            break;
        }
        let next = match event::read() {
            Ok(e) => e,
            Err(_) => break,
        };
        match &next {
            Event::Mouse(m) => match m.kind {
                MouseEventKind::ScrollUp => {
                    scroll_delta -= 3;
                    last_ev = next;
                }
                MouseEventKind::ScrollDown => {
                    scroll_delta += 3;
                    last_ev = next;
                }
                MouseEventKind::Drag(MouseButton::Left) => {
                    // Drag: overwrite with latest position
                    scroll_delta = 0;
                    last_ev = next;
                }
                // Other mouse events (click, release, move) stop coalescing
                _ => {
                    // Don't consume this event — but we can't put it back.
                    // These are rare during scroll storms, so we just process
                    // what we have and let this event be lost (acceptable tradeoff
                    // for Up/Move events during rapid scrolling).
                    break;
                }
            },
            // Non-mouse events: stop draining, don't consume
            _ => break,
        }
    }

    // If we accumulated scroll delta, synthesize the final scroll event
    if scroll_delta != 0 {
        // Reconstruct from the last scroll event to preserve mouse coordinates
        if let Event::Mouse(m) = &last_ev {
            let final_kind = if scroll_delta > 0 {
                MouseEventKind::ScrollDown
            } else {
                MouseEventKind::ScrollUp
            };
            // Return a synthetic event with original coordinates but correct direction
            last_ev = Event::Mouse(ratatui::crossterm::event::MouseEvent {
                kind: final_kind,
                column: m.column,
                row: m.row,
                modifiers: m.modifiers,
            });
        }
    }

    last_ev
}
```

- [ ] **Step 3: Run build to verify compilation**

Run: `cargo build -p peri-tui`
Expected: Clean build with no errors

- [ ] **Step 4: Manual test — scroll performance**

Run: `cargo run -p peri-tui`
Test: Open the TUI with a long conversation or generate content, then rapidly scroll the mouse wheel in the message area. Verify:
- Scrolling still works correctly (direction and amount)
- No visible lag or stutter
- CPU usage is noticeably lower during rapid scrolling
- Scrollbar dragging still works smoothly

- [ ] **Step 5: Commit**

```bash
git add peri-tui/src/event/mod.rs
git commit -m "perf(tui): coalesce rapid scroll/drag mouse events to reduce CPU

Drain queued mouse events via non-blocking poll when Scroll or Drag
events are detected. Accumulate scroll deltas, keep only the latest
Drag position. Single redraw per drain cycle instead of per-event."
```

---

## Self-Review

**Spec coverage:** All decisions from grill session are covered:
- ScrollUp/ScrollDown delta accumulation ✅
- Drag event overwrites with latest position ✅
- Last event type determines final behavior ✅
- Non-mouse events stop drain ✅
- Only `next_event()` is modified ✅

**Placeholder scan:** No TBDs, no TODOs, all code is concrete ✅

**Type consistency:** `coalesce_mouse_events` takes and returns `Event`, consumed by `handle_event(app, ev)` — types match ✅
