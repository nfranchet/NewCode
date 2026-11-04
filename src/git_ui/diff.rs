//! Unified diff rendering, hunk controls, and preview dismissal.

use super::{GitUi, actions::Action};
use eframe::egui::{self, Color32};

impl GitUi {
    pub(crate) fn dismiss_diff(&mut self) {
        self.diff_hidden = true;
    }

    pub(crate) fn show_diff(&mut self, ui: &mut egui::Ui) -> bool {
        if self.diff_hidden {
            return false;
        }
        let Some(diff) = &self.diff else {
            return false;
        };
        let mut close = false;
        let mut patch = None;
        let staged = self.diff_target.as_ref().is_some_and(|(_, staged)| *staged);
        ui.horizontal(|ui| {
            ui.strong(
                self.diff_target
                    .as_ref()
                    .map(|(path, _)| path.as_str())
                    .unwrap_or("Git diff"),
            );
            ui.label(if staged {
                "HEAD → Index"
            } else {
                "Index → Working tree"
            });
            close = ui.button("Close diff").clicked();
        });
        let lines: Vec<_> = diff.split_inclusive('\n').collect();
        let first = lines.iter().position(|line| line.starts_with("@@"));
        egui::ScrollArea::both().show(ui, |ui| {
            if diff.is_empty() {
                ui.label("No text changes available.");
            }
            for (index, line) in lines.iter().enumerate() {
                if line.starts_with("@@")
                    && !diff.contains("new file mode")
                    && !diff.contains("deleted file mode")
                    && !diff.contains("rename from")
                {
                    if ui
                        .add_enabled(
                            self.pending.is_none(),
                            egui::Button::new(if staged { "Unstage hunk" } else { "Stage hunk" }),
                        )
                        .clicked()
                    {
                        let end = lines
                            .iter()
                            .enumerate()
                            .skip(index + 1)
                            .find(|(_, l)| l.starts_with("@@"))
                            .map(|(i, _)| i)
                            .unwrap_or(lines.len());
                        patch = Some(format!(
                            "{}{}",
                            lines[..first.unwrap_or(0)].concat(),
                            lines[index..end].concat()
                        ));
                    }
                }
                let color = if line.starts_with('+') {
                    Color32::DARK_GREEN
                } else if line.starts_with('-') {
                    Color32::DARK_RED
                } else {
                    Color32::DARK_GRAY
                };
                ui.add(
                    egui::Label::new(
                        egui::RichText::new(line.trim_end_matches('\n'))
                            .monospace()
                            .color(color),
                    )
                    .selectable(true),
                );
            }
        });
        if let Some(patch) = patch {
            self.start(Action::Patch(patch, staged), ui.ctx());
            self.diff = None;
        }
        if close {
            self.diff = None;
        }
        true
    }
}
