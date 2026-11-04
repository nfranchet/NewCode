//! Runtime catalog choices; provider secrets and options are never retained.
use super::{OpenCodeUi, string};
use serde_json::Value;
#[derive(Clone)]
pub(super) struct Model {
    pub provider: String,
    pub id: String,
    pub label: String,
    pub variants: Vec<String>,
    pub context: Option<u64>,
}
impl OpenCodeUi {
    pub(super) fn load_models(&mut self, value: Value) {
        self.models.clear();
        let connected: Vec<_> = value["connected"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .collect();
        for provider in value["all"].as_array().into_iter().flatten() {
            let provider_id = string(provider, "id");
            if !connected.contains(&provider_id) {
                continue;
            }
            if let Some(models) = provider["models"].as_object() {
                for (id, model) in models {
                    if model
                        .pointer("/capabilities/input/text")
                        .and_then(Value::as_bool)
                        == Some(false)
                    {
                        continue;
                    }
                    let variants = model["variants"]
                        .as_object()
                        .map(|v| {
                            v.iter()
                                .filter(|(_, v)| v["disabled"] != true)
                                .map(|(k, _)| k.clone())
                                .collect()
                        })
                        .unwrap_or_default();
                    self.models.push(Model {
                        provider: provider_id.into(),
                        id: id.clone(),
                        label: format!(
                            "{} / {}",
                            string(provider, "name"),
                            model["name"].as_str().unwrap_or(id)
                        ),
                        variants,
                        context: model.pointer("/limit/context").and_then(Value::as_u64),
                    });
                }
            }
        }
        self.models.sort_by(|a, b| a.label.cmp(&b.label));
        if !self
            .models
            .iter()
            .any(|m| self.model.as_ref() == Some(&(m.provider.clone(), m.id.clone())))
        {
            let configured = self
                .models
                .iter()
                .find(|m| value["default"][&m.provider].as_str() == Some(m.id.as_str()))
                .or_else(|| self.models.first());
            self.model = configured.map(|m| (m.provider.clone(), m.id.clone()));
        }
        self.validate_variant();
    }
    pub(super) fn load_agents(&mut self, value: Value) {
        self.agents = value
            .as_array()
            .into_iter()
            .flatten()
            .filter(|a| a["hidden"] != true && matches!(string(a, "mode"), "primary" | "all"))
            .map(|a| string(a, "name").to_owned())
            .filter(|s| !s.is_empty())
            .collect();
        if !self.agents.contains(&self.agent) {
            self.agent = self
                .agents
                .iter()
                .find(|a| a.as_str() == "plan")
                .or_else(|| self.agents.first())
                .cloned()
                .unwrap_or_default();
        }
    }
    pub(super) fn selected_model(&self) -> Option<&Model> {
        self.models
            .iter()
            .find(|m| self.model.as_ref() == Some(&(m.provider.clone(), m.id.clone())))
    }
    pub(super) fn validate_variant(&mut self) {
        if !self
            .selected_model()
            .is_some_and(|m| m.variants.contains(&self.variant))
        {
            self.variant.clear();
        }
    }
}
