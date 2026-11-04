//! Document lifecycle, clipboard commands, file tabs, and close confirmation.

use super::EditorApp;
use crate::ui_theme as theme;
use eframe::egui::{
    self, Color32, FontId,
    text::{CCursor, CCursorRange},
};
use std::{fs, path::PathBuf};

impl EditorApp {
    pub(super) fn open_file(&mut self, path: PathBuf) {
        match fs::read_to_string(&path) {
            Ok(s) => {
                if !self.open_files.contains(&path) {
                    self.open_files.push(path.clone());
                }
                self.active_file = Some(path);
                self.buffer = s;
                self.dirty = false;
                self.reveal_active_tab = true;
                self.status = "File opened".into();
            }
            Err(e) => self.status = format!("Could not open file: {e}"),
        }
    }

    pub(super) fn save_file(&mut self) {
        if let Some(p) = &self.active_file {
            match fs::write(p, &self.buffer) {
                Ok(()) => {
                    self.dirty = false;
                    self.status = format!("Saved {}", p.display());
                }
                Err(e) => self.status = format!("Save failed: {e}"),
            }
        }
    }

    pub(super) fn close_file(&mut self, path: PathBuf) {
        if self.active_file.as_ref() == Some(&path) && self.dirty {
            self.close_confirmation = Some(path);
        } else {
            self.finish_close_file(path);
        }
    }

    pub(super) fn finish_close_file(&mut self, path: PathBuf) {
        let Some(index) = self.open_files.iter().position(|open| *open == path) else {
            return;
        };
        let was_active = self.active_file.as_ref() == Some(&path);
        self.open_files.remove(index);
        if was_active {
            self.active_file = None;
            self.buffer.clear();
            self.dirty = false;
            self.pending_editor_cursor = None;
            self.find_open = false;
            self.find_active = None;
            if !self.open_files.is_empty() {
                let next = self.open_files[index.min(self.open_files.len() - 1)].clone();
                self.open_file(next);
            } else {
                self.status = "File closed".into();
            }
        } else {
            self.status = "File closed".into();
        }
    }

    pub(super) fn copy_selection(&mut self, ctx: &egui::Context) {
        let id = egui::Id::new("main_editor_text");
        let Some(range) = egui::TextEdit::load_state(ctx, id)
            .and_then(|state| state.cursor.char_range())
            .filter(|range| !range.is_empty())
        else {
            self.status = "Select text in the editor to copy".into();
            return;
        };
        let range = range.as_sorted_char_range();
        let copied: String = self
            .buffer
            .chars()
            .skip(range.start)
            .take(range.end - range.start)
            .collect();
        ctx.copy_text(copied);
        self.status = "Copied selection".into();
    }

    pub(super) fn paste_from_clipboard(&mut self, ctx: &egui::Context) {
        ctx.memory_mut(|memory| memory.request_focus(egui::Id::new("main_editor_text")));
        ctx.send_viewport_cmd(egui::ViewportCommand::RequestPaste);
        self.status = "Paste requested".into();
    }

