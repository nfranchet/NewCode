use eframe::egui;
use focus_addon_sdk::{Context, Manifest, Message, read_message, write_message};
use std::{
    io::BufReader,
    path::Path,
    process::{Child, Command, Stdio},
    sync::mpsc::{self, Receiver, SyncSender},
    time::{Duration, Instant},
};

pub(super) struct Process {
    child: Option<Child>,
    sender: SyncSender<Message>,
    pub receiver: Receiver<Result<Message, String>>,
    pub started: Instant,
    pub ready: bool,
    pub last_tick: Instant,
}
impl Process {
    pub fn start(
        directory: &Path,
        manifest: &Manifest,
        context: &Context,
        ctx: &egui::Context,
    ) -> Result<Self, String> {
        let mut command = Command::new(directory.join(&manifest.executable));
        command
            .current_dir(directory)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        for (key, _) in std::env::vars_os() {
            let name = key.to_string_lossy().to_ascii_uppercase();
            if ["TOKEN", "PASSWORD", "SECRET", "API_KEY", "CREDENTIAL"]
                .iter()
                .any(|word| name.contains(word))
            {
                command.env_remove(key);
            }
        }
        let mut child = command
            .spawn()
            .map_err(|e| format!("Cannot start addon: {e}"))?;
        let stdout = child.stdout.take().unwrap();
        let mut stdin = child.stdin.take().unwrap();
        let stderr = child.stderr.take().unwrap();
        let (sender, requests) = mpsc::sync_channel(64);
        let (events, receiver) = mpsc::sync_channel(128);
        let ctx_reader = ctx.clone();
        let error_events = events.clone();
        std::thread::spawn(move || {
            while let Ok(message) = requests.recv() {
                if write_message(&mut stdin, &message).is_err() {
                    let _ = error_events.send(Err("Addon input closed".into()));
                    break;
                }
            }
        });
        std::thread::spawn(move || {
            let mut reader = BufReader::new(stdout);
            loop {
                let event = match read_message(&mut reader) {
                    Ok(Some(message)) => Ok(message),
                    Ok(None) => Err("Addon exited. Use Restart to reconnect.".into()),
                    Err(e) => Err(format!("Invalid addon protocol: {e}")),
                };
                let finished = event.is_err();
                if events.send(event).is_err() {
                    break;
                }
                ctx_reader.request_repaint();
                if finished {
                    break;
                }
            }
        });
        let id = manifest.id.clone();
        std::thread::spawn(move || {
            // Bound each diagnostic read as well as protocol reads.
            use std::io::{BufRead, Read};
            let mut reader = BufReader::new(stderr);
            loop {
                let mut line = Vec::new();
                if reader
                    .by_ref()
                    .take(4096)
                    .read_until(b'\n', &mut line)
                    .unwrap_or(0)
                    == 0
                {
                    break;
                }
                for line in crate::logging::redaction::sanitize(&String::from_utf8_lossy(&line)) {
                    log::debug!("Addon {}: {}", id, line);
                }
            }
        });
        let process = Self {
            child: Some(child),
            sender,
            receiver,
            started: Instant::now(),
            ready: false,
            last_tick: Instant::now(),
        };
        process.send(Message::request(
            0,
            "initialize",
            serde_json::json!({"protocol": focus_addon_sdk::PROTOCOL_VERSION, "context":context}),
        ))?;
        Ok(process)
    }
    pub fn send(&self, message: Message) -> Result<(), String> {
        self.sender
            .try_send(message)
            .map_err(|_| "Addon is not accepting messages. Restart it to reconnect.".into())
    }
    pub fn expired(&mut self) -> bool {
        (!self.ready && self.started.elapsed() > Duration::from_secs(5))
            || self
                .child
                .as_mut()
                .is_none_or(|child| child.try_wait().ok().flatten().is_some())
    }
    pub fn stop_now(mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}
impl Drop for Process {
    fn drop(&mut self) {
        let _ = self.send(Message::notification("shutdown", ()));
        if let Some(mut child) = self.child.take() {
            std::thread::spawn(move || {
                let start = Instant::now();
                while start.elapsed() < Duration::from_secs(2) {
                    if child.try_wait().ok().flatten().is_some() {
                        return;
                    }
                    std::thread::sleep(Duration::from_millis(25));
                }
                let _ = child.kill();
                let _ = child.wait();
            });
        }
    }
}
