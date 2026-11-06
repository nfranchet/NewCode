//! Provider catalog adapter for the shared assistant composer.
use super::{CodexUi, catalog::resolve_effort};
use crate::assistant_ui::composer::{self, Choice, Selector};
use eframe::egui;
use serde_json::json;
impl CodexUi {
    pub(super) fn show_composer(&mut self, ui: &mut egui::Ui, _ctx: &egui::Context) {
        let connected = self.thread_id.is_some()
            && self.server.is_some()
            && !self.requests.values().any(|m| {
                matches!(
                    m.as_str(),
                    "thread/start" | "thread/resume" | "collaborationMode/list"
                )
            });
        let enabled = connected && !self.busy && self.pending.is_empty();
        let catalog = self
            .models
            .iter()
            .find(|m| m["model"].as_str() == Some(&self.model));
        self.effort = resolve_effort(catalog, self.effort.as_deref());
        let efforts = catalog
            .and_then(|m| m["supportedReasoningEfforts"].as_array())
            .cloned()
            .unwrap_or_default();
        let models: Vec<_> = self
            .models
            .iter()
            .filter(|m| m["model"].is_string())
            .collect();
        let selectors = [
            Selector {
                id: "codex_model",
                title: "Model",
                label: if self.model.is_empty() {
                    "Default model".into()
                } else {
                    self.model.clone()
                },
                selected: models
                    .iter()
                    .position(|m| m["model"].as_str() == Some(&self.model)),
                enabled,
                options: models
                    .iter()
                    .map(|m| Choice {
                        label: m["displayName"]
                            .as_str()
                            .or(m["model"].as_str())
                            .unwrap_or("Model")
                            .into(),
                        detail: m["model"].as_str().unwrap_or_default().into(),
                    })
                    .collect(),
            },
            Selector {
                id: "codex_effort",
                title: "Thinking",
                label: self.effort.clone().unwrap_or("Default".into()),
                selected: efforts
                    .iter()
                    .position(|e| e["reasoningEffort"].as_str() == self.effort.as_deref()),
                enabled,
                options: efforts
                    .iter()
                    .map(|e| Choice {
                        label: e["reasoningEffort"].as_str().unwrap_or("Default").into(),
                        detail: e["description"].as_str().unwrap_or_default().into(),
                    })
                    .collect(),
            },
            Selector {
                id: "codex_mode",
                title: "Mode",
                label: self
                    .mode
                    .as_ref()
                    .and_then(|m| m["name"].as_str())
                    .unwrap_or("Default")
                    .into(),
                selected: self
                    .modes
                    .iter()
                    .position(|m| Some(m) == self.mode.as_ref()),
                enabled,
                options: self
                    .modes
                    .iter()
                    .map(|m| Choice {
                        label: m["name"].as_str().unwrap_or("Mode").into(),
                        detail: m["mode"].as_str().unwrap_or_default().into(),
                    })
                    .collect(),
            },
        ];
        let id = ui.make_persistent_id("codex_composer");
        let action = composer::show(
            ui,
            id,
            &mut self.input,
            self.composer_input_height,
            enabled,
            self.busy,
            self.turn_id.is_some(),
            selectors,
        );
        if let Some(index) = action.selected[0] {
            self.model = models[index]["model"].as_str().unwrap().into();
        }
        if let Some(index) = action.selected[1] {
            self.effort = efforts[index]["reasoningEffort"]
                .as_str()
                .map(str::to_owned);
        }
        if let Some(index) = action.selected[2] {
            self.mode = Some(self.modes[index].clone());
        }
        if let Some(backwards) = action.cycle.filter(|_| enabled && !self.modes.is_empty()) {
            let index = self
                .modes
                .iter()
                .position(|m| Some(m) == self.mode.as_ref())
                .unwrap_or(0);
            let next =
                (index + if backwards { self.modes.len() - 1 } else { 1 }) % self.modes.len();
            self.mode = Some(self.modes[next].clone());
        }
        if action.send {
            self.send_turn();
        }
        if action.interrupt {
            if let (Some(thread), Some(turn)) = (&self.thread_id, &self.turn_id) {
                let _ = self.send("turn/interrupt", json!({"threadId":thread,"turnId":turn}));
            }
        }
    }
}
