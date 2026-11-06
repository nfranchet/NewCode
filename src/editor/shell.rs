//! PTY view rendering, resizing, and focused terminal keyboard input.

use super::EditorApp;
use crate::{
    terminal::Terminal,
    ui_theme::{self as theme, Icon},
};
use eframe::egui::{self, Color32};
use std::path::Path;

impl EditorApp {
    pub(super) fn terminal_ui(ui: &mut egui::Ui, term: &mut Option<Terminal>, error: Option<&str>) {
        if let Some(terminal) = term {
            let style = egui_tty::TerminalStyle {
                font: crate::terminal_fonts::font_id(),
                scheme: egui_tty::ColorScheme::Dark,
                ..Default::default()
            };
            terminal.poll();
            terminal.view.set_color_scheme(style.scheme);
            if let Ok((_, background)) = terminal.view.drawn_colors() {
                ui.painter()
                    .rect_filled(ui.available_rect_before_wrap(), 0.0, background);
            }
            let response = terminal.view.ui(ui, &style);
            let id = super::terminal_input::terminal_id();
            ui.ctx().data_mut(|data| {
                data.insert_temp(id.with("widget"), response.id);
                data.insert_temp(id.with("rect"), response.rect);
            });
            if response.has_focus() {
                // The widget has encoded these; do not let another panel handle them again.
                ui.input_mut(|input| {
                    input.events.retain(|event| {
                        !matches!(
                            event,
                            egui::Event::Key { .. }
                                | egui::Event::Text(_)
                                | egui::Event::Copy
                                | egui::Event::Cut
                                | egui::Event::Paste(_)
                                | egui::Event::Ime(_)
                        )
                    })
                });
            }
        } else {
            ui.colored_label(
                Color32::LIGHT_RED,
                error.unwrap_or("Terminal failed to start"),
            );
        }
    }
    pub(super) fn restart_terminal(
        &mut self,
        ctx: &egui::Context,
        cwd: Option<&Path>,
        runtime: Option<&crate::dev_container::Runtime>,
    ) {
        let command = self
            .top
            .as_ref()
            .map(|terminal| terminal.command.clone())
            .unwrap_or_default();
        drop(self.top.take());
        match Terminal::spawn_with_runtime(&command, cwd, runtime, ctx.clone()) {
            Ok(terminal) => {
                self.top = Some(terminal);
                self.terminal_error = None;
            }
            Err(error) => {
                log::warn!("Terminal startup failed");
                self.terminal_error = Some(format!("Terminal failed to start: {error}"));
            }
        }
    }
    pub(super) fn shell_panel(
        &mut self,
        root: &mut egui::Ui,
        cwd: Option<&Path>,
        runtime: Option<&crate::dev_container::Runtime>,
    ) {
        let context = root.ctx().clone();
        let ctx = &context;
        if let Some(terminal) = self.top.as_mut() {
            terminal.poll();
        }
        let top_height = self.settings.top_height;
        let top_response = egui::Panel::bottom("top_terminal")
            .resizable(true)
            .default_size(top_height)
            .min_size(80.0)
            .show(root, |ui| {
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
                                self.restart_terminal(ctx, cwd, runtime);
                            }
                        });
                    }
                });
                ui.separator();
                if self.bottom_tab == super::logs::Tab::Terminal {
                    Self::terminal_ui(ui, &mut self.top, self.terminal_error.as_deref());
                } else {
                    let widget_id = ctx.data(|data| {
                        data.get_temp::<egui::Id>(
                            super::terminal_input::terminal_id().with("widget"),
                        )
                    });
                    if let Some(id) = widget_id {
                        ctx.memory_mut(|memory| memory.surrender_focus(id));
                    }
                    ctx.data_mut(|data| {
                        data.remove::<egui::Rect>(super::terminal_input::terminal_id().with("rect"))
                    });
                    self.logs.ui(ui);
                }
            });
        self.settings.top_height = top_response.response.rect.height().max(80.0);
    }
}
