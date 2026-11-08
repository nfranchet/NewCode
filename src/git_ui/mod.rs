//! Source Control coordinates repository work, retained reviews, and a stable panel.
use crate::{
    dev_container::Runtime,
    ui_theme::{self as theme, Icon},
};
use actions::{Pending, RefreshReply, Reply, ReviewReply, Ticket};
use eframe::egui;
use model::Selection;
use presentation::{Notice, configure_input_style};
use repository::Snapshot;
use std::{
    path::{Path, PathBuf},
    time::Instant,
};
mod actions;
mod diff;
mod diff_rows;
mod file_list;
mod fingerprint;
mod model;
mod panel;
mod patch;
mod patch_model;
mod presentation;
mod repository;
mod review;
mod stats;
#[cfg(test)]
mod tests;

#[derive(Default)]
pub(crate) struct GitUi {
    workspace: Option<PathBuf>,
    runtime: Option<Runtime>,
    snapshot: Option<Snapshot>,
    repository_generation: u64,
    data_generation: u64,
    refresh: Option<Pending<RefreshReply>>,
    pending: Option<Pending<Reply>>,
    review_request: Option<Pending<ReviewReply>>,
    refresh_requested: bool,
    refreshed: Option<Instant>,
    notice: Option<Notice>,
    commit: String,
    diff: Option<review::DiffView>,
    review_generation: u64,
    diff_target: Option<Selection>,
    diff_hidden: bool,
    review_status: review::ReviewStatus,
}
impl GitUi {
    fn ticket(&self) -> Ticket {
        Ticket {
            repository: self.repository_generation,
            data: self.data_generation,
        }
    }
    fn busy(&self) -> bool {
        self.pending.is_some()
            || self
                .workspace
                .as_deref()
                .is_some_and(crate::repository_jobs::busy)
    }
    pub(crate) fn external_change(&mut self) {
        self.data_generation = self.data_generation.wrapping_add(1);
        self.refresh = None;
        self.review_request = None;
        self.diff = None;
        self.refresh_requested = true;
    }
    fn sync_workspace(&mut self, workspace: Option<&Path>, runtime: Option<&Runtime>) {
        if self.workspace.as_deref() != workspace || self.runtime.as_ref() != runtime {
            *self = Self {
                workspace: workspace.map(Path::to_path_buf),
                runtime: runtime.cloned(),
                repository_generation: self.repository_generation.wrapping_add(1),
                ..Self::default()
            };
        }
    }
    pub(crate) fn ui(
        &mut self,
        ui: &mut egui::Ui,
        workspace: Option<&Path>,
        runtime: Option<&Runtime>,
    ) -> Option<PathBuf> {
        configure_input_style(ui);
        self.sync_workspace(workspace, runtime);
        self.poll_reply();
        let manual_refresh = self.panel_header(ui);
        if workspace.is_none() {
            ui.vertical_centered(|ui| {
                theme::empty_art(ui, Icon::Git, theme::colors(ui).success);
                ui.strong("Open a workspace");
                ui.label("Review, stage, and commit its saved changes here.");
            });
            return None;
        }

        self.commit_form(ui);
        self.notice_area(ui);
        let open = self.change_groups(ui);
        self.schedule_refresh(ui.ctx(), manual_refresh);
        open
    }
}
