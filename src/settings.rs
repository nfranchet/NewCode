use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct Settings {
    pub(crate) workspace: Option<PathBuf>,
    pub(crate) session_workspace: Option<PathBuf>,
    pub(crate) open_files: Vec<PathBuf>,
    pub(crate) active_file: Option<PathBuf>,
    pub(crate) top_height: f32,
    pub(crate) explorer_width: f32,
    pub(crate) right_width: f32,
    pub(crate) right_command: String,
    pub(crate) addon_enabled: std::collections::BTreeMap<String, bool>,
    pub(crate) selected_right_addon: Option<String>,
    pub(crate) addon_state: std::collections::BTreeMap<String, serde_json::Value>,
    #[serde(flatten)]
    pub(crate) legacy: std::collections::BTreeMap<String, toml::Value>,
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn manifest_migration_preserves_independent_preferences_and_sessions() {
        let mut settings:Settings=toml::from_str("codex_enabled = false\nopencode_enabled = true\ncodex_app_server = false\nselected_assistant = 'opencode'\n[opencode_sessions]\n'/workspace' = 'session-42'\n").unwrap();
        let manifests: Vec<focus_addon_sdk::Manifest> = [
            include_str!("../addons/codex/addon.toml"),
            include_str!("../addons/opencode/addon.toml"),
        ]
        .into_iter()
        .map(|s| toml::from_str(s).unwrap())
        .collect();
        settings.migrate_addons(&manifests);
        assert_eq!(settings.addon_enabled.get("codex"), Some(&false));
        assert_eq!(settings.addon_enabled.get("opencode"), Some(&true));
        assert_eq!(settings.selected_right_addon.as_deref(), Some("opencode"));
        assert_eq!(settings.addon_state["codex"]["autostart"], false);
        assert_eq!(
            settings.addon_state["opencode"]["sessions"]["/workspace"],
            "session-42"
        );
        settings.addon_enabled.insert("codex".into(), true);
        settings.migrate_addons(&manifests);
        assert_eq!(settings.addon_enabled.get("codex"), Some(&true));
        let text = toml::to_string(&settings).unwrap();
        let restored: Settings = toml::from_str(&text).unwrap();
        assert_eq!(restored.addon_state, settings.addon_state);
        assert!(!text.contains("codex_enabled"));
        assert!(!text.contains("selected_assistant"));
    }
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            workspace: None,
            session_workspace: None,
            open_files: Vec::new(),
            active_file: None,
            top_height: 180.0,
            explorer_width: 250.0,
            right_width: 390.0,
            right_command: String::new(),
            addon_enabled: Default::default(),
            selected_right_addon: None,
            addon_state: Default::default(),
            legacy: Default::default(),
        }
    }
}

impl Settings {
    pub(crate) fn migrate_addons(&mut self, manifests: &[focus_addon_sdk::Manifest]) {
        let mut consumed = std::collections::HashSet::new();
        if let Some(disabled) = self
            .legacy
            .get("disabled_addons")
            .and_then(toml::Value::as_array)
        {
            for id in disabled.iter().filter_map(toml::Value::as_str) {
                self.addon_enabled.entry(id.into()).or_insert(false);
            }
        }
        for manifest in manifests {
            let migration = &manifest.migration;
            consumed.extend(migration.enabled.iter().cloned());
            consumed.extend(migration.selection.iter().cloned());
            consumed.extend(migration.sessions.iter().cloned());
            consumed.extend(migration.autostart.iter().cloned());
            if !self.addon_enabled.contains_key(&manifest.id) {
                let enabled = migration
                    .enabled
                    .iter()
                    .filter_map(|key| self.legacy.get(key).and_then(toml::Value::as_bool))
                    .next();
                self.addon_enabled.insert(
                    manifest.id.clone(),
                    enabled.unwrap_or(manifest.enabled_by_default),
                );
            }
            if self.selected_right_addon.is_none()
                && migration
                    .selection
                    .as_ref()
                    .and_then(|key| self.legacy.get(key))
                    .and_then(toml::Value::as_str)
                    == Some(manifest.id.as_str())
            {
                self.selected_right_addon = Some(manifest.id.clone());
            }
            let state = self
                .addon_state
                .entry(manifest.id.clone())
                .or_insert_with(|| serde_json::json!({}));
            for (field, key) in [
                ("sessions", migration.sessions.as_ref()),
                ("autostart", migration.autostart.as_ref()),
            ] {
                if state.get(field).is_none() {
                    if let Some(value) = key.and_then(|key| self.legacy.get(key)) {
                        if let Ok(value) = serde_json::to_value(value) {
                            state[field] = value;
                        }
                    }
                }
            }
        }
        for key in consumed {
            self.legacy.remove(&key);
        }
        self.legacy.remove("disabled_addons");
    }
}
