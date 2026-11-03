#![cfg_attr(not(target_os = "linux"), allow(unused))]

use eframe::egui::{self, Color32, FontId, TextFormat, TextStyle, text::LayoutJob};
use portable_pty::{Child, CommandBuilder, MasterPty, PtySize, native_pty_system};
use serde::{Deserialize, Serialize};
use std::{
    collections::VecDeque,
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::mpsc::{self, Receiver},
    thread,
    time::Duration,
};

#[derive(Clone, Serialize, Deserialize)]
struct Settings {
    workspace: Option<PathBuf>,
    top_height: f32,
    explorer_width: f32,
    right_width: f32,
    right_command: String,
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

struct Terminal {
    input: Box<dyn Write + Send>,
    master: Box<dyn MasterPty + Send>,
    _child: Box<dyn Child + Send + Sync>,
    rx: Receiver<Vec<u8>>,
    command: String,
    cells: Vec<Vec<Cell>>,
    cols: u16,
    rows: u16,
    cursor_x: usize,
    cursor_y: usize,
    saved_cursor: (usize, usize),
    scrollback: VecDeque<Vec<Cell>>,
    escape: String,
    in_escape: bool,
    in_osc: bool,
    osc_escape: bool,
    focused: bool,
    fg: Color32,
    bg: Color32,
}
#[derive(Clone)]
struct Cell {
    ch: char,
    fg: Color32,
    bg: Color32,
}
impl Default for Cell {
    fn default() -> Self {
        Self {
            ch: ' ',
            fg: Color32::from_rgb(220, 220, 220),
            bg: Color32::from_rgb(18, 18, 18),
        }
    }
}
impl Terminal {
    fn spawn(command: &str, cwd: Option<&Path>) -> Result<Self, String> {
        let pty = native_pty_system();
        let pair = pty
            .openpty(PtySize {
                rows: 24,
                cols: 80,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|e| e.to_string())?;
        let mut cmd = if command.trim().is_empty() {
            let mut shell_cmd = CommandBuilder::new("/bin/bash");
            shell_cmd.arg("--noprofile");
            shell_cmd.arg("--norc");
            shell_cmd.arg("-i");
            shell_cmd.env("SHELL", "/bin/bash");
            shell_cmd
        } else {
            let mut c = CommandBuilder::new("/bin/sh");
            c.arg("-l");
            c.arg("-c");
            c.arg(command);
            c
        };
        cmd.env("TERM", "xterm-256color");
        cmd.env("COLORTERM", "truecolor");
        cmd.cwd(
            cwd.map(Path::to_path_buf)
                .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from("/"))),
        );
        let child = pair.slave.spawn_command(cmd).map_err(|e| e.to_string())?;
        drop(pair.slave);
        let mut reader = pair.master.try_clone_reader().map_err(|e| e.to_string())?;
        let (tx, rx) = mpsc::channel();
        thread::spawn(move || {
            let mut buf = [0u8; 2048];
            loop {
                match reader.read(&mut buf) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => {
                        let _ = tx.send(String::from_utf8_lossy(&buf[..n]).as_bytes().to_vec());
                    }
                }
            }
        });
        Ok(Self {
            input: pair.master.take_writer().map_err(|e| e.to_string())?,
            master: pair.master,
            _child: child,
            rx,
            command: command.into(),
            cells: vec![vec![Cell::default(); 80]; 24],
            cols: 80,
            rows: 24,
            cursor_x: 0,
            cursor_y: 0,
            saved_cursor: (0, 0),
            scrollback: VecDeque::new(),
            escape: String::new(),
            in_escape: false,
            in_osc: false,
            osc_escape: false,
            focused: false,
            fg: Color32::from_rgb(220, 220, 220),
            bg: Color32::from_rgb(18, 18, 18),
        })
    }
    fn poll(&mut self) {
        while let Ok(bytes) = self.rx.try_recv() {
            for ch in String::from_utf8_lossy(&bytes).chars() {
                self.put_char(ch);
            }
        }
    }
    fn send(&mut self, bytes: &[u8]) {
        let _ = self.input.write_all(bytes);
        let _ = self.input.flush();
    }
    fn resize(&mut self, cols: usize, rows: usize) {
        let cols = cols.clamp(2, 500) as u16;
        let rows = rows.clamp(2, 200) as u16;
        if cols == self.cols && rows == self.rows {
            return;
        }
        let mut next = vec![vec![Cell::default(); cols as usize]; rows as usize];
        let trim_top = (self.rows as usize).saturating_sub(rows as usize);
        for (y, line) in self
            .cells
            .iter()
            .skip(trim_top)
            .take(rows as usize)
            .enumerate()
        {
            for (x, cell) in line.iter().take(cols as usize).enumerate() {
                next[y][x] = cell.clone();
            }
        }
        self.cells = next;
        self.cols = cols;
        self.rows = rows;
        self.cursor_x = self.cursor_x.min(cols as usize - 1);
        self.cursor_y = self
            .cursor_y
            .saturating_sub(trim_top)
            .min(rows as usize - 1);
        let _ = self.master.resize(PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        });
    }
    fn put_char(&mut self, ch: char) {
        if self.in_osc {
            if ch == '\x07' || (self.osc_escape && ch == '\\') {
                self.in_osc = false;
                self.osc_escape = false;
            } else {
                self.osc_escape = ch == '\x1b';
            }
            return;
        }
        if self.in_escape {
            if self.escape.is_empty() {
                match ch {
                    '[' => self.escape.push('['),
                    ']' => {
                        self.in_osc = true;
                        self.in_escape = false;
                    }
                    '7' => {
                        self.saved_cursor = (self.cursor_x, self.cursor_y);
                        self.in_escape = false;
                    }
                    '8' => {
                        self.cursor_x = self.saved_cursor.0;
                        self.cursor_y = self.saved_cursor.1;
                        self.in_escape = false;
                    }
                    _ => self.in_escape = false,
                }
            } else {
                self.escape.push(ch);
                if ch.is_ascii_alphabetic() || ch == '~' {
                    self.apply_escape();
                    self.escape.clear();
                    self.in_escape = false;
                }
            }
            return;
        }
        match ch {
            '\x1b' => {
                self.in_escape = true;
                self.escape.clear();
            }
            '\r' => self.cursor_x = 0,
            '\n' => self.line_feed(),
            '\x08' => self.cursor_x = self.cursor_x.saturating_sub(1),
            '\t' => self.cursor_x = ((self.cursor_x / 8) + 1) * 8,
            c if !c.is_control() => {
                if self.cursor_x >= self.cols as usize {
                    self.cursor_x = 0;
                    self.line_feed();
                }
                self.cells[self.cursor_y][self.cursor_x] = Cell {
                    ch: c,
                    fg: self.fg,
                    bg: self.bg,
                };
                self.cursor_x += 1;
            }
            _ => {}
        }
    }
    fn line_feed(&mut self) {
        if self.cursor_y + 1 >= self.rows as usize {
            let line = self.cells.remove(0);
            self.scrollback.push_back(line);
            if self.scrollback.len() > 500 {
                self.scrollback.pop_front();
            }
            self.cells.push(vec![Cell::default(); self.cols as usize]);
        } else {
            self.cursor_y += 1;
        }
    }
    fn apply_escape(&mut self) {
        if self.escape.starts_with('[') {
            let body = &self.escape[1..];
            let final_char = body.chars().last().unwrap_or(' ');
            let params = body
                .trim_end_matches(|c: char| c.is_ascii_alphabetic() || c == '~')
                .trim_start_matches('?');
            let nums: Vec<usize> = params.split(';').map(|s| s.parse().unwrap_or(0)).collect();
            let n = nums.first().copied().filter(|n| *n > 0).unwrap_or(1);
            match final_char {
                'A' => self.cursor_y = self.cursor_y.saturating_sub(n),
                'B' => self.cursor_y = (self.cursor_y + n).min(self.rows as usize - 1),
                'C' => self.cursor_x = (self.cursor_x + n).min(self.cols as usize - 1),
                'D' => self.cursor_x = self.cursor_x.saturating_sub(n),
                'G' | '`' => self.cursor_x = n.saturating_sub(1).min(self.cols as usize - 1),
                'd' => self.cursor_y = n.saturating_sub(1).min(self.rows as usize - 1),
                'E' => {
                    self.cursor_y = (self.cursor_y + n).min(self.rows as usize - 1);
                    self.cursor_x = 0;
                }
                'F' => {
                    self.cursor_y = self.cursor_y.saturating_sub(n);
                    self.cursor_x = 0;
                }
                'H' | 'f' => {
                    self.cursor_y = nums
                        .first()
                        .copied()
                        .unwrap_or(1)
                        .saturating_sub(1)
                        .min(self.rows as usize - 1);
                    self.cursor_x = nums
                        .get(1)
                        .copied()
                        .unwrap_or(1)
                        .saturating_sub(1)
                        .min(self.cols as usize - 1);
                }
                'J' => {
                    let mode = nums.first().copied().unwrap_or(0);
                    if mode == 2 || mode == 3 {
                        self.cells =
                            vec![vec![Cell::default(); self.cols as usize]; self.rows as usize];
                        self.cursor_x = 0;
                        self.cursor_y = 0;
                    } else if mode == 0 {
                        for y in self.cursor_y..self.rows as usize {
                            let start = if y == self.cursor_y { self.cursor_x } else { 0 };
                            for c in &mut self.cells[y][start..] {
                                *c = Cell::default();
                            }
                        }
                    }
                }
                'K' => {
                    let mode = nums.first().copied().unwrap_or(0);
                    let (a, b) = match mode {
                        1 => (0, self.cursor_x + 1),
                        2 => (0, self.cols as usize),
                        _ => (self.cursor_x, self.cols as usize),
                    };
                    for c in &mut self.cells[self.cursor_y][a..b] {
                        *c = Cell::default();
                    }
                }
                's' => self.saved_cursor = (self.cursor_x, self.cursor_y),
                'u' => {
                    self.cursor_x = self.saved_cursor.0;
                    self.cursor_y = self.saved_cursor.1;
                }
                'm' => self.apply_sgr(&nums),
                _ => {}
            }
        } else if self.escape == "7" {
            self.saved_cursor = (self.cursor_x, self.cursor_y);
        } else if self.escape == "8" {
            self.cursor_x = self.saved_cursor.0;
            self.cursor_y = self.saved_cursor.1;
        }
    }
    fn apply_sgr(&mut self, values: &[usize]) {
        let vals = if values.is_empty() { &[0][..] } else { values };
        let mut i = 0;
        while i < vals.len() {
            match vals[i] {
                0 => {
                    self.fg = Color32::from_rgb(220, 220, 220);
                    self.bg = Color32::from_rgb(18, 18, 18);
                }
                30..=37 => self.fg = ansi_color(vals[i] - 30, false),
                90..=97 => self.fg = ansi_color(vals[i] - 90, true),
                40..=47 => self.bg = ansi_color(vals[i] - 40, false),
                38 | 48 if vals.get(i + 1) == Some(&2) && vals.len() >= i + 5 => {
                    let color = Color32::from_rgb(
                        vals[i + 2].min(255) as u8,
                        vals[i + 3].min(255) as u8,
                        vals[i + 4].min(255) as u8,
                    );
                    if vals[i] == 38 {
                        self.fg = color;
                    } else {
                        self.bg = color;
                    }
                    i += 4;
                }
                38 | 48 if vals.get(i + 1) == Some(&5) && vals.len() >= i + 3 => {
                    let color = indexed_color(vals[i + 2]);
                    if vals[i] == 38 {
                        self.fg = color;
                    } else {
                        self.bg = color;
                    }
                    i += 2;
                }
                _ => {}
            }
            i += 1;
        }
    }
}
fn indexed_color(index: usize) -> Color32 {
    const BASE: [[u8; 3]; 16] = [
        [0, 0, 0],
        [205, 49, 49],
        [13, 188, 121],
        [229, 229, 16],
        [36, 114, 200],
        [188, 63, 188],
        [17, 168, 205],
        [229, 229, 229],
        [102, 102, 102],
        [241, 76, 76],
        [35, 209, 139],
        [245, 245, 67],
        [59, 142, 234],
        [214, 112, 214],
        [41, 184, 219],
        [255, 255, 255],
    ];
    if index < 16 {
        let c = BASE[index];
        return Color32::from_rgb(c[0], c[1], c[2]);
    }
    if index < 232 {
        let n = index - 16;
        let levels = [0, 95, 135, 175, 215, 255];
        return Color32::from_rgb(
            levels[n / 36] as u8,
            levels[(n / 6) % 6] as u8,
            levels[n % 6] as u8,
        );
    }
    let gray = (8 + 10 * (index.min(255) - 232)) as u8;
    Color32::from_gray(gray)
}
fn ansi_color(n: usize, bright: bool) -> Color32 {
    let base = [
        [0, 0, 0],
        [205, 49, 49],
        [13, 188, 121],
        [229, 229, 16],
        [36, 114, 200],
        [188, 63, 188],
        [17, 168, 205],
        [229, 229, 229],
    ][n.min(7)];
    let add = if bright { 25 } else { 0 };
    Color32::from_rgb(
        (base[0] + add).min(255) as u8,
        (base[1] + add).min(255) as u8,
        (base[2] + add).min(255) as u8,
    )
}

