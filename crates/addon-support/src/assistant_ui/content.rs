use crate::ui_theme as theme;
use egui;
pub use egui_commonmark::CommonMarkCache;

pub fn frame(kind: &str) -> egui::Frame {
    let fill = match kind {
        "You" => theme::BLUE_SOFT,
        "Proposed plan" => theme::TEAL_SOFT,
        "Error" => egui::Color32::from_rgb(255, 241, 243),
        "Status" => theme::SURFACE,
        _ => egui::Color32::WHITE,
    };
    egui::Frame::new()
        .fill(fill)
        .stroke(egui::Stroke::new(1.0, theme::BORDER))
        .corner_radius(7)
        .inner_margin(10)
}
pub fn heading(ui: &mut egui::Ui, kind: &str) {
    ui.label(
        egui::RichText::new(kind)
            .small()
            .strong()
            .color(match kind {
                "You" => theme::BLUE,
                "Error" => theme::RED,
                "Status" => theme::GREEN,
                _ => theme::TEAL,
            }),
    );
}
pub fn markdown(ui: &mut egui::Ui, cache: &mut CommonMarkCache, text: &str) {
    let width = ui.available_width();
    egui::ScrollArea::horizontal()
        .id_salt("markdown_width")
        .auto_shrink([false, true])
        .show(ui, |ui| {
            // Let the viewer wrap paragraphs, while table cells use their natural width.
            ui.style_mut().wrap_mode = None;
            ui.set_width(width);
            egui_commonmark::CommonMarkViewer::new().show(ui, cache, text);
        });
}
pub fn reasoning(
    ui: &mut egui::Ui,
    cache: &mut CommonMarkCache,
    id: impl std::hash::Hash + std::fmt::Debug,
    text: &str,
) {
    egui::CollapsingHeader::new("Reasoning")
        .id_salt(("reasoning", id))
        .default_open(true)
        .show(ui, |ui| markdown(ui, cache, text));
}
pub fn literal(ui: &mut egui::Ui, text: &str) {
    ui.add(egui::Label::new(text).wrap().selectable(true));
}
pub fn approval_frame() -> egui::Frame {
    egui::Frame::new()
        .fill(egui::Color32::from_rgb(255, 248, 231))
        .stroke(egui::Stroke::new(1.0, theme::AMBER))
        .corner_radius(7)
        .inner_margin(10)
}
