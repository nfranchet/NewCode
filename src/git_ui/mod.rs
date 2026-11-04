//! Source Control state and public panel entry points.

use crate::dev_container::Runtime;
use crate::ui_theme::{self as theme, Icon};
use actions::Reply;
use eframe::egui;
use presentation::configure_input_style;
use repository::Snapshot;
use std::{
    path::{Path, PathBuf},
    sync::mpsc,
    time::Instant,
};
mod actions;
mod diff;
mod diff_rows;
mod fingerprint;
mod panel;
mod patch;
mod patch_model;
mod presentation;
mod repository;
mod review;
#[cfg(test)]
mod tests;

#[derive(Default)]
pub(crate) struct GitUi {
    workspace: Option<PathBuf>,
    snapshot: Option<Snapshot>,
    pending: Option<mpsc::Receiver<Reply>>,
    refreshed: Option<Instant>,
    message: String,
    commit: String,
    diff: Option<review::DiffView>,
    review_generation: u64,
    diff_target: Option<(String, bool)>,
    diff_hidden: bool,
    runtime: Option<Runtime>,
}

impl GitUi {
    pub(crate) fn ui(
        &mut self,
        ui: &mut egui::Ui,
        workspace: Option<&Path>,
        runtime: Option<&Runtime>,
    ) -> Option<PathBuf> {
        configure_input_style(ui);
        if self.workspace.as_deref() != workspace || self.runtime.as_ref() != runtime {
            *self = Self {
                workspace: workspace.map(Path::to_path_buf),
                runtime: runtime.cloned(),
                ..Self::default()
            };
        }
        self.poll_reply();
        if workspace.is_none() {
            ui.vertical_centered(|ui| {
                theme::empty_art(ui, Icon::Git, theme::GREEN);
                ui.strong("Changes at a glance");
                ui.label("Open a workspace to review and commit changes.");
            });
            return None;
        }
        let busy = self.pending.is_some();
        let mut action = self.refresh_controls(ui, busy);
        let mut open = None;
        if self.snapshot.is_some() {
            self.repository_heading(ui);
            self.change_groups(ui, busy, &mut action);
            ui.small("Space: stage/unstage selected file");
            ui.separator();
            open = self.selected_file_actions(ui, busy, &mut action);
            ui.separator();
            self.commit_form(ui, busy, &mut action);
        }
        if !self.message.is_empty() {
            ui.separator();
            ui.label(&self.message);
        }
        if let Some(action) = action {
            self.start(action, ui.ctx());
        }
        open
    }
}
