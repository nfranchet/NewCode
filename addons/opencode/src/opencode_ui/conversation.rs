//! Readable streaming messages, tools, reasoning, and completed Plan continuation.
use super::{OpenCodeUi, short, string};
use crate::assistant_ui::content;
use crate::ui_theme as theme;
use egui;
impl OpenCodeUi {
    pub(super) fn conversation(&mut self, ui: &mut egui::Ui) {
        let height = ui.available_height().max(0.0);
        let mut go = false;
        let go_ready = self.go_ready();
        let can_send = self.can_send();
        egui::ScrollArea::vertical()
            .id_salt(("assistant_conversation", &self.session))
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
                self.approvals(ui);
                if self.messages.is_empty() {
                    crate::assistant_ui::empty(ui);
                }
                for (index, message) in self.messages.iter().enumerate() {
                    ui.push_id((&self.session, &message.id), |ui| {
                        let role = if string(&message.info, "role") == "user" {
                            "You"
                        } else {
                            "OpenCode"
                        };
                        content::frame(role).show(ui, |ui| {
                            ui.set_min_width(ui.available_width());
                            content::heading(ui, role);
                            for (part_index, part) in message.parts.iter().enumerate() {
                                let part_id = string(part, "id");
                                let part_id = if part_id.is_empty() {
                                    egui::Id::new(("part", part_index))
                                } else {
                                    egui::Id::new(("part", part_id))
                                };
                                ui.push_id(part_id, |ui| match string(part, "type") {
                                    "text" => {
                                        let text = string(part, "text");
                                        if !text.is_empty() {
                                            if role == "You" {
                                                content::literal(ui, text);
                                            } else {
                                                content::markdown(ui, &mut self.markdown, text);
                                            }
                                        }
                                    }
                                    "reasoning" => {
                                        let text = string(part, "text");
                                        if !text.trim().is_empty() {
                                            content::reasoning(
                                                ui,
                                                &mut self.markdown,
                                                string(part, "id"),
                                                text,
                                            );
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
                                });
                            }
                            if let Some(error) = message
                                .info
                                .pointer("/error/data/message")
                                .and_then(serde_json::Value::as_str)
                            {
                                ui.colored_label(theme::RED, error);
                            }
                            if index + 1 == self.messages.len() && go_ready {
                                go = ui
                                    .add_enabled_ui(can_send, |ui| {
                                        theme::button(
                                            ui,
                                            theme::Icon::Send,
                                            "Go",
                                            theme::TEAL,
                                            true,
                                        )
                                    })
                                    .inner
                                    .on_hover_text("Switch to Build and send Go")
                                    .clicked();
                            }
                        });
                    });
                    ui.add_space(10.0);
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
