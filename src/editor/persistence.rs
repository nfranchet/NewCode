//! Settings writes, workspace selection, and restoration of saved file tabs.

use super::EditorApp;
use crate::settings::Settings;
use std::path::PathBuf;

pub(super) fn restore_open_files(settings: &Settings) -> (Vec<PathBuf>, Option<PathBuf>, String) {
    let same_workspace = match (&settings.workspace, &settings.session_workspace) {
        (Some(current), Some(saved)) => {
            current == saved
                || std::fs::canonicalize(current)
                    .ok()
                    .zip(std::fs::canonicalize(saved).ok())
                    .is_some_and(|(a, b)| a == b)
        }
        (None, None) => true,
        _ => false,
    };
    if !same_workspace {
        return (Vec::new(), None, String::new());
    }
    let mut files = Vec::new();
    let mut contents = std::collections::HashMap::new();
    for path in &settings.open_files {
        if contents.contains_key(path) {
            continue;
        }
        if let Ok(text) = std::fs::read_to_string(path) {
            files.push(path.clone());
            contents.insert(path.clone(), text);
        }
    }
    let active = settings
        .active_file
        .as_ref()
        .filter(|p| contents.contains_key(*p))
        .cloned()
        .or_else(|| files.first().cloned());
    let buffer = active
        .as_ref()
        .and_then(|p| contents.remove(p))
        .unwrap_or_default();
    (files, active, buffer)
}

impl EditorApp {
    pub(crate) fn save_settings(&self) -> bool {
        if self.settings.workspace.is_none() {
            return true;
        }
        let mut settings = self.settings.clone();
        settings.session_workspace = settings.workspace.clone();
        settings.open_files = self.open_files.clone();
        settings.active_file = self.active_file.clone();
        match crate::settings::write_workspace(&self.settings_path, &settings) {
            Ok(()) => true,
            Err(error) => {
                log::warn!("Workspace settings write failed: {error}");
                false
            }
        }
    }

    pub(super) fn choose_workspace(&mut self) {
        if let Some(path) = rfd::FileDialog::new().pick_folder() {
            self.request_workspace(path);
        }
    }

    pub(super) fn request_workspace(&mut self, path: PathBuf) {
        let Ok(path) = path.canonicalize() else {
            self.status = "Cannot open workspace folder".into();
            return;
        };
        if self
            .settings
            .workspace
            .as_deref()
            .and_then(|p| p.canonicalize().ok())
            .as_ref()
            == Some(&path)
        {
            return;
        }
        if self.addons.worktree_busy() || self.addons.manager_busy() {
            self.status = "Wait for the add-on operation before switching workspaces".into();
            return;
        }
        self.pending_workspace = Some(path);
    }

    pub(super) fn workspace_switch_dialog(&mut self, ctx: &eframe::egui::Context) {
        if self.pending_workspace.is_none() {
            return;
        }
        use super::close_dialog::CloseChoice;
        if !self.open_files.iter().any(|p| self.file_is_dirty(p)) {
            self.resolve_workspace_switch(CloseChoice::Discard, ctx);
            return;
        }
        let mut choice = None;
        eframe::egui::Modal::new(eframe::egui::Id::new("switch_workspace")).show(ctx, |ui| {
            ui.heading("Unsaved changes");
            ui.label("Save changes before switching workspaces?");
            ui.horizontal(|ui| {
                for (label, action) in [
                    ("Save All", CloseChoice::Save),
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
            self.resolve_workspace_switch(choice, ctx);
        }
    }

    pub(super) fn resolve_workspace_switch(
        &mut self,
        choice: super::close_dialog::CloseChoice,
        ctx: &eframe::egui::Context,
    ) {
        use super::close_dialog::CloseChoice;
        if matches!(choice, CloseChoice::Cancel) {
            self.pending_workspace = None;
            return;
        }
        if matches!(choice, CloseChoice::Save) {
            let dirty: Vec<_> = self
                .open_files
                .iter()
                .filter(|p| self.file_is_dirty(p))
                .cloned()
                .collect();
            if !dirty.iter().all(|p| self.save_document(p)) {
                return;
            }
        }
        if let Some(path) = self.pending_workspace.take() {
            self.switch_workspace(path, ctx);
        }
    }

    pub(super) fn switch_workspace(&mut self, path: PathBuf, ctx: &eframe::egui::Context) {
        if self.addons.worktree_busy() || self.addons.manager_busy() {
            self.status = "Wait for the add-on operation before switching workspaces".into();
            return;
        }
        // Read the destination before touching the current editor.
        let (settings, settings_path) = match crate::settings::load_workspace(Some(&path)) {
            Ok(loaded) => loaded,
            Err(error) => {
                self.status = error;
                return;
            }
        };
        if !self.save_settings() {
            self.status = "Could not save current workspace settings; switch cancelled".into();
            return;
        }
        let suppressed = self.addons.suppressed.clone();
        self.addons = crate::addons::Addons::default();
        self.addons.suppressed = suppressed;
        self.addons
            .set_workspace_storage(settings.workspace.as_deref());
        self.addons.discover(&settings.addon_enabled);
        self.addons
            .apply_settings(&settings.addon_enabled, &settings.addon_state);
        self.top = None;
        self.terminal_error = None;
        self.dev_container = Default::default();
        self.git_ui = Default::default();
        self.settings = settings;
        self.settings_path = settings_path;
        let (files, active, buffer) = restore_open_files(&self.settings);
        self.open_files = files;
        self.active_file = active;
        self.buffer = buffer;
        self.dirty = false;
        self.unsaved_files.clear();
        self.document_ids.clear();
        for path in &self.open_files {
            self.next_document_id += 1;
            self.document_ids.insert(
                path.clone(),
                eframe::egui::Id::new(("document_editor", self.next_document_id)),
            );
        }
        self.preview_file = None;
        self.creation = None;
        self.deletion = None;
        self.rename_target = None;
        self.rename_text.clear();
        self.search_query.clear();
        self.search_results.clear();
        self.find_open = false;
        self.find_query.clear();
        self.find_query_snapshot.clear();
        self.find_active = None;
        self.pending_editor_cursor = None;
        self.close_confirmation = None;
        self.close_queue.clear();
        self.open_file_filter.clear();
        self.sidebar_mode = super::SidebarMode::Explorer;
        self.bottom_tab = Default::default();
        self.logs = Default::default();
        self.reveal_active_tab = true;
        *self.editor_layout_cache.lock().unwrap() = Default::default();
        // Clear panel sizes, scroll offsets, and focus belonging to the old workspace.
        ctx.memory_mut(|memory| *memory = Default::default());
        crate::ui_theme::apply_theme(ctx, &crate::ui_theme::nord_light());
        let cwd = self.settings.workspace.clone();
        self.restart_terminal(ctx, cwd.as_deref(), None);
        self.status = format!("Workspace: {}", path.display());
    }
}
