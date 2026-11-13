//! Browser launch requests are serialized; children outlive their gateway.
use crate::{
    browser::registry,
    workspaces::{self, Workspace},
};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    io,
    os::unix::{
        fs::{MetadataExt, PermissionsExt},
        process::CommandExt,
    },
    path::PathBuf,
    process::{Command, Stdio},
    sync::{Arc, Mutex},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
fn alive(pid: u32) -> bool {
    pid > 0 && pid <= i32::MAX as u32 && unsafe { libc::kill(pid as libc::pid_t, 0) } == 0
}
#[derive(Clone, Serialize, Deserialize)]
struct Launch {
    pid: u32,
    started: u64,
    #[serde(skip)]
    error: Option<String>,
}
#[derive(Clone)]
pub(super) struct State {
    launches: Arc<Mutex<HashMap<String, Launch>>>,
    directory: PathBuf,
}
impl State {
    pub fn new() -> io::Result<Self> {
        let directory = registry::runtime_directory()?.join("launches");
        std::fs::create_dir_all(&directory)?;
        let metadata = directory.symlink_metadata()?;
        if !metadata.is_dir() || metadata.uid() != unsafe { libc::geteuid() } {
            return Err(io::Error::other("Invalid launch directory"));
        }
        std::fs::set_permissions(&directory, std::fs::Permissions::from_mode(0o700))?;
        let mut launches = HashMap::new();
        for entry in std::fs::read_dir(&directory)?.flatten() {
            let path = entry.path();
            let id = path
                .file_stem()
                .and_then(|id| id.to_str())
                .unwrap_or("")
                .to_owned();
            let metadata = path.symlink_metadata()?;
            if id.len() != 64
                || !id.bytes().all(|byte| byte.is_ascii_hexdigit())
                || !metadata.is_file()
                || metadata.uid() != unsafe { libc::geteuid() }
                || metadata.mode() & 0o077 != 0
                || metadata.len() > 4096
            {
                continue;
            }
            if let Some(launch) = std::fs::read(&path)
                .ok()
                .and_then(|bytes| serde_json::from_slice::<Launch>(&bytes).ok())
                .filter(|launch| alive(launch.pid))
            {
                launches.insert(id, launch);
            } else {
                let _ = std::fs::remove_file(path);
            }
        }
        Ok(Self {
            launches: Arc::new(Mutex::new(launches)),
            directory,
        })
    }
    fn claim_path(&self, id: &str) -> PathBuf {
        self.directory.join(format!("{id}.json"))
    }
    pub fn catalog(&self, records: &[registry::Record]) -> io::Result<Vec<serde_json::Value>> {
        let workspaces = workspaces::list()?;
        let mut launches = self
            .launches
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        Ok(workspaces.into_iter().map(|workspace| {
            let editors: Vec<_> = records.iter().filter(|record| record.workspace.as_ref() == Some(&workspace.path)).map(|record| record.id.clone()).collect();
            if let Some(launch) = launches.get_mut(&workspace.id) {
                if !alive(launch.pid) && launch.error.is_none() { launch.error = Some("Editor exited before registering. Try Start again.".into()); }
                if alive(launch.pid) && now().saturating_sub(launch.started) >= 30 { launch.error = Some("Editor startup timed out; its process is still running. No additional copy will be started.".into()); }
            }
            if !editors.is_empty() { launches.remove(&workspace.id); let _ = std::fs::remove_file(self.claim_path(&workspace.id)); }
            let pending = launches.get(&workspace.id);
            let launching = pending.is_some_and(|launch| alive(launch.pid));
            let error = pending.and_then(|launch| launch.error.clone());
            let state = if !editors.is_empty() { "running" } else if error.is_some() { "failed" } else if launching { "starting" } else if !workspace.path.is_dir() { "unavailable" } else { "stopped" };
            serde_json::json!({"id": workspace.id, "name": workspace.path.file_name().unwrap_or(workspace.path.as_os_str()).to_string_lossy(), "path": workspace.path, "state": state, "editors": editors, "error": error, "launching": launching})
        }).collect())
    }
    pub fn start(
        &self,
        workspace: Workspace,
        records: &[registry::Record],
    ) -> Result<(u16, serde_json::Value), String> {
        let mut launches = self
            .launches
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        if let Some(record) = records
            .iter()
            .find(|record| record.workspace.as_ref() == Some(&workspace.path))
        {
            return Ok((200, serde_json::json!({"editor_id": record.id})));
        }
        if launches
            .get(&workspace.id)
            .is_some_and(|launch| alive(launch.pid))
        {
            return Ok((202, serde_json::json!({"workspace_id": workspace.id})));
        }
        let checked = workspaces::remember(&workspace.path)
            .map_err(|error| format!("Cannot open workspace: {error}"))?;
        if checked.id != workspace.id {
            return Err("Workspace path changed; add its current location again".into());
        }
        let binary = editor_binary().map_err(|error| error.to_string())?;
        let log_path = crate::app_paths::data_root()
            .map_err(|error| error.to_string())?
            .join("logs")
            .join(format!("headless-{}.log", workspace.id));
        let output =
            crate::logging::session::fresh_file(&log_path).map_err(|error| error.to_string())?;
        let mut command = Command::new(binary);
        command
            .args(["--headless", "--foreground"])
            .arg(&workspace.path)
            .current_dir(&workspace.path)
            .env_remove("FOCUS_EDITOR_SERVER_USERNAME")
            .env_remove("FOCUS_EDITOR_SERVER_PASSWORD")
            .stdin(Stdio::null())
            .stdout(output.try_clone().map_err(|error| error.to_string())?)
            .stderr(output);
        // SAFETY: the pre-exec hook invokes only the async-signal-safe setsid syscall.
        unsafe {
            command.pre_exec(|| {
                if libc::setsid() == -1 {
                    Err(io::Error::last_os_error())
                } else {
                    Ok(())
                }
            });
        }
        let mut child = command
            .spawn()
            .map_err(|error| format!("Could not launch editor: {error}"))?;
        let launch = Launch {
            pid: child.id(),
            started: now(),
            error: None,
        };
        let claim_error = registry::write_private(&self.claim_path(&workspace.id), &launch).err();
        launches.insert(workspace.id.clone(), launch.clone());
        let state = self.clone();
        let id = workspace.id.clone();
        let expected_workspace = workspace.path.clone();
        std::thread::spawn(move || {
            loop {
                let registered = registry::records().is_ok_and(|records| {
                    records.iter().any(|record| {
                        record.pid == launch.pid
                            && record.mode == "headless"
                            && record.workspace.as_ref() == Some(&expected_workspace)
                    })
                });
                if registered {
                    state
                        .launches
                        .lock()
                        .unwrap_or_else(|error| error.into_inner())
                        .remove(&id);
                    let _ = std::fs::remove_file(state.claim_path(&id));
                    // Reap the child on normal exit. Dropping the gateway never kills it.
                    let _ = child.wait();
                    break;
                }
                match child.try_wait() {
                    Ok(Some(status)) => {
                        let mut launches = state
                            .launches
                            .lock()
                            .unwrap_or_else(|error| error.into_inner());
                        if let Some(pending) = launches
                            .get_mut(&id)
                            .filter(|pending| pending.pid == launch.pid)
                        {
                            pending.error = Some(format!(
                                "Editor exited during startup ({status}). See {}",
                                log_path.display()
                            ));
                        }
                        let _ = std::fs::remove_file(state.claim_path(&id));
                        break;
                    }
                    Ok(None) => {}
                    Err(error) => {
                        log::warn!("Could not inspect starting editor: {error}");
                        break;
                    }
                }
                std::thread::sleep(Duration::from_millis(100));
            }
        });
        if let Some(error) = claim_error {
            log::warn!("Could not persist pending editor launch: {error}");
        }
        Ok((202, serde_json::json!({"workspace_id": workspace.id})))
    }
}

