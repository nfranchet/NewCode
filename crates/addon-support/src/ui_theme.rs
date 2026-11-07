use egui::{self, Color32, FontId, Rect, Response, Sense, Stroke, Ui, Vec2};
use std::path::Path;

pub const INK: Color32 = Color32::from_rgb(36, 51, 73);
pub const MUTED: Color32 = Color32::from_rgb(99, 116, 139);
pub const BLUE: Color32 = Color32::from_rgb(37, 94, 203);
pub const BLUE_SOFT: Color32 = Color32::from_rgb(232, 240, 255);
pub const TEAL: Color32 = Color32::from_rgb(12, 117, 112);
pub const TEAL_SOFT: Color32 = Color32::from_rgb(229, 246, 243);
pub const GREEN: Color32 = Color32::from_rgb(31, 119, 70);
pub const AMBER: Color32 = Color32::from_rgb(146, 88, 8);
pub const RED: Color32 = Color32::from_rgb(180, 49, 64);
pub const BORDER: Color32 = Color32::from_rgb(213, 224, 237);
pub const SURFACE: Color32 = Color32::from_rgb(245, 248, 253);
pub const NAVY: Color32 = Color32::from_rgb(27, 45, 71);

pub fn apply(ctx: &egui::Context) {
    let mut style = (*ctx.style_of(egui::Theme::Light)).clone();
    style.visuals = egui::Visuals::light();
    style.visuals.override_text_color = Some(INK);
    style.visuals.panel_fill = SURFACE;
    style.visuals.window_fill = Color32::WHITE;
    style.visuals.extreme_bg_color = Color32::WHITE;
    style.visuals.faint_bg_color = BLUE_SOFT;
    style.visuals.code_bg_color = SURFACE;
    style.visuals.window_corner_radius = egui::CornerRadius::same(8);
    style.visuals.window_stroke = Stroke::new(1.0_f32, BORDER);
    style.visuals.selection.bg_fill = Color32::from_rgb(210, 228, 255);
    style.visuals.selection.stroke = Stroke::new(1.5_f32, BLUE);
    style.visuals.hyperlink_color = BLUE;
    style.visuals.warn_fg_color = AMBER;
    style.visuals.error_fg_color = RED;
    style.visuals.text_cursor.stroke = Stroke::new(3.0_f32, INK);
    style.visuals.text_cursor.blink = false;
    for visuals in [
        &mut style.visuals.widgets.inactive,
        &mut style.visuals.widgets.noninteractive,
    ] {
        visuals.bg_fill = Color32::WHITE;
        visuals.weak_bg_fill = Color32::TRANSPARENT;
        visuals.bg_stroke = Stroke::new(1.0_f32, BORDER);
        visuals.fg_stroke = Stroke::new(1.0_f32, INK);
        visuals.corner_radius = egui::CornerRadius::same(5);
    }
    style.visuals.widgets.hovered.bg_fill = BLUE_SOFT;
    style.visuals.widgets.hovered.weak_bg_fill = BLUE_SOFT;
    style.visuals.widgets.hovered.bg_stroke = Stroke::new(1.0_f32, BLUE);
    style.visuals.widgets.hovered.fg_stroke = Stroke::new(1.0_f32, BLUE);
    style.visuals.widgets.active.bg_fill = Color32::from_rgb(210, 228, 255);
    style.visuals.widgets.active.weak_bg_fill = BLUE_SOFT;
    style.visuals.widgets.active.bg_stroke = Stroke::new(1.5_f32, BLUE);
    style.visuals.widgets.active.fg_stroke = Stroke::new(1.0_f32, BLUE);
    style.spacing.item_spacing = egui::vec2(6.0, 6.0);
    style.spacing.button_padding = egui::vec2(8.0, 5.0);
    style.spacing.interact_size = egui::vec2(24.0, 28.0);
    style
        .text_styles
        .insert(egui::TextStyle::Body, FontId::proportional(14.0));
    style
        .text_styles
        .insert(egui::TextStyle::Button, FontId::proportional(13.0));
    style
        .text_styles
        .insert(egui::TextStyle::Heading, FontId::proportional(18.0));
    style
        .text_styles
        .insert(egui::TextStyle::Monospace, FontId::monospace(14.0));
    ctx.set_theme(egui::Theme::Light);
    ctx.set_style_of(egui::Theme::Light, style);
}

