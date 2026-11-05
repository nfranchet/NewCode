use crate::dev_container::Runtime;
use eframe::egui;
use serde_json::{Value, json};
use std::{
    io::{BufRead, BufReader, Write},
    path::Path,
    process::{Child, ChildStdin, Command, Stdio},
    sync::{
        Arc, Mutex,
        mpsc::{self, Receiver},
    },
    thread,
};

pub(crate) struct AppServer {
    child: Child,
    input: Arc<Mutex<ChildStdin>>,
    events: Receiver<Value>,
    next_id: u64,
}

impl AppServer {
    pub(crate) fn spawn(
        cwd: Option<&Path>,
        runtime: Option<&Runtime>,
        ctx: egui::Context,
    ) -> Result<Self, String> {
        log::info!("spawn requested");
        let mut command = runtime.map_or_else(
            || Command::new("codex"),
            |runtime| runtime.assistant_command("codex"),
        );
        if runtime.is_some() {
            command.args(["-c", "cli_auth_credentials_store=\"file\""]);
        }
        command
            .args(["app-server", "--listen", "stdio://"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        if runtime.is_none()
            && let Some(path) = cwd
        {
            command.current_dir(path);
        }
        let mut child = command.spawn().map_err(|e| {
            log::warn!("Codex server startup failed; check executable and runtime configuration");
            format!("Could not start codex app-server: {e}")
        })?;
        let input = Arc::new(Mutex::new(
            child
                .stdin
                .take()
                .ok_or("Could not open app-server input")?,
        ));
        let stdout = child
            .stdout
            .take()
            .ok_or("Could not open app-server output")?;
        let stderr = child
            .stderr
            .take()
            .ok_or("Could not read app-server errors")?;
        let (tx, events) = mpsc::channel();
        let output_tx = tx.clone();
        thread::spawn(move || {
            for line in BufReader::new(stdout).lines() {
                match line {
                    Ok(line) => {
                        match serde_json::from_str::<Value>(&line) {
                            Ok(value) => {
                                let _ = output_tx.send(value);
                                ctx.request_repaint();
                            }
                            Err(error) => {
                                let _ = output_tx.send(json!({"_error": format!("Invalid app-server response: {error}")}));
                            }
                        }
                    }
                    Err(error) => {
                        let _ = output_tx
                            .send(json!({"_error": format!("App-server read failed: {error}")}));
                        break;
                    }
                }
            }
            let _ = output_tx.send(json!({"_closed": true}));
        });
        thread::spawn(move || {
            let _keep_channel_alive = tx;
            for line in BufReader::new(stderr).lines().flatten() {
                if !line.trim().is_empty() {
                    crate::logging::process_line("focus_editor::app_server", &line);
                }
            }
        });
        Ok(Self {
            child,
            input,
            events,
            next_id: 1,
        })
    }

    /// Local loopback transport for tests; never launches Codex or contacts a service.
    #[cfg(test)]
    pub(crate) fn capture_requests() -> (Self, Receiver<Value>) {
        let mut child = Command::new("cat")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        let input = Arc::new(Mutex::new(child.stdin.take().unwrap()));
        let stdout = child.stdout.take().unwrap();
        let (capture_tx, captured) = mpsc::channel();
        thread::spawn(move || {
            for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                if let Ok(value) = serde_json::from_str(&line) {
                    let _ = capture_tx.send(value);
                }
            }
        });
        let (_, events) = mpsc::channel();
        (
            Self {
                child,
                input,
                events,
                next_id: 1,
            },
            captured,
        )
    }

    #[cfg(test)]
    pub(crate) fn disconnect_capture(&mut self) {
        self.child.kill().unwrap();
        self.child.wait().unwrap();
    }

    pub(crate) fn request(&mut self, method: &str, params: Value) -> Result<u64, String> {
        log::debug!("Codex request method={method}");
        let id = self.next_id;
        self.next_id += 1;
        self.write(json!({"id":id,"method":method,"params":params}))?;
        Ok(id)
    }

    pub(crate) fn notify(&mut self, method: &str, params: Value) -> Result<(), String> {
        self.write(json!({"method":method,"params":params}))
    }

    pub(crate) fn respond(&mut self, id: Value, result: Value) -> Result<(), String> {
        log::debug!("respond requested");
        self.write(json!({"id":id,"result":result}))
    }

    fn write(&mut self, value: Value) -> Result<(), String> {
        let mut input = self
            .input
            .lock()
            .map_err(|_| "App-server input is unavailable".to_string())?;
        serde_json::to_writer(&mut *input, &value).map_err(|e| e.to_string())?;
        input
            .write_all(b"\n")
            .and_then(|_| input.flush())
            .map_err(|e| format!("App-server write failed: {e}"))
    }

    pub(crate) fn drain(&self) -> Vec<Value> {
        self.events.try_iter().collect()
    }
}

impl Drop for AppServer {
    fn drop(&mut self) {
        log::debug!("Codex transport shutting down");
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
