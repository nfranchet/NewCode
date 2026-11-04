//! Inline entry drafts, filename validation, creation, and focus-based cancellation.

use super::{EditorApp, SidebarMode};
use crate::ui_theme::{self as theme, Icon};
use eframe::egui;
use std::{
    fs,
    path::{Path, PathBuf},
};

pub(super) struct NewEntry {
    pub(super) parent: PathBuf,
    pub(super) workspace: Option<PathBuf>,
    pub(super) is_dir: bool,
    pub(super) name: String,
    pub(super) error: String,
    pub(super) needs_focus: bool,
}

impl NewEntry {
    pub(super) fn id(&self) -> egui::Id {
        egui::Id::new(("explorer_new_entry", &self.parent, self.is_dir))
    }

    pub(super) fn create(&self) -> Result<PathBuf, String> {
        log::debug!(
            "Create filesystem entry requested directory={}",
            self.is_dir
        );
        if self.name.trim().is_empty()
            || matches!(self.name.as_str(), "." | "..")
            || self.name.contains(['/', '\\', '\0'])
        {
            return Err("Enter a single file or folder name.".into());
        }
        let path = self.parent.join(&self.name);
        let result = if self.is_dir {
            fs::create_dir(&path)
        } else {
            fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&path)
                .map(|_| ())
        };
        result.map(|_| path).map_err(|e| {
            log::warn!("Filesystem entry creation failed kind={:?}", e.kind());
            format!("Could not create entry: {e}")
        })
    }
}

impl EditorApp {
    pub(super) fn create_entry(&mut self, is_dir: bool) {
        log::debug!("Begin inline creation directory={is_dir}");
        let parent = self
            .active_file
            .as_deref()
            .and_then(Path::parent)
            .map(Path::to_path_buf)
            .or_else(|| self.settings.workspace.clone());
        if let Some(parent) = parent {
            let parent = if parent.as_os_str().is_empty() {
                std::env::current_dir().unwrap_or(parent)
            } else {
                parent
            };
            let parent = fs::canonicalize(&parent).unwrap_or(parent);
            self.creation = Some(NewEntry {
                parent,
                workspace: self.settings.workspace.clone(),
                is_dir,
                name: String::new(),
                error: String::new(),
                needs_focus: true,
            });
        }
    }

    pub(super) fn creation_row(&mut self, ui: &mut egui::Ui) {
        let Some(mut entry) = self.creation.take() else {
            return;
        };
        let id = entry.id();
        let response = ui
            .horizontal(|ui| {
                theme::icon(
                    ui,
                    if entry.is_dir {
                        Icon::Folder
                    } else {
                        Icon::File
                    },
                    theme::BLUE,
                );
                ui.add(
                    egui::TextEdit::singleline(&mut entry.name)
                        .id(id)
                        .hint_text(if entry.is_dir {
                            "Folder name…"
                        } else {
                            "File name…"
                        })
                        .desired_width(ui.available_width()),
                )
            })
            .inner;
        if entry.needs_focus {
            response.request_focus();
            ui.scroll_to_rect(response.rect, None);
            entry.needs_focus = false;
        }
        let owns_keyboard =
            response.has_focus() || response.lost_focus() || ui.memory(|m| m.has_focus(id));
        if owns_keyboard
            && ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape))
        {
            log::debug!("Inline creation cancelled");
            ui.memory_mut(|m| m.surrender_focus(id));
            return;
        }
        if owns_keyboard && ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Enter))
        {
            match entry.create() {
                Ok(path) => {
                    self.status = format!("Created {}", path.display());
                    ui.memory_mut(|m| m.surrender_focus(id));
                    return;
                }
                Err(error) => {
                    entry.error = error;
                    response.request_focus();
                }
            }
        } else if response.lost_focus() {
            log::debug!("Inline creation cancelled after focus change");
            return;
        }
        if !entry.error.is_empty() {
            ui.colored_label(theme::RED, &entry.error);
        }
        self.creation = Some(entry);
    }
    pub(super) fn cancel_creation_for_workspace_change(&mut self) {
        if self.creation.as_ref().is_some_and(|entry| {
            self.sidebar_mode != SidebarMode::Explorer || self.settings.workspace != entry.workspace
        }) {
            self.creation = None;
        }
    }
    pub(super) fn cancel_unfocused_creation(&mut self, ctx: &egui::Context) {
        if self.creation.as_ref().is_some_and(|entry| {
            !entry.needs_focus
                && (!ctx.memory(|m| m.has_focus(entry.id())) || !ctx.input(|i| i.focused))
        }) {
            self.creation = None;
        }
    }
}
