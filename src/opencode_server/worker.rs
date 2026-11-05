//! Process startup, bounded HTTP requests, and process cleanup.
use super::{Event, Events, Request, stream};
use crate::dev_container::Runtime;
use reqwest::blocking::Client;
use serde_json::Value;
use std::{
    io::{BufRead, BufReader},
    os::unix::process::CommandExt,
    path::PathBuf,
    process::{Child, Command, Stdio},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver},
    },
    time::{Duration, Instant},
};

pub(super) fn run(
    cwd: PathBuf,
    runtime: Option<Runtime>,
    commands: Receiver<Request>,
    events: Events,
    cancel: Arc<AtomicBool>,
    owned: Arc<Mutex<Option<Child>>>,
) {
    log::debug!("OpenCode worker starting");
    let result = serve(&cwd, runtime.as_ref(), commands, &events, &cancel, &owned);
    if let Some(mut child) = owned.lock().unwrap().take() {
        super::kill(&mut child);
        let _ = child.wait();
    }
    if !cancel.load(Ordering::Relaxed) {
        log::warn!("OpenCode worker stopped unexpectedly; details available in panel");
        events.send(Event::Closed(
            result.err().unwrap_or_else(|| "OpenCode stopped".into()),
        ));
    }
}
fn serve(
    cwd: &std::path::Path,
    runtime: Option<&Runtime>,
    commands: Receiver<Request>,
    events: &Events,
    cancel: &Arc<AtomicBool>,
    owned: &Arc<Mutex<Option<Child>>>,
) -> Result<(), String> {
    let (url_tx, url_rx) = mpsc::channel();
    {
        let mut guard = owned.lock().unwrap();
        if cancel.load(Ordering::Relaxed) {
            return Ok(());
        }
        let mut command = runtime.map_or_else(
            || Command::new("opencode"),
            |runtime| runtime.assistant_command("opencode"),
        );
        command.process_group(0);
        let child = command
            .args([
                "serve",
                "--hostname",
                if runtime.is_some() {
                    "0.0.0.0"
                } else {
                    "127.0.0.1"
                },
                "--port",
                "0",
            ])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        if runtime.is_none() {
            child.current_dir(cwd);
        }
        let mut child = child.spawn().map_err(|e| {
            format!("Could not start OpenCode: {e}. Install opencode and check PATH.")
        })?;
        let stdout = child.stdout.take().ok_or("OpenCode stdout unavailable")?;
        let stderr = child.stderr.take().ok_or("OpenCode stderr unavailable")?;
        *guard = Some(child);
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                if let Some(url) = line.strip_prefix("opencode server listening on ") {
                    let _ = url_tx.send(url.trim().to_string());
                }
            }
        });
        std::thread::spawn(move || {
            for line in BufReader::new(stderr).lines().map_while(Result::ok) {
                crate::logging::process_line("focus_editor::opencode_server", &line);
            }
        });
    }
    let deadline = Instant::now() + Duration::from_secs(15);
    let url = loop {
        if cancel.load(Ordering::Relaxed) {
            return Ok(());
        }
        if let Ok(url) = url_rx.recv_timeout(Duration::from_millis(50)) {
            break url;
        }
        if let Some(status) = owned
            .lock()
            .unwrap()
            .as_mut()
            .and_then(|c| c.try_wait().ok().flatten())
        {
            return Err(format!(
                "OpenCode exited during startup ({status}). Check its CLI configuration."
            ));
        }
        if Instant::now() >= deadline {
            return Err(
                "OpenCode startup timed out. Check its CLI configuration and retry Start.".into(),
            );
        }
    };
    let parsed = reqwest::Url::parse(&url).map_err(|e| e.to_string())?;
    if parsed.scheme() != "http"
        || !matches!(parsed.host_str(), Some("127.0.0.1") | Some("0.0.0.0"))
    {
        return Err("OpenCode returned an unexpected server address".into());
    }
    let cwd = runtime.map_or_else(
        || Ok(cwd.to_path_buf()),
        |runtime| runtime.container_path(cwd),
    )?;
    let url = if let Some(runtime) = runtime {
        let address = runtime.container_ip()?;
        format!(
            "http://{address}:{}",
            parsed.port().ok_or("OpenCode did not return a port")?
        )
    } else {
        url
    };
    let client = Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .connect_timeout(Duration::from_secs(3))
        .build()
        .map_err(|e| e.to_string())?;
    log::debug!("OpenCode HTTP connection established");
    let health = execute(&client, &url, &cwd, "GET", "/global/health", None)?;
    events.send(Event::Ready(
        health["version"].as_str().unwrap_or("unknown").into(),
    ));
    let stream_client = client.clone();
    let stream_events = events.clone();
    let stream_cancel = cancel.clone();
    let stream_url = url.clone();
    let stream_cwd = cwd.to_path_buf();
    std::thread::spawn(move || {
        stream::listen(
            stream_client,
            stream_url,
            stream_cwd,
            stream_events,
            stream_cancel,
        )
    });
    while !cancel.load(Ordering::Relaxed) {
        if let Some(status) = owned
            .lock()
            .unwrap()
            .as_mut()
            .and_then(|c| c.try_wait().ok().flatten())
        {
            return Err(format!("OpenCode server exited ({status})"));
        }
        match commands.recv_timeout(Duration::from_millis(50)) {
            Ok(request) => {
                let result = execute(
                    &client,
                    &url,
                    &cwd,
                    &request.method,
                    &request.path,
                    request.body.as_ref(),
                );
                events.send(Event::Reply(request.id, result));
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(_) => break,
        }
    }
    Ok(())
}

