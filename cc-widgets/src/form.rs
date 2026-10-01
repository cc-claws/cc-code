use std::{collections::HashMap, hash::Hash};

use crate::input_field::InputState;

/// 表单字段枚举 trait——由使用方实现
pub trait FormField: Copy + Eq + Hash + 'static {
    fn next(self) -> Self;
    fn prev(self) -> Self;
    fn label(self) -> &'static str;
}

pub struct FormState<F: FormField> {
    active: F,
    fields: HashMap<F, InputState>,
}

impl<F: FormField> FormState<F> {
    pub fn new(fields: impl Iterator<Item = F>) -> Self {
        let map: HashMap<F, InputState> = fields.map(|f| (f, InputState::new())).collect();
        let active = map.keys().copied().next().unwrap();
        Self {
            active,
            fields: map,
        }
    }

    pub fn with_active(fields: &[F], active: F) -> Self {
        let mut state = Self::new(fields.iter().copied());
        state.active = active;
        state
    }

    pub fn next_field(&mut self) {
        self.active = self.active.next();
    }

    pub fn prev_field(&mut self) {
        self.active = self.active.prev();
    }

    pub fn active_field(&self) -> F {
        self.active
    }

    pub fn set_active(&mut self, field: F) {
        self.active = field;
    }

    pub fn input(&self, field: F) -> &InputState {
        self.fields.get(&field).expect("FormState: field not found")
    }

    pub fn input_mut(&mut self, field: F) -> &mut InputState {
        self.fields
            .get_mut(&field)
            .expect("FormState: field not found")
    }

    pub fn active_input(&self) -> &InputState {
        self.input(self.active)
    }

    pub fn active_input_mut(&mut self) -> &mut InputState {
        self.input_mut(self.active)
    }

    pub fn handle_char(&mut self, c: char) {
        self.active_input_mut().insert(c);
    }
    pub fn handle_backspace(&mut self) {
        self.active_input_mut().backspace();
    }
    pub fn handle_delete(&mut self) {
        self.active_input_mut().delete();
    }
    pub fn handle_cursor_left(&mut self) {
        self.active_input_mut().cursor_left();
    }
    pub fn handle_cursor_right(&mut self) {
        self.active_input_mut().cursor_right();
    }
    pub fn handle_cursor_home(&mut self) {
        self.active_input_mut().cursor_home();
    }
    pub fn handle_cursor_end(&mut self) {
        self.active_input_mut().cursor_end();
    }
    pub fn handle_paste(&mut self, text: &str) {
        self.active_input_mut().paste(text);
    }
}

#[cfg(test)]
#[path = "form_test.rs"]
mod tests;
