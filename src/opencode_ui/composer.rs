//! Send controls and backend-supplied model, variant, and agent choices.
use super::{OpenCodeUi, Operation, short};
use eframe::egui;
impl OpenCodeUi {
    pub(super) fn composer(&mut self, ui: &mut egui::Ui) {
        let id = ui.make_persistent_id("opencode_composer");
        if self.can_send() && ui.memory(|m| m.has_focus(id)) {
            let backwards = ui.input_mut(|i| {
                if i.consume_key(egui::Modifiers::SHIFT, egui::Key::Tab) {
                    Some(true)
                } else if i.consume_key(egui::Modifiers::NONE, egui::Key::Tab) {
                    Some(false)
                } else {
                    None
                }
            });
            if let Some(backwards) = backwards {
                if !self.agents.is_empty() {
                    let current = self
                        .agents
                        .iter()
                        .position(|a| a == &self.agent)
                        .unwrap_or(0);
                    self.agent = self.agents[(current
                        + if backwards { self.agents.len() - 1 } else { 1 })
                        % self.agents.len()]
                    .clone();
                }
            }
        }
        ui.separator();
        let input = ui.add(
            egui::TextEdit::multiline(&mut self.input)
                .id(id)
                .lock_focus(true)
                .hint_text("Message OpenCode…")
                .desired_rows(3)
                .desired_width(f32::INFINITY),
        );
        let send_key = input.has_focus()
            && ui.input_mut(|i| i.consume_key(egui::Modifiers::CTRL, egui::Key::Enter));
        ui.horizontal_wrapped(|ui| {
            if ui
                .add_enabled(
                    self.can_send() && !self.input.trim().is_empty(),
                    egui::Button::new("Send"),
                )
                .on_hover_text("Ctrl+Enter to send")
                .clicked()
                || send_key
            {
                self.submit(false);
            }
            if self.busy
                && ui
                    .add_enabled(
                        !self
                            .requests
                            .values()
                            .any(|p| matches!(p.operation, Operation::Abort)),
                        egui::Button::new("Interrupt"),
                    )
                    .clicked()
            {
                if let Some(id) = &self.session {
                    self.request(
                        Operation::Abort,
                        "POST",
                        format!("/session/{id}/abort"),
                        None,
                    );
                }
            }
            self.selectors(ui);
        });
    }
    fn selectors(&mut self, ui: &mut egui::Ui) {
        let enabled = self.online && !self.locked();
        crate::ui_theme::wrap_control(ui, 145.0);
        let label = self
            .selected_model()
            .map(|m| short(&m.label, 22))
            .unwrap_or_else(|| "Model…".into());
        let old_model = self.model.clone();
        ui.add_enabled_ui(enabled, |ui| {
            egui::ComboBox::from_id_salt("opencode_model")
                .width(135.0)
                .truncate()
                .selected_text(label)
                .show_ui(ui, |ui| {
                    for model in &self.models {
                        ui.selectable_value(
                            &mut self.model,
                            Some((model.provider.clone(), model.id.clone())),
                            &model.label,
                        );
                    }
                });
        });
        if self.model != old_model {
            self.validate_variant();
        }
        let variants = self
            .selected_model()
            .map(|m| m.variants.clone())
            .unwrap_or_default();
        crate::ui_theme::wrap_control(ui, 110.0);
        ui.add_enabled_ui(enabled && !variants.is_empty(), |ui| {
            egui::ComboBox::from_id_salt("opencode_variant")
                .width(100.0)
                .truncate()
                .selected_text(if self.variant.is_empty() {
                    "Thinking: Default"
                } else {
                    &self.variant
                })
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut self.variant, String::new(), "Default");
                    for variant in variants {
                        ui.selectable_value(&mut self.variant, variant.clone(), variant);
                    }
                });
        });
        crate::ui_theme::wrap_control(ui, 95.0);
        ui.add_enabled_ui(enabled, |ui| {
            egui::ComboBox::from_id_salt("opencode_agent")
                .width(85.0)
                .truncate()
                .selected_text(if self.agent.is_empty() {
                    "Agent…"
                } else {
                    &self.agent
                })
                .show_ui(ui, |ui| {
                    for agent in &self.agents {
                        ui.selectable_value(&mut self.agent, agent.clone(), agent);
                    }
                });
        });
    }
}
