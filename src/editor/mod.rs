//! Application state and pane orchestration. Feature implementations live in sibling modules.
use crate::{settings::Settings, terminal::Terminal, ui_theme as theme};
use creation::NewEntry;
use deletion::Deletion;
use eframe::egui;
use persistence::restore_open_files;
use search::FileMatches;
use std::{
    collections::HashMap,
    fs,
    path::PathBuf,
    sync::{Arc, Mutex},
};
use text_view::EditorLayoutCache;

mod addons;
mod clipboard;
mod close_dialog;
mod creation;
mod deletion;
mod document_paths;
mod documents;
mod explorer;
mod layout;
mod logs;
mod persistence;
mod search;
mod shell;
mod sidebar;
mod tabs;
mod terminal_input;
mod text_view;

#[cfg(test)]
mod tests;

#[derive(Clone, Copy, PartialEq, Eq)]
enum SidebarMode {
    Explorer,
    Search,
    Git,
    Addon,
    AddonManager,
}

pub(crate) struct EditorApp {
    settings: Settings,
    settings_path: PathBuf,
    open_files: Vec<PathBuf>,
    active_file: Option<PathBuf>,
    preview_file: Option<PathBuf>,
    unsaved_files: HashMap<PathBuf, String>,
    document_ids: HashMap<PathBuf, egui::Id>,
    next_document_id: u64,
    buffer: String,
    dirty: bool,
    top: Option<Terminal>,
    terminal_error: Option<String>,
    bottom_tab: logs::Tab,
    logs: logs::Logs,
    dev_container: crate::dev_container::DevContainer,
    creation: Option<NewEntry>,
    deletion: Option<Deletion>,
    rename_target: Option<PathBuf>,
    rename_text: String,
    status: String,
    sidebar_mode: SidebarMode,
    git_ui: crate::git_ui::GitUi,
    addons: crate::addons::Addons,
    search_query: String,
    search_results: Vec<FileMatches>,
    pending_editor_cursor: Option<usize>,
    find_open: bool,
    find_query: String,
    find_query_snapshot: String,
    find_active: Option<usize>,
    close_confirmation: Option<PathBuf>,
    close_queue: std::collections::VecDeque<PathBuf>,
    open_file_filter: String,
    reveal_active_tab: bool,
    editor_layout_cache: Arc<Mutex<EditorLayoutCache>>,
}

