//! Claim terminal keyboard input before egui navigation or editor shortcuts run.
use super::EditorApp;
use crate::terminal_input;
use eframe::egui::{self, Event};

pub(super) fn terminal_id() -> egui::Id {
    egui::Id::new("shell_terminal_surface")
}

impl EditorApp {
    pub(super) fn route_terminal_input(&mut self, ctx: &egui::Context, raw: &mut egui::RawInput) {
        if self.bottom_tab != super::logs::Tab::Terminal {
            return;
        }
        let id = terminal_id();
        let Some(terminal) = self.top.as_mut() else {
            return;
        };
        let mut owned = raw.focused && ctx.memory(|memory| memory.has_focus(id));
        if !owned {
            return;
        }
        let rect = ctx.data(|data| data.get_temp::<egui::Rect>(id.with("rect")));
        terminal.poll();
        // Process in order: keys before an outside click belong to the terminal;
        // keys after it must remain available to the clicked panel.
        let mut modifiers = raw.modifiers;
        raw.events.retain(|event| {
            if let Event::Key {
                modifiers: current, ..
            } = event
            {
                modifiers = *current;
            }
            match event {
                Event::PointerButton {
                    pos, pressed: true, ..
                } if !rect.is_some_and(|rect| rect.contains(*pos)) => owned = false,
                Event::WindowFocused(false) => owned = false,
                _ => {}
            }
            if owned && terminal_input::owns(event) {
                let bytes = terminal_input::encode(event, modifiers, terminal.input_modes);
                if !bytes.is_empty() {
                    terminal.send(&bytes);
                }
                false
            } else {
                true
            }
        });
    }
}
