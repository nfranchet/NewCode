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

pub(crate) fn runtime_directory() -> io::Result<PathBuf> {
    directory().map(|directory| directory.parent().unwrap().to_path_buf())
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
    announcer: Option<Announcer>,
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
        if let Some(path) = &record.workspace {
            let _ = crate::workspaces::remember(path);
        }
        let mut registration = Self {
            announcer: None,
            path: directory()?.join(format!("{id}.json")),
            record,
        };
        registration.write()?;
        registration.announcer = Some(Announcer::start(registration.record.id.clone()));
        Ok(registration)
    }
    pub fn update(&mut self, title: String, workspace: Option<PathBuf>) {
        if self.record.title != title || self.record.workspace != workspace {
            self.record.title = title;
            self.record.workspace = workspace;
            if let Some(path) = &self.record.workspace {
                let _ = crate::workspaces::remember(path);
            }
            if let Err(error) = self.write() {
                log::warn!("Could not update editor registration: {error}");
            }
            if let Some(announcer) = &self.announcer {
                let _ = announcer.wake.try_send(());
            }
        }
    }
    fn write(&self) -> io::Result<()> {
        write_private(&self.path, &self.record)
    }
}
impl Drop for Registration {
    fn drop(&mut self) {
        self.announcer.take();
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

pub(crate) fn write_private(path: &std::path::Path, value: &impl Serialize) -> io::Result<()> {
    let mut file = tempfile::NamedTempFile::new_in(
        path.parent()
            .ok_or_else(|| io::Error::other("Missing parent directory"))?,
    )?;
    file.as_file()
        .set_permissions(std::fs::Permissions::from_mode(0o600))?;
    serde_json::to_writer(file.as_file_mut(), value)?;
    file.flush()?;
    file.persist(path).map_err(|error| error.error)?;
    Ok(())
}

#[derive(Serialize, Deserialize)]
pub(crate) struct ServerDescriptor {
    pub version: u32,
    pub pid: u32,
    pub generation: String,
    pub endpoint: String,
    pub token: String,
}
impl ServerDescriptor {
    pub fn path() -> io::Result<PathBuf> {
        Ok(runtime_directory()?.join("server.json"))
    }
    fn read() -> Option<Self> {
        let path = Self::path().ok()?;
        let metadata = path.symlink_metadata().ok()?;
        if !metadata.is_file()
            || metadata.uid() != unsafe { libc::geteuid() }
            || metadata.mode() & 0o077 != 0
            || metadata.len() > 65536
        {
            return None;
        }
        let descriptor: Self = serde_json::from_slice(&std::fs::read(path).ok()?).ok()?;
        if descriptor.version != 1
            || descriptor.pid == 0
            || descriptor.pid > i32::MAX as u32
            || descriptor.token.len() != 48
            || descriptor.generation.len() != 48
        {
            return None;
        }
        let url = reqwest::Url::parse(&descriptor.endpoint).ok()?;
        if url.scheme() != "http"
            || url.host_str() != Some("127.0.0.1")
            || url.port().is_none()
            || url.path() != "/"
            || url.query().is_some()
            || url.fragment().is_some()
            || !url.username().is_empty()
            || url.password().is_some()
        {
            return None;
        }
        if unsafe { libc::kill(descriptor.pid as libc::pid_t, 0) } != 0 {
            return None;
        }
        Some(descriptor)
    }
}
struct Announcer {
    wake: std::sync::mpsc::SyncSender<()>,
    stopped: std::sync::Arc<std::sync::atomic::AtomicBool>,
    worker: Option<std::thread::JoinHandle<()>>,
}
impl Announcer {
    fn start(id: String) -> Self {
        use std::sync::{
            Arc,
            atomic::{AtomicBool, Ordering},
            mpsc,
        };
        let (wake, receiver) = mpsc::sync_channel(1);
        let stopped = Arc::new(AtomicBool::new(false));
        let worker_stopped = stopped.clone();
        let worker = std::thread::spawn(move || {
            let Ok(client) = reqwest::blocking::Client::builder()
                .no_proxy()
                .timeout(std::time::Duration::from_secs(1))
                .redirect(reqwest::redirect::Policy::none())
                .build()
            else {
                return;
            };
            let mut generation = String::new();
            while !worker_stopped.load(Ordering::Acquire) {
                if let Some(server) = ServerDescriptor::read() {
                    let registered = client
                        .post(format!("{}/register", server.endpoint))
                        .header("Origin", &server.endpoint)
                        .header("X-Focus-Registration", &server.token)
                        .json(&serde_json::json!({"id": id}))
                        .send()
                        .is_ok_and(|response| response.status().is_success());
                    if registered && generation != server.generation {
                        generation = server.generation;
                        log::info!("Registered editor with running server");
                    }
                }
                let _ = receiver.recv_timeout(std::time::Duration::from_secs(5));
            }
        });
        Self {
            wake,
            stopped,
            worker: Some(worker),
        }
    }
}
impl Drop for Announcer {
    fn drop(&mut self) {
        self.stopped
            .store(true, std::sync::atomic::Ordering::Release);
        let _ = self.wake.try_send(());
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}
