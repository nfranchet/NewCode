//! PTY view rendering, resizing, and focused terminal keyboard input.

use super::EditorApp;
use crate::{
    terminal::Terminal,
    ui_theme::{self as theme, Icon},
};
use eframe::egui::{self, Color32, FontId, TextFormat, text::LayoutJob};
use std::path::Path;

impl EditorApp {
    pub(super) fn terminal_ui(
        ctx: &egui::Context,
        ui: &mut egui::Ui,
        title: &str,
        term: &mut Option<Terminal>,
        cwd: Option<&Path>,
    ) {
        ui.horizontal(|ui| {
            theme::icon(ui, Icon::Terminal, theme::BLUE);
            ui.label(egui::RichText::new(title).strong().color(theme::BLUE));
            if theme::button(ui, Icon::Refresh, "Restart", theme::BLUE, false).clicked() {
                let cmd = term.as_ref().map(|t| t.command.clone()).unwrap_or_default();
                *term = Terminal::spawn(&cmd, cwd, ctx.clone()).ok();
            }
        });
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
            let (rect, response) = ui.allocate_exact_size(available, egui::Sense::click());
            if response.clicked() {
                response.request_focus();
            }
            t.focused = response.has_focus();
            ui.painter().rect_filled(rect, 0.0, theme::TERMINAL_BG);
            for (y, line) in t.cells.iter().enumerate() {
                let mut job = LayoutJob::default();
                for cell in line {
                    job.append(
                        &cell.ch.to_string(),
                        0.0,
                        TextFormat {
                            font_id: font.clone(),
                            color: cell.fg,
                            background: cell.bg,
                            ..Default::default()
                        },
                    );
                }
                let galley = ui.fonts(|f| f.layout_job(job));
                ui.painter().galley(
                    rect.min + egui::vec2(0.0, y as f32 * line_h),
                    galley,
                    Color32::WHITE,
                );
            }
            if t.focused {
                let cursor = egui::Rect::from_min_size(
                    rect.min + egui::vec2(t.cursor_x as f32 * cell_w, t.cursor_y as f32 * line_h),
                    egui::vec2(cell_w, line_h),
                );
                ui.painter().rect_stroke(
                    cursor,
                    0.0,
                    egui::Stroke::new(1.0_f32, Color32::WHITE),
                    egui::StrokeKind::Inside,
                );
                for event in ui.input(|i| i.events.clone()) {
                    match event {
                        egui::Event::Text(s) | egui::Event::Paste(s) => t.send(s.as_bytes()),
                        egui::Event::Key {
                            key,
                            pressed: true,
                            modifiers,
                            ..
                        } => {
                            let bytes: &[u8] = if modifiers.ctrl || modifiers.command {
                                match key {
                                    egui::Key::C => &[3],
                                    egui::Key::D => &[4],
                                    egui::Key::Z => &[26],
                                    egui::Key::A => &[1],
                                    egui::Key::E => &[5],
                                    egui::Key::L => &[12],
                                    _ => &[],
                                }
                            } else {
                                match key {
                                    egui::Key::Enter => b"\r",
                                    egui::Key::Backspace => b"\x7f",
                                    egui::Key::Tab => b"\t",
                                    egui::Key::Escape => b"\x1b",
                                    egui::Key::ArrowUp => b"\x1b[A",
                                    egui::Key::ArrowDown => b"\x1b[B",
                                    egui::Key::ArrowRight => b"\x1b[C",
                                    egui::Key::ArrowLeft => b"\x1b[D",
                                    egui::Key::Home => b"\x1b[H",
                                    egui::Key::End => b"\x1b[F",
                                    egui::Key::Delete => b"\x1b[3~",
                                    egui::Key::PageUp => b"\x1b[5~",
                                    egui::Key::PageDown => b"\x1b[6~",
                                    _ => &[],
                                }
                            };
                            if !bytes.is_empty() {
                                t.send(bytes);
                            }
                        }
                        _ => {}
                    }
                }
            }
        } else {
            ui.colored_label(Color32::LIGHT_RED, "Terminal failed to start");
        }
    }
    pub(super) fn shell_panel(&mut self, ctx: &egui::Context, cwd: Option<&Path>) {
        let top_height = self.settings.top_height;
        let top_response = egui::TopBottomPanel::bottom("top_terminal")
            .resizable(true)
            .default_height(top_height)
            .min_height(80.0)
            .show(ctx, |ui| {
                Self::terminal_ui(ctx, ui, "Shell", &mut self.top, cwd);
            });
        self.settings.top_height = top_response.response.rect.height().max(80.0);
    }
}
