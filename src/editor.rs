use crate::{settings::Settings, syntax::colored_job, terminal::Terminal};
use eframe::egui::{
    self, Color32, FontId, TextFormat, TextStyle,
    text::{CCursor, CCursorRange, LayoutJob},
};
use std::{
    fs,
    path::{Path, PathBuf},
    time::Duration,
};

#[derive(Clone, Copy, PartialEq, Eq)]
enum SidebarMode {
    Explorer,
    Search,
}

struct FileMatches {
    path: PathBuf,
    lines: Vec<(usize, String)>,
}

fn collect_search_files(dir: &Path, files: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(kind) = entry.file_type() else {
            continue;
        };
        if kind.is_dir() {
            if entry.file_name() != ".git" {
                collect_search_files(&path, files);
            }
        } else if kind.is_file() {
            files.push(path);
        }
    }
}

pub(crate) struct EditorApp {
    settings: Settings,
    settings_path: PathBuf,
    open_files: Vec<PathBuf>,
    active_file: Option<PathBuf>,
    buffer: String,
    dirty: bool,
    top: Option<Terminal>,
    right: Option<Terminal>,
    rename_target: Option<PathBuf>,
    rename_text: String,
    status: String,
    sidebar_mode: SidebarMode,
    search_query: String,
    search_results: Vec<FileMatches>,
    pending_editor_cursor: Option<usize>,
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
        let directory_argument = std::env::args()
            .nth(1)
            .map(PathBuf::from)
            .filter(|p| p.is_dir());
        let mut settings = settings;
        if let Some(directory) = directory_argument {
            settings.workspace = Some(directory);
        }
        let cwd = settings.workspace.as_deref();
        let top = Terminal::spawn("", cwd).ok();
        let right = Terminal::spawn(&settings.right_command, cwd).ok();
        let mut style = (*cc.egui_ctx.style()).clone();
        style
            .text_styles
            .insert(TextStyle::Monospace, FontId::monospace(14.0));
        style
            .text_styles
            .insert(TextStyle::Heading, FontId::proportional(18.0));
        style.spacing.item_spacing = egui::vec2(8.0, 7.0);
        style.spacing.button_padding = egui::vec2(10.0, 6.0);
        style.spacing.interact_size = egui::vec2(22.0, 24.0);
        style.visuals.dark_mode = false;
        style.visuals.override_text_color = Some(Color32::from_rgb(48, 49, 51));
        style.visuals.panel_fill = Color32::from_rgb(248, 248, 248);
        style.visuals.window_fill = Color32::WHITE;
        style.visuals.extreme_bg_color = Color32::WHITE;
        style.visuals.faint_bg_color = Color32::from_rgb(243, 243, 243);
        style.visuals.code_bg_color = Color32::from_rgb(250, 250, 250);
        style.visuals.window_corner_radius = egui::CornerRadius::same(4);
        style.visuals.selection.bg_fill = Color32::from_rgb(204, 231, 255);
        style.visuals.selection.stroke = egui::Stroke::new(1.0_f32, Color32::from_rgb(0, 122, 204));
        style.visuals.widgets.inactive.bg_fill = Color32::TRANSPARENT;
        style.visuals.widgets.inactive.weak_bg_fill = Color32::TRANSPARENT;
        style.visuals.widgets.inactive.bg_stroke = egui::Stroke::new(1.0_f32, Color32::TRANSPARENT);
        style.visuals.widgets.inactive.corner_radius = egui::CornerRadius::same(3);
        style.visuals.widgets.hovered.bg_fill = Color32::from_rgb(231, 231, 231);
        style.visuals.widgets.hovered.weak_bg_fill = Color32::from_rgb(239, 239, 239);
        style.visuals.widgets.hovered.bg_stroke =
            egui::Stroke::new(1.0_f32, Color32::from_rgb(218, 218, 218));
        style.visuals.widgets.hovered.corner_radius = egui::CornerRadius::same(3);
        style.visuals.widgets.active.bg_fill = Color32::from_rgb(218, 232, 246);
        style.visuals.widgets.active.weak_bg_fill = Color32::from_rgb(218, 232, 246);
        style.visuals.widgets.active.bg_stroke =
            egui::Stroke::new(1.0_f32, Color32::from_rgb(0, 122, 204));
        style.visuals.widgets.active.corner_radius = egui::CornerRadius::same(3);
        cc.egui_ctx.set_style(style);
        Self {
            settings,
            settings_path,
            open_files: Vec::new(),
            active_file: None,
            buffer: String::new(),
            dirty: false,
            top,
            right,
            rename_target: None,
            rename_text: String::new(),
            status: "Ready".into(),
            sidebar_mode: SidebarMode::Explorer,
            search_query: String::new(),
            search_results: Vec::new(),
            pending_editor_cursor: None,
        }
    }
    fn save_settings(&self) {
        if let Some(parent) = self.settings_path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        if let Ok(s) = toml::to_string_pretty(&self.settings) {
            let _ = fs::write(&self.settings_path, s);
        }
    }
    fn choose_workspace(&mut self) {
        if let Some(p) = rfd::FileDialog::new().pick_folder() {
            self.settings.workspace = Some(p.clone());
            self.status = format!("Workspace: {}", p.display());
            self.save_settings();
        }
    }
    fn open_file(&mut self, path: PathBuf) {
        match fs::read_to_string(&path) {
            Ok(s) => {
                if !self.open_files.contains(&path) {
                    self.open_files.push(path.clone());
                }
                self.active_file = Some(path);
                self.buffer = s;
                self.dirty = false;
                self.status = "File opened".into();
            }
            Err(e) => self.status = format!("Could not open file: {e}"),
        }
    }
    fn search_workspace(&mut self) {
        let query = self.search_query.trim().to_lowercase();
        self.search_results.clear();
        if query.is_empty() {
            self.status = "Enter text to search for".into();
            return;
        }
        let Some(root) = self.settings.workspace.clone() else {
            self.status = "Choose a workspace before searching".into();
            return;
        };
        let mut paths = Vec::new();
        collect_search_files(&root, &mut paths);
        paths.sort();
        let mut matched_lines = 0usize;
        for path in paths {
            let Ok(contents) = fs::read_to_string(&path) else {
                continue;
            };
            let lines: Vec<_> = contents
                .lines()
                .enumerate()
                .filter_map(|(index, line)| {
                    line.to_lowercase()
                        .contains(&query)
                        .then(|| (index + 1, line.trim().to_string()))
                })
                .collect();
            if !lines.is_empty() {
                matched_lines += lines.len();
                self.search_results.push(FileMatches { path, lines });
            }
        }
        self.status = if matched_lines == 0 {
            format!("No matches for ‘{}’", self.search_query.trim())
        } else {
            format!(
                "Found {matched_lines} matching lines in {} files",
                self.search_results.len()
            )
        };
    }
    fn open_search_result(&mut self, path: PathBuf, line_number: usize) {
        self.open_file(path);
        if self.active_file.is_some() {
            self.pending_editor_cursor = Some(
                self.buffer
                    .split_inclusive('\n')
                    .take(line_number.saturating_sub(1))
                    .map(|line| line.chars().count())
                    .sum(),
            );
        }
    }
    fn save_file(&mut self) {
        if let Some(p) = &self.active_file {
            match fs::write(p, &self.buffer) {
                Ok(()) => {
                    self.dirty = false;
                    self.status = format!("Saved {}", p.display());
                }
                Err(e) => self.status = format!("Save failed: {e}"),
            }
        }
    }
    fn tree(&mut self, ui: &mut egui::Ui, dir: &Path) {
        let Ok(entries) = fs::read_dir(dir) else {
            return;
        };
        let mut paths: Vec<_> = entries.flatten().map(|e| e.path()).collect();
        paths.sort_by_key(|p| {
            (
                !p.is_dir(),
                p.file_name().unwrap_or_default().to_os_string(),
            )
        });
        for path in paths {
            let name = path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();
            if path.is_dir() {
                let response = egui::CollapsingHeader::new(format!("📁 {name}"))
                    .id_salt(&path)
                    .show(ui, |ui| self.tree(ui, &path));
                response
                    .header_response
                    .context_menu(|ui| self.file_menu(ui, &path));
            } else {
                let response = ui.selectable_label(
                    self.active_file.as_ref() == Some(&path),
                    format!("📄 {name}"),
                );
                if response.double_clicked() {
                    self.open_file(path.clone());
                }
                response.context_menu(|ui| self.file_menu(ui, &path));
            }
        }
    }
    fn file_menu(&mut self, ui: &mut egui::Ui, path: &Path) {
        if ui.button("Rename…").clicked() {
            self.rename_target = Some(path.to_path_buf());
            self.rename_text = path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();
            ui.close();
        }
        if ui.button("Move to…").clicked() {
            if let Some(dest) = rfd::FileDialog::new()
                .set_directory(path.parent().unwrap_or(Path::new(".")))
                .save_file()
            {
                if let Err(e) = fs::rename(path, &dest) {
                    self.status = format!("Move failed: {e}");
                } else {
                    self.update_open_path(path, &dest);
                }
            }
            ui.close();
        }
        if ui.button("Delete").clicked() {
            let result = if path.is_dir() {
                fs::remove_dir_all(path)
            } else {
                fs::remove_file(path)
            };
            self.status = result
                .map(|_| "Deleted".into())
                .unwrap_or_else(|e| format!("Delete failed: {e}"));
            ui.close();
        }
    }
    fn update_open_path(&mut self, from: &Path, to: &Path) {
        for open in &mut self.open_files {
            if open == from {
                *open = to.to_path_buf();
            } else if open.starts_with(from) {
                if let Ok(suffix) = open.strip_prefix(from) {
                    *open = to.join(suffix);
                }
            }
        }
        if let Some(active) = &mut self.active_file {
            if active == from {
                *active = to.to_path_buf();
            } else if active.starts_with(from) {
                if let Ok(suffix) = active.strip_prefix(from) {
                    *active = to.join(suffix);
                }
            }
        }
    }
    fn create_entry(&mut self, is_dir: bool) {
        let Some(root) = &self.settings.workspace else {
            return;
        };
        let Some(path) = rfd::FileDialog::new().set_directory(root).save_file() else {
            return;
        };
        let result = if is_dir {
            fs::create_dir_all(&path)
        } else {
            fs::write(&path, "")
        };
        self.status = result
            .map(|_| "Created".into())
            .unwrap_or_else(|e| format!("Create failed: {e}"));
    }
    fn terminal_ui(
        ui: &mut egui::Ui,
        title: &str,
        term: &mut Option<Terminal>,
        cwd: Option<&Path>,
    ) {
        ui.horizontal(|ui| {
            ui.strong(title);
            if ui.button("↻ Restart").clicked() {
                let cmd = term.as_ref().map(|t| t.command.clone()).unwrap_or_default();
                *term = Terminal::spawn(&cmd, cwd).ok();
            }
        });
        if let Some(t) = term {
            t.poll();
            let available = ui.available_size().max(egui::vec2(80.0, 40.0));
            let font = FontId::monospace(14.0);
            let (cell_w, line_h) = ui.fonts(|f| {
                let g = f.layout_no_wrap("M".into(), font.clone(), Color32::WHITE);
                (g.size().x, g.size().y)
            });
            t.resize(
                (available.x / cell_w.max(1.0)).floor() as usize,
                (available.y / line_h.max(1.0)).floor() as usize,
            );
            let (rect, response) = ui.allocate_exact_size(available, egui::Sense::click());
            if response.clicked() {
                response.request_focus();
            }
            t.focused = response.has_focus();
            ui.painter()
                .rect_filled(rect, 0.0, Color32::from_rgb(18, 18, 18));
            for (y, line) in t.cells.iter().enumerate() {
                let mut job = LayoutJob::default();
                for cell in line {
                    job.append(
                        &cell.ch.to_string(),
                        0.0,
                        TextFormat {
                            font_id: font.clone(),
                            color: cell.fg,
                            background: cell.bg,
                            ..Default::default()
                        },
                    );
                }
                let galley = ui.fonts(|f| f.layout_job(job));
                ui.painter().galley(
                    rect.min + egui::vec2(0.0, y as f32 * line_h),
                    galley,
                    Color32::WHITE,
                );
            }
            if t.focused {
                let cursor = egui::Rect::from_min_size(
                    rect.min + egui::vec2(t.cursor_x as f32 * cell_w, t.cursor_y as f32 * line_h),
                    egui::vec2(cell_w, line_h),
                );
                ui.painter().rect_stroke(
                    cursor,
                    0.0,
                    egui::Stroke::new(1.0_f32, Color32::WHITE),
                    egui::StrokeKind::Inside,
                );
                for event in ui.input(|i| i.events.clone()) {
                    match event {
                        egui::Event::Text(s) | egui::Event::Paste(s) => t.send(s.as_bytes()),
                        egui::Event::Key {
                            key,
                            pressed: true,
                            modifiers,
                            ..
                        } => {
                            let bytes: &[u8] = if modifiers.ctrl || modifiers.command {
                                match key {
                                    egui::Key::C => &[3],
                                    egui::Key::D => &[4],
                                    egui::Key::Z => &[26],
                                    egui::Key::A => &[1],
                                    egui::Key::E => &[5],
                                    egui::Key::L => &[12],
                                    _ => &[],
                                }
                            } else {
                                match key {
                                    egui::Key::Enter => b"\r",
                                    egui::Key::Backspace => b"\x7f",
                                    egui::Key::Tab => b"\t",
                                    egui::Key::Escape => b"\x1b",
                                    egui::Key::ArrowUp => b"\x1b[A",
                                    egui::Key::ArrowDown => b"\x1b[B",
                                    egui::Key::ArrowRight => b"\x1b[C",
                                    egui::Key::ArrowLeft => b"\x1b[D",
                                    egui::Key::Home => b"\x1b[H",
                                    egui::Key::End => b"\x1b[F",
                                    egui::Key::Delete => b"\x1b[3~",
                                    egui::Key::PageUp => b"\x1b[5~",
                                    egui::Key::PageDown => b"\x1b[6~",
                                    _ => &[],
                                }
                            };
                            if !bytes.is_empty() {
                                t.send(bytes);
                            }
                        }
                        _ => {}
                    }
                }
            }
        } else {
            ui.colored_label(Color32::LIGHT_RED, "Terminal failed to start");
        }
    }
}

