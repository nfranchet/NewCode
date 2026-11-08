//! File tab rendering and activation without reloading unsaved buffers.
use super::EditorApp;
use super::close_dialog::CloseTabs;
use crate::ui_theme as theme;
use eframe::egui::{self, FontId};
impl EditorApp {
    pub(super) fn document_tabs(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        if !self.open_files.is_empty() {
            let mut close_tab = None;
            let mut switch_tab = None;
            let mut pin_tab = None;
            let open_files = self.open_files.clone();
            let active_file = self.active_file.clone();
            egui::Frame::new()
                .fill(theme::colors(ui).chrome)
                .show(ui, |ui| {
                    ui.spacing_mut().item_spacing = egui::Vec2::ZERO;
                    ui.set_min_width(ui.available_width());
                    ui.horizontal(|ui| {
                        let tabs_width = (ui.available_width() - 48.0).max(64.0);
                        ui.allocate_ui_with_layout(
                            egui::vec2(tabs_width, theme::metrics(ui).tab_height),
                            egui::Layout::left_to_right(egui::Align::Min),
                            |ui| {
                                let mut active_tab_rect = None;
                                let tab_scroll = egui::ScrollArea::horizontal()
                                    .id_salt("open_file_tabs")
                                    .auto_shrink([false, false])
                                    .scroll_bar_visibility(
                                        egui::scroll_area::ScrollBarVisibility::AlwaysHidden,
                                    )
                                    .show(ui, |ui| {
                                        ui.horizontal_top(|ui| {
                                            for p in &open_files {
                                                let active = active_file.as_ref() == Some(p);
                                                let dirty = self.file_is_dirty(p);
                                                let preview = self.preview_file.as_ref() == Some(p);
                                                let label = format!(
                                                    "{}{}",
                                                    p.file_name()
                                                        .unwrap_or_default()
                                                        .to_string_lossy(),
                                                    if dirty { " •" } else { "" }
                                                );
                                                let font = FontId::proportional(11.0);
                                                let color = if dirty {
                                                    theme::colors(ui).warning
                                                } else {
                                                    theme::colors(ui).text
                                                };
                                                let mut job = theme::file_text(ui, p, &label);
                                                for section in &mut job.sections {
                                                    section.format.font_id = font.clone();
                                                    section.format.italics = preview;
                                                }
                                                if let Some(filename) = job.sections.last_mut() {
                                                    filename.format.color = color;
                                                }
                                                let galley =
                                                    ui.fonts_mut(|fonts| fonts.layout_job(job));
                                                let close_width = 26.0;
                                                let padding = 12.0;
                                                let size = egui::vec2(
                                                    (galley.size().x + close_width + 2.0 * padding)
                                                        .max(theme::metrics(ui).tab_min_width),
                                                    theme::metrics(ui).tab_height,
                                                );
                                                let (rect, _) = ui.allocate_exact_size(
                                                    size,
                                                    egui::Sense::hover(),
                                                );
                                                let tab_id = egui::Id::new(("file_tab", p));
                                                let label_rect = egui::Rect::from_min_max(
                                                    rect.min,
                                                    egui::pos2(
                                                        rect.max.x - close_width,
                                                        rect.max.y,
                                                    ),
                                                );
                                                let close_rect = egui::Rect::from_min_max(
                                                    egui::pos2(
                                                        rect.max.x - close_width,
                                                        rect.min.y,
                                                    ),
                                                    rect.max,
                                                );
                                                let select = ui.interact(
                                                    label_rect,
                                                    tab_id,
                                                    egui::Sense::click(),
                                                );
                                                let close = ui.interact(
                                                    close_rect,
                                                    tab_id.with("close"),
                                                    egui::Sense::click(),
                                                );
                                                let fill = if active {
                                                    theme::colors(ui).editor
                                                } else if select.hovered() || close.hovered() {
                                                    theme::colors(ui).hover
                                                } else {
                                                    theme::colors(ui).chrome
                                                };
                                                ui.painter().rect_filled(
                                                    rect,
                                                    ui.visuals().widgets.inactive.corner_radius,
                                                    fill,
                                                );
                                                ui.painter().vline(
                                                    rect.right(),
                                                    rect.y_range(),
                                                    egui::Stroke::new(
                                                        1.0,
                                                        theme::colors(ui).border,
                                                    ),
                                                );
                                                if active {
                                                    ui.painter().rect_filled(
                                                        egui::Rect::from_min_size(
                                                            rect.min,
                                                            egui::vec2(rect.width(), 2.0),
                                                        ),
                                                        0.0,
                                                        theme::colors(ui).accent,
                                                    );
                                                    ui.painter().rect_stroke(
                                                        rect,
                                                        ui.visuals().widgets.inactive.corner_radius,
                                                        egui::Stroke::new(
                                                            1.0_f32,
                                                            theme::colors(ui).border,
                                                        ),
                                                        egui::StrokeKind::Inside,
                                                    );
                                                }
                                                ui.painter().galley(
                                                    rect.min
                                                        + egui::vec2(
                                                            padding,
                                                            (size.y - galley.size().y) * 0.5,
                                                        ),
                                                    galley,
                                                    if dirty {
                                                        theme::colors(ui).warning
                                                    } else {
                                                        theme::colors(ui).text
                                                    },
                                                );
                                                ui.painter().text(
                                                    close_rect.center(),
                                                    egui::Align2::CENTER_CENTER,
                                                    "×",
                                                    FontId::proportional(16.0),
                                                    theme::colors(ui).muted,
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
                                                let close =
                                                    close.on_hover_text("Close file (Ctrl+W)");
                                                select.context_menu(|ui| {
                                                    ui.add_enabled_ui(
                                                        self.close_confirmation.is_none(),
                                                        |ui| {
                                                            if ui.button("Close file").clicked() {
                                                                close_tab = Some(CloseTabs::File(
                                                                    p.clone(),
                                                                ));
                                                                ui.close();
                                                            }
                                                            if ui
                                                                .add_enabled(
                                                                    open_files.len() > 1,
                                                                    egui::Button::new(
                                                                        "Close others",
                                                                    ),
                                                                )
                                                                .clicked()
                                                            {
                                                                close_tab = Some(
                                                                    CloseTabs::Others(p.clone()),
                                                                );
                                                                ui.close();
                                                            }
                                                            if ui.button("Close all").clicked() {
                                                                close_tab = Some(CloseTabs::All);
                                                                ui.close();
                                                            }
                                                        },
                                                    );
                                                });
                                                if select.double_clicked() {
                                                    pin_tab = Some(p.clone());
                                                }
                                                if select.clicked() {
                                                    switch_tab = Some(p.clone());
                                                }
                                                if close.clicked() {
                                                    close_tab = Some(CloseTabs::File(p.clone()));
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
                                        let tab_left =
                                            rect.left() - viewport.left() + state.offset.x;
                                        let tab_right =
                                            rect.right() - viewport.left() + state.offset.x;
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
                        ui.menu_button(format!("… {}", open_files.len()), |ui| {
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
                                        if !filter.is_empty()
                                            && !full_path.to_lowercase().contains(&filter)
                                        {
                                            continue;
                                        }
                                        let name =
                                            path.file_name().unwrap_or_default().to_string_lossy();
                                        let selected = active_file.as_ref() == Some(path);
                                        if ui
                                            .selectable_label(selected, {
                                                let suffix = if self.file_is_dirty(path) {
                                                    " •"
                                                } else {
                                                    ""
                                                };
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
            if let Some(action) = close_tab {
                self.close_tabs(action);
            }
        }
    }
}
