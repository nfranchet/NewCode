//! Source text rendering, layout caching, gutter painting, and cursor scrolling.

use super::{EditorApp, search::highlight_find_matches};
use crate::{syntax::colored_job, ui_theme as theme};
use eframe::egui::{
    self, FontId, TextStyle,
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
    line_height: f32,
}

impl EditorApp {
    pub(super) fn text_view(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let path = self.active_file.clone().unwrap();
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
            let line_height = theme::metrics(ui).editor_line_height;
            {
                let cached = layout_cache.lock().unwrap();
                if cached.text == source
                    && cached.extension == extension
                    && cached.query == find_query
                    && cached.active_match == find_active
                    && cached.colors == Some(colors)
                    && cached.font.as_ref() == Some(&font)
                    && cached.line_height == line_height
                {
                    if let Some(galley) = &cached.galley {
                        if galley.pixels_per_point == ui.ctx().pixels_per_point() {
                            return galley.clone();
                        }
                    }
                }
            }
            let mut job = colored_job(source, &extension, colors, font.clone());
            highlight_find_matches(&mut job, source, &find_query, find_active, colors);
            job.wrap.max_width = f32::INFINITY;
            job.first_row_min_height = line_height;
            for section in &mut job.sections {
                section.format.line_height = Some(line_height);
            }
            let galley = ui.fonts_mut(|fonts| fonts.layout_job(job));
            let mut cached = layout_cache.lock().unwrap();
            cached.text = source.to_owned();
            cached.extension = extension.clone();
            cached.query = find_query.clone();
            cached.active_match = find_active;
            cached.galley = Some(galley.clone());
            cached.colors = Some(colors);
            cached.font = Some(font);
            cached.line_height = line_height;
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
        let metrics = theme::metrics(ui);
        let scroll_id = editor_id.with("source_scroll");
        let viewport_width = ui.available_width();
        let response = egui::ScrollArea::both()
            .id_salt(scroll_id)
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.add_space(metrics.editor_top_padding);
                let colors = theme::colors(ui);
                let font = TextStyle::Monospace.resolve(ui.style());
                let line_count = self.buffer.split('\n').count().max(1);
                let digit_width = ui.fonts_mut(|f| {
                    f.layout_no_wrap("0".into(), font.clone(), colors.faint)
                        .size()
                        .x
                });
                let gutter_width = metrics
                    .editor_gutter_width
                    .max(digit_width * line_count.to_string().len() as f32 + 24.0);
                let text_width =
                    (viewport_width - gutter_width - metrics.editor_gutter_gap).max(1.0);
                let editor_galley = layouter(ui, &self.buffer, text_width);
                let viewport_clip = ui.clip_rect();
                let response = ui
                    .horizontal_top(|ui| {
                        ui.spacing_mut().item_spacing.x = metrics.editor_gutter_gap;
                        let (allocated_gutter, _) = ui.allocate_exact_size(
                            egui::vec2(
                                gutter_width,
                                editor_galley.size().y + metrics.editor_line_height,
                            ),
                            egui::Sense::hover(),
                        );
                        // The source scrolls on both axes. Paint and interact with the gutter
                        // at the viewport's left edge so horizontal scrolling cannot move it.
                        let gutter_rect = egui::Rect::from_min_size(
                            egui::pos2(viewport_clip.left(), allocated_gutter.top()),
                            allocated_gutter.size(),
                        );
                        ui.interact(gutter_rect, editor_id.with("gutter"), egui::Sense::hover());
                        let source_clip = egui::Rect::from_min_max(
                            egui::pos2(
                                viewport_clip.left() + gutter_width + metrics.editor_gutter_gap,
                                viewport_clip.top(),
                            ),
                            viewport_clip.max,
                        );
                        ui.set_clip_rect(source_clip);
                        let edit = egui::TextEdit::multiline(&mut self.buffer)
                            .id(editor_id)
                            .font(TextStyle::Monospace)
                            .desired_width(text_width)
                            .desired_rows(1)
                            .code_editor()
                            .frame(egui::Frame::NONE)
                            .layouter(&mut layouter)
                            .show(ui);
                        ui.set_clip_rect(viewport_clip);
                        let painter = ui
                            .painter()
                            .with_clip_rect(viewport_clip.intersect(gutter_rect));
                        painter.rect_filled(gutter_rect, 0.0, colors.editor);
                        let mut char_offset = 0;
                        for (index, line) in self.buffer.split('\n').enumerate() {
                            let line_rect = edit.galley.pos_from_cursor(CCursor::new(char_offset));
                            let line_y = edit.galley_pos.y + line_rect.top();
                            if line_y + metrics.editor_line_height >= viewport_clip.top()
                                && line_y <= viewport_clip.bottom()
                            {
                                let mut job = egui::text::LayoutJob::single_section(
                                    (index + 1).to_string(),
                                    egui::TextFormat {
                                        font_id: font.clone(),
                                        color: colors.faint,
                                        line_height: Some(metrics.editor_line_height),
                                        ..Default::default()
                                    },
                                );
                                job.first_row_min_height = metrics.editor_line_height;
                                let number = ui.fonts_mut(|fonts| fonts.layout_job(job));
                                painter.galley(
                                    egui::pos2(
                                        gutter_rect.right() - 12.0 - number.size().x,
                                        line_y,
                                    ),
                                    number,
                                    colors.faint,
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
                            target.min.x -= gutter_width + metrics.editor_gutter_gap;
                            target.min.y -= metrics.editor_line_height * 4.0;
                            target.max.y -= metrics.editor_line_height * 4.0;
                            ui.scroll_to_rect(target, Some(egui::Align::TOP));
                        }
                        edit.response
                    })
                    .inner;
                ui.add_space(metrics.editor_bottom_padding);
                response
            });
        #[cfg(test)]
        ui.ctx()
            .data_mut(|data| data.insert_temp(editor_id.with("scroll_state_id"), response.id));
        let response = response.inner;
        if response.changed() {
            self.mark_modified();
        }
        if response.has_focus() && ui.input(|i| i.modifiers.ctrl && i.key_pressed(egui::Key::S)) {
            self.save_file();
        }
    }
}
