use crate::{codex_ui::CodexUi, settings::Settings, syntax::colored_job, terminal::Terminal};
use eframe::egui::{
    self, Color32, FontId, TextFormat, TextStyle,
    text::{CCursor, CCursorRange, LayoutJob, LayoutSection},
};
use std::{
    fs,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

#[derive(Clone, Copy, PartialEq, Eq)]
enum SidebarMode {
    Explorer,
    Search,
    Git,
}

struct FileMatches {
    path: PathBuf,
    lines: Vec<(usize, String)>,
}

#[derive(Default)]
struct EditorLayoutCache {
    text: String,
    extension: String,
    query: String,
    active_match: Option<usize>,
    galley: Option<Arc<egui::Galley>>,
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

fn find_ranges(text: &str, query: &str) -> Vec<(usize, usize)> {
    let needle = query.to_lowercase();
    if needle.is_empty() {
        return Vec::new();
    }
    let mut folded = String::new();
    let mut original_char = Vec::new();
    for (index, ch) in text.chars().enumerate() {
        for lower in ch.to_lowercase() {
            folded.push(lower);
            original_char.push(index);
        }
    }
    folded
        .match_indices(&needle)
        .filter_map(|(byte, _)| {
            let start_folded = folded[..byte].chars().count();
            let end_folded = start_folded + needle.chars().count();
            let start = *original_char.get(start_folded)?;
            let end = original_char.get(end_folded.saturating_sub(1))? + 1;
            Some((start, end))
        })
        .collect()
}

fn highlight_find_matches(
    job: &mut LayoutJob,
    text: &str,
    query: &str,
    active_match: Option<usize>,
) {
    let matches = find_ranges(text, query);
    if matches.is_empty() {
        return;
    }
    let char_bytes: Vec<usize> = text
        .char_indices()
        .map(|(byte, _)| byte)
        .chain(std::iter::once(text.len()))
        .collect();
    let byte_matches: Vec<(usize, usize)> = matches
        .iter()
        .filter_map(|(start, end)| Some((*char_bytes.get(*start)?, *char_bytes.get(*end)?)))
        .collect();
    let active_bytes = active_match.and_then(|index| byte_matches.get(index).copied());
    let original_sections = std::mem::take(&mut job.sections);
    for section in original_sections {
        let section_start = section.byte_range.start;
        let section_end = section.byte_range.end;
        let mut boundaries = vec![section_start, section_end];
        for (start, end) in &byte_matches {
            if *start < section_end && *end > section_start {
                boundaries.push((*start).max(section_start));
                boundaries.push((*end).min(section_end));
            }
        }
        boundaries.sort_unstable();
        boundaries.dedup();
        for pair in boundaries.windows(2) {
            let start = pair[0];
            let end = pair[1];
            if start == end {
                continue;
            }
            let mut part = section.clone();
            part.byte_range = start..end;
            if let Some((active_start, active_end)) = active_bytes
                && start >= active_start
                && end <= active_end
            {
                part.format.background = Color32::from_rgb(255, 190, 100);
            } else if byte_matches
                .iter()
                .any(|(match_start, match_end)| start >= *match_start && end <= *match_end)
            {
                part.format.background = Color32::from_rgb(255, 239, 166);
            }
            job.sections.push(LayoutSection {
                leading_space: part.leading_space,
                byte_range: part.byte_range,
                format: part.format,
            });
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
    codex_ui: CodexUi,
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
            codex_ui,
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
                self.reveal_active_tab = true;
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
    fn close_file(&mut self, path: PathBuf) {
        if self.active_file.as_ref() == Some(&path) && self.dirty {
            self.close_confirmation = Some(path);
        } else {
            self.finish_close_file(path);
        }
    }
    fn finish_close_file(&mut self, path: PathBuf) {
        let Some(index) = self.open_files.iter().position(|open| *open == path) else {
            return;
        };
        let was_active = self.active_file.as_ref() == Some(&path);
        self.open_files.remove(index);
        if was_active {
            self.active_file = None;
            self.buffer.clear();
            self.dirty = false;
            self.pending_editor_cursor = None;
            self.find_open = false;
            self.find_active = None;
            if !self.open_files.is_empty() {
                let next = self.open_files[index.min(self.open_files.len() - 1)].clone();
                self.open_file(next);
            } else {
                self.status = "File closed".into();
            }
        } else {
            self.status = "File closed".into();
        }
    }
    fn copy_selection(&mut self, ctx: &egui::Context) {
        let id = egui::Id::new("main_editor_text");
        let Some(range) = egui::TextEdit::load_state(ctx, id)
            .and_then(|state| state.cursor.char_range())
            .filter(|range| !range.is_empty())
        else {
            self.status = "Select text in the editor to copy".into();
            return;
        };
        let range = range.as_sorted_char_range();
        let copied: String = self
            .buffer
            .chars()
            .skip(range.start)
            .take(range.end - range.start)
            .collect();
        ctx.copy_text(copied);
        self.status = "Copied selection".into();
    }
    fn paste_from_clipboard(&mut self, ctx: &egui::Context) {
        ctx.memory_mut(|memory| memory.request_focus(egui::Id::new("main_editor_text")));
        ctx.send_viewport_cmd(egui::ViewportCommand::RequestPaste);
        self.status = "Paste requested".into();
    }
    fn select_all(&mut self, ctx: &egui::Context) {
        let id = egui::Id::new("main_editor_text");
        let mut state = egui::TextEdit::load_state(ctx, id).unwrap_or_default();
        state.cursor.set_char_range(Some(CCursorRange::two(
            CCursor::new(0),
            CCursor::new(self.buffer.chars().count()),
        )));
        egui::TextEdit::store_state(ctx, id, state);
        ctx.memory_mut(|memory| memory.request_focus(id));
    }
    fn navigate_find(&mut self, ctx: &egui::Context, reverse: bool) {
        let matches = find_ranges(&self.buffer, &self.find_query);
        if matches.is_empty() {
            self.find_active = None;
            self.status = if self.find_query.is_empty() {
                "Enter text to find in this file".into()
            } else {
                "No matches in this file".into()
            };
            return;
        }
        let index = match self.find_active {
            Some(current) if reverse => (current + matches.len() - 1) % matches.len(),
            Some(current) => (current + 1) % matches.len(),
            None if reverse => matches.len() - 1,
            None => 0,
        };
        self.find_active = Some(index);
        self.pending_editor_cursor = Some(matches[index].0);
        ctx.memory_mut(|memory| memory.request_focus(egui::Id::new("find_in_file_query")));
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
        ctx: &egui::Context,
        ui: &mut egui::Ui,
        title: &str,
        term: &mut Option<Terminal>,
        cwd: Option<&Path>,
    ) {
        ui.horizontal(|ui| {
            ui.strong(format!("▸ {title}"));
            if ui.button("↻ Restart").clicked() {
                let cmd = term.as_ref().map(|t| t.command.clone()).unwrap_or_default();
                *term = Terminal::spawn(&cmd, cwd, ctx.clone()).ok();
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
        let opened_find = ctx
            .input(|input| input.modifiers.command && input.key_pressed(egui::Key::F))
            && self.active_file.is_some();
        if opened_find {
            self.find_open = true;
        }
        if ctx.input(|input| input.modifiers.command && input.key_pressed(egui::Key::W)) {
            if let Some(path) = self.active_file.clone() {
                self.close_file(path);
            }
        }
        egui::TopBottomPanel::top("toolbar")
            .frame(
                egui::Frame::new()
                    .fill(Color32::from_rgb(248, 248, 248))
                    .inner_margin(egui::Margin::symmetric(12, 9)),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    if ui.button("📁  Workspace").clicked() {
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
                    ui.menu_button("Edit", |ui| {
                        if ui
                            .add_enabled(
                                self.active_file.is_some(),
                                egui::Button::new("💾  Save   Ctrl+S"),
                            )
                            .clicked()
                        {
                            self.save_file();
                            ui.close();
                        }
                        if ui
                            .add_enabled(
                                self.active_file.is_some(),
                                egui::Button::new("⧉  Copy   Ctrl+C"),
                            )
                            .clicked()
                        {
                            self.copy_selection(ctx);
                            ui.close();
                        }
                        if ui
                            .add_enabled(
                                self.active_file.is_some(),
                                egui::Button::new("📋  Paste   Ctrl+V"),
                            )
                            .clicked()
                        {
                            self.paste_from_clipboard(ctx);
                            ui.close();
                        }
                        ui.separator();
                        if ui
                            .add_enabled(
                                self.active_file.is_some(),
                                egui::Button::new("🔎  Find in File   Ctrl+F"),
                            )
                            .clicked()
                        {
                            self.find_open = true;
                            ctx.memory_mut(|memory| {
                                memory.request_focus(egui::Id::new("find_in_file_query"))
                            });
                            ui.close();
                        }
                        if ui
                            .add_enabled(
                                self.active_file.is_some(),
                                egui::Button::new("Select All   Ctrl+A"),
                            )
                            .clicked()
                        {
                            self.select_all(ctx);
                            ui.close();
                        }
                    });
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
                Self::terminal_ui(ctx, ui, "Shell", &mut self.top, cwd.as_deref());
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
                            egui::RichText::new("🗂").size(18.0),
                        ),
                    );
                    let explorer = explorer.on_hover_text("Explorer");
                    if explorer.clicked() {
                        self.sidebar_mode = SidebarMode::Explorer;
                    }
                    if ui
                        .add_sized(
                            [34.0, 40.0],
                            egui::Button::selectable(
                                self.sidebar_mode == SidebarMode::Git,
                                egui::RichText::new("⑂").size(22.0),
                            ),
                        )
                        .on_hover_text("Source Control (Git)")
                        .clicked()
                    {
                        self.sidebar_mode = SidebarMode::Git;
                    }
                    let search = ui.add_sized(
                        [34.0, 40.0],
                        egui::Button::selectable(
                            self.sidebar_mode == SidebarMode::Search,
                            egui::RichText::new("🔍").size(18.0),
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
                if self.sidebar_mode == SidebarMode::Explorer {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 4.0;
                        ui.heading("🗂 Explorer");
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.small_button("📁").on_hover_text("New folder").clicked() {
                                self.create_entry(true);
                            }
                            if ui.small_button("📄").on_hover_text("New file").clicked() {
                                self.create_entry(false);
                            }
                        });
                    });
                } else if self.sidebar_mode == SidebarMode::Git {
                    ui.heading("⑂ Source Control");
                } else {
                    ui.heading("🔎 Search");
                }
                ui.separator();
                match self.sidebar_mode {
                    SidebarMode::Git => {
                        if let Some(path) = self.git_ui.ui(ui, self.settings.workspace.as_deref()) {
                            self.open_file(path);
                        }
                    }
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
                                    .hint_text("🔎 Search in project…")
                                    .desired_width(f32::INFINITY),
                            );
                            if response.lost_focus()
                                && ui.input(|i| i.key_pressed(egui::Key::Enter))
                            {
                                run_search = true;
                            }
                            if ui.button("🔎 Search").clicked() {
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
                self.codex_ui.show(ui, cwd.as_deref(), ctx);
            });
        self.settings.right_width = right_response.response.rect.width().max(260.0);
        egui::CentralPanel::default().show(ctx, |ui| {
            if !self.open_files.is_empty() {
                let mut close_tab = None;
                let mut switch_tab = None;
                let open_files = self.open_files.clone();
                let active_file = self.active_file.clone();
                ui.horizontal(|ui| {
                    let tabs_width = (ui.available_width() - 48.0).max(64.0);
                    ui.allocate_ui_with_layout(
                        egui::vec2(tabs_width, 34.0),
                        egui::Layout::left_to_right(egui::Align::Center),
                        |ui| {
                            let mut active_tab_rect = None;
                            let tab_scroll = egui::ScrollArea::horizontal()
                                .id_salt("open_file_tabs")
                                .auto_shrink([false, false])
                                .show(ui, |ui| {
                                    ui.horizontal(|ui| {
                                        for p in &open_files {
                                            let active = active_file.as_ref() == Some(p);
                                            let dirty = active && self.dirty;
                                            let label = format!(
                                                "{}{}",
                                                p.file_name().unwrap_or_default().to_string_lossy(),
                                                if dirty { " •" } else { "" }
                                            );
                                            let font = FontId::proportional(13.0);
                                            let galley = ui.painter().layout_no_wrap(
                                                label,
                                                font.clone(),
                                                if dirty {
                                                    Color32::from_rgb(176, 112, 0)
                                                } else {
                                                    Color32::from_rgb(55, 55, 55)
                                                },
                                            );
                                            let close_width = 26.0;
                                            let padding = 12.0;
                                            let size = egui::vec2(
                                                galley.size().x + close_width + padding,
                                                30.0,
                                            );
                                            let (rect, _) =
                                                ui.allocate_exact_size(size, egui::Sense::hover());
                                            let tab_id = egui::Id::new(("file_tab", p));
                                            let label_rect = egui::Rect::from_min_max(
                                                rect.min,
                                                egui::pos2(rect.max.x - close_width, rect.max.y),
                                            );
                                            let close_rect = egui::Rect::from_min_max(
                                                egui::pos2(rect.max.x - close_width, rect.min.y),
                                                rect.max,
                                            );
                                            let select = ui.interact(
                                                label_rect,
                                                tab_id,
                                                egui::Sense::click(),
                                            );
                                            let close = ui.interact(
                                                close_rect,
                                                tab_id.with("close"),
                                                egui::Sense::click(),
                                            );
                                            let fill = if active {
                                                Color32::WHITE
                                            } else if select.hovered() || close.hovered() {
                                                Color32::from_rgb(242, 242, 242)
                                            } else {
                                                Color32::from_rgb(236, 236, 236)
                                            };
                                            ui.painter().rect_filled(
                                                rect,
                                                egui::CornerRadius::same(3),
                                                fill,
                                            );
                                            if active {
                                                ui.painter().rect_stroke(
                                                    rect,
                                                    egui::CornerRadius::same(3),
                                                    egui::Stroke::new(
                                                        1.0_f32,
                                                        Color32::from_rgb(215, 215, 215),
                                                    ),
                                                    egui::StrokeKind::Inside,
                                                );
                                            }
                                            ui.painter().galley(
                                                rect.min
                                                    + egui::vec2(
                                                        padding * 0.5,
                                                        (size.y - galley.size().y) * 0.5,
                                                    ),
                                                galley,
                                                if dirty {
                                                    Color32::from_rgb(176, 112, 0)
                                                } else {
                                                    Color32::from_rgb(55, 55, 55)
                                                },
                                            );
                                            ui.painter().text(
                                                close_rect.center(),
                                                egui::Align2::CENTER_CENTER,
                                                "×",
                                                FontId::proportional(16.0),
                                                Color32::from_rgb(100, 100, 100),
                                            );
                                            let select =
                                                select.on_hover_text(p.display().to_string());
                                            let close = close.on_hover_text("Close file (Ctrl+W)");
                                            if select.clicked() {
                                                switch_tab = Some(p.clone());
                                            }
                                            if close.clicked() {
                                                close_tab = Some(p.clone());
                                            }
                                            if active {
                                                active_tab_rect = Some(rect);
                                            }
                                        }
                                    });
                                });
                            if self.reveal_active_tab {
                                if let Some(rect) = active_tab_rect {
                                    let mut state = tab_scroll.state;
                                    let viewport = tab_scroll.inner_rect;
                                    let tab_left = rect.left() - viewport.left() + state.offset.x;
                                    let tab_right = rect.right() - viewport.left() + state.offset.x;
                                    if tab_left < state.offset.x {
                                        state.offset.x = tab_left;
                                    } else if tab_right > state.offset.x + viewport.width() {
                                        state.offset.x = tab_right - viewport.width();
                                    }
                                    state.offset.x = state.offset.x.clamp(
                                        0.0,
                                        (tab_scroll.content_size.x - viewport.width()).max(0.0),
                                    );
                                    state.store(ctx, tab_scroll.id);
                                }
                                self.reveal_active_tab = false;
                            }
                        },
                    );
                    ui.menu_button(format!("⌄ {}", open_files.len()), |ui| {
                        ui.set_min_width(240.0);
                        ui.add(
                            egui::TextEdit::singleline(&mut self.open_file_filter)
                                .hint_text("Find an open file…")
                                .desired_width(f32::INFINITY),
                        );
                        let filter = self.open_file_filter.to_lowercase();
                        egui::ScrollArea::vertical()
                            .max_height(360.0)
                            .show(ui, |ui| {
                                for path in &open_files {
                                    let full_path = path.display().to_string();
                                    if !filter.is_empty()
                                        && !full_path.to_lowercase().contains(&filter)
                                    {
                                        continue;
                                    }
                                    let name =
                                        path.file_name().unwrap_or_default().to_string_lossy();
                                    let selected = active_file.as_ref() == Some(path);
                                    if ui
                                        .selectable_label(selected, format!("{name}\n{full_path}"))
                                        .clicked()
                                    {
                                        switch_tab = Some(path.clone());
                                        ui.close();
                                    }
                                }
                            });
                    });
                });
                if let Some(path) = switch_tab {
                    self.git_ui.dismiss_diff();
                    if self.active_file.as_ref() != Some(&path) {
                        self.open_file(path);
                    }
                }
                if let Some(path) = close_tab {
                    self.close_file(path);
                }
                ui.separator();
            }
            if self.sidebar_mode == SidebarMode::Git && self.git_ui.show_diff(ui) {
                return;
            }
            if self.active_file.is_some() {
                if self.find_open {
                    let matches = find_ranges(&self.buffer, &self.find_query);
                    let mut query_changed = false;
                    let mut navigate = None;
                    let mut close_find = false;
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new("Find").strong());
                        let query = ui.add(
                            egui::TextEdit::singleline(&mut self.find_query)
                                .id(egui::Id::new("find_in_file_query"))
                                .hint_text("Search in this file")
                                .desired_width(210.0),
                        );
                        if opened_find {
                            query.request_focus();
                        }
                        query_changed = query.changed();
                        if query.lost_focus()
                            && ui.input(|input| input.key_pressed(egui::Key::Enter))
                        {
                            navigate = Some(ui.input(|input| input.modifiers.shift));
                        }
                        if ui.button("↑").on_hover_text("Previous match").clicked() {
                            navigate = Some(true);
                        }
                        if ui.button("↓").on_hover_text("Next match").clicked() {
                            navigate = Some(false);
                        }
                        ui.label(format!(
                            "{} / {}",
                            self.find_active.map(|index| index + 1).unwrap_or(0),
                            matches.len()
                        ));
                        close_find = ui.button("×").on_hover_text("Close find").clicked();
                    });
                    if query_changed || self.find_query_snapshot != self.find_query {
                        self.find_query_snapshot = self.find_query.clone();
                        self.find_active = None;
                    }
                    if let Some(reverse) = navigate {
                        self.navigate_find(ctx, reverse);
                    }
                    if close_find {
                        self.find_open = false;
                        self.find_query.clear();
                        self.find_query_snapshot.clear();
                        self.find_active = None;
                    }
                    if ui.input(|input| input.key_pressed(egui::Key::Escape)) {
                        self.find_open = false;
                        self.find_active = None;
                    }
                }
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
                let find_query = if self.find_open {
                    self.find_query.clone()
                } else {
                    String::new()
                };
                let find_active = self.find_active;
                let layout_cache = self.editor_layout_cache.clone();
                let extension = ext.to_string();
                let mut layouter =
                    move |ui: &egui::Ui, text: &dyn egui::TextBuffer, _wrap_width: f32| {
                        let source = text.as_str();
                        {
                            let cached = layout_cache.lock().unwrap();
                            if cached.text == source
                                && cached.extension == extension
                                && cached.query == find_query
                                && cached.active_match == find_active
                            {
                                if let Some(galley) = &cached.galley {
                                    return galley.clone();
                                }
                            }
                        }
                        let mut job = colored_job(source, &extension);
                        highlight_find_matches(&mut job, source, &find_query, find_active);
                        job.wrap.max_width = f32::INFINITY;
                        let galley = ui.fonts(|fonts| fonts.layout_job(job));
                        let mut cached = layout_cache.lock().unwrap();
                        cached.text = source.to_owned();
                        cached.extension = extension.clone();
                        cached.query = find_query.clone();
                        cached.active_match = find_active;
                        cached.galley = Some(galley.clone());
                        galley
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
                let response = egui::ScrollArea::both()
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        let line_count = self
                            .buffer
                            .split('\n')
                            .count()
                            .saturating_sub(usize::from(self.buffer.ends_with('\n')))
                            .max(1);
                        let row_height = ui.fonts(|f| f.row_height(&FontId::monospace(14.0)));
                        let digit_width = ui.fonts(|f| {
                            f.layout_no_wrap("0".into(), FontId::monospace(14.0), Color32::WHITE)
                                .size()
                                .x
                        });
                        let gutter_width = digit_width * line_count.to_string().len() as f32 + 16.0;
                        let text_width = (ui.available_width() - gutter_width - 8.0).max(60.0);
                        let editor_galley = layouter(ui, &self.buffer, text_width);
                        ui.horizontal_top(|ui| {
                            let (gutter_rect, _) = ui.allocate_exact_size(
                                egui::vec2(gutter_width, editor_galley.size().y + row_height),
                                egui::Sense::hover(),
                            );
                            let edit = egui::TextEdit::multiline(&mut self.buffer)
                                .id(editor_id)
                                .font(TextStyle::Monospace)
                                .desired_width(text_width)
                                .desired_rows(1)
                                .code_editor()
                                .layouter(&mut layouter)
                                .show(ui);
                            let visible = ui.clip_rect().intersect(gutter_rect);
                            let row_top = edit.galley_pos.y;
                            let number_width = line_count.to_string().len();
                            let mut char_offset = 0;
                            for (index, line) in
                                self.buffer.split('\n').take(line_count).enumerate()
                            {
                                let line_rect =
                                    edit.galley.pos_from_cursor(CCursor::new(char_offset));
                                let line_y = row_top + line_rect.top();
                                if line_y + line_rect.height() >= visible.top()
                                    && line_y <= visible.bottom()
                                {
                                    ui.painter().text(
                                        egui::pos2(gutter_rect.right() - 8.0, line_y),
                                        egui::Align2::RIGHT_TOP,
                                        format!("{:>number_width$}", index + 1),
                                        FontId::monospace(14.0),
                                        Color32::from_gray(115),
                                    );
                                }
                                char_offset += line.chars().count() + 1;
                            }
                            if let Some(cursor) = jump_cursor {
                                if !self.find_open {
                                    edit.response.request_focus();
                                }
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
        if let Some(path) = self.close_confirmation.clone() {
            egui::Window::new("Unsaved changes")
                .collapsible(false)
                .resizable(false)
                .show(ctx, |ui| {
                    ui.label(format!(
                        "Save changes to {} before closing?",
                        path.file_name().unwrap_or_default().to_string_lossy()
                    ));
                    ui.horizontal(|ui| {
                        if ui.button("Save").clicked() {
                            self.save_file();
                            if !self.dirty {
                                self.close_confirmation = None;
                                self.finish_close_file(path.clone());
                            }
                        }
                        if ui.button("Discard").clicked() {
                            self.close_confirmation = None;
                            self.finish_close_file(path.clone());
                        }
                        if ui.button("Cancel").clicked() {
                            self.close_confirmation = None;
                        }
                    });
                });
        }
    }
}
