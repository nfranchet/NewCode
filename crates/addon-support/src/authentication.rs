//! Optional utilities for addons that launch authenticated tools inside a container.
use crate::runtime::Runtime;
use std::{
    io::Write,
    path::Path,
    process::{Command, Stdio},
};

pub fn command(runtime: &Runtime, program: &str, environment: &[&str]) -> Command {
    let mut command = Command::new("docker");
    command
        .args(["exec", "-i", "-w"])
        .arg(&runtime.workspace_folder);
    if let Some(user) = &runtime.remote_user {
        command.args(["-u", user]);
    }
    for key in environment {
        if std::env::var_os(key).is_some() {
            command.args(["-e", key]);
        }
    }
    command.arg(&runtime.container_id).args([
        "sh",
        "-c",
        r#"home=$(getent passwd "$(id -u)" | cut -d: -f6)
[ -n "$home" ] || exit 1
HOME=$home; export HOME
exec "$@""#,
        "addon-tool",
        program,
    ]);
    command
}

pub fn copy_json(runtime: &Runtime, source: &Path, destination: &str) -> Result<(), String> {
    let bytes = match std::fs::read(source) {
        Ok(bytes) => bytes,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            log::warn!("Host authentication file absent; existing tool credentials will be used");
            return Ok(());
        }
        Err(_) => return Err("Cannot read addon authentication file".into()),
    };
    if !serde_json::from_slice::<serde_json::Value>(&bytes).is_ok_and(|v| v.is_object()) {
        return Err("Invalid addon authentication JSON".into());
    }
    // `destination` is addon-owned shell code, never user input or credential bytes.
    let script = write_script(destination);
    let mut child = runtime
        .command("sh")
        .args(["-c", &script])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|_| "Cannot start addon authentication transfer")?;
    let written = child
        .stdin
        .take()
        .ok_or("Authentication input unavailable")?
        .write_all(&bytes);
    let status = child
        .wait()
        .map_err(|_| "Cannot finish addon authentication transfer")?;
    if written.is_err() || !status.success() {
        return Err("Addon authentication transfer failed".into());
    }
    Ok(())
}

fn write_script(destination: &str) -> String {
    format!(
        r#"set -eu
home=$(getent passwd "$(id -u)" | cut -d: -f6)
[ -n "$home" ] || exit 1
directory={destination}
umask 077
mkdir -p "$directory"
chmod 700 "$directory"
temporary=$(mktemp "$directory/.auth.XXXXXXXX")
trap 'rm -f "$temporary"' EXIT HUP INT TERM
cat > "$temporary"
chmod 600 "$temporary"
mv -f "$temporary" "$directory/auth.json"
"#
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    #[cfg(target_os = "linux")]
    fn authentication_transfer_is_private_and_preserves_other_files() {
        use std::os::unix::fs::PermissionsExt;
        let root = std::env::temp_dir().join(format!("focus-addon-auth-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("keep.txt"), "existing addon data").unwrap();
        let mut child = Command::new("sh")
            .args(["-c", &write_script("$FOCUS_TEST_AUTH_DIR")])
            .env("FOCUS_TEST_AUTH_DIR", &root)
            .stdin(Stdio::piped())
            .spawn()
            .unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(br#"{"token":"fixture"}"#)
            .unwrap();
        assert!(child.wait().unwrap().success());
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
            std::fs::read_to_string(root.join("keep.txt")).unwrap(),
            "existing addon data"
        );
        std::fs::remove_dir_all(root).unwrap();
    }
}
