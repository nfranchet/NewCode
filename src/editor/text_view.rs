//! Source text rendering, layout caching, gutter painting, and cursor scrolling.

use super::{EditorApp, search::highlight_find_matches};
use crate::{syntax::colored_job, ui_theme as theme};
use eframe::egui::{
    self, Color32, FontId, TextStyle,
    text::{CCursor, CCursorRange},
};
use std::sync::Arc;

#[derive(Default)]
pub(super) struct EditorLayoutCache {
    text: String,
    extension: String,
    query: String,
    active_match: Option<usize>,
    galley: Option<Arc<egui::Galley>>,
    colors: Option<theme::Colors>,
    font: Option<FontId>,
}

impl EditorApp {
    pub(super) fn text_view(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let path = self.active_file.clone().unwrap();
        if self.dirty {
            theme::badge(
                ui,
                "Modified",
                theme::colors(ui).warning,
                theme::colors(ui).warning_soft,
            );
        }
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
        let find_query = if self.find_open {
            self.find_query.clone()
        } else {
            String::new()
        };
        let find_active = self.find_active;
        let layout_cache = self.editor_layout_cache.clone();
        let extension = ext.to_string();
        let mut layouter = move |ui: &egui::Ui, text: &dyn egui::TextBuffer, _wrap_width: f32| {
            let source = text.as_str();
            let colors = theme::colors(ui);
            let font = TextStyle::Monospace.resolve(ui.style());
            {
                let cached = layout_cache.lock().unwrap();
                if cached.text == source
                    && cached.extension == extension
                    && cached.query == find_query
                    && cached.active_match == find_active
                    && cached.colors == Some(colors)
                    && cached.font.as_ref() == Some(&font)
                {
                    if let Some(galley) = &cached.galley {
                        return galley.clone();
                    }
                }
            }
            let mut job = colored_job(source, &extension, colors, font.clone());
            highlight_find_matches(&mut job, source, &find_query, find_active, colors);
            job.wrap.max_width = f32::INFINITY;
            let galley = ui.fonts_mut(|fonts| fonts.layout_job(job));
            let mut cached = layout_cache.lock().unwrap();
            cached.text = source.to_owned();
            cached.extension = extension.clone();
            cached.query = find_query.clone();
            cached.active_match = find_active;
            cached.galley = Some(galley.clone());
            cached.colors = Some(colors);
            cached.font = Some(font);
            galley
        };
        let editor_id = self.editor_id();
        let jump_cursor = self.pending_editor_cursor.take();
        if let Some(cursor) = jump_cursor {
            let mut state = egui::TextEdit::load_state(ctx, editor_id).unwrap_or_default();
            state
                .cursor
                .set_char_range(Some(CCursorRange::one(CCursor::new(cursor))));
            egui::TextEdit::store_state(ctx, editor_id, state);
        }
        let response = egui::ScrollArea::both()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                let line_count = self
                    .buffer
                    .split('\n')
                    .count()
                    .saturating_sub(usize::from(self.buffer.ends_with('\n')))
                    .max(1);
                let row_height =
                    ui.fonts_mut(|f| f.row_height(&TextStyle::Monospace.resolve(ui.style())));
                let digit_width = ui.fonts_mut(|f| {
                    f.layout_no_wrap(
                        "0".into(),
                        TextStyle::Monospace.resolve(ui.style()),
                        Color32::WHITE,
                    )
                    .size()
                    .x
                });
                let gutter_width = digit_width * line_count.to_string().len() as f32 + 16.0;
                let text_width = (ui.available_width() - gutter_width - 8.0).max(60.0);
                let editor_galley = layouter(ui, &self.buffer, text_width);
                ui.horizontal_top(|ui| {
                    let (gutter_rect, _) = ui.allocate_exact_size(
                        egui::vec2(gutter_width, editor_galley.size().y + row_height),
                        egui::Sense::hover(),
                    );
                    let edit = egui::TextEdit::multiline(&mut self.buffer)
                        .id(editor_id)
                        .font(TextStyle::Monospace)
                        .desired_width(text_width)
                        .desired_rows(1)
                        .code_editor()
                        .frame(egui::Frame::NONE)
                        .layouter(&mut layouter)
                        .show(ui);
                    let visible = ui.clip_rect().intersect(gutter_rect);
                    let row_top = edit.galley_pos.y;
                    let number_width = line_count.to_string().len();
                    let mut char_offset = 0;
                    for (index, line) in self.buffer.split('\n').take(line_count).enumerate() {
                        let line_rect = edit.galley.pos_from_cursor(CCursor::new(char_offset));
                        let line_y = row_top + line_rect.top();
                        if line_y + line_rect.height() >= visible.top()
                            && line_y <= visible.bottom()
                        {
                            ui.painter().text(
                                egui::pos2(gutter_rect.right() - 8.0, line_y),
                                egui::Align2::RIGHT_TOP,
                                format!("{:>number_width$}", index + 1),
                                TextStyle::Monospace.resolve(ui.style()),
                                theme::colors(ui).faint,
                            );
                        }
                        char_offset += line.chars().count() + 1;
                    }
                    if let Some(cursor) = jump_cursor {
                        if !self.find_open {
                            edit.response.request_focus();
                        }
                        let mut target = edit
                            .galley
                            .pos_from_cursor(CCursor::new(cursor))
                            .translate(edit.galley_pos.to_vec2());
                        target.min.y -= row_height * 4.0;
                        target.max.y -= row_height * 4.0;
                        ui.scroll_to_rect(target, Some(egui::Align::TOP));
                    }
                    edit.response
                })
                .inner
            })
            .inner;
        if response.changed() {
            self.mark_modified();
        }
        if response.has_focus() && ui.input(|i| i.modifiers.ctrl && i.key_pressed(egui::Key::S)) {
            self.save_file();
        }
    }
}
