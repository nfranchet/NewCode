//! Workspace-owned assistant data, mounted independently of container lifetime.
use super::{Configuration, Runtime, compose};
use std::{fs, path::Path, process::Command};

const IGNORE: &str = "/.devcontainer/mounted_files/";

fn ignore(workspace: &Path) -> Result<(), String> {
    let path = workspace.join(".gitignore");
    let mut contents = match fs::read_to_string(&path) {
        Ok(contents) => contents,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(error) => return Err(format!("Cannot read .gitignore: {error}")),
    };
    if !contents.lines().any(|line| line.trim() == IGNORE) {
        if !contents.is_empty() && !contents.ends_with('\n') { contents.push('\n'); }
        contents.push_str(IGNORE);
        contents.push('\n');
        fs::write(path, contents).map_err(|error| format!("Cannot ignore assistant data: {error}"))?;
    }
    Ok(())
}

pub(super) fn prepare(configuration: &Configuration) -> Result<(), String> {
    ignore(&configuration.workspace)?;
    let root = configuration.workspace.join(".devcontainer/mounted_files");
    fs::create_dir_all(&root).map_err(|error| format!("Cannot create assistant storage: {error}"))?;
    let ids = compose::compose(configuration, ["ps", "--all", "-q", &configuration.service])?;
    let id = String::from_utf8_lossy(&ids).trim().to_owned();
    if id.is_empty() { return Ok(()); }
    if root.join("codex").exists() && root.join("share/opencode").exists() { return Ok(()); }
    // Starting an old stopped container lets us resolve its actual user's home and
    // environment before Compose replaces it. No shell guesses about usernames.
    checked(Command::new("docker").args(["start", &id]), "start previous container for migration")?;
    let runtime = Runtime { host_workspace: configuration.workspace.clone(), container_id: id,
        workspace_folder: configuration.workspace_folder.clone(), remote_user: configuration.remote_user.clone() };
    for (relative, expression) in [
        ("codex", "${CODEX_HOME:-$home/.codex}"),
        ("share/opencode", "${XDG_DATA_HOME:-$home/.local/share}/opencode"),
    ] {
        let destination = root.join(relative);
        if destination.exists() { continue; }
        let script = format!(r#"set -eu
home=$(getent passwd "$(id -u)" | cut -d: -f6)
[ -n "$home" ]
directory={expression}
if [ -d "$directory" ]; then printf '%s' "$directory"; fi"#);
        let output = checked(runtime.command("sh").args(["-c", &script]), "resolve previous assistant data")?;
        let source = String::from_utf8(output).map_err(|_| "Invalid assistant data path")?;
        if source.is_empty() { continue; }
        let parent = destination.parent().ok_or("Invalid persistence destination")?;
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        let staging = tempfile::tempdir_in(parent).map_err(|error| error.to_string())?;
        checked(Command::new("docker").arg("cp").arg(format!("{}:{source}/.", runtime.container_id)).arg(staging.path()), "migrate assistant data")?;
        fs::rename(staging.path(), &destination).map_err(|error| format!("Cannot save migrated assistant data: {error}"))?;
    }
    Ok(())
}

fn checked(command: &mut Command, operation: &str) -> Result<Vec<u8>, String> {
    let output = command.output().map_err(|error| format!("Cannot {operation}: {error}"))?;
    if !output.status.success() { return Err(format!("Cannot {operation}; see Docker configuration and storage permissions")); }
    Ok(output.stdout)
}

pub(super) fn initialize(runtime: &Runtime) -> Result<(), String> {
    let identity = checked(runtime.command("sh").args(["-c", "printf '%s:%s' \"$(id -u)\" \"$(id -g)\""]), "resolve assistant storage owner")?;
    let identity = String::from_utf8(identity).map_err(|_| "Invalid container identity")?;
    if !identity.split(':').all(|part| !part.is_empty() && part.bytes().all(|byte| byte.is_ascii_digit())) || identity.matches(':').count() != 1 {
        return Err("Invalid container identity".into());
    }
    checked(Command::new("docker").args(["exec", "-u", "0", &runtime.container_id, "sh", "-c",
        "set -eu; mkdir -p /focus-editor-assistant-data/codex /focus-editor-assistant-data/share/opencode; chown -R \"$1\" /focus-editor-assistant-data; chmod 700 /focus-editor-assistant-data /focus-editor-assistant-data/codex /focus-editor-assistant-data/share /focus-editor-assistant-data/share/opencode", "assistant-storage", &identity]), "initialize assistant storage permissions")?;
    checked(runtime.command("sh").args(["-c", "test -w /focus-editor-assistant-data/codex && test -w /focus-editor-assistant-data/share/opencode"]), "verify writable assistant storage")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ignore_is_idempotent_and_preserves_existing_contents() {
        let root = tempfile::tempdir().unwrap();
        fs::write(root.path().join(".gitignore"), "existing").unwrap();
        ignore(root.path()).unwrap();
        ignore(root.path()).unwrap();
        assert_eq!(fs::read_to_string(root.path().join(".gitignore")).unwrap(), format!("existing\n{IGNORE}\n"));
    }
}
