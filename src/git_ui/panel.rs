//! Fixed header/composer and a separately scrolling, compact file list.
use super::{GitUi, actions::Action, presentation::fixed_label};
use crate::ui_theme::{self as theme, Icon};
use eframe::egui::{self, RichText};
use std::{path::Path, time::Duration};

impl GitUi {
    pub(super) fn panel_header(&self, ui: &mut egui::Ui) -> bool {
        let mut refresh = false;
        ui.horizontal(|ui| {
            let width = (ui.available_width() - 48.0 - 2.0 * ui.spacing().item_spacing.x).max(1.0);
            fixed_label(
                ui,
                [width, 28.0],
                egui::Label::new(
                    RichText::new("Source Control")
                        .strong()
                        .color(theme::colors(ui).success),
                )
                .halign(egui::Align::Min)
                .truncate(),
            );
            ui.allocate_ui(egui::vec2(20.0, 28.0), |ui| {
                ui.set_min_size(egui::vec2(20.0, 28.0));
                if self
                    .refresh
                    .as_ref()
                    .is_some_and(|request| request.foreground)
                    || self.review_request.is_some()
                {
                    ui.ctx().request_repaint_after(Duration::from_millis(200));
                }
                if self.busy()
                    || self.review_request.as_ref().is_some_and(|request| {
                        request.started.elapsed() >= Duration::from_millis(200)
                    })
                    || self.refresh.as_ref().is_some_and(|request| {
                        request.foreground
                            && request.started.elapsed() >= Duration::from_millis(200)
                    })
                {
                    ui.spinner();
                }
            });
            refresh = ui
                .add_enabled_ui(self.workspace.is_some(), |ui| {
                    theme::button(ui, Icon::Refresh, "", theme::colors(ui).success, false)
                })
                .inner
                .on_hover_text("Refresh repository status")
                .clicked();
        });
        let root = self
            .snapshot
            .as_ref()
            .map(|snapshot| snapshot.root.as_path())
            .or(self.workspace.as_deref());
        let repository = root
            .and_then(Path::file_name)
            .map(|name| name.to_string_lossy())
            .unwrap_or_default();
        let branch = self
            .snapshot
            .as_ref()
            .map(|snapshot| snapshot.branch.as_str())
            .unwrap_or("Loading repository…");
        let caption = if self.workspace.is_some() {
            format!("{repository} · {branch}")
        } else {
            "No workspace selected".into()
        };
        fixed_label(
            ui,
            [ui.available_width(), 20.0],
            egui::Label::new(
                RichText::new(caption)
                    .size(12.0)
                    .color(theme::colors(ui).muted),
            )
            .halign(egui::Align::Min)
            .truncate(),
        )
        .on_hover_text(
            root.map(|root| format!("{}\n{branch}", root.display()))
                .unwrap_or_default(),
        );
        ui.separator();
        refresh
    }
    pub(super) fn commit_form(&mut self, ui: &mut egui::Ui) {
        egui::ScrollArea::vertical()
            .id_salt(("git_commit_draft", &self.workspace))
            .max_height(52.0)
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.add(
                    egui::TextEdit::multiline(&mut self.commit)
                        .id(egui::Id::new(("git_commit_message", &self.workspace)))
                        .hint_text("Commit message")
                        .desired_rows(2)
                        .desired_width(f32::INFINITY),
                );
            });
        let blocker = self.commit_blocker();
        let button = ui.add_enabled(
            blocker.is_none(),
            egui::Button::new(RichText::new("Commit staged").color(theme::colors(ui).on_accent))
                .fill(theme::colors(ui).success)
                .stroke(egui::Stroke::NONE)
                .min_size(egui::vec2(ui.available_width(), 28.0)),
        );
        let clicked = button
            .on_hover_text(blocker.unwrap_or("Commit only the staged changes"))
            .clicked();
        let hint = blocker.map(str::to_owned).unwrap_or_else(|| {
            format!(
                "{} staged files",
                self.snapshot
                    .as_ref()
                    .map_or(0, |snapshot| snapshot.staged_count())
            )
        });
        fixed_label(
            ui,
            [ui.available_width(), 18.0],
            egui::Label::new(
                RichText::new(hint)
                    .size(12.0)
                    .color(theme::colors(ui).muted),
            )
            .halign(egui::Align::Min)
            .truncate(),
        )
        .on_hover_text("Git uses saved files. Save editor changes before staging.");
        if clicked {
            self.start(Action::Commit(self.commit.clone()), ui.ctx());
        }
    }
    pub(super) fn notice_area(&self, ui: &mut egui::Ui) {
        let width = ui.available_width();
        ui.allocate_ui(egui::vec2(width, 24.0), |ui| {
            ui.set_min_size(egui::vec2(width, 24.0));
            if let Some(notice) = &self.notice {
                ui.horizontal(|ui| {
                    let label_width =
                        (width - if notice.details.is_some() { 36.0 } else { 0.0 }).max(1.0);
                    fixed_label(
                        ui,
                        [label_width, 20.0],
                        egui::Label::new(RichText::new(&notice.title).size(12.0).color(
                            if notice.error {
                                theme::colors(ui).error
                            } else {
                                theme::colors(ui).success
                            },
                        ))
                        .halign(egui::Align::Min)
                        .truncate(),
                    )
                    .on_hover_text(&notice.title);
                    if let Some(details) = &notice.details {
                        ui.menu_button("…", |ui| {
                            ui.set_width(280.0);
                            egui::ScrollArea::vertical()
                                .max_height(180.0)
                                .show(ui, |ui| {
                                    ui.add(egui::Label::new(details).wrap().selectable(true));
                                });
                        })
                        .response
                        .on_hover_text("Show Git details");
                    }
                });
            }
        });
    }
}
