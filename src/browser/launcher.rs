//! Own a Chromium app window and its disposable profile, separately from the user's browser.
use std::{
    ffi::{OsStr, OsString},
    io::{self, BufRead, BufReader, Read},
    os::unix::{fs::PermissionsExt, process::CommandExt},
    path::{Path, PathBuf},
    process::{Child, Command, ExitStatus, Stdio},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

const INSTALL_HELP: &str = "Install Chromium or Google Chrome, or use --ui for the native window.";

fn executable(path: &Path) -> bool {
    path.metadata()
        .is_ok_and(|metadata| metadata.is_file() && metadata.permissions().mode() & 0o111 != 0)
}

fn discover(path: Option<&OsStr>, applications: &[PathBuf]) -> io::Result<PathBuf> {
    let directories: Vec<_> = path
        .map(std::env::split_paths)
        .into_iter()
        .flatten()
        .collect();
    for name in [
        "chromium",
        "chromium-browser",
        "google-chrome",
        "google-chrome-stable",
    ] {
        if let Some(candidate) = directories
            .iter()
            .map(|directory| directory.join(name))
            .find(|path| executable(path))
        {
            return Ok(candidate);
        }
    }
    applications
        .iter()
        .find(|path| executable(path))
        .cloned()
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, INSTALL_HELP))
}

pub(super) fn browser_executable() -> io::Result<PathBuf> {
    let mut applications = Vec::new();
    if cfg!(target_os = "macos") {
        let mut roots = vec![PathBuf::from("/Applications")];
        if let Some(home) = dirs::home_dir() {
            roots.push(home.join("Applications"));
        }
        for root in roots {
            applications.extend([
                root.join("Google Chrome.app/Contents/MacOS/Google Chrome"),
                root.join("Chromium.app/Contents/MacOS/Chromium"),
            ]);
        }
    }
    discover(std::env::var_os("PATH").as_deref(), &applications)
}

pub(super) struct AppWindow {
    child: Child,
    profile: Option<tempfile::TempDir>,
    diagnostics: Arc<Mutex<String>>,
}

impl AppWindow {
    pub fn start(url: &str) -> io::Result<Self> {
        Self::spawn(&browser_executable()?, url)
    }

    pub(super) fn spawn(executable: &Path, url: &str) -> io::Result<Self> {
        let profile = tempfile::Builder::new()
            .prefix("focus-editor-browser-")
            .tempdir()?;
        std::fs::set_permissions(profile.path(), std::fs::Permissions::from_mode(0o700))?;
        let mut command = Command::new(executable);
        let mut profile_argument = OsString::from("--user-data-dir=");
        profile_argument.push(profile.path());
        command
            .arg(format!("--app={url}"))
            .arg(profile_argument)
            .args([
                "--window-size=1440,900",
                "--no-first-run",
                "--no-default-browser-check",
                "--disable-extensions",
                "--disable-sync",
                "--disable-background-mode",
                "--disable-background-networking",
                "--class=focus-editor",
            ])
            // Wrapper scripts must not inherit flags that disable the renderer sandbox
            // or redirect this window into a personal browser profile.
            .env_remove("CHROMIUM_FLAGS")
            .env_remove("CHROME_FLAGS")
            .env_remove("CHROMIUM_USER_FLAGS")
            .env_remove("CHROME_USER_DATA_DIR")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .process_group(0);
        let mut child = command.spawn().map_err(|error| {
            io::Error::new(
                error.kind(),
                format!("Cannot start the editor browser: {error}. {INSTALL_HELP}"),
            )
        })?;
        let stderr = child.stderr.take().unwrap();
        let diagnostics = Arc::new(Mutex::new(String::new()));
        let target = diagnostics.clone();
        let url = url.to_owned();
        let secret = url.split_once('#').map(|(_, token)| token.to_owned());
        std::thread::spawn(move || {
            let mut reader = BufReader::new(stderr);
            loop {
                let mut line = Vec::new();
                if std::io::Read::by_ref(&mut reader)
                    .take(4096)
                    .read_until(b'\n', &mut line)
                    .unwrap_or(0)
                    == 0
                {
                    break;
                }
                let mut line = String::from_utf8_lossy(&line).replace(&url, "[editor URL]");
                if let Some(secret) = &secret {
                    line = line.replace(secret, "[session]");
                }
                let mut tail = target.lock().unwrap_or_else(|error| error.into_inner());
                tail.push_str(&line);
                if let Some(secret) = &secret {
                    // A long diagnostic can split a token across bounded reads.
                    *tail = tail.replace(secret, "[session]");
                }
                if tail.len() > 8192 {
                    let mut start = tail.len() - 8192;
                    while !tail.is_char_boundary(start) {
                        start += 1;
                    }
                    tail.drain(..start);
                }
            }
        });
        Ok(Self {
            child,
            profile: Some(profile),
            diagnostics,
        })
    }

