//! Transcript cards, expandable activity, and pending user-response UI.

use super::CodexUi;
use crate::assistant_ui::content;
use crate::ui_theme::{self as theme, Icon};
use egui;

impl CodexUi {
    pub(super) fn show_conversation(&mut self, ui: &mut egui::Ui) {
        egui::ScrollArea::vertical()
            .id_salt(("assistant_conversation", &self.thread_id))
            .max_height(ui.available_height().max(0.0))
            .auto_shrink([false, false])
            .stick_to_bottom(true)
            .show(ui, |ui| {
                if self.messages.is_empty() {
                    crate::assistant_ui::empty(ui);
                }
                let mut go_clicked = false;
                let go_plan_id = self.go_plan_id().map(str::to_owned);
                let go_reason = self.go_unavailable_reason();
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
                    ui.push_id((&self.thread_id, message.view_id()), |ui| {
                        match message.kind.as_str() {
                            "Reasoning" => {
                                content::reasoning(ui, &mut self.markdown, "text", &message.text)
                            }
                            "Command" | "Files" => {
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
                                egui::CollapsingHeader::new(title).id_salt("activity").show(
                                    ui,
                                    |ui| {
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
                                                    egui::RichText::new(&message.output)
                                                        .monospace(),
                                                )
                                                .wrap(),
                                            );
                                        }
                                    },
                                );
                            }
                            _ => {
                                content::frame(&message.kind).show(ui, |ui| {
                                    content::heading(ui, &message.kind);
                                    if message.kind == "You" {
                                        content::literal(ui, &message.text);
                                    } else {
                                        content::markdown(ui, &mut self.markdown, &message.text);
                                    }
                                    if message.kind == "Proposed plan"
                                        && go_plan_id.as_deref() == Some(message.id.as_str())
                                    {
                                        let reason = go_reason;
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
                    content::approval_frame().show(ui, |ui| {
                        theme::heading(ui, Icon::Warning, "Your response is needed", theme::AMBER);
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
    }
}