#[derive(Clone, Copy)]
pub enum Icon {
    Folder,
    File,
    Search,
    Git,
    Addons,
    Save,
    Copy,
    Paste,
    NewFolder,
    NewFile,
    Refresh,
    Send,
    Stop,
    Spark,
    Check,
    Warning,
}

// Small vector icons share a 20-unit grid and don't depend on system emoji fonts.
pub fn paint_icon(ui: &Ui, rect: Rect, icon: Icon, color: Color32) {
    let painter = ui.painter();
    let p = |x: f32, y: f32| rect.min + egui::vec2(x, y) * (rect.width() / 20.0);
    let stroke = Stroke::new(1.6_f32, color);
    let line = |points: &[(f32, f32)]| {
        painter.add(egui::Shape::line(
            points.iter().map(|&(x, y)| p(x, y)).collect(),
            stroke,
        ));
    };
    let box_at = |x, y, w, h| {
        painter.rect_stroke(
            Rect::from_min_max(p(x, y), p(x + w, y + h)),
            2.0,
            stroke,
            egui::StrokeKind::Inside,
        );
    };
    match icon {
        Icon::Folder | Icon::NewFolder => {
            line(&[
                (2., 16.),
                (2., 5.),
                (8., 5.),
                (10., 7.),
                (18., 7.),
                (18., 16.),
                (2., 16.),
            ]);
            if matches!(icon, Icon::NewFolder) {
                line(&[(10., 9.), (10., 14.)]);
                line(&[(7.5, 11.5), (12.5, 11.5)]);
            }
        }
        Icon::File | Icon::NewFile => {
            line(&[
                (4., 18.),
                (4., 2.),
                (12., 2.),
                (16., 6.),
                (16., 18.),
                (4., 18.),
            ]);
            line(&[(12., 2.), (12., 6.), (16., 6.)]);
            if matches!(icon, Icon::NewFile) {
                line(&[(10., 9.), (10., 15.)]);
                line(&[(7., 12.), (13., 12.)]);
            } else {
                line(&[(7., 10.), (13., 10.)]);
                line(&[(7., 13.), (12., 13.)]);
            }
        }
        Icon::Search => {
            painter.circle_stroke(p(8., 8.), rect.width() * 0.27, stroke);
            line(&[(12., 12.), (18., 18.)]);
        }
        Icon::Addons => {
            for (x, y) in [(3., 3.), (12., 3.), (3., 12.), (12., 12.)] {
                box_at(x, y, 5., 5.);
            }
        }
        Icon::Git => {
            line(&[(5., 5.), (5., 15.)]);
            line(&[(5., 12.), (14., 9.), (14., 5.)]);
            for (x, y) in [(5., 3.), (5., 17.), (14., 3.)] {
                painter.circle_filled(p(x, y), rect.width() * 0.12, color);
            }
        }
        Icon::Save => {
            box_at(3., 2., 14., 16.);
            box_at(6., 2., 8., 5.);
            box_at(6., 12., 8., 6.);
        }
        Icon::Copy => {
            box_at(6., 6., 11., 12.);
            line(&[(13., 4.), (13., 2.), (3., 2.), (3., 14.), (4., 14.)]);
        }
        Icon::Paste => {
            box_at(4., 4., 12., 14.);
            box_at(7., 2., 6., 4.);
            line(&[(7., 10.), (13., 10.)]);
            line(&[(7., 13.), (12., 13.)]);
        }
        Icon::Refresh => {
            line(&[
                (16., 7.),
                (13., 3.),
                (7., 3.),
                (3., 7.),
                (3., 13.),
                (7., 17.),
                (13., 17.),
                (17., 13.),
            ]);
            line(&[(16., 2.), (16., 7.), (11., 7.)]);
        }
        Icon::Send => {
            line(&[(2., 3.), (18., 10.), (2., 17.), (5., 10.), (2., 3.)]);
            line(&[(5., 10.), (13., 10.)]);
        }
        Icon::Stop => {
            box_at(4., 4., 12., 12.);
        }
        Icon::Spark => {
            line(&[
                (10., 1.),
                (12., 7.),
                (18., 10.),
                (12., 12.),
                (10., 19.),
                (8., 12.),
                (2., 10.),
                (8., 7.),
                (10., 1.),
            ]);
        }
        Icon::Check => {
            line(&[(3., 10.), (8., 15.), (17., 5.)]);
        }
        Icon::Warning => {
            line(&[(10., 2.), (19., 17.), (1., 17.), (10., 2.)]);
            line(&[(10., 7.), (10., 11.)]);
            painter.circle_filled(p(10., 14.), 1.0, color);
        }
    }
}

