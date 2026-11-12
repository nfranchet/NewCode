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
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            workspace: None,
            session_workspace: None,
            open_files: Vec::new(),
            active_file: None,
            top_height: 210.0,
            explorer_width: 232.0,
            right_width: 310.0,
            right_command: String::new(),
            addon_enabled: Default::default(),
            selected_right_addon: None,
            addon_state: Default::default(),
        }
    }
}

/// Load only the requested workspace; a plain launch never restores a session.
pub(crate) fn load_workspace(
    workspace: Option<&std::path::Path>,
) -> Result<(Settings, PathBuf), String> {
    let Some(workspace) = workspace else {
        return Ok((Settings::default(), PathBuf::new()));
    };
    let workspace = crate::app_paths::workspace_path(workspace).map_err(|e| e.to_string())?;
    let path = crate::app_paths::workspace_dir(&workspace)
        .map_err(|e| e.to_string())?
        .join("settings.toml");
    let settings = load_workspace_file(&workspace, &path)?;
    Ok((settings, path))
}

fn load_workspace_file(
    workspace: &std::path::Path,
    path: &std::path::Path,
) -> Result<Settings, String> {
    let mut settings = match std::fs::read_to_string(path) {
        Ok(text) => toml::from_str::<Settings>(&text).unwrap_or_else(|_| {
            log::warn!("Invalid workspace settings; using defaults");
            Settings::default()
        }),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Settings::default(),
        Err(e) => return Err(format!("Cannot read workspace settings: {e}")),
    };
    settings.workspace = Some(workspace.to_path_buf());
    Ok(settings)
}

pub(crate) fn write_workspace(path: &std::path::Path, settings: &Settings) -> Result<(), String> {
    use std::io::Write;
    let parent = path.parent().ok_or("Missing settings directory")?;
    std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    let text = toml::to_string_pretty(settings).map_err(|e| e.to_string())?;
    let mut file = tempfile::NamedTempFile::new_in(parent).map_err(|e| e.to_string())?;
    file.write_all(text.as_bytes()).map_err(|e| e.to_string())?;
    file.as_file().sync_all().map_err(|e| e.to_string())?;
    file.persist(path).map_err(|e| e.to_string())?;
    Ok(())
}

#[cfg(test)]
mod workspace_tests {
    use super::*;
    #[test]
    fn plain_launch_has_no_workspace_or_restored_state() {
        let (settings, path) = load_workspace(None).unwrap();
        assert!(settings.workspace.is_none());
        assert!(settings.open_files.is_empty());
        assert!(settings.active_file.is_none());
        assert!(settings.addon_enabled.is_empty());
        assert!(path.as_os_str().is_empty());
    }

    #[test]
    fn workspace_records_restore_independent_tabs_settings_and_addons() {
        let root = tempfile::tempdir().unwrap();
        let a = root.path().join("a");
        let b = root.path().join("b");
        std::fs::create_dir_all(&a).unwrap();
        std::fs::create_dir_all(&b).unwrap();
        let file_a = a.join("a.txt");
        let file_b = b.join("b.txt");
        std::fs::write(&file_a, "a").unwrap();
        std::fs::write(&file_b, "b").unwrap();
        let config_a = root.path().join("state-a/settings.toml");
        let config_b = root.path().join("state-b/settings.toml");
        for (workspace, file, path, enabled, width) in [
            (&a, &file_a, &config_a, true, 240.0),
            (&b, &file_b, &config_b, false, 350.0),
        ] {
            let settings = Settings {
                workspace: Some(workspace.clone()),
                session_workspace: Some(workspace.clone()),
                open_files: vec![file.clone()],
                active_file: Some(file.clone()),
                explorer_width: width,
                addon_enabled: [("codex".into(), enabled)].into(),
                addon_state: [("codex".into(), serde_json::json!({"test": enabled}))].into(),
                ..Default::default()
            };
            write_workspace(path, &settings).unwrap();
        }
        // Rewriting A must not alter B, even when both are open.
        let mut settings_a = load_workspace_file(&a, &config_a).unwrap();
        settings_a.right_command = "workspace A".into();
        write_workspace(&config_a, &settings_a).unwrap();
        let settings_b = load_workspace_file(&b, &config_b).unwrap();
        assert!(settings_a.addon_enabled["codex"]);
        assert!(!settings_b.addon_enabled["codex"]);
        assert_ne!(settings_a.addon_state, settings_b.addon_state);
        assert_eq!(settings_a.explorer_width, 240.0);
        assert_eq!(settings_b.explorer_width, 350.0);
        assert!(settings_b.right_command.is_empty());
        assert_eq!(settings_a.open_files, [file_a.clone()]);
        assert_eq!(settings_a.active_file, Some(file_a));
        assert_eq!(settings_b.open_files, [file_b.clone()]);
        assert_eq!(settings_b.active_file, Some(file_b));
    }

    #[test]
    fn missing_and_invalid_workspace_settings_use_defaults() {
        let root = tempfile::tempdir().unwrap();
        let config = root.path().join("settings.toml");
        for contents in [None, Some("invalid [")] {
            if let Some(text) = contents {
                std::fs::write(&config, text).unwrap();
            }
            let settings = load_workspace_file(root.path(), &config).unwrap();
            assert_eq!(settings.workspace.as_deref(), Some(root.path()));
            assert!(settings.open_files.is_empty());
            assert!(settings.addon_enabled.is_empty());
            assert_eq!(settings.explorer_width, Settings::default().explorer_width);
        }
    }

    #[test]
    fn obsolete_addon_keys_are_ignored_and_not_saved() {
        let settings: Settings = toml::from_str("codex_enabled = false\nselected_assistant = 'opencode'\ndisabled_addons = ['codex']\n[opencode_sessions]\n'/workspace' = 'session-42'\n").unwrap();
        assert!(settings.addon_enabled.is_empty());
        assert!(settings.selected_right_addon.is_none());
        assert!(settings.addon_state.is_empty());
        let text = toml::to_string(&settings).unwrap();
        for key in [
            "codex_enabled",
            "selected_assistant",
            "disabled_addons",
            "opencode_sessions",
        ] {
            assert!(!text.contains(key));
        }
    }
}