fn editor_binary() -> io::Result<PathBuf> {
    let executable = |path: &PathBuf| {
        path.metadata()
            .is_ok_and(|metadata| metadata.is_file() && metadata.mode() & 0o111 != 0)
    };
    let sibling = std::env::current_exe()?
        .parent()
        .ok_or_else(|| io::Error::other("Cannot find server installation"))?
        .join("focus-editor");
    if executable(&sibling) {
        return Ok(sibling);
    }
    if let Some(binary) = std::env::var_os("PATH").and_then(|path| {
        std::env::split_paths(&path)
            .map(|directory| directory.join("focus-editor"))
            .find(executable)
    }) {
        return Ok(binary);
    }
    Err(io::Error::other(
        "Install focus-editor beside focus-editor-server or put it on PATH",
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn pending_launch_survives_restart_and_prevents_duplicates() {
        if std::env::var_os("FOCUS_PENDING_TEST").is_none() {
            let root = tempfile::tempdir().unwrap();
            let output = Command::new(std::env::current_exe().unwrap()).args(["--exact", "server::workspaces::tests::pending_launch_survives_restart_and_prevents_duplicates", "--nocapture"])
                .env("FOCUS_PENDING_TEST", "1").env("XDG_DATA_HOME", root.path().join("data")).output().unwrap();
            assert!(
                output.status.success(),
                "{}\n{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
            return;
        }
        let root = tempfile::tempdir().unwrap();
        let workspace = crate::workspaces::remember(root.path()).unwrap();
        let state = State::new().unwrap();
        let path = state.claim_path(&workspace.id);
        registry::write_private(
            &path,
            &Launch {
                pid: std::process::id(),
                started: now().saturating_sub(31),
                error: None,
            },
        )
        .unwrap();
        drop(state);
        let recovered = State::new().unwrap();
        let catalog = recovered.catalog(&[]).unwrap();
        assert_eq!(catalog[0]["state"], "failed");
        assert_eq!(catalog[0]["launching"], true);
        assert_eq!(recovered.start(workspace.clone(), &[]).unwrap().0, 202);
        assert!(registry::records().unwrap().is_empty());
        registry::write_private(
            &path,
            &Launch {
                pid: i32::MAX as u32,
                started: now(),
                error: None,
            },
        )
        .unwrap();
        drop(recovered);
        let recovered = State::new().unwrap();
        assert_eq!(recovered.catalog(&[]).unwrap()[0]["state"], "stopped");
        assert!(!path.exists());
    }
}
