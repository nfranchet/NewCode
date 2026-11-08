use crate::ui_theme as theme;
use egui;
pub use egui_commonmark::CommonMarkCache;

pub fn frame(ui: &egui::Ui, kind: &str) -> egui::Frame {
    let fill = match kind {
        "You" => theme::colors(ui).accent_soft,
        "Proposed plan" => theme::colors(ui).accent_soft,
        "Error" => theme::colors(ui).error_soft,
        "Status" => theme::colors(ui).panel,
        _ => theme::colors(ui).raised,
    };
    egui::Frame::new()
        .fill(fill)
        .stroke(egui::Stroke::new(1.0, theme::colors(ui).border))
        .corner_radius(ui.visuals().window_corner_radius)
        .inner_margin(10)
}
/// Shared message decoration keeps provider adapters independent of appearance.
pub fn message<R>(
    ui: &mut egui::Ui,
    kind: &str,
    show: impl FnOnce(&mut egui::Ui) -> R,
) -> egui::InnerResponse<R> {
    let response = frame(ui, kind).show(ui, show);
    let rect = response.response.rect;
    let colors = theme::colors(ui);
    ui.painter().rect_filled(
        egui::Rect::from_min_size(
            rect.min,
            egui::vec2(theme::metrics(ui).message_border_width, rect.height()),
        ),
        0.0,
        if kind == "You" {
            colors.accent
        } else {
            colors.border_strong
        },
    );
    response
}

pub fn heading(ui: &mut egui::Ui, kind: &str) {
    ui.label(
        egui::RichText::new(kind)
            .small()
            .strong()
            .color(match kind {
                "You" => theme::colors(ui).accent,
                "Error" => theme::colors(ui).error,
                "Status" => theme::colors(ui).success,
                _ => theme::colors(ui).accent,
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
pub fn approval_frame(ui: &egui::Ui) -> egui::Frame {
    egui::Frame::new()
        .fill(theme::colors(ui).warning_soft)
        .stroke(egui::Stroke::new(1.0, theme::colors(ui).warning))
        .corner_radius(ui.visuals().window_corner_radius)
        .inner_margin(10)
}
