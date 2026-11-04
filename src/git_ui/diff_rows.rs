//! Equal-height comparison rows with independent horizontal scrolling.
use super::patch_model::{Hunk, LineKind};
use crate::ui_theme as theme;
use eframe::egui::{self, Color32, RichText};

pub(super) fn show(ui: &mut egui::Ui, id: u64, index: usize, hunk: &Hunk) {
    ui.columns(2, |columns| {
        for (side, column) in columns.iter_mut().enumerate() {
            let width = column.available_width();
            egui::ScrollArea::horizontal()
                .id_salt(("comparison", id, index, side))
                .show(column, |ui| {
                    ui.set_min_width(width);
                    ui.spacing_mut().item_spacing.y = 0.0;
                    ui.spacing_mut().interact_size.y = 20.0;
                    for row in &hunk.rows {
                        let line = if side == 0 { &row.before } else { &row.after };
                        let kind = line.as_ref().map(|line| line.kind);
                        let fill = if hunk.staged {
                            theme::BLUE_SOFT
                        } else {
                            match kind {
                                Some(LineKind::Removed) => Color32::from_rgb(255, 235, 235),
                                Some(LineKind::Added) => Color32::from_rgb(230, 248, 235),
                                _ => Color32::WHITE,
                            }
                        };
                        egui::Frame::new().fill(fill).show(ui, |ui| {
                            ui.set_min_width(width);
                            ui.horizontal(|ui| {
                                ui.set_min_height(20.0);
                                let number = line
                                    .as_ref()
                                    .map(|l| {
                                        format!(
                                            "{}{}",
                                            l.number,
                                            if l.no_newline { "*" } else { "" }
                                        )
                                    })
                                    .unwrap_or_default();
                                ui.add_sized(
                                    [48.0, 20.0],
                                    egui::Label::new(
                                        RichText::new(number).monospace().color(theme::MUTED),
                                    ),
                                )
                                .on_hover_text("* marks a line without a final newline");
                                let color = match kind {
                                    Some(LineKind::Removed) => theme::RED,
                                    Some(LineKind::Added) => theme::GREEN,
                                    _ => theme::INK,
                                };
                                let text = line.as_ref().map(|l| l.text.as_str()).unwrap_or("");
                                ui.add(
                                    egui::Label::new(
                                        RichText::new(text).monospace().size(13.0).color(color),
                                    )
                                    .wrap_mode(egui::TextWrapMode::Extend)
                                    .selectable(true),
                                );
                            });
                        });
                    }
                });
        }
    });
}
