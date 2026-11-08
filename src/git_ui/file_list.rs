//! Collapsible change groups, compact file rows, and index actions.
use super::{
    GitUi,
    actions::Action,
    model::{Group, Selection},
    presentation::{fixed_label, group_color},
    repository::{Change, Snapshot},
    stats::LineStats,
};
use crate::ui_theme::{self as theme, Icon};
use eframe::egui::{self, Color32, RichText};
use std::path::PathBuf;
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
        let count_widths = count_widths(ui, snapshot);
        let mut action = None;
        let mut open = None;
        egui::ScrollArea::vertical()
            .id_salt(("git_changes", &self.workspace))
            .auto_shrink([false, false])
            .show(ui, |ui| {
                if snapshot.files.is_empty() {
                    ui.horizontal(|ui| {
                        theme::icon(ui, Icon::Check, theme::colors(ui).success);
                        ui.add(
                            egui::Label::new(
                                RichText::new("Working tree clean")
                                    .strong()
                                    .color(theme::colors(ui).success),
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
                                    .color(group_color(theme::colors(ui), group)),
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
                                let row = file_row(
                                    ui,
                                    change,
                                    group,
                                    selected,
                                    row_id,
                                    snapshot.counts[group.side().counts_index()]
                                        .get(&change.path)
                                        .copied()
                                        .unwrap_or_default(),
                                    count_widths,
                                );
                                if row.clicked() {
                                    row.request_focus();
                                    if !busy && !row.double_clicked() {
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
                                if !busy && (space || row.double_clicked()) {
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
    row_id: egui::Id,
    counts: LineStats,
    count_widths: [f32; 2],
) -> egui::Response {
    let side = group.side();
    let status = change.status(side);
    let count_text = count_text(counts);
    let mut tooltip = format!("{}\n{status}", change.path);
    if let Some(old) = &change.old {
        tooltip.push_str(&format!("\nRenamed from {old}"));
    }
    tooltip.push_str(&format!("\n{} {}", count_text[0], count_text[1]));
    match counts {
        LineStats::Binary => tooltip.push_str("\nBinary file: line counts unavailable."),
        LineStats::Unavailable => tooltip.push_str("\nLine counts unavailable for this file."),
        LineStats::Text { .. } => {}
    }
    tooltip.push_str("\nDouble-click, Space, or right-click to stage/unstage.");
    let width = ui.available_width();
    let contents = egui::Frame::new()
        .fill(if selected {
            theme::colors(ui).accent_soft
        } else {
            Color32::TRANSPARENT
        })
        .inner_margin(3)
        .show(ui, |ui| {
            ui.set_width((width - 6.0).max(1.0));
            ui.allocate_ui_with_layout(
                egui::vec2(ui.available_width(), 22.0),
                egui::Layout::left_to_right(egui::Align::Center),
                |ui| {
                    ui.spacing_mut().item_spacing.x = 6.0;
                    let status_width = 16.0;
                    let available = ui.available_width() - status_width - 18.0;
                    // Keep a filename column even with large counts in a narrow panel.
                    let limit = ((available - 24.0) / 2.0).max(1.0);
                    let counts = count_widths.map(|width| width.min(limit));
                    let name_width = (available - counts[0] - counts[1]).max(1.0);
                    fixed_label(
                        ui,
                        [status_width, 22.0],
                        egui::Label::new(
                            RichText::new(change.status_letter(side).to_string())
                                .monospace()
                                .color(if group == Group::Conflicts {
                                    theme::colors(ui).error
                                } else {
                                    theme::colors(ui).muted
                                }),
                        )
                        .selectable(false)
                        .truncate(),
                    );
                    fixed_label(
                        ui,
                        [name_width, 22.0],
                        egui::Label::new(
                            RichText::new(&change.path)
                                .strong()
                                .color(theme::colors(ui).text),
                        )
                        .selectable(false)
                        .truncate(),
                    );
                    for (text, width) in count_text.iter().zip(counts) {
                        ui.allocate_ui_with_layout(
                            egui::vec2(width, 22.0),
                            egui::Layout::right_to_left(egui::Align::Center),
                            |ui| {
                                ui.set_min_size(egui::vec2(width, 22.0));
                                ui.add(
                                    egui::Label::new(
                                        RichText::new(text)
                                            .monospace()
                                            .color(theme::colors(ui).success),
                                    )
                                    .selectable(false)
                                    .truncate(),
                                );
                            },
                        );
                    }
                },
            );
        });
    let row = ui.interact(contents.response.rect, row_id, egui::Sense::click());
    if row.has_focus() {
        ui.painter().rect_stroke(
            row.rect,
            3.0,
            egui::Stroke::new(1.0_f32, theme::colors(ui).accent),
            egui::StrokeKind::Inside,
        );
    }
    row.on_hover_text(tooltip)
}

fn count_text(counts: LineStats) -> [String; 2] {
    match counts {
        LineStats::Text { added, deleted } => [format!("+{added}"), format!("-{deleted}")],
        LineStats::Binary | LineStats::Unavailable => ["+—".into(), "-—".into()],
    }
}

fn count_widths(ui: &egui::Ui, snapshot: &Snapshot) -> [f32; 2] {
    let mut widths = [28.0_f32; 2];
    let color = theme::colors(ui).success;
    for counts in snapshot.counts.iter().flat_map(|side| side.values()) {
        for (width, text) in widths.iter_mut().zip(count_text(*counts)) {
            let measured = ui.fonts_mut(|fonts| {
                fonts
                    .layout_no_wrap(text, egui::FontId::monospace(14.0), color)
                    .size()
                    .x
            });
            *width = width.max(measured.ceil());
        }
    }
    widths
}
