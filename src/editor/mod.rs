//! Application state and pane orchestration. Feature implementations live in sibling modules.
use crate::{codex_ui::CodexUi, settings::Settings, terminal::Terminal, ui_theme as theme};
use creation::NewEntry;
use deletion::Deletion;
use eframe::egui;
use persistence::restore_open_files;
use search::FileMatches;
use std::{
    fs,
    path::PathBuf,
    sync::{Arc, Mutex},
};
use text_view::EditorLayoutCache;

mod creation;
mod deletion;
mod documents;
mod explorer;
mod layout;
mod persistence;
mod search;
mod shell;
mod sidebar;
mod text_view;

#[cfg(test)]
mod tests;

#[derive(Clone, Copy, PartialEq, Eq)]
enum SidebarMode {
    Explorer,
    Search,
    Git,
}

pub(crate) struct EditorApp {
    settings: Settings,
    settings_path: PathBuf,
    open_files: Vec<PathBuf>,
    active_file: Option<PathBuf>,
    buffer: String,
    dirty: bool,
    top: Option<Terminal>,
    codex_ui: CodexUi,
    creation: Option<NewEntry>,
    deletion: Option<Deletion>,
    rename_target: Option<PathBuf>,
    rename_text: String,
    status: String,
    sidebar_mode: SidebarMode,
    git_ui: crate::git_ui::GitUi,
    search_query: String,
    search_results: Vec<FileMatches>,
    pending_editor_cursor: Option<usize>,
    find_open: bool,
    find_query: String,
    find_query_snapshot: String,
    find_active: Option<usize>,
    close_confirmation: Option<PathBuf>,
    open_file_filter: String,
    reveal_active_tab: bool,
    editor_layout_cache: Arc<Mutex<EditorLayoutCache>>,
}

impl EditorApp {
    pub(crate) fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let settings_path = dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("focus-editor/settings.toml");
        let settings: Settings = fs::read_to_string(&settings_path)
            .ok()
            .and_then(|s| toml::from_str(&s).ok())
            .unwrap_or_default();
        let args: Vec<String> = std::env::args().skip(1).collect();
        let no_codex = args.iter().any(|arg| arg == "--no-codex");
        let directory_argument = args
            .iter()
            .find(|arg| !arg.starts_with('-'))
            .map(PathBuf::from)
            .filter(|p| p.is_dir());
        let mut settings = settings;
        // Older settings launched Codex as a shell command. Keep the terminal a normal shell.
        if settings.right_command.trim() == "codex --resume" {
            settings.right_command.clear();
        }
        if let Some(directory) = directory_argument {
            settings.workspace = Some(directory);
        }
        let cwd = settings.workspace.as_deref();
        let top = Terminal::spawn("", cwd, cc.egui_ctx.clone()).ok();
        let mut codex_ui = CodexUi::default();
        if settings.codex_app_server && !no_codex {
            codex_ui.start(cwd, cc.egui_ctx.clone());
        }
        theme::apply(&cc.egui_ctx);
        let (open_files, active_file, buffer) = restore_open_files(&settings);
        Self {
            settings,
            settings_path,
            open_files,
            active_file,
            buffer,
            dirty: false,
            top,
            codex_ui,
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

    fn render(&mut self, ctx: &egui::Context) {
        let opened_find = self.editor_shortcuts(ctx);
        self.toolbar(ctx);
        self.status_bar(ctx);
        let cwd = self.settings.workspace.clone();
        self.shell_panel(ctx, cwd.as_deref());
        self.activity_bar(ctx);
        self.cancel_creation_for_workspace_change();
        self.sidebar(ctx);
        self.codex_panel(ctx, cwd.as_deref());
        self.document_panel(ctx, opened_find);
        self.rename_dialog(ctx);
        self.close_dialog(ctx);
        self.deletion_dialog(ctx);
        self.cancel_unfocused_creation(ctx);
    }
}
impl eframe::App for EditorApp {
    fn save(&mut self, _storage: &mut dyn eframe::Storage) {
        self.save_settings();
    }
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.render(ctx);
    }
}
