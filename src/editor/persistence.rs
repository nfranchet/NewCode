//! Settings writes, workspace selection, and restoration of saved file tabs.

use super::EditorApp;
use crate::settings::Settings;
use std::{fs, path::PathBuf};

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
    pub(super) fn save_settings(&self) {
        if let Some(parent) = self.settings_path.parent() {
            if let Err(error) = fs::create_dir_all(parent) {
                log::warn!("Settings directory creation failed kind={:?}", error.kind());
                return;
            }
        }
        let mut settings = self.settings.clone();
        settings.session_workspace = settings.workspace.clone();
        settings.open_files = self.open_files.clone();
        settings.active_file = self.active_file.clone();
        match toml::to_string_pretty(&settings) {
            Ok(text) => {
                if let Err(error) = fs::write(&self.settings_path, text) {
                    log::warn!("Settings write failed kind={:?}", error.kind());
                }
            }
            Err(_) => log::warn!("Settings serialization failed"),
        }
    }

    pub(super) fn choose_workspace(&mut self) {
        if let Some(p) = rfd::FileDialog::new().pick_folder() {
            self.creation = None;
            log::info!("Workspace selected path={}", p.display());
            self.settings.workspace = Some(p.clone());
            self.status = format!("Workspace: {}", p.display());
            self.save_settings();
        }
    }
}
