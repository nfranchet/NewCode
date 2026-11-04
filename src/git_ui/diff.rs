//! Stable side-by-side review and per-hunk index controls.
use super::{GitUi, actions::Action, diff_rows};
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
        let Some(view) = &mut self.diff else {
            return false;
        };
        let mut close = false;
        let mut reload = false;
        let mut selected = None;
        let busy = self.pending.is_some();
        ui.horizontal_wrapped(|ui| {
            ui.strong(&view.path);
            ui.label(if view.original_staged {
                "Review: staged changes"
            } else {
                "Review: working changes"
            });
            reload = ui
                .add_enabled(!busy, egui::Button::new("Reload diff"))
                .clicked();
            close = ui.button("Close diff").clicked();
        });
        if let Some(reason) = &view.stale {
            ui.colored_label(theme::AMBER, reason);
        }
        if let Some(notice) = &view.notice {
            ui.label(notice);
        }
        ui.columns(2, |columns| {
            columns[0].strong("Before");
            columns[1].strong("After");
        });
        egui::ScrollArea::vertical()
            .id_salt(("git_review", view.id))
            .show(ui, |ui| {
                for (index, hunk) in view.hunks.iter().enumerate() {
                    let border = if view.selected_hunk == Some(index) {
                        theme::BLUE
                    } else {
                        theme::BORDER
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
                                                theme::BLUE
                                            } else {
                                                theme::MUTED
                                            },
                                            if hunk.staged { "Staged" } else { "Unstaged" },
                                        );
                                    },
                                );
                                if ui
                                    .add_enabled(
                                        !busy && view.stale.is_none() && view.hunk_actions,
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
                            diff_rows::show(ui, view.id, index, hunk);
                        });
                    if view.scroll_to == Some(index) {
                        ui.scroll_to_rect(response.response.rect, Some(egui::Align::TOP));
                    }
                    ui.add_space(8.0);
                }
            });
        view.scroll_to = None;
        let action = if reload {
            let staged = self
                .diff_target
                .as_ref()
                .filter(|(p, _)| p == &view.path)
                .map(|(_, s)| *s)
                .unwrap_or(view.original_staged);
            Some(Action::Diff(view.path.clone(), staged))
        } else if let Some(index) = selected {
            view.selected_hunk = Some(index);
            match view.patch_task(index) {
                Ok(task) => Some(Action::Patch(task)),
                Err(error) => {
                    self.message = error;
                    None
                }
            }
        } else {
            None
        };
        if close {
            self.diff_hidden = true;
        }
        if let Some(action) = action {
            self.start(action, ui.ctx());
        }
        true
    }
}
