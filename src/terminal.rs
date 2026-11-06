//! Shell process lifecycle and a PTY adapter for the Ghostty-backed egui widget.
use portable_pty::{Child, CommandBuilder, MasterPty, PtySize, native_pty_system};
use std::{
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread::{self, JoinHandle},
};

pub(crate) struct Terminal {
    pub(crate) view: egui_tty::Terminal,
    pub(crate) command: String,
    cwd: Option<PathBuf>,
    container_id: Option<String>,
    pty: Arc<Pty>,
    stopping: Arc<AtomicBool>,
    reader: Option<JoinHandle<()>>,
}

struct Pty {
    input: Mutex<Box<dyn Write + Send>>,
    master: Mutex<Box<dyn MasterPty + Send>>,
    child: Mutex<Box<dyn Child + Send + Sync>>,
}

impl egui_tty::Tty for Pty {
    fn write(&self, bytes: &[u8]) -> egui_tty::Result<()> {
        let mut input = self.input.lock().map_err(egui_tty::Error::msg)?;
        input
            .write_all(bytes)
            .and_then(|_| input.flush())
            .map_err(egui_tty::Error::msg)
    }
    fn resize(&self, cols: u16, rows: u16) -> egui_tty::Result<()> {
        self.master
            .lock()
            .map_err(egui_tty::Error::msg)?
            .resize(PtySize {
                cols: cols.max(2),
                rows: rows.max(2),
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(egui_tty::Error::msg)
    }
    fn has_exited(&self) -> bool {
        self.child
            .lock()
            .is_ok_and(|mut child| child.try_wait().is_ok_and(|status| status.is_some()))
    }
}

impl Terminal {
    pub(crate) fn spawn(
        command: &str,
        cwd: Option<&Path>,
        ctx: eframe::egui::Context,
    ) -> Result<Self, String> {
        Self::spawn_with_runtime(command, cwd, None, ctx)
    }
    pub(crate) fn spawn_with_runtime(
        command: &str,
        cwd: Option<&Path>,
        runtime: Option<&crate::dev_container::Runtime>,
        repaint_ctx: eframe::egui::Context,
    ) -> Result<Self, String> {
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
            // docker exec does not otherwise forward the client's terminal environment.
            command_builder.arg("-e");
            command_builder.arg("TERM=xterm-256color");
            command_builder.arg("-e");
            command_builder.arg("COLORTERM=truecolor");
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
        let mut child = pair
            .slave
            .spawn_command(cmd)
            .map_err(|error| error.to_string())?;
        drop(pair.slave);
        // Acquire the handles before handing ownership to the widget; failure must reap the child.
        let handles = (|| {
            let reader = pair
                .master
                .try_clone_reader()
                .map_err(|error| error.to_string())?;
            let writer = pair
                .master
                .take_writer()
                .map_err(|error| error.to_string())?;
            let fd = pair
                .master
                .as_raw_fd()
                .ok_or("PTY has no pollable descriptor")?;
            Ok::<_, String>((reader, writer, fd))
        })();
        let (mut reader, writer, fd) = match handles {
            Ok(handles) => handles,
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(error);
            }
        };
        let pty = Arc::new(Pty {
            input: Mutex::new(writer),
            master: Mutex::new(pair.master),
            child: Mutex::new(child),
        });
        let (tx, output) = mpsc::sync_channel(128);
        let stopping = Arc::new(AtomicBool::new(false));
        let mut terminal = Self {
            view: egui_tty::Terminal::with_scrollback(
                egui_tty::TtyStream {
                    output,
                    tty: pty.clone(),
                },
                10_000,
            )
            .map_err(|error| error.to_string())?
            .with_label("shell"),
            command: command.into(),
            cwd: cwd.map(Path::to_path_buf),
            container_id: runtime.map(|runtime| runtime.container_id.clone()),
            pty,
            stopping: stopping.clone(),
            reader: None,
        };
        terminal.reader = Some(thread::spawn(move || {
            let mut buffer = [0u8; 8192];
            while !stopping.load(Ordering::Acquire) {
                let mut descriptor = libc::pollfd {
                    fd,
                    events: libc::POLLIN,
                    revents: 0,
                };
                // SAFETY: descriptor is valid; Terminal retains the master until this thread joins.
                let ready = unsafe { libc::poll(&mut descriptor, 1, 50) };
                if ready == 0 {
                    continue;
                }
                if ready < 0 {
                    if std::io::Error::last_os_error().kind() == std::io::ErrorKind::Interrupted {
                        continue;
                    }
                    break;
                }
                match reader.read(&mut buffer) {
                    Ok(0) => break,
                    Ok(count) => {
                        // Bound queued output without dropping bytes; shutdown can interrupt backpressure.
                        let mut bytes = buffer[..count].to_vec();
                        loop {
                            match tx.try_send(bytes) {
                                Ok(()) => {
                                    repaint_ctx.request_repaint();
                                    break;
                                }
                                Err(mpsc::TrySendError::Full(pending)) => {
                                    if stopping.load(Ordering::Acquire) {
                                        return;
                                    }
                                    bytes = pending;
                                    thread::sleep(std::time::Duration::from_millis(5));
                                }
                                Err(mpsc::TrySendError::Disconnected(_)) => return,
                            }
                        }
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
                    Err(_) => break,
                }
            }
            repaint_ctx.request_repaint();
        }));
        Ok(terminal)
    }
    pub(crate) fn matches_context(
        &self,
        cwd: Option<&Path>,
        runtime: Option<&crate::dev_container::Runtime>,
    ) -> bool {
        self.cwd.as_deref() == cwd
            && self.container_id.as_deref() == runtime.map(|runtime| runtime.container_id.as_str())
    }
    pub(crate) fn poll(&mut self) {
        self.view.poll();
    }
    #[cfg(test)]
    pub(crate) fn send(&self, bytes: &[u8]) {
        if self.view.send(bytes).is_err() {
            log::warn!("Terminal input write failed");
        }
    }
    #[cfg(test)]
    pub(crate) fn capture_output(&mut self, input: Box<dyn Write + Send>) -> mpsc::Sender<Vec<u8>> {
        *self.pty.input.lock().unwrap() = input;
        let (tx, output) = mpsc::channel();
        self.view = egui_tty::Terminal::new(egui_tty::TtyStream {
            output,
            tty: self.pty.clone(),
        })
        .unwrap();
        tx
    }
    #[cfg(test)]
    pub(crate) fn process_id(&self) -> Option<u32> {
        self.pty.child.lock().unwrap().process_id()
    }
}
impl Drop for Terminal {
    fn drop(&mut self) {
        self.stopping.store(true, Ordering::Release);
        // Hang up the current foreground job as a normal terminal close would.
        if let Ok(master) = self.pty.master.lock() {
            if let Some(fd) = master.as_raw_fd() {
                // SAFETY: fd belongs to the live PTY. Only signal its foreground process group.
                let group = unsafe { libc::tcgetpgrp(fd) };
                if group > 0 && group != unsafe { libc::getpgrp() } {
                    unsafe {
                        libc::kill(-group, libc::SIGHUP);
                    }
                }
            }
        }
        if let Ok(mut child) = self.pty.child.lock() {
            if !child.try_wait().is_ok_and(|status| status.is_some()) {
                let _ = child.kill();
            }
            let _ = child.wait();
        }
        if let Some(reader) = self.reader.take() {
            let _ = reader.join();
        }
    }
}
impl Drop for Pty {
    fn drop(&mut self) {
        // Also covers widget initialization failures before Terminal exists.
        if let Ok(child) = self.child.get_mut() {
            if !child.try_wait().is_ok_and(|status| status.is_some()) {
                let _ = child.kill();
            }
            let _ = child.wait();
        }
    }
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
        let entry = unsafe { entry.assume_init() };
        if entry.pw_shell.is_null() {
            return None;
        }
        let shell = unsafe { CStr::from_ptr(entry.pw_shell) };
        return Some(PathBuf::from(std::ffi::OsStr::from_bytes(shell.to_bytes())));
    }
}

#[cfg(test)]
mod tests;
