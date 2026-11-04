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
    pub(crate) codex_app_server: bool,
    pub(crate) codex_enabled: bool,
    pub(crate) opencode_enabled: bool,
    pub(crate) selected_assistant: Assistant,
    pub(crate) opencode_sessions: std::collections::HashMap<PathBuf, String>,
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
            codex_app_server: true,
            codex_enabled: true,
            opencode_enabled: false,
            selected_assistant: Assistant::Codex,
            opencode_sessions: Default::default(),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Assistant {
    #[default]
    Codex,
    #[serde(rename = "opencode", alias = "open_code")]
    OpenCode,
}