    pub(super) fn select_all(&mut self, ctx: &egui::Context) {
        let id = egui::Id::new("main_editor_text");
        let mut state = egui::TextEdit::load_state(ctx, id).unwrap_or_default();
        state.cursor.set_char_range(Some(CCursorRange::two(
            CCursor::new(0),
            CCursor::new(self.buffer.chars().count()),
        )));
        egui::TextEdit::store_state(ctx, id, state);
        ctx.memory_mut(|memory| memory.request_focus(id));
    }
    pub(super) fn document_tabs(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        if !self.open_files.is_empty() {
            let mut close_tab = None;
            let mut switch_tab = None;
            let open_files = self.open_files.clone();
            let active_file = self.active_file.clone();
            ui.horizontal(|ui| {
                let tabs_width = (ui.available_width() - 48.0).max(64.0);
                ui.allocate_ui_with_layout(
                    egui::vec2(tabs_width, 34.0),
                    egui::Layout::left_to_right(egui::Align::Center),
                    |ui| {
                        let mut active_tab_rect = None;
                        let tab_scroll = egui::ScrollArea::horizontal()
                            .id_salt("open_file_tabs")
                            .auto_shrink([false, false])
                            .show(ui, |ui| {
                                ui.horizontal(|ui| {
                                    for p in &open_files {
                                        let active = active_file.as_ref() == Some(p);
                                        let dirty = active && self.dirty;
                                        let label = format!(
                                            "{}{}",
                                            p.file_name().unwrap_or_default().to_string_lossy(),
                                            if dirty { " •" } else { "" }
                                        );
                                        let font = FontId::proportional(13.0);
                                        let galley = ui.painter().layout_no_wrap(
                                            format!("{}  {label}", theme::file_kind(p).0),
                                            font.clone(),
                                            if dirty {
                                                Color32::from_rgb(176, 112, 0)
                                            } else {
                                                theme::INK
                                            },
                                        );
                                        let close_width = 26.0;
                                        let padding = 12.0;
                                        let size = egui::vec2(
                                            galley.size().x + close_width + padding,
                                            30.0,
                                        );
                                        let (rect, _) =
                                            ui.allocate_exact_size(size, egui::Sense::hover());
                                        let tab_id = egui::Id::new(("file_tab", p));
                                        let label_rect = egui::Rect::from_min_max(
                                            rect.min,
                                            egui::pos2(rect.max.x - close_width, rect.max.y),
                                        );
                                        let close_rect = egui::Rect::from_min_max(
                                            egui::pos2(rect.max.x - close_width, rect.min.y),
                                            rect.max,
                                        );
                                        let select =
                                            ui.interact(label_rect, tab_id, egui::Sense::click());
                                        let close = ui.interact(
                                            close_rect,
                                            tab_id.with("close"),
                                            egui::Sense::click(),
                                        );
                                        let fill = if active {
                                            Color32::WHITE
                                        } else if select.hovered() || close.hovered() {
                                            theme::BLUE_SOFT
                                        } else {
                                            theme::SURFACE
                                        };
                                        ui.painter().rect_filled(
                                            rect,
                                            egui::CornerRadius::same(3),
                                            fill,
                                        );
                                        if active {
                                            ui.painter().rect_filled(
                                                egui::Rect::from_min_size(
                                                    rect.min,
                                                    egui::vec2(rect.width(), 3.0),
                                                ),
                                                1.0,
                                                theme::BLUE,
                                            );
                                            ui.painter().rect_stroke(
                                                rect,
                                                egui::CornerRadius::same(3),
                                                egui::Stroke::new(1.0_f32, theme::BORDER),
                                                egui::StrokeKind::Inside,
                                            );
                                        }
                                        ui.painter().galley(
                                            rect.min
                                                + egui::vec2(
                                                    padding * 0.5,
                                                    (size.y - galley.size().y) * 0.5,
                                                ),
                                            galley,
                                            if dirty {
                                                Color32::from_rgb(176, 112, 0)
                                            } else {
                                                theme::INK
                                            },
                                        );
                                        ui.painter().text(
                                            close_rect.center(),
                                            egui::Align2::CENTER_CENTER,
                                            "×",
                                            FontId::proportional(16.0),
                                            theme::MUTED,
                                        );
                                        let select = select.on_hover_text(p.display().to_string());
                                        let close = close.on_hover_text("Close file (Ctrl+W)");
                                        if select.clicked() {
                                            switch_tab = Some(p.clone());
                                        }
                                        if close.clicked() {
                                            close_tab = Some(p.clone());
                                        }
                                        if active {
                                            active_tab_rect = Some(rect);
                                        }
                                    }
                                });
                            });
                        if self.reveal_active_tab {
                            if let Some(rect) = active_tab_rect {
                                let mut state = tab_scroll.state;
                                let viewport = tab_scroll.inner_rect;
                                let tab_left = rect.left() - viewport.left() + state.offset.x;
                                let tab_right = rect.right() - viewport.left() + state.offset.x;
                                if tab_left < state.offset.x {
                                    state.offset.x = tab_left;
                                } else if tab_right > state.offset.x + viewport.width() {
                                    state.offset.x = tab_right - viewport.width();
                                }
                                state.offset.x = state.offset.x.clamp(
                                    0.0,
                                    (tab_scroll.content_size.x - viewport.width()).max(0.0),
                                );
                                state.store(ctx, tab_scroll.id);
                            }
                            self.reveal_active_tab = false;
                        }
                    },
                );
                ui.menu_button(format!("⌄ {}", open_files.len()), |ui| {
                    ui.set_min_width(240.0);
                    ui.add(
                        egui::TextEdit::singleline(&mut self.open_file_filter)
                            .hint_text("Find an open file…")
                            .desired_width(f32::INFINITY),
                    );
                    let filter = self.open_file_filter.to_lowercase();
                    egui::ScrollArea::vertical()
                        .max_height(360.0)
                        .show(ui, |ui| {
                            for path in &open_files {
                                let full_path = path.display().to_string();
                                if !filter.is_empty() && !full_path.to_lowercase().contains(&filter)
                                {
                                    continue;
                                }
                                let name = path.file_name().unwrap_or_default().to_string_lossy();
                                let selected = active_file.as_ref() == Some(path);
                                if ui
                                    .selectable_label(selected, format!("{name}\n{full_path}"))
                                    .clicked()
                                {
                                    switch_tab = Some(path.clone());
                                    ui.close();
                                }
                            }
                        });
                });
            });
            if let Some(path) = switch_tab {
                self.git_ui.dismiss_diff();
                if self.active_file.as_ref() != Some(&path) {
                    self.open_file(path);
                }
            }
            if let Some(path) = close_tab {
                self.close_file(path);
            }
            ui.separator();
        }
    }
    pub(super) fn close_dialog(&mut self, ctx: &egui::Context) {
        if let Some(path) = self.close_confirmation.clone() {
            egui::Window::new("Unsaved changes")
                .collapsible(false)
                .resizable(false)
                .show(ctx, |ui| {
                    ui.label(format!(
                        "Save changes to {} before closing?",
                        path.file_name().unwrap_or_default().to_string_lossy()
                    ));
                    ui.horizontal(|ui| {
                        if ui.button("Save").clicked() {
                            self.save_file();
                            if !self.dirty {
                                self.close_confirmation = None;
                                self.finish_close_file(path.clone());
                            }
                        }
                        if ui.button("Discard").clicked() {
                            self.close_confirmation = None;
                            self.finish_close_file(path.clone());
                        }
                        if ui.button("Cancel").clicked() {
                            self.close_confirmation = None;
                        }
                    });
                });
        }
    }
}
