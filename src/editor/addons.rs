use super::EditorApp;
use eframe::egui;

impl EditorApp {
    pub(super) fn update_addons(&mut self, ctx: &egui::Context) {
        self.addons.manager_tick(ctx, &self.settings.addon_enabled);
        self.addons.sync(
            self.settings.workspace.as_deref(),
            self.dev_container.runtime(),
        );
        if self.addons.tick(
            ctx,
            self.dirty || !self.unsaved_files.is_empty(),
            self.sidebar_mode == super::SidebarMode::Addon,
        ) {
            self.git_ui.external_change();
            // Checkout changes disk contents. Retain every unsaved editor buffer.
            let removed: Vec<_> = self
                .open_files
                .iter()
                .filter(|p| !p.is_file() && !self.file_is_dirty(p))
                .cloned()
                .collect();
            for path in removed {
                self.finish_close_file(path);
            }
            if !self.dirty {
                if let Some(path) = &self.active_file {
                    match std::fs::read_to_string(path) {
                        Ok(text) => self.buffer = text,
                        Err(error) => {
                            self.status =
                                format!("Could not reload file after Git operation: {error}")
                        }
                    }
                }
            }
        }
    }
    pub(super) fn addon_dialogs(&mut self, ctx: &egui::Context) {
        self.addons.credential_dialog(ctx);
        self.addons.manager_dialog(ctx);
        if !self.addons.checkout_waiting() {
            return;
        }
        let mut save = false;
        let mut cancel = false;
        egui::Window::new("Save files before changing branches")
            .collapsible(false).resizable(false).anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                ui.label("A Git addon operation needs to change the worktree. Save unsaved files before continuing.");
                ui.horizontal(|ui| { save = ui.button("Save all and continue").clicked(); cancel = ui.button("Cancel").clicked(); });
            });
        if cancel {
            self.addons.confirm_checkout(false, ctx);
        }
        if save {
            let paths: Vec<_> = self
                .open_files
                .iter()
                .filter(|p| self.file_is_dirty(p))
                .cloned()
                .collect();
            if paths.iter().all(|path| self.save_document(path)) {
                self.addons.confirm_checkout(true, ctx);
            }
        }
    }
}
