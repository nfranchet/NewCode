//! Clipboard commands for the active document.
use super::EditorApp;
use eframe::egui::{
    self,
    text::{CCursor, CCursorRange},
};
impl EditorApp {
    pub(super) fn copy_selection(&mut self, ctx: &egui::Context) {
        let id = self.editor_id();
        let Some(range) = egui::TextEdit::load_state(ctx, id)
            .and_then(|state| state.cursor.char_range())
            .filter(|range| !range.is_empty())
        else {
            self.status = "Select text in the editor to copy".into();
            return;
        };
        let range = range.as_sorted_char_range();
        let copied: String = self
            .buffer
            .chars()
            .skip(range.start)
            .take(range.end - range.start)
            .collect();
        ctx.copy_text(copied);
        self.status = "Copied selection".into();
    }

    pub(super) fn paste_from_clipboard(&mut self, ctx: &egui::Context) {
        ctx.memory_mut(|memory| memory.request_focus(self.editor_id()));
        ctx.send_viewport_cmd(egui::ViewportCommand::RequestPaste);
        self.status = "Paste requested".into();
    }

    pub(super) fn select_all(&mut self, ctx: &egui::Context) {
        let id = self.editor_id();
        let mut state = egui::TextEdit::load_state(ctx, id).unwrap_or_default();
        state.cursor.set_char_range(Some(CCursorRange::two(
            CCursor::new(0),
            CCursor::new(self.buffer.chars().count()),
        )));
        egui::TextEdit::store_state(ctx, id, state);
        ctx.memory_mut(|memory| memory.request_focus(id));
    }
}
