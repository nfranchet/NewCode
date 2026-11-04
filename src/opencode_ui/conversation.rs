//! Readable streaming messages, tools, reasoning, and completed Plan continuation.
use super::{OpenCodeUi, short, string};
use crate::ui_theme as theme;
use eframe::egui;
impl OpenCodeUi {
    pub(super) fn conversation(&mut self, ui: &mut egui::Ui) {
        let height = (ui.available_height() - 155.0).max(45.0);
        let mut go = false;
        egui::ScrollArea::vertical()
            .id_salt("opencode_conversation")
            .auto_shrink([false, false])
            .max_height(height)
            .min_scrolled_height(height)
            .stick_to_bottom(true)
            .show(ui, |ui| {
                if self.history_before.is_some()
                    && ui
                        .add_enabled(!self.locked(), egui::Button::new("Load earlier messages"))
                        .clicked()
                {
                    self.older_messages();
                }
                if self.messages.is_empty() {
                    ui.weak("Choose or create an OpenCode session to begin.");
                }
                for (index, message) in self.messages.iter().enumerate() {
                    ui.push_id(&message.id, |ui| {
                        egui::Frame::new()
                            .fill(if string(&message.info, "role") == "user" {
                                theme::BLUE_SOFT
                            } else {
                                egui::Color32::WHITE
                            })
                            .inner_margin(8)
                            .show(ui, |ui| {
                                ui.set_min_width(ui.available_width());
                                ui.strong(if string(&message.info, "role") == "user" {
                                    "You"
                                } else {
                                    "OpenCode"
                                });
                                for part in &message.parts {
                                    match string(part, "type") {
                                        "text" => {
                                            let text = string(part, "text");
                                            if !text.is_empty() {
                                                ui.add(
                                                    egui::Label::new(text).wrap().selectable(true),
                                                );
                                            }
                                        }
                                        "reasoning" => {
                                            let text = string(part, "text");
                                            if !text.trim().is_empty() {
                                                egui::CollapsingHeader::new("Reasoning")
                                                    .id_salt(string(part, "id"))
                                                    .show(ui, |ui| {
                                                        ui.add(
                                                            egui::Label::new(text)
                                                                .wrap()
                                                                .selectable(true),
                                                        );
                                                    });
                                            }
                                        }
                                        "tool" => {
                                            let state = &part["state"];
                                            let label = format!(
                                                "{} · {}",
                                                string(part, "tool"),
                                                string(state, "status")
                                            );
                                            egui::CollapsingHeader::new(label)
                                                .id_salt(string(part, "id"))
                                                .show(ui, |ui| {
                                                    if let Some(command) = state
                                                        .pointer("/input/command")
                                                        .and_then(serde_json::Value::as_str)
                                                    {
                                                        ui.add(
                                                            egui::Label::new(
                                                                egui::RichText::new(command)
                                                                    .monospace(),
                                                            )
                                                            .wrap()
                                                            .selectable(true),
                                                        );
                                                    }
                                                    for key in ["title", "output", "error"] {
                                                        let text = string(state, key);
                                                        if !text.is_empty() {
                                                            ui.add(
                                                                egui::Label::new(text)
                                                                    .wrap()
                                                                    .selectable(true),
                                                            );
                                                        }
                                                    }
                                                });
                                        }
                                        "file" => {
                                            ui.label(format!(
                                                "File: {}",
                                                short(string(part, "filename"), 80)
                                            ));
                                        }
                                        _ => {}
                                    }
                                }
                                if let Some(error) = message
                                    .info
                                    .pointer("/error/data/message")
                                    .and_then(serde_json::Value::as_str)
                                {
                                    ui.colored_label(theme::RED, error);
                                }
                                if index + 1 == self.messages.len() && self.go_ready() {
                                    go = ui
                                        .add_enabled(self.can_send(), egui::Button::new("Go"))
                                        .on_hover_text("Switch to Build and send Go")
                                        .clicked();
                                }
                            });
                    });
                    ui.add_space(7.0);
                }
                if self.busy {
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.weak("OpenCode is working…");
                    });
                }
            });
        if go {
            self.submit(true);
        }
    }
}
