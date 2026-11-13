//! Private per-user discovery; instance capabilities never appear in the public catalog.
use serde::{Deserialize, Serialize};
use std::{
    io::{self, Write},
    os::unix::fs::{MetadataExt, PermissionsExt},
    path::PathBuf,
};

pub(crate) fn random_id() -> io::Result<String> {
    let mut bytes = [0; 24];
    getrandom::fill(&mut bytes).map_err(io::Error::other)?;
    Ok(bytes.iter().map(|byte| format!("{byte:02x}")).collect())
}

#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct Record {
    pub version: u32,
    pub id: String,
    pub pid: u32,
    pub endpoint: String,
    pub token: String,
    pub title: String,
    pub workspace: Option<PathBuf>,
    pub mode: String,
}

fn directory() -> io::Result<PathBuf> {
    let runtime = crate::app_paths::data_root()?.join("runtime");
    let directory = runtime.join("instances");
    std::fs::create_dir_all(runtime.parent().unwrap())?;
    for path in [&runtime, &directory] {
        match std::fs::create_dir(path) {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(error),
        }
        let metadata = path.symlink_metadata()?;
        if !metadata.is_dir() || metadata.uid() != unsafe { libc::geteuid() } {
            return Err(io::Error::other(
                "Instance registry must be a directory owned by the current user",
            ));
        }
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700))?;
    }
    Ok(directory)
}

pub(crate) struct Registration {
    path: PathBuf,
    pub record: Record,
}
impl Registration {
    pub fn new(
        endpoint: String,
        token: String,
        mode: &str,
        workspace: Option<PathBuf>,
    ) -> io::Result<Self> {
        let id = random_id()?;
        let record = Record {
            version: 1,
            id: id.clone(),
            pid: std::process::id(),
            endpoint,
            token,
            title: crate::launch::window_title(workspace.as_deref()),
            workspace,
            mode: mode.into(),
        };
        let registration = Self {
            path: directory()?.join(format!("{id}.json")),
            record,
        };
        registration.write()?;
        Ok(registration)
    }
    pub fn update(&mut self, title: String, workspace: Option<PathBuf>) {
        if self.record.title != title || self.record.workspace != workspace {
            self.record.title = title;
            self.record.workspace = workspace;
            if let Err(error) = self.write() {
                log::warn!("Could not update editor registration: {error}");
            }
        }
    }
    fn write(&self) -> io::Result<()> {
        let mut file = tempfile::NamedTempFile::new_in(self.path.parent().unwrap())?;
        file.as_file()
            .set_permissions(std::fs::Permissions::from_mode(0o600))?;
        serde_json::to_writer(file.as_file_mut(), &self.record)?;
        file.flush()?;
        file.persist(&self.path).map_err(|error| error.error)?;
        Ok(())
    }
}
impl Drop for Registration {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

pub(crate) fn records() -> io::Result<Vec<Record>> {
    let mut records = Vec::new();
    for entry in std::fs::read_dir(directory()?)?.flatten() {
        let path = entry.path();
        let Ok(metadata) = path.symlink_metadata() else {
            continue;
        };
        if !metadata.is_file()
            || metadata.uid() != unsafe { libc::geteuid() }
            || metadata.permissions().mode() & 0o077 != 0
            || metadata.len() > 65536
        {
            continue;
        }
        let Ok(bytes) = std::fs::read(&path) else {
            continue;
        };
        let Ok(record) = serde_json::from_slice::<Record>(&bytes) else {
            continue;
        };
        if record.pid == 0
            || record.pid > i32::MAX as u32
            || record.token.is_empty()
            || record.token.len() > 128
            || record.version != 1
            || record.id.len() != 48
            || !record.id.bytes().all(|byte| byte.is_ascii_hexdigit())
            || path.file_stem().and_then(|value| value.to_str()) != Some(&record.id)
        {
            continue;
        }
        let Ok(url) = reqwest::Url::parse(&record.endpoint) else {
            continue;
        };
        if url.scheme() != "http"
            || url.host_str() != Some("127.0.0.1")
            || url.port().is_none()
            || !url.username().is_empty()
            || url.password().is_some()
            || url.path() != "/"
            || url.query().is_some()
            || url.fragment().is_some()
        {
            continue;
        }
        if unsafe { libc::kill(record.pid as libc::pid_t, 0) } != 0 {
            if io::Error::last_os_error().raw_os_error() == Some(libc::ESRCH) {
                let _ = std::fs::remove_file(path);
            }
            continue;
        }
        records.push(record);
    }
    records.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(records)
}