impl EditorApp {
    pub(crate) fn new(cc: &eframe::CreationContext<'_>) -> Self {
        crate::logging::session::attach(&cc.egui_ctx);
        let settings_path = dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("focus-editor/settings.toml");
        let settings: Settings = match fs::read_to_string(&settings_path) {
            Ok(text) => match toml::from_str(&text) {
                Ok(settings) => settings,
                Err(_) => {
                    log::warn!("Invalid settings configuration; using defaults");
                    Settings::default()
                }
            },
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Settings::default(),
            Err(error) => {
                log::warn!(
                    "Could not read settings kind={:?}; using defaults",
                    error.kind()
                );
                Settings::default()
            }
        };
        let args: Vec<_> = std::env::args_os().skip(1).collect();
        let directory_argument = crate::logging::Arguments::parse(args.clone())
            .ok()
            .and_then(|a| a.workspace)
            .filter(|p| p.is_dir());
        let mut settings = settings;
        if let Some(directory) = directory_argument {
            settings.workspace = Some(directory);
        }
        cc.egui_ctx
            .send_viewport_cmd(egui::ViewportCommand::Title(crate::launch::window_title(
                settings.workspace.as_deref(),
            )));
        log::info!(
            "Workspace initialized selected={}",
            settings.workspace.is_some()
        );
        let cwd = settings.workspace.as_deref();
        let (top, terminal_error) = match Terminal::spawn("", cwd, cc.egui_ctx.clone()) {
            Ok(terminal) => (Some(terminal), None),
            Err(error) => (None, Some(format!("Terminal failed to start: {error}"))),
        };
        theme::apply(&cc.egui_ctx);
        let (open_files, active_file, buffer) = restore_open_files(&settings);
        let next_document_id = open_files.len() as u64;
        let document_ids = open_files
            .iter()
            .enumerate()
            .map(|(index, path)| {
                (
                    path.clone(),
                    egui::Id::new(("document_editor", index as u64 + 1)),
                )
            })
            .collect();
        let mut addons = crate::addons::Addons::default();
        addons.discover(&settings.addon_enabled);
        settings.migrate_addons(
            &addons
                .entries
                .iter()
                .map(|a| a.manifest.clone())
                .collect::<Vec<_>>(),
        );
        addons.suppressed = crate::logging::Arguments::parse(args)
            .map(|a| a.disabled_addons)
            .unwrap_or_default();
        addons.apply_settings(&settings.addon_enabled, &settings.addon_state);
        Self {
            settings,
            settings_path,
            open_files,
            active_file,
            preview_file: None,
            unsaved_files: HashMap::new(),
            document_ids,
            next_document_id,
            buffer,
            dirty: false,
            top,
            terminal_error,
            bottom_tab: Default::default(),
            logs: Default::default(),
            dev_container: Default::default(),
            creation: None,
            deletion: None,
            rename_target: None,
            rename_text: String::new(),
            status: "Ready".into(),
            sidebar_mode: SidebarMode::Explorer,
            git_ui: crate::git_ui::GitUi::default(),
            addons,
            search_query: String::new(),
            search_results: Vec::new(),
            pending_editor_cursor: None,
            find_open: false,
            find_query: String::new(),
            find_query_snapshot: String::new(),
            find_active: None,
            close_confirmation: None,
            close_queue: Default::default(),
            open_file_filter: String::new(),
            reveal_active_tab: true,
            editor_layout_cache: Arc::new(Mutex::new(EditorLayoutCache::default())),
        }
    }

    fn render(&mut self, root: &mut egui::Ui) {
        let context = root.ctx().clone();
        let ctx = &context;
        self.dev_container
            .sync_workspace(self.settings.workspace.as_deref());
        if self.dev_container.tick() {
            let cwd = self.settings.workspace.clone();
            self.restart_terminal(
                ctx,
                cwd.as_deref(),
                self.dev_container.runtime().cloned().as_ref(),
            );
            self.status = self
                .dev_container
                .label()
                .map(|name| format!("Connected to {name}"))
                .unwrap_or_else(|| "Dev Container stopped".into());
        }
        let cwd = self.settings.workspace.clone();
        let runtime = self.dev_container.runtime().cloned();
        if self
            .top
            .as_ref()
            .is_some_and(|terminal| !terminal.matches_context(cwd.as_deref(), runtime.as_ref()))
        {
            self.restart_terminal(ctx, cwd.as_deref(), runtime.as_ref());
        }
        let opened_find = self.editor_shortcuts(ctx);
        self.update_addons(ctx);
        self.toolbar(root);
        self.update_window_title(ctx);
        self.status_bar(root);
        let cwd = self.settings.workspace.clone();
        let runtime = self.dev_container.runtime().cloned();
        self.shell_panel(root, cwd.as_deref(), runtime.as_ref());
        self.activity_bar(root);
        self.cancel_creation_for_workspace_change();
        self.sidebar(root);
        self.addons.right_panel(
            root,
            &mut self.settings.right_width,
            &mut self.settings.selected_right_addon,
        );
        self.document_panel(root, opened_find);
        self.rename_dialog(ctx);
        self.close_dialog(ctx);
        self.addon_dialogs(ctx);
        self.deletion_dialog(ctx);
        self.cancel_unfocused_creation(ctx);
    }
}
impl eframe::App for EditorApp {
    fn raw_input_hook(&mut self, ctx: &egui::Context, raw: &mut egui::RawInput) {
        self.addons.route_input(ctx, raw);
        self.route_terminal_input(ctx, raw);
    }
    fn save(&mut self, _storage: &mut dyn eframe::Storage) {
        self.save_settings();
    }
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.render(ui);
    }
}
