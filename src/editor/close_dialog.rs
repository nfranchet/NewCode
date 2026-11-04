//! Save, discard, or cancel closing any modified tab.
use super::EditorApp;
use eframe::egui;

#[derive(Clone, Copy)]
pub(super) enum CloseChoice {
    Save,
    Discard,
    Cancel,
}

impl EditorApp {
    pub(super) fn resolve_close(&mut self, choice: CloseChoice) {
        let Some(path) = self.close_confirmation.clone() else {
            return;
        };
        match choice {
            CloseChoice::Save if !self.save_document(&path) => return,
            CloseChoice::Cancel => {
                self.close_confirmation = None;
                return;
            }
            _ => {}
        }
        self.finish_close_file(path);
    }

    pub(super) fn close_dialog(&mut self, ctx: &egui::Context) {
        let Some(path) = self.close_confirmation.clone() else {
            return;
        };
        let mut choice = None;
        egui::Window::new("Unsaved changes")
            .collapsible(false)
            .resizable(false)
            .show(ctx, |ui| {
                ui.label(format!(
                    "Save changes to {} before closing?",
                    path.file_name().unwrap_or_default().to_string_lossy()
                ));
                ui.horizontal(|ui| {
                    for (label, action) in [
                        ("Save", CloseChoice::Save),
                        ("Discard", CloseChoice::Discard),
                        ("Cancel", CloseChoice::Cancel),
                    ] {
                        if ui.button(label).clicked() {
                            choice = Some(action);
                        }
                    }
                });
            });
        if let Some(choice) = choice {
            self.resolve_close(choice);
        }
    }
}
