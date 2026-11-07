//! Provider catalog adapter for the shared assistant composer.
use super::{OpenCodeUi, Operation};
use crate::assistant_ui::composer::{self, Choice, Selector};
use egui;
impl OpenCodeUi {
    pub(super) fn composer(&mut self, ui: &mut egui::Ui) {
        let enabled = self.online && !self.locked();
        let variants = self
            .selected_model()
            .map(|m| m.variants.clone())
            .unwrap_or_default();
        let selectors = [
            Selector {
                id: "opencode_model",
                title: "Model",
                label: self
                    .selected_model()
                    .map(|m| m.label.clone())
                    .unwrap_or("Default model".into()),
                selected: self
                    .models
                    .iter()
                    .position(|m| self.model.as_ref() == Some(&(m.provider.clone(), m.id.clone()))),
                enabled,
                options: self
                    .models
                    .iter()
                    .map(|m| Choice {
                        label: m.label.clone(),
                        detail: format!("{} / {}", m.provider, m.id),
                    })
                    .collect(),
            },
            Selector {
                id: "opencode_variant",
                title: "Thinking",
                label: if self.variant.is_empty() {
                    "Default".into()
                } else {
                    self.variant.clone()
                },
                selected: if self.variant.is_empty() {
                    Some(0)
                } else {
                    variants
                        .iter()
                        .position(|v| v == &self.variant)
                        .map(|i| i + 1)
                },
                enabled: enabled && !variants.is_empty(),
                options: std::iter::once(Choice {
                    label: "Default".into(),
                    detail: "Provider default thinking".into(),
                })
                .chain(variants.iter().map(|v| Choice {
                    label: v.clone(),
                    detail: v.clone(),
                }))
                .collect(),
            },
            Selector {
                id: "opencode_agent",
                title: "Mode / agent",
                label: if self.agent.is_empty() {
                    "Default".into()
                } else {
                    self.agent.clone()
                },
                selected: self.agents.iter().position(|a| a == &self.agent),
                enabled,
                options: self
                    .agents
                    .iter()
                    .map(|a| Choice {
                        label: a.clone(),
                        detail: a.clone(),
                    })
                    .collect(),
            },
        ];
        let can_send = self.can_send();
        let can_interrupt = !self
            .requests
            .values()
            .any(|p| matches!(p.operation, Operation::Abort));
        let id = ui.make_persistent_id("opencode_composer");
        let action = composer::show(
            ui,
            id,
            &mut self.input,
            self.composer_input_height,
            can_send,
            self.busy,
            can_interrupt,
            selectors,
        );
        if let Some(index) = action.selected[0] {
            let model = &self.models[index];
            self.model = Some((model.provider.clone(), model.id.clone()));
            self.validate_variant();
        } else if let Some(index) = action.selected[1] {
            self.variant = if index == 0 {
                String::new()
            } else {
                variants[index - 1].clone()
            };
        }
        if let Some(index) = action.selected[2] {
            self.agent = self.agents[index].clone();
        }
        if let Some(backwards) = action.cycle.filter(|_| enabled && !self.agents.is_empty()) {
            let index = self
                .agents
                .iter()
                .position(|a| a == &self.agent)
                .unwrap_or(0);
            self.agent = self.agents
                [(index + if backwards { self.agents.len() - 1 } else { 1 }) % self.agents.len()]
            .clone();
        }
        if action.send {
            self.submit(false);
        }
        if action.interrupt {
            if let Some(id) = &self.session {
                self.request(
                    Operation::Abort,
                    "POST",
                    format!("/session/{id}/abort"),
                    None,
                );
            }
        }
    }
}