pub(super) fn authorize(
    builder: reqwest::blocking::RequestBuilder,
) -> reqwest::blocking::RequestBuilder {
    if let Ok(password) = std::env::var("OPENCODE_SERVER_PASSWORD") {
        builder.basic_auth(
            std::env::var("OPENCODE_SERVER_USERNAME").unwrap_or_else(|_| "opencode".into()),
            Some(password),
        )
    } else {
        builder
    }
}
pub(super) fn endpoint(
    base: &str,
    path: &str,
    cwd: &std::path::Path,
) -> Result<reqwest::Url, String> {
    let mut url = reqwest::Url::parse(&format!("{base}{path}")).map_err(|e| e.to_string())?;
    url.query_pairs_mut()
        .append_pair("directory", &cwd.to_string_lossy());
    Ok(url)
}
pub(super) fn execute(
    client: &Client,
    base: &str,
    cwd: &std::path::Path,
    method: &str,
    path: &str,
    body: Option<&Value>,
) -> Result<Value, String> {
    let url = endpoint(base, path, cwd)?;
    let method = reqwest::Method::from_bytes(method.as_bytes()).map_err(|e| e.to_string())?;
    let mut request = authorize(client.request(method, url)).timeout(Duration::from_secs(10));
    if let Some(body) = body {
        request = request.json(body);
    }
    let response = request.send().map_err(|e|format!("OpenCode request failed: {e}. Refresh history before retrying a prompt; it may have been accepted."))?;
    let status = response.status();
    let text = response.text().map_err(|e| e.to_string())?;
    if !status.is_success() {
        let detail = serde_json::from_str::<Value>(&text).ok().and_then(|v| {
            v.get("message")
                .or_else(|| v.pointer("/data/message"))
                .and_then(Value::as_str)
                .map(str::to_owned)
        });
        return Err(format!(
            "OpenCode HTTP {status}{}",
            detail.map(|d| format!(": {d}")).unwrap_or_default()
        ));
    }
    if text.is_empty() {
        return Ok(Value::Null);
    }
    serde_json::from_str(&text).map_err(|_| {
        "OpenCode returned an unsupported response. Check the installed version.".into()
    })
}
