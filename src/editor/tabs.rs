//! File tab rendering and activation without reloading unsaved buffers.
use super::EditorApp;
use crate::ui_theme as theme;
use eframe::egui::{self, Color32, FontId};
impl EditorApp {
    pub(super) fn document_tabs(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        if !self.open_files.is_empty() {
            let mut close_tab = None;
            let mut switch_tab = None;
            let mut pin_tab = None;
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
                                        let dirty = self.file_is_dirty(p);
                                        let preview = self.preview_file.as_ref() == Some(p);
                                        let label = format!(
                                            "{}{}",
                                            p.file_name().unwrap_or_default().to_string_lossy(),
                                            if dirty { " •" } else { "" }
                                        );
                                        let font = FontId::proportional(13.0);
                                        let color = if dirty {
                                            Color32::from_rgb(176, 112, 0)
                                        } else {
                                            theme::INK
                                        };
                                        let job = egui::text::LayoutJob::single_section(
                                            format!("{}  {label}", theme::file_kind(p).0),
                                            egui::TextFormat {
                                                font_id: font.clone(),
                                                color,
                                                italics: preview,
                                                ..Default::default()
                                            },
                                        );
                                        let galley = ui.fonts(|fonts| fonts.layout_job(job));
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
                                        let tooltip = if preview {
                                            format!(
                                                "{}\nPreview — double-click to keep open",
                                                p.display()
                                            )
                                        } else {
                                            p.display().to_string()
                                        };
                                        let select = select.on_hover_text(tooltip);
                                        let close = close.on_hover_text("Close file (Ctrl+W)");
                                        if select.double_clicked() {
                                            pin_tab = Some(p.clone());
                                        }
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
                                    .selectable_label(selected, {
                                        let suffix =
                                            if self.file_is_dirty(path) { " •" } else { "" };
                                        let text = egui::RichText::new(format!(
                                            "{name}{suffix}\n{full_path}"
                                        ));
                                        if self.preview_file.as_ref() == Some(path) {
                                            text.italics()
                                        } else {
                                            text
                                        }
                                    })
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
                    self.activate_file(path);
                }
            }
            if let Some(path) = pin_tab {
                self.keep_preview(&path);
            }
            if let Some(path) = close_tab {
                self.close_file(path);
            }
            ui.separator();
        }
    }
}
