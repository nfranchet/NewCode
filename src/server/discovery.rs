//! One active gateway per user; private editor hello messages never use browser credentials.
use crate::browser::{
    self,
    registry::{self, ServerDescriptor},
};
use std::{
    fs::File,
    io,
    os::unix::fs::{OpenOptionsExt, PermissionsExt},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};
use tiny_http::{Method, Server};

pub(super) struct Discovery {
    _lock: File,
    descriptor: ServerDescriptor,
    stopped: Arc<AtomicBool>,
    worker: Option<std::thread::JoinHandle<()>>,
}
impl Discovery {
    pub fn start() -> io::Result<Self> {
        let directory = registry::runtime_directory()?;
        let lock = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW)
            .open(directory.join("server.lock"))?;
        lock.set_permissions(std::fs::Permissions::from_mode(0o600))?;
        if unsafe {
            libc::flock(
                std::os::fd::AsRawFd::as_raw_fd(&lock),
                libc::LOCK_EX | libc::LOCK_NB,
            )
        } != 0
        {
            return Err(io::Error::other(
                "focus-editor-server is already running for this user and data directory",
            ));
        }
        let server = Server::http("127.0.0.1:0").map_err(io::Error::other)?;
        let endpoint = format!(
            "http://{}",
            server
                .server_addr()
                .to_ip()
                .ok_or_else(|| io::Error::other("Invalid registration listener"))?
        );
        let descriptor = ServerDescriptor {
            version: 1,
            pid: std::process::id(),
            generation: registry::random_id()?,
            endpoint: endpoint.clone(),
            token: registry::random_id()?,
        };
        registry::write_private(&ServerDescriptor::path()?, &descriptor)?;
        let token = descriptor.token.clone();
        let stopped = Arc::new(AtomicBool::new(false));
        let worker_stopped = stopped.clone();
        let worker = std::thread::spawn(move || {
            let Ok(client) = reqwest::blocking::Client::builder()
                .no_proxy()
                .timeout(Duration::from_secs(1))
                .redirect(reqwest::redirect::Policy::none())
                .build()
            else {
                return;
            };
            while !worker_stopped.load(Ordering::Acquire) {
                let Ok(Some(mut request)) = server.recv_timeout(Duration::from_millis(100)) else {
                    continue;
                };
                if request.method() != &Method::Post
                    || request.url() != "/register"
                    || browser::header(&request, "Host")
                        != Some(endpoint.trim_start_matches("http://"))
                    || browser::header(&request, "Origin") != Some(endpoint.as_str())
                    || browser::header(&request, "X-Focus-Registration") != Some(token.as_str())
                {
                    browser::respond(request, 403, "text/plain", "Invalid registration", false);
                    continue;
                }
                let id = browser::read_body(&mut request, 1024)
                    .ok()
                    .and_then(|body| serde_json::from_slice::<serde_json::Value>(&body).ok())
                    .and_then(|value| value["id"].as_str().map(str::to_owned));
                let record = id.and_then(|id| {
                    registry::records()
                        .ok()?
                        .into_iter()
                        .find(|record| record.id == id)
                });
                let Some(record) = record else {
                    browser::respond(request, 404, "text/plain", "Unknown editor", false);
                    continue;
                };
                let info = client
                    .get(format!("{}/api/info", record.endpoint))
                    .header("Origin", &record.endpoint)
                    .header("X-Focus-Token", &record.token)
                    .send()
                    .ok()
                    .filter(|response| response.status().is_success())
                    .and_then(|response| response.json::<serde_json::Value>().ok());
                if !info.is_some_and(|info| info["id"] == record.id && info["version"] == 1) {
                    browser::respond(
                        request,
                        409,
                        "text/plain",
                        "Editor registration could not be verified",
                        false,
                    );
                    continue;
                }
                if let Some(path) = record.workspace {
                    let _ = crate::workspaces::remember(&path);
                }
                browser::respond(request, 200, "application/json", "{}", false);
            }
        });
        Ok(Self {
            _lock: lock,
            descriptor,
            stopped,
            worker: Some(worker),
        })
    }
}
impl Drop for Discovery {
    fn drop(&mut self) {
        self.stopped.store(true, Ordering::Release);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
        if let Ok(path) = ServerDescriptor::path() {
            if std::fs::read(&path)
                .ok()
                .and_then(|bytes| serde_json::from_slice::<ServerDescriptor>(&bytes).ok())
                .is_some_and(|descriptor| descriptor.generation == self.descriptor.generation)
            {
                let _ = std::fs::remove_file(path);
            }
        }
    }
}
