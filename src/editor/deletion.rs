//! Confirmed deletion, filesystem removal, and affected-tab cleanup.

use super::EditorApp;
use crate::ui_theme::{self as theme, Icon};
use eframe::egui::{self, Color32};
use std::{
    fs,
    path::{Path, PathBuf},
};

pub(super) struct Deletion {
    pub(super) path: PathBuf,
    pub(super) error: String,
    pub(super) needs_focus: bool,
}

impl EditorApp {
    pub(super) fn confirm_delete(&mut self, path: &Path) -> Result<(), String> {
        log::debug!("Delete confirmed path={}", path.display());
        let metadata = fs::symlink_metadata(path).map_err(|e| format!("Delete failed: {e}"))?;
        let directory = metadata.is_dir();
        let result = if directory {
            fs::remove_dir_all(path)
        } else {
            fs::remove_file(path)
        };
        result.map_err(|e| {
            log::warn!("Filesystem deletion failed kind={:?}", e.kind());
            format!("Delete failed: {e}")
        })?;
        let affected = |p: &Path| p == path || (directory && p.starts_with(path));
        self.open_files.retain(|p| !affected(p));
        self.unsaved_files.retain(|p, _| !affected(p));
        self.document_ids.retain(|p, _| !affected(p));
        if self.preview_file.as_deref().is_some_and(affected) {
            self.preview_file = None;
        }
        if self.active_file.as_deref().is_some_and(affected) {
            self.active_file = None;
            self.buffer.clear();
            self.dirty = false;
            self.pending_editor_cursor = None;
            self.find_open = false;
            self.find_active = None;
            if let Some(next) = self.open_files.first().cloned() {
                self.activate_file(next);
            }
        }
        if self.close_confirmation.as_deref().is_some_and(affected) {
            self.close_confirmation = None;
        }
        if self
            .creation
            .as_ref()
            .is_some_and(|entry| affected(&entry.parent))
        {
            self.creation = None;
        }
        self.status = format!("Deleted {}", path.display());
        log::info!("Deleted entry path={}", path.display());
        Ok(())
    }

    pub(super) fn deletion_has_unsaved(&self, path: &Path) -> bool {
        let directory = fs::symlink_metadata(path).is_ok_and(|metadata| metadata.is_dir());
        self.open_files.iter().any(|open| {
            self.file_is_dirty(open) && (open == path || (directory && open.starts_with(path)))
        })
    }

    pub(super) fn deletion_dialog(&mut self, ctx: &egui::Context) {
        let Some(mut deletion) = self.deletion.take() else {
            return;
        };
        let mut cancel = false;
        let mut confirm = false;
        let modal = egui::Modal::new(egui::Id::new("delete_confirmation")).show(ctx, |ui| {
            ui.set_width(380.0_f32.min(ctx.viewport_rect().width() - 40.0));
            theme::heading(ui, Icon::Warning, "Delete this entry?", theme::RED);
            ui.label(
                egui::RichText::new(
                    deletion
                        .path
                        .file_name()
                        .unwrap_or_default()
                        .to_string_lossy(),
                )
                .strong(),
            );
            ui.add(egui::Label::new(deletion.path.display().to_string()).wrap());
            ui.label("This permanently deletes the entry from disk.");
            if fs::symlink_metadata(&deletion.path).is_ok_and(|m| m.is_dir()) {
                ui.colored_label(
                    theme::RED,
                    "All files and subfolders inside will also be deleted.",
                );
            }
            if self.deletion_has_unsaved(&deletion.path) {
                ui.colored_label(
                    theme::AMBER,
                    "Unsaved edits in affected files will be discarded.",
                );
            }
            if !deletion.error.is_empty() {
                ui.colored_label(theme::RED, &deletion.error);
            }
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                let response = ui.button("Cancel");
                if deletion.needs_focus {
                    response.request_focus();
                    deletion.needs_focus = false;
                }
                cancel = response.clicked();
                let response = ui
                    .add(
                        egui::Button::new(egui::RichText::new("Delete").color(Color32::WHITE))
                            .fill(theme::RED),
                    )
                    .on_hover_text("Permanently delete this entry");
                // Require an explicit pointer click for the destructive action.
                confirm = response.clicked()
                    && ui.input(|i| i.pointer.button_released(egui::PointerButton::Primary));
            });
        });
        if cancel || modal.should_close() {
            return;
        }
        if confirm {
            match self.confirm_delete(&deletion.path) {
                Ok(()) => return,
                Err(error) => deletion.error = error,
            }
        }
        self.deletion = Some(deletion);
    }
}
