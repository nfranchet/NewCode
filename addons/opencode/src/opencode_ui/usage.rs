//! Reported session totals, loaded-message fallback, and model context capacity.
use super::{OpenCodeUi, string};
use egui;
impl OpenCodeUi {
    pub(super) fn usage(&mut self, ui: &mut egui::Ui) {
        egui::CollapsingHeader::new("Usage / status")
            .id_salt("opencode_usage")
            .show(ui, |ui| {
                ui.small(format!("OpenCode {}", self.version));
                if let Some(id) = &self.session {
                    ui.small(format!("Session: {id}"));
                }
                ui.small("Account quota unavailable from OpenCode");
                let total_cost = self
                    .messages
                    .iter()
                    .filter_map(|m| m.info["cost"].as_f64())
                    .sum::<f64>();
                let cost = self.session_info["cost"].as_f64().unwrap_or(total_cost);
                let tokens = self.session_info.get("tokens").filter(|v| !v.is_null());
                if let Some(tokens) = tokens {
                    ui.small(format!(
                        "Session tokens · input {} · output {} · reasoning {}",
                        tokens["input"], tokens["output"], tokens["reasoning"]
                    ));
                } else {
                    let sum = |key: &str| {
                        self.messages
                            .iter()
                            .filter(|m| string(&m.info, "role") == "assistant")
                            .filter_map(|m| m.info["tokens"][key].as_u64())
                            .sum::<u64>()
                    };
                    ui.small(format!(
                        "Loaded messages · input {} · output {} · reasoning {}",
                        sum("input"),
                        sum("output"),
                        sum("reasoning")
                    ));
                }
                ui.small(format!(
                    "{} cost: ${cost:.4}",
                    if self.session_info["cost"].is_number() {
                        "Session"
                    } else {
                        "Loaded messages"
                    }
                ));
                if let Some(limit) = self.selected_model().and_then(|m| m.context) {
                    ui.small(format!("Model context limit: {limit} tokens"));
                    if let Some(last) = self.messages.iter().rev().find_map(|m| {
                        m.info
                            .pointer("/tokens/total")
                            .and_then(serde_json::Value::as_u64)
                    }) {
                        ui.small(format!(
                            "Last reported context: {last} tokens ({:.0}%)",
                            last as f64 / limit.max(1) as f64 * 100.0
                        ));
                    }
                }
                if ui
                    .add_enabled(self.online, egui::Button::new("Refresh usage"))
                    .clicked()
                {
                    self.refresh();
                }
            });
    }
}
