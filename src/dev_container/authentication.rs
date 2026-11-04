//! Authentication-only transfer. Never copy assistant configuration or session data.
use super::Runtime;
use std::{
    io::Write,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

const AUTH_ENV: &[&str] = &[
    "OPENAI_API_KEY",
    "OPENAI_BASE_URL",
    "OPENAI_ORG_ID",
    "OPENAI_PROJECT_ID",
    "ANTHROPIC_API_KEY",
    "ANTHROPIC_AUTH_TOKEN",
    "ANTHROPIC_BASE_URL",
    "GEMINI_API_KEY",
    "GOOGLE_API_KEY",
    "OPENCODE_API_KEY",
    "OPENCODE_SERVER_PASSWORD",
    "OPENCODE_SERVER_USERNAME",
];

// Docker exec does not necessarily set HOME to the selected user's home.
const USER_EXEC: &str = r#"home=$(getent passwd "$(id -u)" | cut -d: -f6)
[ -n "$home" ] || exit 1
HOME=$home; export HOME
exec "$@""#;

const WRITE_AUTH: &str = r#"set -eu
home=$(getent passwd "$(id -u)" | cut -d: -f6)
[ -n "$home" ] || exit 1
case "$1" in
  codex) directory=${CODEX_HOME:-$home/.codex} ;;
  opencode) directory=${XDG_DATA_HOME:-$home/.local/share}/opencode ;;
  *) exit 1 ;;
esac
umask 077
mkdir -p "$directory"
chmod 700 "$directory"
temporary=$(mktemp "$directory/.auth.XXXXXXXX")
trap 'rm -f "$temporary"' EXIT HUP INT TERM
cat > "$temporary"
chmod 600 "$temporary"
mv -f "$temporary" "$directory/auth.json"
"#;

fn auth_paths(
    home: &Path,
    codex_home: Option<PathBuf>,
    data_home: Option<PathBuf>,
) -> [(&'static str, PathBuf); 2] {
    [
        (
            "codex",
            codex_home
                .unwrap_or_else(|| home.join(".codex"))
                .join("auth.json"),
        ),
        (
            "opencode",
            data_home
                .unwrap_or_else(|| home.join(".local/share"))
                .join("opencode/auth.json"),
        ),
    ]
}

impl Runtime {
    /// Called on the Dev Container lifecycle worker before assistants reconnect.
    pub(crate) fn prepare_authentication(&self) -> Result<(), String> {
        let home =
            dirs::home_dir().ok_or("Cannot locate host home for assistant authentication")?;
        for (assistant, path) in auth_paths(
            &home,
            std::env::var_os("CODEX_HOME")
                .filter(|v| !v.is_empty())
                .map(PathBuf::from),
            std::env::var_os("XDG_DATA_HOME")
                .filter(|v| !v.is_empty())
                .map(PathBuf::from),
        ) {
            let bytes = match std::fs::read(&path) {
                Ok(bytes) => bytes,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                    log::warn!(
                        "{assistant}: host auth.json absent; existing container credentials or API environment will be used. Keyring-only credentials cannot be copied; use file-backed login on the host."
                    );
                    continue;
                }
                Err(_) => return Err(format!("Cannot read host {assistant} authentication file")),
            };
            validate_auth(&bytes)
                .map_err(|_| format!("Invalid host {assistant} authentication JSON"))?;
            let mut child = self
                .command("sh")
                .args(["-c", WRITE_AUTH, "authentication", assistant])
                .stdin(Stdio::piped())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .map_err(|_| format!("Cannot start {assistant} authentication transfer"))?;
            let written = child
                .stdin
                .take()
                .ok_or("Authentication input unavailable")?
                .write_all(&bytes);
            let status = child
                .wait()
                .map_err(|_| format!("Cannot finish {assistant} authentication transfer"))?;
            if written.is_err() || !status.success() {
                return Err(format!(
                    "Could not write {assistant} authentication for the configured container user"
                ));
            }
            log::debug!("{assistant} authentication copied to Dev Container (no session data)");
        }
        Ok(())
    }

    /// Forward values by environment name, keeping secrets out of argv and logs.
    pub(crate) fn assistant_command(&self, program: &str) -> Command {
        let mut command = Command::new("docker");
        command
            .args(["exec", "-i", "-w"])
            .arg(&self.workspace_folder);
        if let Some(user) = &self.remote_user {
            command.args(["-u", user]);
        }
        for name in AUTH_ENV {
            if std::env::var_os(name).is_some() {
                command.args(["-e", name]);
            }
        }
        command
            .arg(&self.container_id)
            .args(["sh", "-c", USER_EXEC, "assistant", program]);
        command
    }
}

fn validate_auth(bytes: &[u8]) -> Result<(), ()> {
    let value: serde_json::Value = serde_json::from_slice(bytes).map_err(|_| ())?;
    if value.is_object() { Ok(()) } else { Err(()) }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(target_os = "linux")]
    use std::os::unix::fs::PermissionsExt;
    #[test]
    #[cfg(target_os = "linux")] // Exercises a Linux container shell (getent).
    fn transfer_is_private_atomic_and_keeps_packages_and_history() {
        let root = std::env::temp_dir().join(format!("focus-auth-{}", std::process::id()));
        std::fs::create_dir_all(root.join("packages")).unwrap();
        std::fs::write(root.join("packages/installed"), "keep").unwrap();
        std::fs::write(root.join("history.jsonl"), "keep").unwrap();
        std::fs::write(root.join("auth.json"), "old").unwrap();
        let mut child = Command::new("sh")
            .args(["-c", WRITE_AUTH, "test", "codex"])
            .env("CODEX_HOME", &root)
            .stdin(Stdio::piped())
            .spawn()
            .unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(br#"{"token":"fake"}"#)
            .unwrap();
        assert!(child.wait().unwrap().success());
        assert_eq!(
            std::fs::read(root.join("auth.json")).unwrap(),
            br#"{"token":"fake"}"#
        );
        assert_eq!(
            std::fs::metadata(&root).unwrap().permissions().mode() & 0o777,
            0o700
        );
        assert_eq!(
            std::fs::metadata(root.join("auth.json"))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
        assert_eq!(
            std::fs::read_to_string(root.join("history.jsonl")).unwrap(),
            "keep"
        );
        assert!(root.join("packages/installed").exists());
        assert!(!std::fs::read_dir(&root).unwrap().any(|entry| {
            entry
                .unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with(".auth.")
        }));
        let failed = Command::new("sh")
            .args(["-c", WRITE_AUTH, "test", "codex"])
            .env("CODEX_HOME", root.join("auth.json/not-a-directory"))
            .stdin(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .unwrap();
        assert!(!failed.success());
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn discovers_only_authentication_with_overrides() {
        let paths = auth_paths(
            Path::new("/host"),
            Some("/custom/codex".into()),
            Some("/custom/data".into()),
        );
        assert_eq!(paths[0].1, PathBuf::from("/custom/codex/auth.json"));
        assert_eq!(paths[1].1, PathBuf::from("/custom/data/opencode/auth.json"));
        let defaults = auth_paths(Path::new("/host"), None, None);
        assert_eq!(defaults[0].1, PathBuf::from("/host/.codex/auth.json"));
    }
    #[test]
    fn rejects_invalid_credentials_without_echoing_them() {
        assert!(validate_auth(br#"{"provider":{"token":"fake"}}"#).is_ok());
        assert!(validate_auth(b"secret malformed content").is_err());
        assert!(validate_auth(b"[]").is_err());
    }
}