impl eframe::App for EditorApp {
    fn save(&mut self, _storage: &mut dyn eframe::Storage) {
        self.save_settings();
    }
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        ctx.request_repaint_after(Duration::from_millis(100));
        if let Some(t) = &mut self.top {
            t.poll();
        }
        if let Some(t) = &mut self.right {
            t.poll();
        }
        egui::TopBottomPanel::top("toolbar")
            .frame(
                egui::Frame::new()
                    .fill(Color32::from_rgb(248, 248, 248))
                    .inner_margin(egui::Margin::symmetric(12, 9)),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    if ui.button("⌂  Workspace").clicked() {
                        self.choose_workspace();
                    }
                    if let Some(root) = self.settings.workspace.as_ref() {
                        ui.label(
                            egui::RichText::new(
                                root.file_name().unwrap_or_default().to_string_lossy(),
                            )
                            .color(Color32::from_rgb(100, 100, 100)),
                        );
                    }
                    ui.separator();
                    if ui.button("＋  File").clicked() {
                        self.create_entry(false);
                    }
                    if ui.button("＋  Folder").clicked() {
                        self.create_entry(true);
                    }
                    if ui.button("Save  ·  Ctrl+S").clicked() {
                        self.save_file();
                    }
                    ui.separator();
                    ui.label(
                        egui::RichText::new(&self.status)
                            .small()
                            .color(Color32::from_rgb(100, 100, 100)),
                    );
                });
            });
        let top_height = self.settings.top_height;
        let cwd = self.settings.workspace.clone();
        let top_response = egui::TopBottomPanel::bottom("top_terminal")
            .resizable(true)
            .default_height(top_height)
            .min_height(80.0)
            .show(ctx, |ui| {
                Self::terminal_ui(ui, "Shell", &mut self.top, cwd.as_deref());
            });
        self.settings.top_height = top_response.response.rect.height().max(80.0);
        egui::SidePanel::left("activity_bar")
            .exact_width(42.0)
            .resizable(false)
            .frame(
                egui::Frame::new()
                    .fill(Color32::from_rgb(245, 245, 245))
                    .inner_margin(egui::Margin::symmetric(3, 8)),
            )
            .show(ctx, |ui| {
                ui.vertical_centered(|ui| {
                    let explorer = ui.add_sized(
                        [34.0, 40.0],
                        egui::Button::selectable(
                            self.sidebar_mode == SidebarMode::Explorer,
                            egui::RichText::new("▣").size(20.0),
                        ),
                    );
                    let explorer = explorer.on_hover_text("Explorer");
                    if explorer.clicked() {
                        self.sidebar_mode = SidebarMode::Explorer;
                    }
                    let search = ui.add_sized(
                        [34.0, 40.0],
                        egui::Button::selectable(
                            self.sidebar_mode == SidebarMode::Search,
                            egui::RichText::new("⌕").size(23.0),
                        ),
                    );
                    let search = search.on_hover_text("Search");
                    if search.clicked() {
                        self.sidebar_mode = SidebarMode::Search;
                    }
                });
            });
        let left_width = self.settings.explorer_width;
        let left_response = egui::SidePanel::left("explorer")
            .resizable(true)
            .default_width(left_width)
            .width_range(150.0..=500.0)
            .show(ctx, |ui| {
                ui.heading(match self.sidebar_mode {
                    SidebarMode::Explorer => "Explorer",
                    SidebarMode::Search => "Search",
                });
                ui.separator();
                match self.sidebar_mode {
                    SidebarMode::Explorer => {
                        if let Some(root) = self.settings.workspace.clone() {
                            ui.label(
                                egui::RichText::new(
                                    root.file_name().unwrap_or_default().to_string_lossy(),
                                )
                                .strong(),
                            );
                            egui::ScrollArea::vertical().show(ui, |ui| self.tree(ui, &root));
                        } else {
                            ui.label("Choose a workspace folder to begin.");
                        }
                    }
                    SidebarMode::Search => {
                        let mut run_search = false;
                        ui.horizontal(|ui| {
                            let response = ui.add(
                                egui::TextEdit::singleline(&mut self.search_query)
                                    .hint_text("Search in project…")
                                    .desired_width(f32::INFINITY),
                            );
                            if response.lost_focus()
                                && ui.input(|i| i.key_pressed(egui::Key::Enter))
                            {
                                run_search = true;
                            }
                            if ui.button("Search").clicked() {
                                run_search = true;
                            }
                        });
                        if run_search {
                            self.search_workspace();
                        }
                        let mut open_result = None;
                        egui::ScrollArea::vertical().show(ui, |ui| {
                            for group in &self.search_results {
                                let label = group
                                    .path
                                    .strip_prefix(
                                        self.settings.workspace.as_deref().unwrap_or(Path::new("")),
                                    )
                                    .unwrap_or(&group.path)
                                    .display()
                                    .to_string();
                                ui.horizontal(|ui| {
                                    ui.label(
                                        egui::RichText::new("▰")
                                            .color(Color32::from_rgb(0, 122, 204)),
                                    );
                                    ui.label(
                                        egui::RichText::new(label)
                                            .strong()
                                            .color(Color32::from_rgb(58, 58, 58)),
                                    );
                                });
                                for (line, preview) in &group.lines {
                                    let result = ui.selectable_label(
                                        false,
                                        egui::RichText::new(format!("{line:>4}  {preview}"))
                                            .monospace()
                                            .size(12.0)
                                            .color(Color32::from_rgb(95, 95, 95)),
                                    );
                                    if result.clicked() {
                                        open_result = Some((group.path.clone(), *line));
                                    }
                                }
                                ui.add_space(4.0);
                            }
                        });
                        if let Some((path, line)) = open_result {
                            self.open_search_result(path, line);
                        }
                    }
                }
            });
        self.settings.explorer_width = left_response.response.rect.width().max(150.0);
        let right_width = self.settings.right_width;
        let right_response = egui::SidePanel::right("codex_terminal")
            .resizable(true)
            .default_width(right_width)
            .width_range(260.0..=700.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.strong("Codex command");
                });
                ui.horizontal(|ui| {
                    ui.add(
                        egui::TextEdit::singleline(&mut self.settings.right_command)
                            .desired_width(f32::INFINITY),
                    );
                    if ui.button("Run").clicked() {
                        self.right =
                            Terminal::spawn(&self.settings.right_command, cwd.as_deref()).ok();
                        self.save_settings();
                    }
                });
                ui.separator();
                Self::terminal_ui(ui, "", &mut self.right, cwd.as_deref());
            });
        self.settings.right_width = right_response.response.rect.width().max(260.0);
        egui::CentralPanel::default().show(ctx, |ui| {
            if !self.open_files.is_empty() {
                ui.horizontal_wrapped(|ui| {
                    for p in self.open_files.clone() {
                        let label = p.file_name().unwrap_or_default().to_string_lossy();
                        if ui
                            .selectable_label(
                                self.active_file.as_ref() == Some(&p),
                                format!(
                                    "{label}{}",
                                    if self.active_file.as_ref() == Some(&p) && self.dirty {
                                        " •"
                                    } else {
                                        ""
                                    }
                                ),
                            )
                            .clicked()
                        {
                            self.open_file(p);
                        }
                    }
                });
                ui.separator();
            }
            if self.active_file.is_some() {
                let path = self.active_file.clone().unwrap();
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new(path.file_name().unwrap_or_default().to_string_lossy())
                            .strong()
                            .size(16.0),
                    );
                    ui.label(
                        egui::RichText::new(
                            path.parent().unwrap_or(Path::new("")).display().to_string(),
                        )
                        .small()
                        .color(Color32::from_rgb(120, 120, 120)),
                    );
                    if self.dirty {
                        ui.label(
                            egui::RichText::new("● Modified")
                                .small()
                                .color(Color32::from_rgb(176, 112, 0)),
                        );
                    }
                });
                let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
                let mut layouter = |ui: &egui::Ui, text: &dyn egui::TextBuffer, wrap_width: f32| {
                    let mut job = colored_job(text.as_str(), ext);
                    job.wrap.max_width = wrap_width;
                    ui.fonts(|f| f.layout_job(job))
                };
                let editor_id = egui::Id::new("main_editor_text");
                let jump_cursor = self.pending_editor_cursor.take();
                if let Some(cursor) = jump_cursor {
                    let mut state = egui::TextEdit::load_state(ctx, editor_id).unwrap_or_default();
                    state
                        .cursor
                        .set_char_range(Some(CCursorRange::one(CCursor::new(cursor))));
                    egui::TextEdit::store_state(ctx, editor_id, state);
                }
                let response = egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysVisible)
                    .show(ui, |ui| {
                        let line_count = self.buffer.split('\n').count().max(1);
                        let row_height = ui.fonts(|f| f.row_height(&FontId::monospace(14.0)));
                        let digit_width = ui.fonts(|f| {
                            f.layout_no_wrap("0".into(), FontId::monospace(14.0), Color32::WHITE)
                                .size()
                                .x
                        });
                        let gutter_width = digit_width * line_count.to_string().len() as f32 + 16.0;
                        let text_width = (ui.available_width() - gutter_width - 8.0).max(60.0);
                        let line_heights: Vec<f32> = self
                            .buffer
                            .split('\n')
                            .map(|line| {
                                let mut job = colored_job(line, ext);
                                job.wrap.max_width = text_width;
                                ui.fonts(|f| f.layout_job(job).size().y.max(row_height))
                            })
                            .collect();
                        ui.horizontal_top(|ui| {
                            ui.vertical(|ui| {
                                for (index, height) in line_heights.iter().enumerate() {
                                    let number = index + 1;
                                    ui.allocate_ui_with_layout(
                                        egui::vec2(gutter_width, *height),
                                        egui::Layout::top_down(egui::Align::RIGHT),
                                        |ui| {
                                            ui.add(
                                                egui::Label::new(
                                                    egui::RichText::new(format!(
                                                        "{number:>width$}",
                                                        width = line_count.to_string().len()
                                                    ))
                                                    .monospace()
                                                    .color(Color32::from_gray(115)),
                                                )
                                                .selectable(false)
                                                .halign(egui::Align::RIGHT),
                                            );
                                        },
                                    );
                                }
                            });
                            let edit = egui::TextEdit::multiline(&mut self.buffer)
                                .id(editor_id)
                                .font(TextStyle::Monospace)
                                .desired_width(text_width)
                                .desired_rows(1)
                                .code_editor()
                                .layouter(&mut layouter)
                                .show(ui);
                            if let Some(cursor) = jump_cursor {
                                edit.response.request_focus();
                                let mut target = edit
                                    .galley
                                    .pos_from_cursor(CCursor::new(cursor))
                                    .translate(edit.galley_pos.to_vec2());
                                target.min.y -= row_height * 4.0;
                                target.max.y -= row_height * 4.0;
                                ui.scroll_to_rect(target, Some(egui::Align::TOP));
                            }
                            edit.response
                        })
                        .inner
                    })
                    .inner;
                if response.changed() {
                    self.dirty = true;
                }
                if response.has_focus()
                    && ui.input(|i| i.modifiers.ctrl && i.key_pressed(egui::Key::S))
                {
                    self.save_file();
                }
            } else {
                ui.centered_and_justified(|ui| {
                    ui.label("Double-click a file in the explorer to open it")
                });
            }
        });
        if let Some(path) = self.rename_target.clone() {
            egui::Window::new("Rename")
                .collapsible(false)
                .show(ctx, |ui| {
                    ui.text_edit_singleline(&mut self.rename_text);
                    ui.horizontal(|ui| {
                        if ui.button("Rename").clicked() {
                            let dest = path.with_file_name(&self.rename_text);
                            self.status = match fs::rename(&path, &dest) {
                                Ok(()) => {
                                    self.update_open_path(&path, &dest);
                                    "Renamed".into()
                                }
                                Err(e) => format!("Rename failed: {e}"),
                            };
                            self.rename_target = None;
                        }
                        if ui.button("Cancel").clicked() {
                            self.rename_target = None;
                        }
                    });
                });
        }
    }
}
