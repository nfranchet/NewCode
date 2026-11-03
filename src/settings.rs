use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct Settings {
    pub(crate) workspace: Option<PathBuf>,
    pub(crate) top_height: f32,
    pub(crate) explorer_width: f32,
    pub(crate) right_width: f32,
    pub(crate) right_command: String,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            workspace: None,
            top_height: 180.0,
            explorer_width: 250.0,
            right_width: 390.0,
            right_command: "codex --resume".into(),
        }
    }
}
