use eframe::egui::Color32;
use portable_pty::{Child, CommandBuilder, MasterPty, PtySize, native_pty_system};
use std::{
    collections::VecDeque,
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::mpsc::{self, Receiver},
    thread,
};
use unicode_width::UnicodeWidthChar;

pub(crate) struct Terminal {
    pub(crate) input: Box<dyn Write + Send>,
    pub(crate) master: Box<dyn MasterPty + Send>,
    pub(crate) _child: Box<dyn Child + Send + Sync>,
    pub(crate) rx: Receiver<Vec<u8>>,
    pub(crate) command: String,
    pub(crate) cells: Vec<Vec<Cell>>,
    pub(crate) cols: u16,
    pub(crate) rows: u16,
    pub(crate) cursor_x: usize,
    pub(crate) cursor_y: usize,
    pub(crate) saved_cursor: (usize, usize),
    pub(crate) scrollback: VecDeque<Vec<Cell>>,
    pending_utf8: Vec<u8>,
    pub(crate) escape: String,
    pub(crate) in_escape: bool,
    pub(crate) in_osc: bool,
    pub(crate) osc_escape: bool,
    pub(crate) input_modes: crate::terminal_input::InputModes,
    pub(crate) fg: Color32,
    pub(crate) bg: Color32,
}
#[derive(Clone)]
pub(crate) struct Cell {
    pub(crate) ch: char,
    pub(crate) combining: String,
    pub(crate) width: usize,
    pub(crate) fg: Color32,
    pub(crate) bg: Color32,
}
impl Default for Cell {
    fn default() -> Self {
        Self {
            ch: ' ',
            combining: String::new(),
            width: 1,
            fg: Color32::from_rgb(221, 232, 246),
            bg: crate::ui_theme::TERMINAL_BG,
        }
    }
}
impl Terminal {
    pub(crate) fn spawn(
        command: &str,
        cwd: Option<&Path>,
        repaint_ctx: eframe::egui::Context,
    ) -> Result<Self, String> {
        Self::spawn_with_runtime(command, cwd, None, repaint_ctx)
    }
    pub(crate) fn spawn_with_runtime(
        command: &str,
        cwd: Option<&Path>,
        runtime: Option<&crate::dev_container::Runtime>,
        repaint_ctx: eframe::egui::Context,
    ) -> Result<Self, String> {
        log::info!("spawn_with_runtime requested");
        let pty = native_pty_system();
        let pair = pty
            .openpty(PtySize {
                rows: 24,
                cols: 80,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|e| {
                log::warn!("Terminal startup operation failed");
                e.to_string()
            })?;
        let mut cmd = if let Some(runtime) = runtime {
            let mut command_builder = CommandBuilder::new("docker");
            command_builder.arg("exec");
            command_builder.arg("-it");
            command_builder.arg("-w");
            command_builder.arg(&runtime.workspace_folder);
            if let Some(user) = &runtime.remote_user {
                command_builder.arg("-u");
                command_builder.arg(user);
            }
            command_builder.arg(&runtime.container_id);
            if command.trim().is_empty() {
                command_builder.arg("/bin/sh");
                command_builder.arg("-c");
                command_builder.arg(CONTAINER_SHELL);
            } else {
                command_builder.arg("/bin/sh");
                command_builder.arg("-l");
                command_builder.arg("-c");
                command_builder.arg(command);
            }
            command_builder
        } else if command.trim().is_empty() {
            let shell = configured_shell();
            let mut shell_cmd = CommandBuilder::new(&shell);
            shell_cmd.arg("-i");
            shell_cmd.env("SHELL", &shell);
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
        if runtime.is_none() {
            cmd.cwd(
                cwd.map(Path::to_path_buf).unwrap_or_else(|| {
                    std::env::current_dir().unwrap_or_else(|_| PathBuf::from("/"))
                }),
            );
        }
        let child = pair.slave.spawn_command(cmd).map_err(|e| {
            log::warn!("Terminal startup operation failed");
            e.to_string()
        })?;
        drop(pair.slave);
        let mut reader = pair.master.try_clone_reader().map_err(|e| {
            log::warn!("Terminal startup operation failed");
            e.to_string()
        })?;
        let (tx, rx) = mpsc::channel();
        thread::spawn(move || {
            let mut buf = [0u8; 2048];
            loop {
                match reader.read(&mut buf) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => {
                        if tx.send(buf[..n].to_vec()).is_ok() {
                            repaint_ctx.request_repaint();
                        } else {
                            break;
                        }
                    }
                }
            }
        });
        Ok(Self {
            input: pair.master.take_writer().map_err(|e| {
                log::warn!("Terminal startup operation failed");
                e.to_string()
            })?,
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
            pending_utf8: Vec::new(),
            escape: String::new(),
            in_escape: false,
            in_osc: false,
            osc_escape: false,
            input_modes: Default::default(),
            fg: Color32::from_rgb(221, 232, 246),
            bg: crate::ui_theme::TERMINAL_BG,
        })
    }
    pub(crate) fn poll(&mut self) {
        while let Ok(bytes) = self.rx.try_recv() {
            self.receive(&bytes);
        }
    }
    fn receive(&mut self, bytes: &[u8]) {
        self.pending_utf8.extend_from_slice(bytes);
        loop {
            let (valid, invalid) = match std::str::from_utf8(&self.pending_utf8) {
                Ok(text) => (text.len(), None),
                Err(error) => (error.valid_up_to(), error.error_len()),
            };
            let text = String::from_utf8_lossy(&self.pending_utf8[..valid]).into_owned();
            for ch in text.chars() {
                self.put_char(ch);
            }
            self.pending_utf8.drain(..valid);
            if let Some(length) = invalid {
                self.pending_utf8.drain(..length);
                self.put_char('\u{fffd}');
            } else {
                break;
            }
        }
    }
    pub(crate) fn send(&mut self, bytes: &[u8]) {
        if let Err(error) = self.input.write_all(bytes).and_then(|_| self.input.flush()) {
            log::warn!("Terminal input write failed kind={:?}", error.kind());
        }
    }
    pub(crate) fn resize(&mut self, cols: usize, rows: usize) {
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
        for line in &mut next {
            normalize_line(line);
        }
        self.cells = next;
        log::debug!("Terminal resized columns={cols} rows={rows}");
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
            if ch == '\x1b' {
                self.escape.clear();
                return;
            }
            if matches!(ch, '\x18' | '\x1a') {
                self.escape.clear();
                self.in_escape = false;
                return;
            }
            if self.escape.is_empty() {
                match ch {
                    '[' => self.escape.push('['),
                    '\u{20}'..='\u{2f}' => self.escape.push(ch),
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
            } else if self.escape.starts_with('[') {
                self.escape.push(ch);
                if ('\u{40}'..='\u{7e}').contains(&ch) {
                    self.apply_escape();
                    self.escape.clear();
                    self.in_escape = false;
                }
            } else if ('\u{30}'..='\u{7e}').contains(&ch) {
                // ESC intermediates (0x20..0x2f) introduce sequences such as
                // ESC ( B. Consume their final byte without printing it.
                // Additional intermediate bytes need no storage here because
                // these sequences are currently ignored rather than rendered.
                self.escape.clear();
                self.in_escape = false;
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
            '\t' => self.cursor_x = (((self.cursor_x / 8) + 1) * 8).min(self.cols as usize - 1),
            c if !c.is_control() => {
                let width = c.width().unwrap_or(0);
                if width == 0 {
                    let (y, mut x) = if self.cursor_x > 0 {
                        (
                            self.cursor_y,
                            (self.cursor_x - 1).min(self.cols as usize - 1),
                        )
                    } else if self.cursor_y > 0 {
                        (self.cursor_y - 1, self.cols as usize - 1)
                    } else {
                        return;
                    };
                    if self.cells[y][x].width == 0 && x > 0 {
                        x -= 1;
                    }
                    self.cells[y][x].combining.push(c);
                    return;
                }
                if self.cursor_x + width > self.cols as usize {
                    self.cursor_x = 0;
                    self.line_feed();
                }
                for x in self.cursor_x..self.cursor_x + width {
                    self.clear_cell(x);
                }
                self.cells[self.cursor_y][self.cursor_x] = Cell {
                    ch: c,
                    combining: String::new(),
                    width,
                    fg: self.fg,
                    bg: self.bg,
                };
                if width == 2 {
                    self.cells[self.cursor_y][self.cursor_x + 1] = Cell {
                        width: 0,
                        fg: self.fg,
                        bg: self.bg,
                        ..Cell::default()
                    };
                }
                self.cursor_x += width;
            }
            _ => {}
        }
    }
    fn clear_cell(&mut self, x: usize) {
        let line = &mut self.cells[self.cursor_y];
        if line[x].width == 0 && x > 0 {
            line[x - 1] = Cell::default();
        } else if line[x].width == 2 && x + 1 < line.len() {
            line[x + 1] = Cell::default();
        }
        line[x] = Cell::default();
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
        self.input_modes.update(&self.escape);
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
                            let start = if y == self.cursor_y {
                                self.cursor_x.min(self.cols as usize)
                            } else {
                                0
                            };
                            for c in &mut self.cells[y][start..] {
                                *c = Cell::default();
                            }
                            normalize_line(&mut self.cells[y]);
                        }
                    }
                }
                'K' => {
                    let mode = nums.first().copied().unwrap_or(0);
                    let (a, b) = match mode {
                        1 => (0, (self.cursor_x + 1).min(self.cols as usize)),
                        2 => (0, self.cols as usize),
                        _ => (self.cursor_x.min(self.cols as usize), self.cols as usize),
                    };
                    for c in &mut self.cells[self.cursor_y][a..b] {
                        *c = Cell::default();
                    }
                    normalize_line(&mut self.cells[self.cursor_y]);
                }
                '@' | 'P' | 'X' => {
                    let start = self.cursor_x.min(self.cols as usize - 1);
                    let count = n.min(self.cols as usize - start);
                    let line = &mut self.cells[self.cursor_y];
                    match final_char {
                        '@' => {
                            line[start..].rotate_right(count);
                            line[start..start + count].fill(Cell::default());
                        }
                        'P' => {
                            line[start..].rotate_left(count);
                            let end = line.len();
                            line[end - count..].fill(Cell::default());
                        }
                        _ => line[start..start + count].fill(Cell::default()),
                    }
                    normalize_line(line);
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
                    self.fg = Color32::from_rgb(221, 232, 246);
                    self.bg = crate::ui_theme::TERMINAL_BG;
                }
                39 => self.fg = Cell::default().fg,
                49 => self.bg = Cell::default().bg,
                100..=107 => self.bg = ansi_color(vals[i] - 100, true),
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
// Erasing or cropping half of a wide glyph must clear the remaining half.
fn normalize_line(line: &mut [Cell]) {
    for x in 0..line.len() {
        let valid = match line[x].width {
            0 => x > 0 && line[x - 1].width == 2,
            2 => x + 1 < line.len() && line[x + 1].width == 0,
            _ => true,
        };
        if !valid {
            line[x] = Cell::default();
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

// Resolve in the container, never using the host user's shell path.
const CONTAINER_SHELL: &str = r#"entry=$(getent passwd "$(id -u)" 2>/dev/null)
user_home=$(printf '%s' "$entry" | cut -d: -f6)
[ -z "$user_home" ] || export HOME="$user_home"
shell=$(printf '%s' "$entry" | cut -d: -f7)
[ -n "$shell" ] && [ -x "$shell" ] || shell=${SHELL:-/bin/sh}
[ -x "$shell" ] || shell=/bin/sh
export SHELL="$shell"
exec "$shell" -i"#;

fn configured_shell() -> PathBuf {
    select_shell(
        std::env::var_os("SHELL").map(PathBuf::from),
        account_shell(),
    )
}

fn select_shell(environment: Option<PathBuf>, account: Option<PathBuf>) -> PathBuf {
    use std::os::unix::fs::PermissionsExt;
    environment
        .into_iter()
        .chain(account)
        .find(|path| {
            path.is_absolute()
                && path
                    .metadata()
                    .is_ok_and(|meta| meta.is_file() && meta.permissions().mode() & 0o111 != 0)
        })
        .unwrap_or_else(|| PathBuf::from("/bin/sh"))
}

fn account_shell() -> Option<PathBuf> {
    use std::{ffi::CStr, os::unix::ffi::OsStrExt};
    let mut buffer = vec![0u8; 16384];
    loop {
        let mut entry = std::mem::MaybeUninit::<libc::passwd>::uninit();
        let mut result = std::ptr::null_mut();
        // SAFETY: entry and buffer are writable; pointers are used only while buffer lives.
        let status = unsafe {
            libc::getpwuid_r(
                libc::getuid(),
                entry.as_mut_ptr(),
                buffer.as_mut_ptr().cast(),
                buffer.len(),
                &mut result,
            )
        };
        if status == libc::ERANGE && buffer.len() < 1048576 {
            buffer.resize(buffer.len() * 2, 0);
            continue;
        }
        if status != 0 || result.is_null() {
            return None;
        }
        // SAFETY: successful getpwuid_r initialized entry and its NUL-terminated fields.
        let shell = unsafe { CStr::from_ptr(entry.assume_init().pw_shell) };
        return Some(PathBuf::from(std::ffi::OsStr::from_bytes(shell.to_bytes())));
    }
}

#[cfg(test)]
mod tests;
