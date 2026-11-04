//! Detach before initializing GUI libraries, terminals, or background threads.
use std::{
    ffi::OsString,
    fs::{self, OpenOptions},
    io,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

const CHILD_FLAG: &str = "--focus-editor-detached";

fn should_detach(args: &[OsString]) -> bool {
    !args
        .iter()
        .any(|arg| arg == "--foreground" || arg == CHILD_FLAG)
}

fn log_path() -> io::Result<PathBuf> {
    dirs::data_local_dir()
        .map(|root| root.join("focus-editor/logs/launch.log"))
        .ok_or_else(|| {
            io::Error::other(
                "Cannot find the local data directory for launch logs. Use --foreground.",
            )
        })
}

#[cfg(target_os = "linux")]
fn detached_command(executable: &Path, args: &[OsString], log: &Path) -> io::Result<Command> {
    use std::os::unix::process::CommandExt;
    let parent = log
        .parent()
        .ok_or_else(|| io::Error::other("Invalid launch log path"))?;
    fs::create_dir_all(parent)?;
    let output = OpenOptions::new().create(true).append(true).open(log)?;
    let mut command = Command::new(executable);
    command
        .args(args)
        .arg(CHILD_FLAG)
        .stdin(Stdio::null())
        .stdout(output.try_clone()?)
        .stderr(output);
    // SAFETY: this runs before exec and only calls the async-signal-safe setsid.
    // Launch happens before the application starts any threads.
    unsafe {
        command.pre_exec(|| {
            if libc::setsid() == -1 {
                return Err(io::Error::last_os_error());
            }
            Ok(())
        });
    }
    Ok(command)
}

/// True in the short-lived launcher; false in the GUI or foreground process.
pub(crate) fn detach() -> io::Result<bool> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if !should_detach(&args) {
        return Ok(false);
    }
    #[cfg(target_os = "linux")]
    {
        let log = log_path()?;
        detached_command(&std::env::current_exe()?, &args, &log)?
            .spawn()
            .map_err(|error| {
                io::Error::new(
                    error.kind(),
                    format!(
                        "Cannot launch Focus Editor: {error} (log: {})",
                        log.display()
                    ),
                )
            })?;
        return Ok(true);
    }
    #[cfg(not(target_os = "linux"))]
    Ok(false)
}

/// Resolve display paths without changing workspace/session identity.
pub(crate) fn workspace_display_path(path: &Path) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| path.to_path_buf())
}

pub(crate) fn window_title(workspace: Option<&Path>) -> String {
    match workspace {
        Some(path) => {
            let path = workspace_display_path(path);
            let name = path.file_name().unwrap_or_else(|| path.as_os_str());
            format!("Focus Editor — {}", name.to_string_lossy())
        }
        None => "Focus Editor".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::OsStr;

    #[cfg(target_os = "linux")]
    #[test]
    fn log_level_arguments_are_forwarded_to_background_child() {
        let root = std::env::temp_dir().join(format!("focus-log-forward-{}", std::process::id()));
        let args = vec![
            OsString::from("--log-level"),
            OsString::from("debug"),
            OsString::from("project"),
        ];
        let command =
            detached_command(Path::new("/bin/true"), &args, &root.join("launch.log")).unwrap();
        let forwarded: Vec<_> = command.get_args().map(|a| a.to_owned()).collect();
        assert_eq!(&forwarded[..3], args.as_slice());
        assert_eq!(forwarded[3], CHILD_FLAG);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn foreground_and_child_flags_prevent_recursion() {
        assert!(should_detach(&[]));
        assert!(should_detach(&["--no-codex".into(), "project".into()]));
        for flag in ["--foreground", CHILD_FLAG] {
            assert!(!should_detach(&["project".into(), flag.into()]));
        }
    }

    #[test]
    fn title_uses_workspace_name_with_root_fallback() {
        assert_eq!(window_title(None), "Focus Editor");
        assert_eq!(
            window_title(Some(Path::new("/home/user/My Project"))),
            "Focus Editor — My Project"
        );
        assert_eq!(window_title(Some(Path::new("/"))), "Focus Editor — /");
    }

    #[test]
    fn relative_workspace_titles_use_resolved_folder_names() {
        for relative in [".", "..", "src", "src/.."] {
            let path = Path::new(relative);
            let resolved = path.canonicalize().unwrap();
            let name = resolved.file_name().unwrap_or(resolved.as_os_str());
            assert_eq!(
                window_title(Some(path)),
                format!("Focus Editor — {}", name.to_string_lossy())
            );
            assert_eq!(workspace_display_path(path), resolved);
        }
        let unavailable = Path::new("/focus-editor-missing-workspace/title-test");
        assert_eq!(workspace_display_path(unavailable), unavailable);
        assert_eq!(window_title(Some(unavailable)), "Focus Editor — title-test");
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn detached_child_preserves_arguments_and_outlives_launcher() {
        use std::{
            os::unix::ffi::OsStrExt,
            time::{Duration, Instant},
        };
        let root = std::env::temp_dir().join(format!(
            "focus-launch-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir(&root).unwrap();
        let log = root.join("logs/launch.log");
        let result = root.join("result");
        let script = "printf '%s\\n' \"$PWD\" \"$1\" \"$2\" \"$3\"; printf 'stderr\\n' >&2; read ignored || printf 'stdin closed\\n'; sleep 0.2; printf done > \"$4\"";
        let args = vec![
            "-c".into(),
            script.into(),
            "test".into(),
            "--no-codex".into(),
            "project with spaces".into(),
            OsStr::from_bytes(b"nonutf8-\xff").to_owned(),
            result.clone().into_os_string(),
        ];
        let mut command = detached_command(Path::new("/bin/sh"), &args, &log).unwrap();
        command.current_dir(&root);
        let start = Instant::now();
        let mut child = command.spawn().unwrap();
        assert!(child.try_wait().unwrap().is_none());
        let pid = child.id();
        assert_eq!(
            unsafe { libc::getsid(pid as libc::pid_t) },
            pid as libc::pid_t
        );
        assert!(!result.exists(), "launch waited for child completion");
        while child.try_wait().unwrap().is_none() && start.elapsed() < Duration::from_secs(3) {
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(child.try_wait().unwrap().unwrap().success());
        assert_eq!(fs::read(&result).unwrap(), b"done");
        let bytes = fs::read(&log).unwrap();
        assert!(bytes.starts_with(root.as_os_str().as_bytes()));
        assert!(
            bytes
                .windows(b"--no-codex\nproject with spaces\nnonutf8-\xff\n".len())
                .any(|w| w == b"--no-codex\nproject with spaces\nnonutf8-\xff\n")
        );
        assert!(String::from_utf8_lossy(&bytes).contains("stderr\nstdin closed\n"));
        fs::remove_dir_all(root).unwrap();
    }
}
