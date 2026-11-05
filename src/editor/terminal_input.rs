//! Application focus and clipboard policy; the terminal widget encodes all other input.
use super::EditorApp;
use eframe::egui::{self, Event};

pub(super) fn terminal_id() -> egui::Id {
    egui::Id::new("shell_terminal_surface")
}
pub(super) fn focused(ctx: &egui::Context) -> bool {
    ctx.data(|data| data.get_temp::<egui::Id>(terminal_id().with("widget")))
        .is_some_and(|id| ctx.memory(|memory| memory.has_focus(id)))
}
impl EditorApp {
    pub(super) fn route_terminal_input(&mut self, ctx: &egui::Context, raw: &mut egui::RawInput) {
        if self.bottom_tab != super::logs::Tab::Terminal || !raw.focused {
            return;
        }
        let id = terminal_id();
        let rect = ctx.data(|data| data.get_temp::<egui::Rect>(id.with("rect")));
        let widget_id = ctx.data(|data| data.get_temp::<egui::Id>(id.with("widget")));
        // Take focus on press so egui's focus filter is established before the first Tab.
        if raw.events.iter().any(|event| {
            matches!(event,
            Event::PointerButton { pos, button: egui::PointerButton::Primary, pressed: true, .. }
            if rect.is_some_and(|rect| rect.contains(*pos)))
        }) {
            if let Some(id) = widget_id {
                ctx.memory_mut(|memory| memory.request_focus(id));
            }
        }
        if !focused(ctx) {
            return;
        }
        let Some(terminal) = self.top.as_mut() else {
            return;
        };
        let mut owned = true;
        let mut modifiers = ctx.input(|input| input.modifiers);
        raw.events.retain_mut(|event| {
            if let Event::Key {
                modifiers: current, ..
            }
            | Event::ModifiersChanged(current) = event
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
            if !owned {
                return true;
            }
            match event {
                // Preserve the app's existing Ctrl+C interrupt, even with a selection.
                Event::Copy if !modifiers.shift && !cfg!(target_os = "macos") => {
                    *event = control_key(egui::Key::C);
                    true
                }
                Event::Cut if !modifiers.shift && !cfg!(target_os = "macos") => {
                    *event = control_key(egui::Key::X);
                    true
                }
                Event::Key {
                    key: egui::Key::C,
                    pressed: true,
                    modifiers,
                    ..
                } if modifiers.ctrl && modifiers.shift => {
                    if let Some(text) = terminal.view.selected_text() {
                        ctx.copy_text(text);
                    }
                    false
                }
                Event::Key {
                    key: egui::Key::V,
                    pressed: true,
                    modifiers,
                    ..
                } if modifiers.ctrl && modifiers.shift => {
                    ctx.send_viewport_cmd(egui::ViewportCommand::RequestPaste);
                    false
                }
                _ => true,
            }
        });
    }
}

fn control_key(key: egui::Key) -> Event {
    Event::Key {
        key,
        physical_key: Some(key),
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::CTRL,
    }
}
