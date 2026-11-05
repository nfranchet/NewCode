//! PTY view rendering, resizing, and focused terminal keyboard input.

use super::EditorApp;
use crate::{
    terminal::Terminal,
    ui_theme::{self as theme, Icon},
};
use eframe::egui::{self, Color32, FontId};
use std::path::Path;

impl EditorApp {
    pub(super) fn terminal_ui(ctx: &egui::Context, ui: &mut egui::Ui, term: &mut Option<Terminal>) {
        if let Some(t) = term {
            t.poll();
            let available = ui.available_size().max(egui::vec2(80.0, 40.0));
            let font = FontId::monospace(14.0);
            let (cell_w, line_h) = ui.fonts(|f| {
                let g = f.layout_no_wrap("M".into(), font.clone(), Color32::WHITE);
                (g.size().x, g.size().y)
            });
            t.resize(
                (available.x / cell_w.max(1.0)).floor() as usize,
                (available.y / line_h.max(1.0)).floor() as usize,
            );
            let (_, rect) = ui.allocate_space(available);
            let id = super::terminal_input::terminal_id();
            let response = ui.interact(rect, id, egui::Sense::click());
            ctx.data_mut(|data| data.insert_temp(id.with("rect"), rect));
            if response.clicked() {
                response.request_focus();
            }
            if response.has_focus() {
                ui.memory_mut(|memory| {
                    memory.set_focus_lock_filter(
                        id,
                        egui::EventFilter {
                            tab: true,
                            horizontal_arrows: true,
                            vertical_arrows: true,
                            escape: true,
                        },
                    )
                });
            }
            ui.painter().rect_filled(rect, 0.0, theme::TERMINAL_BG);
            let painter = ui.painter().with_clip_rect(rect);
            for (y, line) in t.cells.iter().enumerate() {
                for (x, cell) in line.iter().enumerate() {
                    let pos = rect.min + egui::vec2(x as f32 * cell_w, y as f32 * line_h);
                    painter.rect_filled(
                        egui::Rect::from_min_size(pos, egui::vec2(cell_w, line_h)),
                        0.0,
                        cell.bg,
                    );
                }
                for (x, cell) in line.iter().enumerate().filter(|(_, cell)| cell.width > 0) {
                    let pos = rect.min + egui::vec2(x as f32 * cell_w, y as f32 * line_h);
                    let text = format!("{}{}", cell.ch, cell.combining);
                    painter.text(pos, egui::Align2::LEFT_TOP, text, font.clone(), cell.fg);
                }
            }
            if response.has_focus() {
                let cursor = egui::Rect::from_min_size(
                    rect.min
                        + egui::vec2(
                            t.cursor_x.min(t.cols as usize - 1) as f32 * cell_w,
                            t.cursor_y as f32 * line_h,
                        ),
                    egui::vec2(cell_w, line_h),
                );
                painter.rect_stroke(
                    cursor,
                    0.0,
                    egui::Stroke::new(1.0_f32, Color32::WHITE),
                    egui::StrokeKind::Inside,
                );
            }
        } else {
            ui.colored_label(Color32::LIGHT_RED, "Terminal failed to start");
        }
    }
    pub(super) fn shell_panel(
        &mut self,
        ctx: &egui::Context,
        cwd: Option<&Path>,
        runtime: Option<&crate::dev_container::Runtime>,
    ) {
        if let Some(terminal) = self.top.as_mut() {
            terminal.poll();
        }
        let top_height = self.settings.top_height;
        let top_response = egui::TopBottomPanel::bottom("top_terminal")
            .resizable(true)
            .default_height(top_height)
            .min_height(80.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.selectable_value(
                        &mut self.bottom_tab,
                        super::logs::Tab::Terminal,
                        "Terminal",
                    );
                    ui.selectable_value(&mut self.bottom_tab, super::logs::Tab::Logs, "Logs");
                    if self.bottom_tab == super::logs::Tab::Terminal {
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if theme::button(ui, Icon::Refresh, "", theme::BLUE, false)
                                .on_hover_text("Restart terminal")
                                .clicked()
                            {
                                let command = self
                                    .top
                                    .as_ref()
                                    .map(|terminal| terminal.command.clone())
                                    .unwrap_or_default();
                                self.top = Terminal::spawn_with_runtime(
                                    &command,
                                    cwd,
                                    runtime,
                                    ctx.clone(),
                                )
                                .ok();
                            }
                        });
                    }
                });
                ui.separator();
                if self.bottom_tab == super::logs::Tab::Terminal {
                    Self::terminal_ui(ctx, ui, &mut self.top);
                } else {
                    ctx.memory_mut(|memory| {
                        memory.surrender_focus(super::terminal_input::terminal_id())
                    });
                    ctx.data_mut(|data| {
                        data.remove::<egui::Rect>(super::terminal_input::terminal_id().with("rect"))
                    });
                    self.logs.ui(ui);
                }
            });
        self.settings.top_height = top_response.response.rect.height().max(80.0);
    }
}
