use egui::{self, Color32, FontId, Rect, Response, Sense, Stroke, Ui, Vec2};
use std::path::Path;

/// Host-owned appearance, also used by independently rendered addon surfaces.
/// Concrete presets belong to the editor; addons only consume semantic roles.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Theme {
    pub style: egui::Style,
    pub colors: Colors,
    pub metrics: Metrics,
}

#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Colors {
    pub text: Color32,
    pub muted: Color32,
    pub faint: Color32,
    pub accent: Color32,
    pub accent_soft: Color32,
    pub accent_strong: Color32,
    pub on_accent: Color32,
    pub window: Color32,
    pub chrome: Color32,
    pub panel: Color32,
    pub raised: Color32,
    pub editor: Color32,
    pub border: Color32,
    pub border_strong: Color32,
    pub activity: Color32,
    pub activity_text: Color32,
    pub selection: Color32,
    pub hover: Color32,
    pub terminal: Color32,
    pub terminal_text: Color32,
    pub success: Color32,
    pub warning: Color32,
    pub error: Color32,
    pub purple: Color32,
    pub success_soft: Color32,
    pub warning_soft: Color32,
    pub error_soft: Color32,
    pub keyword: Color32,
    pub string: Color32,
    pub type_name: Color32,
}

#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct Metrics {
    pub activity_width: f32,
    pub tab_height: f32,
    pub toolbar_height: f32,
    pub status_height: f32,
    pub message_border_width: f32,
    pub panel_header_height: f32,
    pub project_row_height: f32,
    pub tree_row_height: f32,
    pub tree_indent: f32,
    pub tab_min_width: f32,
    pub editor_top_padding: f32,
    pub editor_bottom_padding: f32,
    pub editor_gutter_width: f32,
    pub editor_gutter_gap: f32,
    pub editor_line_height: f32,
}

impl Default for Metrics {
    fn default() -> Self {
        Self {
            activity_width: 44.0,
            tab_height: 37.0,
            toolbar_height: 42.0,
            status_height: 24.0,
            message_border_width: 3.0,
            panel_header_height: 38.0,
            project_row_height: 29.0,
            tree_row_height: 26.0,
            tree_indent: 16.0,
            tab_min_width: 130.0,
            editor_top_padding: 15.0,
            editor_bottom_padding: 24.0,
            editor_gutter_width: 42.0,
            editor_gutter_gap: 8.0,
            editor_line_height: 21.0,
        }
    }
}

impl Theme {
    /// A standalone fallback derived from egui, without depending on an editor preset.
    pub fn from_style(style: egui::Style) -> Self {
        let v = &style.visuals;
        let text = v.text_color();
        let muted = v.weak_text_color();
        let accent = v.selection.stroke.color;
        let panel = v.panel_fill;
        let colors = Colors {
            text,
            muted,
            faint: muted,
            accent,
            accent_soft: v.selection.bg_fill,
            accent_strong: accent,
            on_accent: v.extreme_bg_color,
            window: v.window_fill,
            chrome: v.faint_bg_color,
            panel,
            raised: v.window_fill,
            editor: v.extreme_bg_color,
            border: v.widgets.noninteractive.bg_stroke.color,
            border_strong: v.window_stroke.color,
            activity: panel,
            activity_text: muted,
            selection: v.selection.bg_fill,
            hover: v.widgets.hovered.bg_fill,
            terminal: panel,
            terminal_text: text,
            success: v.hyperlink_color,
            warning: v.warn_fg_color,
            error: v.error_fg_color,
            purple: accent,
            success_soft: v.faint_bg_color,
            warning_soft: v.faint_bg_color,
            error_soft: v.faint_bg_color,
            keyword: accent,
            string: text,
            type_name: accent,
        };
        Self {
            style,
            colors,
            metrics: Metrics::default(),
        }
    }
}

fn theme_id() -> egui::Id {
    egui::Id::new("focus_host_theme")
}

pub fn current(ctx: &egui::Context) -> Theme {
    ctx.data(|data| data.get_temp::<Theme>(theme_id()))
        .unwrap_or_else(|| Theme::from_style((*ctx.global_style()).clone()))
}
pub fn colors(ui: &Ui) -> Colors {
    palette(ui.ctx())
}
pub fn palette(ctx: &egui::Context) -> Colors {
    ctx.data(|data| data.get_temp::<Colors>(theme_id().with("colors")))
        .unwrap_or_else(|| Theme::from_style((*ctx.global_style()).clone()).colors)
}
pub fn metrics(ui: &Ui) -> Metrics {
    ui.ctx()
        .data(|data| data.get_temp::<Metrics>(theme_id().with("metrics")))
        .unwrap_or_else(|| current(ui.ctx()).metrics)
}

/// Applying appearance never resets widget memory, focus, or provider state.
pub fn apply_theme(ctx: &egui::Context, theme: &Theme) {
    let mode = if theme.style.visuals.dark_mode {
        egui::Theme::Dark
    } else {
        egui::Theme::Light
    };
    ctx.set_theme(mode);
    if *ctx.style_of(mode) != theme.style {
        ctx.set_style_of(mode, theme.style.clone());
    }
    ctx.data_mut(|data| {
        data.insert_temp(theme_id(), theme.clone());
        data.insert_temp(theme_id().with("colors"), theme.colors);
        data.insert_temp(theme_id().with("metrics"), theme.metrics);
    });
}

