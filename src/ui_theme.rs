//! Editor-owned presets and terminal font registration. Addons receive resolved appearance.
use eframe::egui::{self, Color32, FontId, Stroke};
pub(crate) use focus_addon_support::ui_theme::*;

pub(crate) fn nord_light() -> Theme {
    let rgb = |hex: u32| Color32::from_rgb((hex >> 16) as u8, (hex >> 8) as u8, hex as u8);
    // ui-mocks/index.html, html[data-theme="nord"].
    let colors = Colors {
        text: rgb(0x263746),
        muted: rgb(0x637989),
        faint: rgb(0x91a2ae),
        accent: rgb(0x4b76a2),
        accent_soft: rgb(0xd9e6f0),
        accent_strong: rgb(0x335e88),
        on_accent: Color32::WHITE,
        window: rgb(0xf4f7f9),
        chrome: rgb(0xe7edf1),
        panel: rgb(0xedf2f5),
        raised: rgb(0xf8fafb),
        editor: rgb(0xfbfcfd),
        border: rgb(0xd3dde3),
        border_strong: rgb(0xb8c7d0),
        activity: rgb(0xdfe7ec),
        activity_text: rgb(0x4d6373),
        selection: rgb(0xdce8f1),
        hover: rgb(0xe2e9ee),
        terminal: rgb(0xe9eff3),
        terminal_text: rgb(0x314756),
        success: rgb(0x39806f),
        warning: rgb(0xb16c38),
        error: rgb(0xb7545d),
        purple: rgb(0x6c68a8),
        success_soft: rgb(0xe2efea),
        warning_soft: rgb(0xf3e9dd),
        error_soft: rgb(0xf3e3e6),
        keyword: rgb(0x7959a8),
        string: rgb(0x387e5b),
        type_name: rgb(0xa26025),
    };
    let mut style = egui::Style::default();
    let v = &mut style.visuals;
    *v = egui::Visuals::light();
    v.override_text_color = Some(colors.text);
    v.weak_text_color = Some(colors.muted);
    v.panel_fill = colors.panel;
    v.window_fill = colors.raised;
    v.extreme_bg_color = colors.raised;
    v.faint_bg_color = colors.hover;
    v.code_bg_color = colors.chrome;
    v.window_corner_radius = egui::CornerRadius::ZERO;
    v.menu_corner_radius = egui::CornerRadius::ZERO;
    v.window_stroke = Stroke::new(1.0, colors.border_strong);
    v.selection.bg_fill = colors.selection;
    v.selection.stroke = Stroke::new(1.0, colors.accent);
    v.hyperlink_color = colors.accent_strong;
    v.warn_fg_color = colors.warning;
    v.error_fg_color = colors.error;
    v.text_cursor.stroke = Stroke::new(2.0, colors.accent);
    v.text_cursor.blink = false;
    for widget in [
        &mut v.widgets.inactive,
        &mut v.widgets.noninteractive,
        &mut v.widgets.hovered,
        &mut v.widgets.active,
        &mut v.widgets.open,
    ] {
        widget.corner_radius = egui::CornerRadius::ZERO;
        widget.bg_stroke = Stroke::new(1.0, colors.border);
        widget.fg_stroke = Stroke::new(1.0, colors.text);
        widget.expansion = 0.0;
    }
    v.widgets.inactive.bg_fill = colors.raised;
    v.widgets.inactive.weak_bg_fill = Color32::TRANSPARENT;
    v.widgets.noninteractive.bg_fill = colors.panel;
    v.widgets.noninteractive.weak_bg_fill = Color32::TRANSPARENT;
    v.widgets.hovered.bg_fill = colors.hover;
    v.widgets.hovered.weak_bg_fill = colors.hover;
    v.widgets.hovered.bg_stroke.color = colors.border_strong;
    v.widgets.active.bg_fill = colors.selection;
    v.widgets.active.weak_bg_fill = colors.selection;
    v.widgets.active.bg_stroke.color = colors.accent;
    v.widgets.open = v.widgets.active;
    style.spacing.item_spacing = egui::vec2(5.0, 4.0);
    style.spacing.button_padding = egui::vec2(7.0, 4.0);
    style.spacing.interact_size = egui::vec2(22.0, 23.0);
    style.spacing.indent = 14.0;
    for (kind, font) in [
        (egui::TextStyle::Body, FontId::proportional(12.0)),
        (egui::TextStyle::Button, FontId::proportional(11.0)),
        (egui::TextStyle::Small, FontId::proportional(10.0)),
        (egui::TextStyle::Heading, FontId::proportional(12.0)),
        (egui::TextStyle::Monospace, FontId::monospace(12.0)),
    ] {
        style.text_styles.insert(kind, font);
    }
    let mut theme = Theme::from_style(style);
    theme.colors = colors;
    theme
}

pub(crate) fn apply(ctx: &egui::Context) {
    crate::terminal_fonts::apply(ctx);
    apply_theme(ctx, &nord_light());
}
