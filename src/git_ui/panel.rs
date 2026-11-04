//! Status groups, selection actions, refresh controls, and commit entry.

use super::{
    GitUi,
    actions::Action,
    presentation::{file_row_text, group_colors, label},
    repository::conflict,
};
use crate::ui_theme::{self as theme, Icon};
use eframe::egui::{self, Color32};
use std::{path::PathBuf, time::Duration};

impl GitUi {
    pub(super) fn refresh_controls(&self, ui: &mut egui::Ui, busy: bool) -> Option<Action> {
        let mut action = None;
        if !busy
            && self
                .refreshed
                .is_none_or(|t| t.elapsed() >= Duration::from_secs(3))
        {
            action = Some(Action::Refresh);
        }
        ui.ctx().request_repaint_after(Duration::from_secs(3));
        ui.horizontal(|ui| {
            if ui
                .add_enabled_ui(!busy, |ui| {
                    theme::button(ui, Icon::Refresh, "Refresh", theme::GREEN, false)
                })
                .inner
                .clicked()
            {
                action = Some(Action::Refresh);
            }
            ui.allocate_ui(egui::vec2(20.0, 20.0), |ui| {
                if busy {
                    ui.spinner();
                }
            });
        });
        action
    }
    pub(super) fn repository_heading(&self, ui: &mut egui::Ui) {
        let Some(s) = &self.snapshot else {
            return;
        };
        ui.horizontal_wrapped(|ui| {
            theme::icon(ui, Icon::Git, theme::GREEN);
            ui.add(
                egui::Label::new(egui::RichText::new(&s.branch).strong().color(theme::INK)).wrap(),
            )
            .on_hover_text(s.root.display().to_string());
        });
        if s.files.iter().any(conflict) {
            ui.colored_label(
                Color32::DARK_RED,
                "Resolve conflicts and stage the resolved files before committing.",
            );
        }
        if s.files.is_empty() {
            egui::Frame::new()
                .fill(theme::TEAL_SOFT)
                .corner_radius(6)
                .inner_margin(8)
                .show(ui, |ui| {
                    theme::heading(ui, Icon::Check, "Working tree clean", theme::GREEN);
                    ui.small("No saved changes to commit.");
                });
        }
    }
    pub(super) fn change_groups(&self, ui: &mut egui::Ui, busy: bool, action: &mut Option<Action>) {
        let Some(s) = &self.snapshot else {
            return;
        };
        egui::ScrollArea::vertical()
            .id_salt("git_changes")
            .max_height((ui.available_height() - 210.0).max(0.0))
            .auto_shrink([false, false])
            .show(ui, |ui| {
                for (group, staged) in [(0, false), (1, false), (2, true), (3, false)] {
                    let (accent, fill) = group_colors(group, ui.visuals().dark_mode);
                    egui::Frame::group(ui.style())
                        .fill(fill)
                        .stroke(egui::Stroke::new(1.0_f32, accent))
                        .show(ui, |ui| {
                            ui.set_min_width(ui.available_width());
                            ui.label(
                                egui::RichText::new(match group {
                                    0 => "Unstaged",
                                    1 => "Untracked",
                                    2 => "Staged",
                                    _ => "Conflicts",
                                })
                                .strong()
                                .color(accent),
                            )
                            .on_hover_text(if staged {
                                format!("Included in the next commit\n{}", s.staged)
                            } else {
                                format!("Not included in the next commit\n{}", s.unstaged)
                            });
                            let files: Vec<_> = s
                                .files
                                .iter()
                                .filter(|c| match group {
                                    0 => c.work != ' ' && c.index != '?' && !conflict(c),
                                    1 => c.index == '?',
                                    2 => c.index != ' ' && c.index != '?' && !conflict(c),
                                    _ => conflict(c),
                                })
                                .collect();
                            if !files.is_empty()
                                && ui
                                    .add_enabled(
                                        !busy,
                                        egui::Button::new(if staged {
                                            "Unstage all"
                                        } else {
                                            "Stage all"
                                        }),
                                    )
                                    .clicked()
                            {
                                let mut paths = Vec::new();
                                for c in &files {
                                    paths.push(c.path.clone());
                                    if let Some(old) = &c.old {
                                        paths.push(old.clone());
                                    }
                                }
                                *action = Some(if staged {
                                    Action::Index {
                                        paths,
                                        stage: false,
                                    }
                                } else {
                                    Action::Index { paths, stage: true }
                                });
                            }
                            ui.small(if staged {
                                "Included in the next commit"
                            } else {
                                "Not in the next commit"
                            });
                            ui.label(
                                egui::RichText::new(format!("{} files", files.len()))
                                    .small()
                                    .color(accent),
                            );
                            for c in files {
                                ui.push_id((&c.path, staged), |ui| {
                                    let code = if staged { c.index } else { c.work };
                                    let title = if conflict(c) { "Conflict" } else { label(code) };
                                    let row = ui.selectable_label(
                                        self.diff_target.as_ref().is_some_and(|(p, cached)| {
                                            p == &c.path && *cached == staged
                                        }),
                                        file_row_text(
                                            &format!("{title}: {}", c.path),
                                            s.counts[usize::from(!staged)]
                                                .get(&c.path)
                                                .map(String::as_str)
                                                .unwrap_or(""),
                                            ui.visuals().text_color(),
                                            ui.visuals().dark_mode,
                                        ),
                                    );
                                    if row.clicked() {
                                        row.request_focus();
                                    }
                                    let space = row.has_focus()
                                        && ui.input_mut(|input| {
                                            let pressed = input.events.iter().any(|event| {
                                                matches!(event, egui::Event::Key {
        key: egui::Key::Space,
        pressed: true,
        repeat: false,
        modifiers,
        ..
    } if *modifiers == egui::Modifiers::NONE)
                                            });
                                            if pressed {
                                                input.consume_key(
                                                    egui::Modifiers::NONE,
                                                    egui::Key::Space,
                                                );
                                            }
                                            pressed
                                        });
                                    if space && !busy {
                                        let mut paths = vec![c.path.clone()];
                                        if let Some(old) = &c.old {
                                            paths.push(old.clone());
                                        }
                                        *action = Some(Action::Index {
                                            paths,
                                            stage: !staged,
                                        });
                                    } else if row.clicked() && !busy {
                                        *action = Some(Action::Diff(c.path.clone(), staged));
                                    }
                                    if let Some(old) = &c.old {
                                        ui.small(format!("From: {old}"));
                                    }
                                });
                            }
                        });
                }
            });
    }
    pub(super) fn selected_file_actions(
        &mut self,
        ui: &mut egui::Ui,
        busy: bool,
        action: &mut Option<Action>,
    ) -> Option<PathBuf> {
        let Some(s) = &self.snapshot else {
            return None;
        };
        let mut open = None;
        if let Some((path, cached)) = &self.diff_target {
            ui.small(format!("Selected: {path}"));
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(
                        !busy,
                        egui::Button::new(if *cached {
                            "Unstage file"
                        } else {
                            "Stage file"
                        }),
                    )
                    .clicked()
                {
                    let mut paths = vec![path.clone()];
                    if let Some(old) = s
                        .files
                        .iter()
                        .find(|c| &c.path == path)
                        .and_then(|c| c.old.clone())
                    {
                        paths.push(old);
                    }
                    *action = Some(Action::Index {
                        paths,
                        stage: !cached,
                    });
                }
                if ui
                    .add_enabled(s.root.join(path).is_file(), egui::Button::new("Open file"))
                    .clicked()
                {
                    open = Some(s.root.join(path));
                    self.diff = None;
                }
            });
        }

        open
    }
    pub(super) fn commit_form(
        &mut self,
        ui: &mut egui::Ui,
        busy: bool,
        action: &mut Option<Action>,
    ) {
        let Some(s) = &self.snapshot else {
            return;
        };
        let staged = s
            .files
            .iter()
            .any(|c| c.index != ' ' && c.index != '?' && !conflict(c));
        let conflicts = s.files.iter().any(conflict);
        ui.add(
            egui::TextEdit::multiline(&mut self.commit)
                .id(egui::Id::new(("git_commit_message", &self.workspace)))
                .hint_text("Commit message…")
                .desired_rows(2)
                .desired_width(f32::INFINITY),
        );
        if ui
            .add_enabled(
                !busy && staged && !conflicts && !self.commit.trim().is_empty(),
                egui::Button::new(
                    egui::RichText::new("Commit staged changes").color(Color32::WHITE),
                )
                .fill(theme::GREEN)
                .stroke(egui::Stroke::NONE),
            )
            .clicked()
        {
            *action = Some(Action::Commit(self.commit.clone()));
        }
        ui.small("Git uses saved files. Save editor changes before staging.");
    }
}
