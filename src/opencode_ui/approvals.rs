//! Provider-owned permissions and single/multiple-choice questions.
use super::{OpenCodeUi, Operation, string};
use crate::ui_theme as theme;
use eframe::egui;
use serde_json::json;
impl OpenCodeUi {
    pub(super) fn approvals(&mut self, ui: &mut egui::Ui) {
        if self.permissions.is_empty() && self.questions.is_empty() {
            return;
        }
        let mut action = None;
        egui::ScrollArea::vertical()
            .id_salt("opencode_approvals")
            .max_height(200.0)
            .show(ui, |ui| {
                for permission in &self.permissions {
                    let id = string(permission, "id");
                    ui.push_id(id, |ui| {
                        ui.colored_label(
                            theme::AMBER,
                            format!(
                                "Permission: {}",
                                permission["permission"]
                                    .as_str()
                                    .or_else(|| permission["action"].as_str())
                                    .unwrap_or("Permission")
                            ),
                        );
                        for pattern in permission
                            .get("patterns")
                            .or_else(|| permission.get("resources"))
                            .and_then(serde_json::Value::as_array)
                            .into_iter()
                            .flatten()
                            .filter_map(serde_json::Value::as_str)
                        {
                            ui.add(egui::Label::new(pattern).wrap());
                        }
                        ui.add_enabled_ui(!self.answering(id), |ui| {
                            ui.horizontal_wrapped(|ui| {
                                for (label, reply) in [
                                    ("Allow once", "once"),
                                    ("Always allow", "always"),
                                    ("Deny", "reject"),
                                ] {
                                    if ui.button(label).clicked() {
                                        action = Some((
                                            false,
                                            id.to_owned(),
                                            format!("/permission/{id}/reply"),
                                            Some(json!({"reply":reply})),
                                        ));
                                    }
                                }
                            });
                        });
                    });
                }
                for question in &self.questions {
                    let id = string(question, "id");
                    let answering = self.answering(id);
                    let questions = question["questions"]
                        .as_array()
                        .cloned()
                        .unwrap_or_default();
                    let answers = self.answers.entry(id.into()).or_default();
                    answers.selected.resize(questions.len(), vec![]);
                    answers.custom.resize(questions.len(), String::new());
                    ui.push_id(id, |ui| {
                        ui.add_enabled_ui(!answering, |ui| {
                            for (index, question) in questions.iter().enumerate() {
                                ui.push_id(index, |ui| {
                                    ui.strong(string(question, "header"));
                                    ui.add(egui::Label::new(string(question, "question")).wrap());
                                    for option in
                                        question["options"].as_array().into_iter().flatten()
                                    {
                                        let label = string(option, "label");
                                        let selected =
                                            answers.selected[index].iter().any(|s| s == label);
                                        if ui
                                            .selectable_label(selected, label)
                                            .on_hover_text(string(option, "description"))
                                            .clicked()
                                        {
                                            if question["multiple"] != true {
                                                answers.selected[index].clear();
                                            }
                                            if selected {
                                                answers.selected[index].retain(|s| s != label);
                                            } else {
                                                answers.selected[index].push(label.into());
                                            }
                                        }
                                    }
                                    if question["custom"] != false {
                                        ui.add(
                                            egui::TextEdit::singleline(&mut answers.custom[index])
                                                .hint_text("Your answer…")
                                                .desired_width(f32::INFINITY),
                                        );
                                    }
                                });
                            }
                            let complete = answers.selected.iter().zip(&answers.custom).all(
                                |(choices, text)| !choices.is_empty() || !text.trim().is_empty(),
                            );
                            ui.horizontal(|ui| {
                                if ui
                                    .add_enabled(complete, egui::Button::new("Submit answer"))
                                    .clicked()
                                {
                                    let values: Vec<Vec<String>> = answers
                                        .selected
                                        .iter()
                                        .zip(&answers.custom)
                                        .zip(&questions)
                                        .map(|((choices, text), question)| {
                                            let mut result = if !text.trim().is_empty()
                                                && question["multiple"] != true
                                            {
                                                vec![]
                                            } else {
                                                choices.clone()
                                            };
                                            if !text.trim().is_empty() {
                                                result.push(text.trim().into());
                                            }
                                            result
                                        })
                                        .collect();
                                    action = Some((
                                        true,
                                        id.to_owned(),
                                        format!("/question/{id}/reply"),
                                        Some(json!({"answers":values})),
                                    ));
                                }
                                if ui.button("Dismiss").clicked() {
                                    action = Some((
                                        true,
                                        id.to_owned(),
                                        format!("/question/{id}/reject"),
                                        None,
                                    ));
                                }
                            });
                        });
                    });
                }
            });
        if let Some((question, id, mut path, body)) = action {
            let requests = if question {
                &self.questions
            } else {
                &self.permissions
            };
            if requests
                .iter()
                .any(|r| string(r, "id") == id && r["_v2"] == true)
            {
                if let Some(session) = &self.session {
                    path = format!("/api/session/{session}{path}");
                }
            }
            self.request(Operation::Answer { question, id }, "POST", path, body);
        }
    }
    fn answering(&self, id: &str) -> bool {
        self.requests
            .values()
            .any(|p| matches!(&p.operation,Operation::Answer{id:current,..} if current==id))
    }
}
