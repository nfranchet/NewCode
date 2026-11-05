use super::{EditorApp, SidebarMode, text_view::EditorLayoutCache};
use crate::{codex_ui::CodexUi, settings::Settings};
use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
};
mod explorer;
mod layout;
mod persistence;

fn test_app() -> EditorApp {
    EditorApp {
        settings: Settings::default(),
        settings_path: PathBuf::from("/tmp/focus-layout-test-unused.toml"),
        open_files: vec![PathBuf::from(
            "a_very_long_source_file_name_for_layout_testing.rs",
        )],
        active_file: Some(PathBuf::from(
            "a_very_long_source_file_name_for_layout_testing.rs",
        )),
        preview_file: None,
        unsaved_files: Default::default(),
        document_ids: Default::default(),
        next_document_id: 0,
        buffer: "fn main() {\n    println!(\"Hello\");\n}\n".into(),
        dirty: true,
        top: None,
        bottom_tab: Default::default(),
        logs: Default::default(),
        dev_container: Default::default(),
        codex_ui: CodexUi::default(),
        opencode_ui: crate::opencode_ui::OpenCodeUi::default(),
        assistant_workspace: None,
        skip_codex_start: false,
        skip_opencode_start: false,
        assistant_workspaces: Default::default(),
        creation: None,
        deletion: None,
        rename_target: None,
        rename_text: String::new(),
        status: "Ready".into(),
        sidebar_mode: SidebarMode::Explorer,
        git_ui: crate::git_ui::GitUi::default(),
        search_query: String::new(),
        search_results: Vec::new(),
        pending_editor_cursor: None,
        find_open: false,
        find_query: String::new(),
        find_query_snapshot: String::new(),
        find_active: None,
        close_confirmation: None,
        open_file_filter: String::new(),
        reveal_active_tab: true,
        editor_layout_cache: Arc::new(Mutex::new(EditorLayoutCache::default())),
    }
}

mod terminal;

mod preview;

mod assistants;
