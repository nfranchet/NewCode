//! Transcript cards, expandable activity, and pending user-response UI.

use super::CodexUi;
use crate::ui_theme::{self as theme, Icon};
use eframe::egui;

impl CodexUi {
    pub(super) fn show_conversation(&mut self, ui: &mut egui::Ui) {
        // Reserve the composer before laying out the scrollable conversation.
        let composer_height = self.composer_height;
        egui::ScrollArea::vertical()
            .id_salt("codex_conversation")
            .max_height((ui.available_height() - composer_height).max(0.0))
            .auto_shrink([false, false])
            .stick_to_bottom(true)
            .show(ui, |ui| {
                if self.messages.is_empty() {
                    ui.vertical_centered(|ui| {
                        theme::empty_art(ui, Icon::Spark, theme::TEAL);
                        ui.label(
                            egui::RichText::new("A little help, a lot of progress")
                                .strong()
                                .color(theme::TEAL),
                        );
                        ui.label("Ask a question, plan a change, or work through a bug.");
                        ui.add_space(8.0);
                        ui.small("Pick a model, thinking level, and mode below.");
                    });
                }
                let mut go_clicked = false;
                for (index, message) in self.messages.iter().enumerate() {
                    if message.kind == "Reasoning" && message.text.trim().is_empty() {
                        if !message.complete && self.busy {
                            ui.push_id(index, |ui| {
                                ui.horizontal(|ui| {
                                    ui.spinner();
                                    ui.weak("Thinking…");
                                });
                            });
                            ui.add_space(10.0);
                        }
                        continue;
                    }
                    ui.push_id(index, |ui| match message.kind.as_str() {
                        "Command" | "Reasoning" | "Files" => {
                            let title = if message.kind == "Command" {
                                format!(
                                    "{} · {}",
                                    if message.complete {
                                        "Executed"
                                    } else {
                                        "Running"
                                    },
                                    "Command"
                                )
                            } else {
                                message.kind.clone()
                            };
                            egui::CollapsingHeader::new(title).show(ui, |ui| {
                                ui.add(
                                    egui::Label::new(
                                        egui::RichText::new(&message.text).monospace(),
                                    )
                                    .wrap()
                                    .selectable(true),
                                );
                                if !message.output.is_empty() {
                                    ui.separator();
                                    ui.label("Output");
                                    ui.add(
                                        egui::Label::new(
                                            egui::RichText::new(&message.output).monospace(),
                                        )
                                        .wrap(),
                                    );
                                }
                            });
                        }
                        _ => {
                            let (accent, fill) = match message.kind.as_str() {
                                "You" => (theme::BLUE, theme::BLUE_SOFT),
                                "Proposed plan" => (theme::TEAL, theme::TEAL_SOFT),
                                "Error" => (theme::RED, egui::Color32::from_rgb(255, 241, 243)),
                                "Status" => (theme::GREEN, theme::SURFACE),
                                _ => (theme::TEAL, egui::Color32::WHITE),
                            };
                            egui::Frame::new()
                                .fill(fill)
                                .stroke(egui::Stroke::new(1.0_f32, theme::BORDER))
                                .corner_radius(7)
                                .inner_margin(10)
                                .show(ui, |ui| {
                                    ui.label(
                                        egui::RichText::new(&message.kind)
                                            .small()
                                            .strong()
                                            .color(accent),
                                    );
                                    ui.add(egui::Label::new(&message.text).wrap().selectable(true));
                                    if message.kind == "Proposed plan"
                                        && self.go_plan_id() == Some(message.id.as_str())
                                    {
                                        let reason = self.go_unavailable_reason();
                                        let response = ui
                                            .add_enabled_ui(reason.is_none(), |ui| {
                                                theme::button(
                                                    ui,
                                                    Icon::Send,
                                                    "Go",
                                                    theme::TEAL,
                                                    true,
                                                )
                                            })
                                            .inner
                                            .on_hover_text(
                                                reason.unwrap_or(
                                                    "Switch to Default mode and send Go",
                                                ),
                                            );
                                        go_clicked |= response.clicked();
                                    }
                                });
                        }
                    });
                    ui.add_space(10.0);
                }
                if go_clicked {
                    if let Err(error) = self.go() {
                        self.status = error.clone();
                        self.messages
                            .push(super::messages::Message::new("Error", error));
                    }
                }
                for index in (0..self.pending.len()).rev() {
                    let pending_id = self.pending[index].id.clone();
                    let pending_method = self.pending[index].method.clone();
                    let pending_prompt = self.pending[index].prompt.clone();
                    let pending_answer_key = self.pending[index].answer_key.clone();
                    egui::Frame::group(ui.style())
                        .fill(egui::Color32::from_rgb(255, 248, 231))
                        .stroke(egui::Stroke::new(1.0_f32, theme::AMBER))
                        .show(ui, |ui| {
                            theme::heading(
                                ui,
                                Icon::Warning,
                                "Your response is needed",
                                theme::AMBER,
                            );
                            ui.label(&pending_prompt);
                            if pending_method.contains("requestUserInput") {
                                ui.add(
                                    egui::TextEdit::multiline(&mut self.answer)
                                        .desired_width(ui.available_width()),
                                );
                                if ui.button("Send response").clicked() {
                                    let answer = self.answer.clone();
                                    self.answer.clear();
                                    self.respond_user_input(
                                        pending_id.clone(),
                                        pending_answer_key,
                                        answer,
                                    );
                                }
                            } else {
                                ui.horizontal_wrapped(|ui| {
                                    if ui.button("Allow").clicked() {
                                        self.respond_decision(index, "accept");
                                    }
                                    if ui.button("Allow this session").clicked() {
                                        self.respond_decision(index, "acceptForSession");
                                    }
                                    if ui.button("Decline").clicked() {
                                        self.respond_decision(index, "decline");
                                    }
                                });
                            }
                        });
                }
            });
        ui.add_space((ui.available_height() - composer_height).max(0.0));
    }
}