pub fn icon(ui: &mut Ui, icon: Icon, color: Color32) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(20.0, 20.0), Sense::hover());
    paint_icon(ui, rect, icon, color);
}

pub fn button(ui: &mut Ui, icon: Icon, label: &str, accent: Color32, primary: bool) -> Response {
    let galley = ui.painter().layout_no_wrap(
        label.into(),
        FontId::proportional(13.0),
        Color32::PLACEHOLDER,
    );
    let width = if label.is_empty() {
        28.0
    } else {
        galley.size().x + 40.0
    };
    let (rect, response) = ui.allocate_exact_size(egui::vec2(width, 28.0), Sense::click());
    let enabled = ui.is_enabled();
    let fill = if primary {
        accent
    } else if response.hovered() || response.has_focus() {
        BLUE_SOFT
    } else {
        Color32::WHITE
    };
    let foreground = if !enabled {
        MUTED
    } else if primary {
        Color32::WHITE
    } else {
        accent
    };
    ui.painter()
        .rect_filled(rect, 5.0, if enabled { fill } else { SURFACE });
    ui.painter().rect_stroke(
        rect,
        5.0,
        Stroke::new(
            if response.has_focus() { 2.0_f32 } else { 1.0 },
            if response.has_focus() { BLUE } else { BORDER },
        ),
        egui::StrokeKind::Inside,
    );
    paint_icon(
        ui,
        Rect::from_min_size(rect.min + egui::vec2(5.0, 6.0), Vec2::splat(16.0)),
        icon,
        foreground,
    );
    ui.painter().galley(
        rect.min + egui::vec2(29.0, (rect.height() - galley.size().y) / 2.0),
        galley,
        foreground,
    );
    let accessible_label = if label.is_empty() {
        match icon {
            Icon::NewFolder => "New folder",
            Icon::NewFile => "New file",
            Icon::Search => "Search",
            Icon::Stop => "Stop",
            Icon::Refresh => "Refresh",
            _ => "Action",
        }
    } else {
        label
    };
    response.widget_info(|| {
        egui::WidgetInfo::labeled(egui::WidgetType::Button, enabled, accessible_label)
    });
    response
}

pub fn nav_button(ui: &mut Ui, icon: Icon, selected: bool, label: &str) -> Response {
    let (rect, response) = ui.allocate_exact_size(egui::vec2(38.0, 42.0), Sense::click());
    if selected || response.hovered() || response.has_focus() {
        ui.painter().rect_filled(
            rect.shrink(2.0),
            6.0,
            if selected {
                BLUE
            } else {
                Color32::from_rgb(46, 66, 93)
            },
        );
    }
    if response.has_focus() {
        ui.painter().rect_stroke(
            rect.shrink(1.0),
            6.0,
            Stroke::new(2.0_f32, Color32::from_rgb(117, 202, 250)),
            egui::StrokeKind::Inside,
        );
    }
    if selected {
        ui.painter().rect_filled(
            Rect::from_min_size(rect.min + egui::vec2(0.0, 12.0), egui::vec2(3.0, 18.0)),
            2.0,
            Color32::from_rgb(117, 202, 250),
        );
    }
    paint_icon(
        ui,
        Rect::from_center_size(rect.center(), Vec2::splat(21.0)),
        icon,
        if selected {
            Color32::WHITE
        } else {
            Color32::from_rgb(187, 205, 228)
        },
    );
    response.widget_info(|| {
        egui::WidgetInfo::labeled(egui::WidgetType::Button, ui.is_enabled(), label)
    });
    response.on_hover_text(label)
}

