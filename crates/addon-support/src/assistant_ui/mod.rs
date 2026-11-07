//! Shared assistant presentation; provider state and protocol actions stay in their adapters.
use crate::ui_theme::{self as theme, Icon};
use egui;
pub mod composer;
pub mod content;
pub mod header;
#[cfg(test)]
mod tests;

pub fn configure(ui: &mut egui::Ui) {
    ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Wrap);
    let color = if ui.visuals().dark_mode {
        egui::Color32::WHITE
    } else {
        theme::INK
    };
    ui.visuals_mut().text_cursor.stroke = egui::Stroke::new(3.0, color);
    ui.visuals_mut().text_cursor.blink = false;
}

pub fn empty(ui: &mut egui::Ui) {
    ui.vertical_centered(|ui| {
        theme::empty_art(ui, Icon::Spark, theme::TEAL);
        ui.strong("A little help, a lot of progress");
        ui.label("Ask a question, plan a change, or work through a bug.");
        ui.small("Choose a session, model, thinking level, and mode.");
    });
}

/// Reserve the footer in this frame, independently of the growing transcript.
pub fn body(ui: &egui::Ui, input: &str) -> (egui::Rect, egui::Rect, f32) {
    let rect = ui.available_rect_before_wrap();
    let font = egui::TextStyle::Body.resolve(ui.style());
    let row = ui.fonts_mut(|fonts| fonts.row_height(&font));
    let height = ui.fonts_mut(|fonts| {
        fonts
            .layout(
                input.to_owned(),
                font,
                theme::INK,
                (rect.width() - 20.0).max(1.0),
            )
            .size()
            .y
    });
    let input_height =
        (height.max(row * 3.0) + 8.0).min(120.0_f32.min(rect.height() * 0.30).max(row + 8.0));
    let footer_height = (input_height + 80.0).min(rect.height());
    let top = (rect.bottom() - footer_height).max(rect.top());
    (
        egui::Rect::from_min_max(rect.min, egui::pos2(rect.right(), top)),
        egui::Rect::from_min_max(egui::pos2(rect.left(), top), rect.max),
        input_height,
    )
}
