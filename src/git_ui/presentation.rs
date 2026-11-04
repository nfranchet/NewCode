//! Shared status labels, group colors, row text, and input cursor styling.

use eframe::egui::{self, Color32};

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

pub(super) fn group_colors(group: usize, dark: bool) -> (Color32, Color32) {
    let (light_accent, dark_accent, light_fill, dark_fill) = match group {
        0 => ([135, 82, 0], [245, 190, 85], [255, 248, 231], [49, 41, 25]),
        1 => (
            [25, 91, 158],
            [110, 180, 250],
            [237, 246, 255],
            [25, 38, 53],
        ),
        2 => (
            [30, 112, 58],
            [110, 210, 145],
            [237, 249, 240],
            [25, 44, 33],
        ),
        _ => (
            [169, 40, 45],
            [250, 125, 130],
            [255, 240, 240],
            [53, 29, 32],
        ),
    };
    let accent = if dark { dark_accent } else { light_accent };
    let fill = if dark { dark_fill } else { light_fill };
    (
        Color32::from_rgb(accent[0], accent[1], accent[2]),
        Color32::from_rgb(fill[0], fill[1], fill[2]),
    )
}

pub(super) fn file_row_text(
    title: &str,
    counts: &str,
    foreground: Color32,
    dark: bool,
) -> egui::text::LayoutJob {
    let mut job = egui::text::LayoutJob::default();
    let font = egui::FontId::proportional(14.0);
    job.append(
        title,
        0.0,
        egui::TextFormat::simple(font.clone(), foreground),
    );
    for count in counts.split_whitespace() {
        let color = if count.starts_with('+') {
            group_colors(2, dark).0
        } else if count.starts_with('−') {
            group_colors(3, dark).0
        } else {
            foreground
        };
        job.append(
            &format!("  {count}"),
            0.0,
            egui::TextFormat::simple(font.clone(), color),
        );
    }
    job
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
