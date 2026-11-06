//! Shared status labels, group colors, fixed label layout, and input styling.

use eframe::egui::{self, Color32};

pub(super) fn fixed_label(ui: &mut egui::Ui, size: [f32; 2], label: egui::Label) -> egui::Response {
    let size = egui::vec2(size[0], size[1]);
    // add_sized centers labels, including labels with left-aligned text.
    ui.allocate_ui_with_layout(
        size,
        egui::Layout::left_to_right(egui::Align::Center),
        |ui| {
            ui.set_min_size(size);
            ui.add(label)
        },
    )
    .inner
}

pub(super) fn label(code: char) -> &'static str {
    match code {
        'M' => "Modified",
        'A' => "Added",
        'D' => "Deleted",
        'R' => "Renamed",
        'C' => "Copied",
        'U' => "Conflict",
        '?' => "Untracked",
        'T' => "Type changed",
        _ => "Changed",
    }
}

pub(super) fn group_color(group: super::model::Group) -> Color32 {
    match group {
        super::model::Group::Conflicts => crate::ui_theme::RED,
        super::model::Group::Staged => crate::ui_theme::GREEN,
        super::model::Group::Changes => crate::ui_theme::INK,
        super::model::Group::Untracked => crate::ui_theme::BLUE,
    }
}

#[derive(Clone, Debug)]
pub(super) struct Notice {
    pub(super) title: String,
    pub(super) details: Option<String>,
    pub(super) error: bool,
}
impl Notice {
    pub(super) fn success(title: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            details: None,
            error: false,
        }
    }
    pub(super) fn error(title: impl Into<String>, details: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            details: Some(details.into()),
            error: true,
        }
    }
}

pub(super) fn configure_input_style(ui: &mut egui::Ui) {
    ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Wrap);
    let cursor_color = if ui.visuals().dark_mode {
        Color32::WHITE
    } else {
        Color32::BLACK
    };
    ui.visuals_mut().text_cursor.stroke = egui::Stroke::new(3.0_f32, cursor_color);
    ui.visuals_mut().text_cursor.blink = false;
}