struct EditorApp {
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
}
impl EditorApp {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
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
        egui::TopBottomPanel::top("toolbar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                if ui.button("📂 Workspace").clicked() {
                    self.choose_workspace();
                }
                if ui.button("New file").clicked() {
                    self.create_entry(false);
                }
                if ui.button("New folder").clicked() {
                    self.create_entry(true);
                }
                if ui.button("Save  Ctrl+S").clicked() {
                    self.save_file();
                }
                ui.separator();
                ui.label(&self.status);
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
        let left_width = self.settings.explorer_width;
        let left_response = egui::SidePanel::left("explorer")
            .resizable(true)
            .default_width(left_width)
            .width_range(150.0..=500.0)
            .show(ctx, |ui| {
                ui.heading("Explorer");
                if let Some(root) = self.settings.workspace.clone() {
                    ui.label(root.file_name().unwrap_or_default().to_string_lossy());
                    egui::ScrollArea::vertical().show(ui, |ui| self.tree(ui, &root));
                } else {
                    ui.label("Choose a workspace folder to begin.");
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
                    ui.label(path.display().to_string());
                    if self.dirty {
                        ui.colored_label(Color32::YELLOW, "Modified");
                    }
                });
                let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
                let mut layouter = |ui: &egui::Ui, text: &dyn egui::TextBuffer, wrap_width: f32| {
                    let mut job = colored_job(text.as_str(), ext);
                    job.wrap.max_width = wrap_width;
                    ui.fonts(|f| f.layout_job(job))
                };
                let response = ui.add(
                    egui::TextEdit::multiline(&mut self.buffer)
                        .font(TextStyle::Monospace)
                        .desired_width(f32::INFINITY)
                        .desired_rows(30)
                        .code_editor()
                        .layouter(&mut layouter),
                );
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
fn colored_job(text: &str, ext: &str) -> LayoutJob {
    let mut job = LayoutJob::default();
    let lang = match ext {
        "rs" => "rust",
        "py" => "python",
        "js" | "jsx" | "ts" | "tsx" => "js",
        "json" => "json",
        "toml" => "toml",
        "html" | "htm" => "html",
        "css" | "scss" => "css",
        "sh" | "bash" => "sh",
        "sql" => "sql",
        "md" => "md",
        _ => "plain",
    };
    let keywords = match lang {
        "rust" => {
            "fn let mut pub use mod struct enum impl trait async await return if else match for while loop in as const static type Self self crate"
        }
        "python" => {
            "def class import from as return if elif else for while in try except finally with lambda yield async await True False None self"
        }
        "js" => {
            "const let var function return if else for while class import export from async await new this true false null"
        }
        "sql" => "SELECT FROM WHERE INSERT INTO UPDATE SET DELETE CREATE TABLE JOIN ON AND OR NULL",
        "sh" => "if then else fi for in do done case esac function export",
        _ => "",
    };
    let default = TextFormat {
        font_id: FontId::monospace(14.0),
        color: Color32::from_rgb(210, 214, 220),
        ..Default::default()
    };
    for line in text.split_inclusive('\n') {
        let trimmed = line.trim_start();
        if trimmed.starts_with("//") || trimmed.starts_with('#') || trimmed.starts_with("<!--") {
            job.append(
                line,
                0.0,
                TextFormat {
                    color: Color32::from_rgb(110, 145, 110),
                    ..default.clone()
                },
            );
            continue;
        }
        let mut word = String::new();
        let mut in_string = false;
        let mut quote = '\0';
        for ch in line.chars() {
            if in_string {
                word.push(ch);
                if ch == quote {
                    job.append(
                        &word,
                        0.0,
                        TextFormat {
                            color: Color32::from_rgb(206, 145, 120),
                            ..default.clone()
                        },
                    );
                    word.clear();
                    in_string = false;
                }
            } else if ch == '\'' || ch == '"' || ch == '`' {
                if !word.is_empty() {
                    append_word(&mut job, &word, &default, keywords, lang);
                    word.clear();
                }
                word.push(ch);
                quote = ch;
                in_string = true;
            } else if ch.is_alphanumeric() || ch == '_' {
                word.push(ch);
            } else {
                if !word.is_empty() {
                    append_word(&mut job, &word, &default, keywords, lang);
                    word.clear();
                }
                job.append(&ch.to_string(), 0.0, default.clone());
            }
        }
        if !word.is_empty() {
            append_word(&mut job, &word, &default, keywords, lang);
        }
    }
    job
}
fn append_word(job: &mut LayoutJob, word: &str, fmt: &TextFormat, keywords: &str, lang: &str) {
    let color = if keywords
        .split_whitespace()
        .any(|k| k.eq_ignore_ascii_case(word))
    {
        Color32::from_rgb(198, 120, 221)
    } else if word.parse::<f64>().is_ok() {
        Color32::from_rgb(209, 154, 102)
    } else if lang == "json" && word.ends_with(':') {
        Color32::from_rgb(97, 175, 239)
    } else {
        fmt.color
    };
    job.append(
        word,
        0.0,
        TextFormat {
            color,
            ..fmt.clone()
        },
    );
}

fn main() -> eframe::Result<()> {
    if !cfg!(target_os = "linux") {
        eprintln!("Focus Editor currently supports Linux only");
        return Ok(());
    }
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1440.0, 900.0])
            .with_min_inner_size([900.0, 600.0]),
        ..Default::default()
    };
    eframe::run_native(
        "Focus Editor",
        options,
        Box::new(|cc| Ok(Box::new(EditorApp::new(cc)))),
    )
}
