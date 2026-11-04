//! Compact lifecycle and workspace session controls.
use super::{OpenCodeUi, short, string};
use crate::ui_theme as theme;
use eframe::egui;
impl OpenCodeUi {
    pub(super) fn header(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        ui.horizontal_wrapped(|ui| {
            ui.strong("OpenCode");
            let state = self.state();
            theme::badge(ui, state, theme::BLUE, theme::BLUE_SOFT).on_hover_text(&self.status);
            if self.server.is_some() {
                if ui.small_button("Stop").clicked() {
                    self.stop();
                }
            } else if ui.button("Start").clicked() {
                let cwd = self.workspace.clone();
                self.start(Some(&cwd), ctx.clone());
            }
            theme::wrap_control(ui, 195.0);
            ui.add_enabled_ui(self.online && !self.locked(), |ui| {
                if ui.small_button("+").on_hover_text("New session").clicked() {
                    self.select_session(None);
                }
                let current = self
                    .sessions
                    .iter()
                    .find(|s| s["id"].as_str() == self.session.as_deref());
                let label = current
                    .map(|s| short(string(s, "title"), 18))
                    .unwrap_or_else(|| "Choose session…".into());
                let mut selected = None;
                egui::ComboBox::from_id_salt("opencode_sessions")
                    .width(110.0)
                    .truncate()
                    .selected_text(label)
                    .show_ui(ui, |ui| {
                        ui.weak("Sessions in this workspace");
                        for session in &self.sessions {
                            let id = string(session, "id");
                            if ui
                                .selectable_label(
                                    self.session.as_deref() == Some(id),
                                    short(string(session, "title"), 100),
                                )
                                .on_hover_text(format!("{}\n{id}", string(session, "title")))
                                .clicked()
                            {
                                selected = Some(id.to_string());
                            }
                        }
                    });
                if let Some(id) = selected {
                    if self.session.as_ref() != Some(&id) {
                        self.select_session(Some(id));
                    }
                }
                if ui
                    .small_button("↻")
                    .on_hover_text("Refresh sessions and conversation")
                    .clicked()
                {
                    self.list_sessions(false);
                    self.refresh();
                }
                if ui
                    .small_button("…")
                    .on_hover_text("Load older sessions")
                    .clicked()
                {
                    self.session_limit += 50;
                    self.list_sessions(false);
                }
            });
        });
        if let Some(error) = &self.error {
            ui.colored_label(theme::RED, error);
        } else if !self.online {
            ui.add(
                egui::Label::new(
                    egui::RichText::new(&self.status)
                        .small()
                        .color(theme::MUTED),
                )
                .wrap(),
            );
        }
        if self.online && self.models.is_empty() {
            ui.colored_label(theme::AMBER,"No connected models. Configure and authenticate a provider using the OpenCode CLI, then restart this connection.");
        }
        if self.online && self.agents.is_empty() {
            ui.colored_label(
                theme::AMBER,
                "No primary agents available in the OpenCode configuration.",
            );
        }
    }
}
