//! Save, discard, or cancel closing any modified tab.
use super::EditorApp;
use eframe::egui;
use std::path::PathBuf;

pub(super) enum CloseTabs {
    File(PathBuf),
    Others(PathBuf),
    All,
}

#[derive(Clone, Copy)]
pub(super) enum CloseChoice {
    Save,
    Discard,
    Cancel,
}

impl EditorApp {
    pub(super) fn close_tabs(&mut self, action: CloseTabs) {
        if self.close_confirmation.is_some() {
            return;
        }
        let paths = match action {
            CloseTabs::File(path) => vec![path],
            CloseTabs::Others(keep) => {
                if !self.open_files.contains(&keep) {
                    return;
                }
                self.activate_file(keep.clone());
                self.open_files
                    .iter()
                    .filter(|path| **path != keep)
                    .cloned()
                    .collect()
            }
            CloseTabs::All => self.open_files.clone(),
        };
        self.close_queue = paths.into();
        self.advance_close_queue();
    }

    fn advance_close_queue(&mut self) {
        while let Some(path) = self.close_queue.pop_front() {
            if !self.open_files.contains(&path) {
                continue;
            }
            if self.file_is_dirty(&path) {
                self.close_confirmation = Some(path);
                break;
            }
            self.finish_close_file(path);
        }
    }

    pub(super) fn resolve_close(&mut self, choice: CloseChoice) {
        let Some(path) = self.close_confirmation.clone() else {
            return;
        };
        match choice {
            CloseChoice::Save if !self.save_document(&path) => return,
            CloseChoice::Cancel => {
                self.close_confirmation = None;
                self.close_queue.clear();
                return;
            }
            _ => {}
        }
        self.finish_close_file(path);
        self.advance_close_queue();
    }

    pub(super) fn close_dialog(&mut self, ctx: &egui::Context) {
        let Some(path) = self.close_confirmation.clone() else {
            return;
        };
        let mut choice = None;
        egui::Modal::new(egui::Id::new("close_unsaved_file")).show(ctx, |ui| {
            ui.heading("Unsaved changes");
            ui.label(format!(
                "Save changes to {} before closing?",
                path.display()
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
