//! Shared widget theme plus editor-only terminal font registration.
pub(crate) use focus_addon_support::ui_theme::*;
pub(crate) fn apply(ctx: &eframe::egui::Context) {
    crate::terminal_fonts::apply(ctx);
    focus_addon_support::ui_theme::apply(ctx);
}