pub fn heading(ui: &mut Ui, symbol: Icon, text: &str, color: Color32) {
    ui.horizontal(|ui| {
        icon(ui, symbol, color);
        ui.label(egui::RichText::new(text).size(17.0).strong().color(color));
    });
}

pub fn badge(ui: &mut Ui, text: &str, color: Color32, fill: Color32) -> Response {
    egui::Frame::new()
        .fill(fill)
        .corner_radius(5)
        .inner_margin(egui::Margin::symmetric(6, 3))
        .show(ui, |ui| {
            ui.label(egui::RichText::new(text).small().strong().color(color))
        })
        .inner
}

pub fn file_kind(path: &Path) -> (&'static str, Color32) {
    match path.extension().and_then(|s| s.to_str()).unwrap_or("") {
        "rs" => ("RS", Color32::from_rgb(156, 73, 30)),
        "py" => ("PY", BLUE),
        "js" | "jsx" => ("JS", AMBER),
        "ts" | "tsx" => ("TS", BLUE),
        "json" | "toml" | "yaml" | "yml" => ("{ }", TEAL),
        "md" | "txt" => ("Aa", MUTED),
        "html" | "css" | "svg" => ("< >", Color32::from_rgb(132, 67, 165)),
        "sh" | "bash" => ("$_", GREEN),
        _ => ("//", MUTED),
    }
}

pub fn file_text(path: &Path, name: &str) -> egui::text::LayoutJob {
    let (kind, color) = file_kind(path);
    let mut job = egui::text::LayoutJob::default();
    job.append(
        &format!("{kind}  "),
        0.0,
        egui::TextFormat {
            font_id: FontId::monospace(11.0),
            color,
            ..Default::default()
        },
    );
    job.append(
        name,
        0.0,
        egui::TextFormat::simple(FontId::proportional(13.0), INK),
    );
    job
}

pub fn empty_art(ui: &mut Ui, symbol: Icon, accent: Color32) {
    let size = egui::vec2(136.0_f32.min(ui.available_width()), 80.0);
    let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
    let center = rect.center();
    ui.painter().circle_filled(center, 34.0, BLUE_SOFT);
    ui.painter()
        .circle_filled(center + egui::vec2(35.0, -19.0), 7.0, TEAL_SOFT);
    ui.painter()
        .circle_filled(center + egui::vec2(-36.0, 22.0), 4.0, accent);
    paint_icon(
        ui,
        Rect::from_center_size(center, Vec2::splat(36.0)),
        symbol,
        accent,
    );
    let stroke = Stroke::new(1.5_f32, BORDER);
    ui.painter().line_segment(
        [
            center + egui::vec2(38.0, 16.0),
            center + egui::vec2(50.0, 16.0),
        ],
        stroke,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn icon_button_supports_keyboard_activation_and_disabled_state() {
        let ctx = egui::Context::default();
        apply(&ctx);
        let mut button_id = egui::Id::NULL;
        let mut clicked = false;
        let mut frame = |enabled: bool, events: Vec<egui::Event>| {
            let _ = crate::test_frame(
                &ctx,
                egui::RawInput {
                    events,
                    ..Default::default()
                },
                |root| {
                    egui::CentralPanel::default().show(root, |ui| {
                        let response = ui
                            .add_enabled_ui(enabled, |ui| {
                                button(ui, Icon::Save, "Save", BLUE, true)
                            })
                            .inner;
                        button_id = response.id;
                        clicked = response.clicked();
                    });
                },
            );
            (button_id, clicked)
        };
        let (id, _) = frame(true, vec![]);
        ctx.memory_mut(|m| m.request_focus(id));
        let enter = || egui::Event::Key {
            key: egui::Key::Enter,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        };
        assert!(
            frame(true, vec![enter()]).1,
            "Custom icon buttons must remain keyboard-accessible"
        );
        assert!(
            !frame(false, vec![enter()]).1,
            "Disabled actions must not activate"
        );
    }
}
