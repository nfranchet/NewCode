//! Stable side-by-side review and per-hunk index controls.
use super::{
    GitUi,
    actions::Action,
    diff_rows,
    model::{Selection, Side},
    presentation::Notice,
    review::ReviewStatus,
};
use crate::ui_theme as theme;
use eframe::egui;

impl GitUi {
    pub(crate) fn dismiss_diff(&mut self) {
        self.diff_hidden = true;
    }

    pub(crate) fn show_diff(&mut self, ui: &mut egui::Ui) -> bool {
        if self.diff_hidden {
            return false;
        }
        let target = self.diff_target.clone().or_else(|| {
            self.diff
                .as_ref()
                .map(|view| Selection::new(&view.path, Side::from(view.original_staged)))
        });
        let Some(target) = target else {
            return false;
        };
        let busy = self.busy();
        let loading = matches!(self.review_status, ReviewStatus::Loading);
        ui.add(egui::Label::new(egui::RichText::new(&target.path).strong()).truncate())
            .on_hover_text(&target.path);
        let mut close = false;
        let mut reload = false;
        let displayed_side = self
            .diff
            .as_ref()
            .filter(|view| view.path == target.path)
            .map(|view| Side::from(view.original_staged))
            .unwrap_or(target.side);
        ui.horizontal_wrapped(|ui| {
            ui.label(if displayed_side.is_staged() {
                "Staged changes"
            } else {
                "Working changes"
            });
            reload = ui
                .add_enabled(!busy && !loading, egui::Button::new("Reload diff"))
                .clicked();
            close = ui.button("Close diff").clicked();
            if loading {
                ui.spinner();
            }
        });
        if close {
            self.dismiss_diff();
            return true;
        }
        if reload {
            self.start(Action::Diff(target.clone()), ui.ctx());
            return true;
        }
        if let ReviewStatus::Failed(error) = &self.review_status {
            ui.colored_label(theme::colors(ui).error, error);
        }
        let Some(view) = &mut self.diff else {
            if loading {
                ui.label("Loading comparison…");
            }
            return true;
        };
        let actions_enabled = !busy && !loading && matches!(self.review_status, ReviewStatus::Idle);
        let mut selected = None;
        if let Some(reason) = &view.stale {
            ui.colored_label(theme::colors(ui).warning, reason);
        }
        if let Some(notice) = &view.notice {
            ui.label(notice);
        }
        ui.columns(2, |columns| {
            columns[0].strong("Before");
            columns[1].strong("After");
        });
        let scroll_id = ui
            .make_persistent_id((
                "git_review_content",
                &self.workspace,
                &view.path,
                view.original_staged,
            ))
            .value();
        egui::ScrollArea::vertical()
            .id_salt((
                "git_review",
                &self.workspace,
                &view.path,
                view.original_staged,
            ))
            .show(ui, |ui| {
                for (index, hunk) in view.hunks.iter().enumerate() {
                    let border = if view.selected_hunk == Some(index) {
                        theme::colors(ui).accent
                    } else {
                        theme::colors(ui).border
                    };
                    let response = egui::Frame::new()
                        .stroke(egui::Stroke::new(1.0_f32, border))
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.allocate_ui_with_layout(
                                    egui::vec2(80.0, 24.0),
                                    egui::Layout::left_to_right(egui::Align::Center),
                                    |ui| {
                                        ui.colored_label(
                                            if hunk.staged {
                                                theme::colors(ui).accent
                                            } else {
                                                theme::colors(ui).muted
                                            },
                                            if hunk.staged { "Staged" } else { "Unstaged" },
                                        );
                                    },
                                );
                                if ui
                                    .add_enabled(
                                        actions_enabled
                                            && view.stale.is_none()
                                            && view.hunk_actions,
                                        egui::Button::new(if hunk.staged {
                                            "Unstage hunk"
                                        } else {
                                            "Stage hunk"
                                        })
                                        .min_size(egui::vec2(125.0, 24.0)),
                                    )
                                    .clicked()
                                {
                                    selected = Some(index);
                                }
                                ui.add(egui::Label::new(&hunk.title).truncate());
                            });
                            diff_rows::show(ui, scroll_id, index, hunk);
                        });
                    if view.scroll_to == Some(index) {
                        ui.scroll_to_rect(response.response.rect, Some(egui::Align::TOP));
                    }
                    ui.add_space(8.0);
                }
            });
        view.scroll_to = None;
        let action = if let Some(index) = selected {
            view.selected_hunk = Some(index);
            match view.patch_task(index) {
                Ok(task) => Some(Action::Patch(task)),
                Err(error) => {
                    self.notice = Some(Notice::error("Could not update hunk", error));
                    None
                }
            }
        } else {
            None
        };
        if let Some(action) = action {
            self.start(action, ui.ctx());
        }
        true
    }
}
