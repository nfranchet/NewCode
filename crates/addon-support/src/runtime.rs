use serde::{Deserialize, Serialize};
use std::{
    path::{Path, PathBuf},
    process::Command,
};
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Runtime {
    pub host_workspace: PathBuf,
    pub container_id: String,
    pub workspace_folder: PathBuf,
    pub remote_user: Option<String>,
}

impl Runtime {
    /// Forward sensitive values by environment name, never in docker's argv.
    pub fn command_with_env(&self, program: &str, values: &[(String, String)]) -> Command {
        let mut command = Command::new("docker");
        command
            .args(["exec", "-i", "-w"])
            .arg(&self.workspace_folder);
        if let Some(user) = &self.remote_user {
            command.args(["-u", user]);
        }
        for (key, value) in values {
            command.arg("-e").arg(key).env(key, value);
        }
        command.arg(&self.container_id).arg(program);
        command
    }
    pub fn command(&self, program: &str) -> Command {
        let mut command = Command::new("docker");
        command
            .args(["exec", "-i", "-w"])
            .arg(&self.workspace_folder);
        if let Some(user) = &self.remote_user {
            command.args(["-u", user]);
        }
        command.arg(&self.container_id).arg(program);
        command
    }

    pub fn container_path(&self, host_path: &Path) -> Result<PathBuf, String> {
        // Settings retain the user's spelling (often `.`), while the runtime
        // stores a canonical workspace. Compare both in the same host namespace.
        // Resolve before checking containment so symlinks and `..` cannot escape.
        let workspace = self
            .host_workspace
            .canonicalize()
            .map_err(|error| format!("Cannot resolve Dev Container workspace: {error}"))?;
        let resolved = host_path.canonicalize().map_err(|error| {
            format!("Cannot resolve host path {}: {error}", host_path.display())
        })?;
        let relative = resolved.strip_prefix(&workspace).map_err(|_| {
            format!(
                "{} is outside the Dev Container workspace",
                host_path.display()
            )
        })?;
        Ok(self.workspace_folder.join(relative))
    }

    pub fn host_path(&self, container_path: &Path) -> Result<PathBuf, String> {
        let relative = container_path
            .strip_prefix(&self.workspace_folder)
            .map_err(|_| {
                format!(
                    "{} is outside the mounted workspace",
                    container_path.display()
                )
            })?;
        Ok(self.host_workspace.join(relative))
    }

    pub fn container_ip(&self) -> Result<String, String> {
        let output = Command::new("docker")
            .args([
                "inspect",
                "--format",
                "{{range .NetworkSettings.Networks}}{{.IPAddress}}{{end}}",
                &self.container_id,
            ])
            .output()
            .map_err(|error| format!("Could not inspect Dev Container: {error}"))?;
        if !output.status.success() {
            return Err(String::from_utf8_lossy(&output.stderr).trim().to_owned());
        }
        let address = String::from_utf8_lossy(&output.stdout).trim().to_owned();
        if address.is_empty() {
            Err("Dev Container has no reachable Docker network address".into())
        } else {
            Ok(address)
        }
    }
}

impl From<focus_addon_sdk::Runtime> for Runtime {
    fn from(r: focus_addon_sdk::Runtime) -> Self {
        Self {
            host_workspace: r.host_workspace,
            container_id: r.container_id,
            workspace_folder: r.workspace_folder,
            remote_user: r.remote_user,
        }
    }
}
impl From<&Runtime> for focus_addon_sdk::Runtime {
    fn from(r: &Runtime) -> Self {
        Self {
            host_workspace: r.host_workspace.clone(),
            container_id: r.container_id.clone(),
            workspace_folder: r.workspace_folder.clone(),
            remote_user: r.remote_user.clone(),
        }
    }
}
