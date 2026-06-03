# Config Panel Instant-Save Design

**Date**: 2026-06-03
**Related Issue**: `spec/issues/2026-05-24-config-panel-interaction-redesign.md`
**Approach**: A — field-level save on every change

## Problem

Current config panel requires Enter to save all fields at once. User wants instant feedback — changes should persist immediately when toggling options or leaving a text field.

## Save Triggers

| Event | Action |
|-------|--------|
| Space/Left/Right on boolean/select field | Toggle + `apply_edit()` + `save_config()` immediately |
| Up/Down moving focus away from text field | `apply_edit()` + `save_config()` before moving cursor |
| Esc | If on text field, save first; then close panel |
| Enter | No-op (`EventResult::Consumed`) |

## Design

### 1. Extract `save_config_now()`

Pull the save logic out of the Enter branch into a reusable helper on the `PanelComponent` impl block. Called from multiple trigger points.

```rust
fn save_config_now(panel: &mut ConfigPanel, ctx: &mut PanelContext) {
    let Some(cfg) = ctx.services.peri_config.as_mut() else { return };
    if let Ok(()) = panel.apply_edit(cfg, &ctx.services.lc) {
        if let Some(ref lang) = cfg.config.language {
            let _ = ctx.services.lc.switch(lang);
        }
        let _ = App::save_config(cfg, ctx.services.config_path_override.as_deref());
    }
}
```

No system_note on save (too frequent). Silent persistence.

### 2. Helper: `is_text_row()`

```rust
fn is_text_row(row: usize) -> bool {
    matches!(row, ROW_THRESHOLD | ROW_PERSONA | ROW_TONE)
}
```

Used to decide whether blur-save is needed before Up/Down/Esc.

### 3. `handle_key` Changes

- **Space/Left/Right** on boolean/select rows: after toggling, call `save_config_now()`
- **Up/Down**: before moving cursor, if `is_text_row(self.cursor)` then `save_config_now()`, then `cursor_up/down()`
- **Esc**: if `is_text_row(self.cursor)` then `save_config_now()`, then `ClosePanel`
- **Enter**: `EventResult::Consumed` (no-op)

### 4. Mouse Click

In `handle_mouse`, when clicking a different row while currently on a text row, call `save_config_now()` before updating cursor.

## What Does NOT Change

- `apply_edit()` signature and logic
- `render_config_panel` rendering
- `PanelComponent` trait
- Test structure in `config_panel_test.rs`

## File Changes

| File | Change |
|------|--------|
| `peri-tui/src/app/config_panel.rs` | Extract `save_config_now()`, add `is_text_row()`, rewrite `handle_key` branches |
| `peri-tui/src/app/config_panel_test.rs` | Update existing Enter-save tests to blur-save tests |