/// Initialize a standalone surface. A host theme supersedes this on surface input.
pub fn apply(ctx: &egui::Context) {
    apply_theme(
        ctx,
        &Theme::from_style((*ctx.style_of(egui::Theme::Light)).clone()),
    );
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
        egui::TextStyle::Button.resolve(ui.style()),
        Color32::PLACEHOLDER,
    );
    let height = ui.spacing().interact_size.y;
    let radius = ui.visuals().widgets.inactive.corner_radius;
    let width = if label.is_empty() {
        height
    } else {
        galley.size().x + 34.0
    };
    let (rect, response) = ui.allocate_exact_size(egui::vec2(width, height), Sense::click());
    let enabled = ui.is_enabled();
    let fill = if primary {
        accent
    } else if response.hovered() || response.has_focus() {
        colors(ui).hover
    } else {
        colors(ui).raised
    };
    let foreground = if !enabled {
        colors(ui).muted
    } else if primary {
        colors(ui).on_accent
    } else {
        accent
    };
    ui.painter()
        .rect_filled(rect, radius, if enabled { fill } else { colors(ui).panel });
    ui.painter().rect_stroke(
        rect,
        radius,
        Stroke::new(
            if response.has_focus() { 2.0_f32 } else { 1.0 },
            if response.has_focus() {
                colors(ui).accent
            } else {
                colors(ui).border
            },
        ),
        egui::StrokeKind::Inside,
    );
    paint_icon(
        ui,
        Rect::from_min_size(
            egui::pos2(rect.left() + 4.0, rect.center().y - 8.0),
            Vec2::splat(16.0),
        ),
        icon,
        foreground,
    );
    ui.painter().galley(
        rect.min + egui::vec2(25.0, (rect.height() - galley.size().y) / 2.0),
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
    let width = metrics(ui).activity_width;
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(width), Sense::click());
    let c = colors(ui);
    if selected || response.hovered() || response.has_focus() {
        ui.painter()
            .rect_filled(rect, 0.0, if selected { c.accent_soft } else { c.hover });
    }
    if response.has_focus() {
        ui.painter().rect_stroke(
            rect.shrink(1.0),
            0.0,
            Stroke::new(1.0, c.accent),
            egui::StrokeKind::Inside,
        );
    }
    if selected {
        ui.painter().rect_filled(
            Rect::from_min_max(
                rect.min + egui::vec2(0.0, 7.0),
                egui::pos2(rect.left() + 2.0, rect.bottom() - 7.0),
            ),
            0.0,
            c.accent,
        );
    }
    paint_icon(
        ui,
        Rect::from_center_size(rect.center(), Vec2::splat(20.0)),
        icon,
        c.activity_text,
    );
    response.widget_info(|| {
        egui::WidgetInfo::labeled(egui::WidgetType::Button, ui.is_enabled(), label)
    });
    response.on_hover_text(label)
}

pub fn heading(ui: &mut Ui, symbol: Icon, text: &str, color: Color32) {
    ui.horizontal(|ui| {
        icon(ui, symbol, color);
        ui.label(egui::RichText::new(text).size(12.0).strong().color(color));
    });
}

pub fn badge(ui: &mut Ui, text: &str, color: Color32, fill: Color32) -> Response {
    egui::Frame::new()
        .fill(fill)
        .corner_radius(ui.visuals().widgets.inactive.corner_radius)
        .inner_margin(egui::Margin::symmetric(6, 3))
        .show(ui, |ui| {
            ui.label(egui::RichText::new(text).small().strong().color(color))
        })
        .inner
}

pub fn file_kind(ui: &Ui, path: &Path) -> (&'static str, Color32) {
    match path.extension().and_then(|s| s.to_str()).unwrap_or("") {
        "rs" => ("R", colors(ui).warning),
        "py" => ("PY", colors(ui).accent),
        "js" | "jsx" => ("JS", colors(ui).warning),
        "ts" | "tsx" => ("TS", colors(ui).accent),
        "json" | "toml" | "yaml" | "yml" => ("T", colors(ui).purple),
        "md" | "txt" => ("M", colors(ui).accent),
        "html" | "css" | "svg" => ("< >", colors(ui).purple),
        "sh" | "bash" => ("$_", colors(ui).success),
        _ => ("//", colors(ui).muted),
    }
}

pub fn file_text(ui: &Ui, path: &Path, name: &str) -> egui::text::LayoutJob {
    let (kind, color) = file_kind(ui, path);
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
        egui::TextFormat::simple(egui::TextStyle::Body.resolve(ui.style()), colors(ui).text),
    );
    job
}

pub fn empty_art(ui: &mut Ui, symbol: Icon, accent: Color32) {
    let size = egui::vec2(136.0_f32.min(ui.available_width()), 80.0);
    let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
    let center = rect.center();
    ui.painter()
        .circle_filled(center, 34.0, colors(ui).accent_soft);
    ui.painter().circle_filled(
        center + egui::vec2(35.0, -19.0),
        7.0,
        colors(ui).accent_soft,
    );
    ui.painter()
        .circle_filled(center + egui::vec2(-36.0, 22.0), 4.0, accent);
    paint_icon(
        ui,
        Rect::from_center_size(center, Vec2::splat(36.0)),
        symbol,
        accent,
    );
    let stroke = Stroke::new(1.5_f32, colors(ui).border);
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
    fn older_theme_metrics_receive_layout_defaults() {
        let metrics: Metrics = serde_json::from_value(serde_json::json!({
            "activity_width": 48.0, "tab_height": 34.0, "toolbar_height": 42.0,
            "status_height": 25.0, "message_border_width": 2.0,
        }))
        .unwrap();
        assert_eq!(metrics.activity_width, 48.0);
        assert_eq!(metrics.editor_line_height, 21.0);
        assert_eq!(metrics.editor_gutter_width, 42.0);
        assert_eq!(metrics.tree_row_height, 26.0);
    }

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
                                button(ui, Icon::Save, "Save", colors(ui).accent, true)
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
