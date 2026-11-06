//! Collapsible change groups, file selection, and inline index actions.
use super::{
    GitUi,
    actions::Action,
    model::{Group, Selection},
    presentation::{fixed_label, group_color},
    repository::Change,
};
use crate::ui_theme::{self as theme, Icon};
use eframe::egui::{self, Color32, RichText};
use std::path::{Path, PathBuf};
impl GitUi {
    pub(super) fn change_groups(&mut self, ui: &mut egui::Ui) -> Option<PathBuf> {
        let Some(snapshot) = &self.snapshot else {
            ui.label(if self.refresh.is_some() {
                "Reading repository status…"
            } else {
                "Use Refresh to retry loading the repository."
            });
            return None;
        };
        let busy = self.busy();
        let mut action = None;
        let mut open = None;
        egui::ScrollArea::vertical()
            .id_salt(("git_changes", &self.workspace))
            .auto_shrink([false, false])
            .show(ui, |ui| {
                if snapshot.files.is_empty() {
                    ui.horizontal(|ui| {
                        theme::icon(ui, Icon::Check, theme::GREEN);
                        ui.add(
                            egui::Label::new(
                                RichText::new("Working tree clean")
                                    .strong()
                                    .color(theme::GREEN),
                            )
                            .selectable(false)
                            .halign(egui::Align::Min)
                            .truncate(),
                        );
                    });
                    ui.small("No saved changes to commit.");
                }
                for group in Group::ALL {
                    let files = snapshot.group(group);
                    if files.is_empty() {
                        continue;
                    }
                    let mut toggle = false;
                    let state_id = ui.make_persistent_id(("git_group", &self.workspace, group));
                    let state = egui::collapsing_header::CollapsingState::load_with_default_open(
                        ui.ctx(),
                        state_id,
                        true,
                    );

                    let mut header = state.show_header(ui, |ui| {
                        let bulk = group != Group::Conflicts;
                        let action_width = if bulk {
                            24.0 + ui.spacing().item_spacing.x
                        } else {
                            0.0
                        };
                        let width = (ui.available_width() - action_width).max(1.0);
                        toggle = fixed_label(
                            ui,
                            [width, 24.0],
                            egui::Label::new(
                                RichText::new(format!("{} ({})", group.title(), files.len()))
                                    .strong()
                                    .color(group_color(group)),
                            )
                            .selectable(false)
                            .halign(egui::Align::Min)
                            .truncate()
                            .sense(egui::Sense::click()),
                        )
                        .clicked();
                        if bulk
                            && ui
                                .add_enabled(
                                    !busy,
                                    egui::Button::new(if group.side().is_staged() {
                                        "−"
                                    } else {
                                        "+"
                                    })
                                    .small()
                                    .min_size(egui::vec2(24.0, 24.0)),
                                )
                                .on_hover_text(if group.side().is_staged() {
                                    "Unstage all staged changes"
                                } else {
                                    "Stage all changes"
                                })
                                .clicked()
                        {
                            let mut paths: Vec<_> =
                                files.iter().flat_map(|file| file.paths()).collect();
                            paths.sort();
                            paths.dedup();
                            action = Some(Action::Index {
                                paths,
                                stage: !group.side().is_staged(),
                            });
                        }
                    });
                    if toggle {
                        header.toggle();
                    }
                    header.body_unindented(|ui| {
                        for change in files {
                            let selection = Selection::new(&change.path, group.side());
                            let selected = self.diff_target.as_ref() == Some(&selection);
                            ui.push_id((&self.workspace, group, &change.path), |ui| {
                                let row_id =
                                    egui::Id::new(("git_file_row", &self.workspace, &selection));
                                let (row, quick_action) = file_row(
                                    ui,
                                    change,
                                    group,
                                    selected,
                                    busy,
                                    row_id,
                                    snapshot.counts[group.side().counts_index()]
                                        .get(&change.path)
                                        .map(String::as_str)
                                        .unwrap_or(""),
                                );
                                if row.clicked() {
                                    row.request_focus();
                                    if !busy {
                                        action = Some(Action::Diff(selection));
                                    }
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
                                if !busy && (space || quick_action) {
                                    action = Some(Action::Index {
                                        paths: change.paths(),
                                        stage: !group.side().is_staged(),
                                    });
                                }
                                row.context_menu(|ui| {
                                    if ui
                                        .add_enabled(
                                            snapshot.root.join(&change.path).is_file(),
                                            egui::Button::new("Open file"),
                                        )
                                        .clicked()
                                    {
                                        open = Some(snapshot.root.join(&change.path));
                                        ui.close();
                                    }
                                    if ui
                                        .add_enabled(
                                            !busy,
                                            egui::Button::new(if group.side().is_staged() {
                                                "Unstage file"
                                            } else {
                                                "Stage file"
                                            }),
                                        )
                                        .clicked()
                                    {
                                        action = Some(Action::Index {
                                            paths: change.paths(),
                                            stage: !group.side().is_staged(),
                                        });
                                        ui.close();
                                    }
                                });
                            });
                        }
                    });
                    ui.add_space(4.0);
                }
            });
        if open.is_some() {
            self.dismiss_diff();
        }
        if let Some(action) = action {
            self.start(action, ui.ctx());
        }
        open
    }
}

fn file_row(
    ui: &mut egui::Ui,
    change: &Change,
    group: Group,
    selected: bool,
    busy: bool,
    row_id: egui::Id,
    counts: &str,
) -> (egui::Response, bool) {
    let side = group.side();
    let path = Path::new(&change.path);
    let name = path
        .file_name()
        .map(|name| name.to_string_lossy())
        .unwrap_or_else(|| change.path.as_str().into());
    let parent = path
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .map(|path| path.display().to_string())
        .unwrap_or_else(|| ".".into());
    let status = change.status(side);
    let mut tooltip = format!("{}\n{status}", change.path);
    if let Some(old) = &change.old {
        tooltip.push_str(&format!("\nRenamed from {old}"));
    }
    if !counts.is_empty() {
        tooltip.push_str(&format!("\n{counts}"));
    }
    let mut quick_action = false;
    let response = ui
        .horizontal(|ui| {
            let width = (ui.available_width() - 28.0 - ui.spacing().item_spacing.x).max(1.0);
            let fill = if selected {
                theme::BLUE_SOFT
            } else {
                Color32::TRANSPARENT
            };
            let contents = egui::Frame::new()
                .fill(fill)
                .inner_margin(3)
                .show(ui, |ui| {
                    ui.set_width((width - 6.0).max(1.0));
                    ui.vertical(|ui| {
                        ui.spacing_mut().item_spacing.y = 0.0;
                        fixed_label(
                            ui,
                            [ui.available_width(), 18.0],
                            egui::Label::new(
                                RichText::new(name.as_ref()).strong().color(theme::INK),
                            )
                            .selectable(false)
                            .halign(egui::Align::Min)
                            .truncate(),
                        );
                        let subtitle = if width >= 220.0 && !counts.is_empty() {
                            format!("{status} · {parent} · {counts}")
                        } else {
                            format!("{status} · {parent}")
                        };
                        fixed_label(
                            ui,
                            [ui.available_width(), 15.0],
                            egui::Label::new(RichText::new(subtitle).size(12.0).color(
                                if group == Group::Conflicts {
                                    theme::RED
                                } else {
                                    theme::MUTED
                                },
                            ))
                            .selectable(false)
                            .halign(egui::Align::Min)
                            .truncate(),
                        );
                    });
                });

            let row = ui.interact(contents.response.rect, row_id, egui::Sense::click());
            if row.has_focus() {
                ui.painter().rect_stroke(
                    row.rect,
                    3.0,
                    egui::Stroke::new(1.0_f32, theme::BLUE),
                    egui::StrokeKind::Inside,
                );
            }
            quick_action = ui
                .add_enabled(
                    !busy,
                    egui::Button::new(if side.is_staged() { "−" } else { "+" })
                        .small()
                        .min_size(egui::vec2(24.0, 24.0)),
                )
                .on_hover_text(if side.is_staged() {
                    "Unstage file"
                } else if group == Group::Conflicts {
                    "Stage resolved file"
                } else {
                    "Stage file"
                })
                .clicked();
            row.on_hover_text(tooltip)
        })
        .inner;
    (response, quick_action)
}
