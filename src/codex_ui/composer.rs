//! Message input, shortcuts, model/thinking/mode selectors, and composer sizing.

use super::{CodexUi, catalog::resolve_effort};
use crate::ui_theme::{self as theme, Icon};
use eframe::egui;
use serde_json::{Value, json};

impl CodexUi {
    pub(super) fn show_composer(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        ui.separator();
        let composer_top = ui.cursor().top();
        let connected = self.thread_id.is_some()
            && self.server.is_some()
            && !self.requests.values().any(|m| {
                matches!(
                    m.as_str(),
                    "thread/start" | "thread/resume" | "collaborationMode/list"
                )
            });
        let composer_id = ui.make_persistent_id("codex_composer");
        if connected && ui.memory(|m| m.has_focus(composer_id)) {
            let direction = ui.input_mut(|input| {
                if input.consume_key(egui::Modifiers::SHIFT, egui::Key::Tab) {
                    Some(true)
                } else if input.consume_key(egui::Modifiers::NONE, egui::Key::Tab) {
                    Some(false)
                } else {
                    None
                }
            });
            if let Some(backward) = direction {
                if !self.busy && self.pending.is_empty() && !self.modes.is_empty() {
                    let current = self
                        .modes
                        .iter()
                        .position(|mode| self.mode.as_ref() == Some(mode))
                        .or_else(|| {
                            self.modes.iter().position(|mode| {
                                mode.get("mode").and_then(Value::as_str) == Some("default")
                            })
                        })
                        .unwrap_or(0);
                    let next = if backward {
                        (current + self.modes.len() - 1) % self.modes.len()
                    } else {
                        (current + 1) % self.modes.len()
                    };
                    self.mode = Some(self.modes[next].clone());
                }
            }
        }
        let composer = ui.add_enabled(
            connected,
            egui::TextEdit::multiline(&mut self.input)
                .id(composer_id)
                .lock_focus(true)
                .hint_text("Message Codex…")
                .desired_width(f32::INFINITY)
                .desired_rows(3),
        );
        ui.horizontal_wrapped(|ui| {
            let can_send = connected && !self.busy && !self.input.trim().is_empty();
            if ui
                .add_enabled_ui(can_send, |ui| {
                    theme::button(ui, Icon::Send, "Send", theme::TEAL, true)
                })
                .inner
                .clicked()
                || (can_send
                    && composer.has_focus()
                    && ui.input_mut(|i| i.consume_key(egui::Modifiers::CTRL, egui::Key::Enter)))
            {
                self.send_turn();
            }
            if self.busy {
                if theme::button(ui, Icon::Stop, "Interrupt", theme::AMBER, false).clicked() {
                    if let (Some(thread_id), Some(turn_id)) = (&self.thread_id, &self.turn_id) {
                        let _ = self.send(
                            "turn/interrupt",
                            json!({"threadId":thread_id,"turnId":turn_id}),
                        );
                    }
                }
            }
            let selectors_enabled = connected && !self.busy && self.pending.is_empty();
            theme::wrap_control(ui, 95.0);
            ui.allocate_ui_with_layout(
                egui::vec2(95.0, 28.0),
                egui::Layout::top_down(egui::Align::Min),
                |ui| {
                    if !selectors_enabled {
                        ui.disable();
                    }
                    egui::ComboBox::from_id_salt("codex_model")
                        .width(95.0)
                        .truncate()
                        .selected_text(if self.model.is_empty() {
                            "Default model".to_string()
                        } else {
                            self.model.chars().take(14).collect::<String>()
                        })
                        .show_ui(ui, |ui| {
                            for model in &self.models {
                                if let Some(id) = model.get("model").and_then(Value::as_str) {
                                    ui.selectable_value(
                                        &mut self.model,
                                        id.to_string(),
                                        model
                                            .get("displayName")
                                            .and_then(Value::as_str)
                                            .unwrap_or(id),
                                    );
                                }
                            }
                        });
                },
            );
            let catalog = self
                .models
                .iter()
                .find(|m| m.get("model").and_then(Value::as_str) == Some(self.model.as_str()));
            self.effort = resolve_effort(catalog, self.effort.as_deref());
            let options = catalog
                .and_then(|m| m.get("supportedReasoningEfforts"))
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            theme::wrap_control(ui, 80.0);
            ui.allocate_ui_with_layout(
                egui::vec2(80.0, 28.0),
                egui::Layout::top_down(egui::Align::Min),
                |ui| {
                    if !selectors_enabled || options.is_empty() {
                        ui.disable();
                    }
                    egui::ComboBox::from_id_salt("codex_effort")
                        .width(80.0)
                        .truncate()
                        .selected_text(self.effort.as_deref().unwrap_or("Default"))
                        .show_ui(ui, |ui| {
                            for option in &options {
                                if let Some(level) =
                                    option.get("reasoningEffort").and_then(Value::as_str)
                                {
                                    ui.selectable_value(
                                        &mut self.effort,
                                        Some(level.to_string()),
                                        level,
                                    )
                                    .on_hover_text(
                                        option
                                            .get("description")
                                            .and_then(Value::as_str)
                                            .unwrap_or(""),
                                    );
                                }
                            }
                        })
                        .response
                        .on_hover_text("Thinking level");
                },
            );
            theme::wrap_control(ui, 95.0);
            ui.allocate_ui_with_layout(
                egui::vec2(95.0, 28.0),
                egui::Layout::top_down(egui::Align::Min),
                |ui| {
                    if !selectors_enabled {
                        ui.disable();
                    }
                    egui::ComboBox::from_id_salt("codex_mode")
                        .width(95.0)
                        .truncate()
                        .selected_text(
                            self.mode
                                .as_ref()
                                .and_then(|v| v.get("name"))
                                .and_then(Value::as_str)
                                .unwrap_or("Codex default"),
                        )
                        .show_ui(ui, |ui| {
                            for mode in &self.modes {
                                ui.selectable_value(
                                    &mut self.mode,
                                    Some(mode.clone()),
                                    mode.get("name").and_then(Value::as_str).unwrap_or("Mode"),
                                );
                            }
                        });
                },
            );
            ui.weak("Ctrl+Enter to send");
        });
        let measured_height = (ui.cursor().top() - composer_top + 10.0).max(120.0);
        if (measured_height - self.composer_height).abs() > 1.0 {
            self.composer_height = measured_height;
            ctx.request_repaint();
        }
    }
}