    pub fn poll(&mut self) -> io::Result<Option<ExitStatus>> {
        self.child.try_wait()
    }

    pub fn exit_error(&self, status: ExitStatus) -> io::Error {
        let tail = self
            .diagnostics
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        io::Error::other(format!(
            "Editor browser exited ({status}). The browser sandbox must be available. {INSTALL_HELP}\n{}",
            tail.trim()
        ))
    }
}

impl Drop for AppWindow {
    fn drop(&mut self) {
        if self.child.try_wait().ok().flatten().is_none() {
            // This group belongs only to the private app window and its subprocesses.
            unsafe {
                libc::kill(-(self.child.id() as libc::pid_t), libc::SIGTERM);
            }
            let start = Instant::now();
            while start.elapsed() < Duration::from_secs(3) {
                if self.child.try_wait().ok().flatten().is_some() {
                    break;
                }
                std::thread::sleep(Duration::from_millis(20));
            }
            if self.child.try_wait().ok().flatten().is_none() {
                unsafe {
                    libc::kill(-(self.child.id() as libc::pid_t), libc::SIGKILL);
                }
            }
            let _ = self.child.wait();
        }
        if let Some(profile) = self.profile.take() {
            if let Err(error) = profile.close() {
                log::warn!("Could not remove the temporary browser profile: {error}");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn discovery_requires_an_executable_browser_and_supports_app_bundles() {
        let root = tempfile::tempdir().unwrap();
        let chromium = root.path().join("chromium");
        std::fs::write(&chromium, "fixture").unwrap();
        std::fs::set_permissions(&chromium, std::fs::Permissions::from_mode(0o600)).unwrap();
        assert!(discover(Some(root.path().as_os_str()), &[]).is_err());
        std::fs::set_permissions(&chromium, std::fs::Permissions::from_mode(0o700)).unwrap();
        assert_eq!(
            discover(Some(root.path().as_os_str()), &[]).unwrap(),
            chromium
        );
        assert_eq!(discover(None, &[chromium.clone()]).unwrap(), chromium);
    }

    #[test]
    fn windows_have_isolated_profiles_and_cleanup_their_owned_processes() {
        let root = tempfile::tempdir().unwrap();
        let program = root.path().join("browser with spaces");
        std::fs::write(
            &program,
            "#!/bin/sh\nprintf '%s\\n' \"$@\" > \"$0.args\"\nexec sleep 30\n",
        )
        .unwrap();
        std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o700)).unwrap();
        let one = AppWindow::spawn(&program, "http://127.0.0.1:1234/#test-secret").unwrap();
        let two = AppWindow::spawn(&program, "http://127.0.0.1:1235/#test-secret").unwrap();
        let first = one.profile.as_ref().unwrap().path().to_owned();
        let second = two.profile.as_ref().unwrap().path().to_owned();
        assert_ne!(first, second);
        assert_eq!(
            first.metadata().unwrap().permissions().mode() & 0o777,
            0o700
        );
        let deadline = Instant::now() + Duration::from_secs(2);
        let arguments = loop {
            if let Ok(args) =
                std::fs::read_to_string(program.with_file_name("browser with spaces.args"))
            {
                if args.contains("--disable-background-mode") {
                    break args;
                }
            }
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(10));
        };
        assert!(arguments.contains("--app=http://127.0.0.1:"));
        assert!(arguments.contains("--user-data-dir="));
        assert!(!arguments.contains("--no-sandbox"));
        assert!(!arguments.contains("--disable-web-security"));
        let pid = one.child.id();
        drop(one);
        assert!(!first.exists());
        assert_eq!(unsafe { libc::kill(pid as libc::pid_t, 0) }, -1);
        assert!(second.exists());
        drop(two);
        assert!(!second.exists());
    }

    #[test]
    fn startup_failure_diagnostics_exclude_session_credentials() {
        let root = tempfile::tempdir().unwrap();
        let program = root.path().join("failed-browser");
        std::fs::write(
            &program,
            "#!/bin/sh\nprintf 'sandbox failed: %s\\n' \"$1\" >&2\nexit 1\n",
        )
        .unwrap();
        std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o700)).unwrap();
        let mut window =
            AppWindow::spawn(&program, "http://127.0.0.1:1234/#private-credential").unwrap();
        let status = loop {
            if let Some(status) = window.poll().unwrap() {
                break status;
            }
            std::thread::sleep(Duration::from_millis(10));
        };
        let deadline = Instant::now() + Duration::from_secs(2);
        while window.diagnostics.lock().unwrap().is_empty() {
            assert!(
                Instant::now() < deadline,
                "Missing browser startup diagnostic"
            );
            std::thread::sleep(Duration::from_millis(10));
        }
        let error = window.exit_error(status).to_string();
        assert!(error.contains("--ui"));
        assert!(error.contains("sandbox failed"));
        assert!(!error.contains("private-credential"));
    }
}
