//! Owned local OpenCode process and asynchronous HTTP/SSE transport.
use crate::dev_container::Runtime;
use egui;
use serde_json::Value;
use std::{
    path::Path,
    process::Child,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, Sender},
    },
};
mod stream;
mod worker;

pub(crate) struct Request {
    pub id: u64,
    pub method: String,
    pub path: String,
    pub body: Option<Value>,
}
pub(crate) enum Event {
    Ready(String),
    Reply(u64, Result<Value, String>),
    Data(Value),
    Reconnected,
    Disconnected(String),
    Closed(String),
}
#[derive(Clone)]
struct Events {
    tx: Sender<Event>,
    ctx: egui::Context,
}
impl Events {
    fn send(&self, event: Event) {
        let _ = self.tx.send(event);
        self.ctx.request_repaint();
    }
}

pub(crate) struct Server {
    commands: Sender<Request>,
    events: Receiver<Event>,
    next: u64,
    cancelled: Arc<AtomicBool>,
    child: Arc<Mutex<Option<Child>>>,
}
impl Server {
    pub(crate) fn spawn(cwd: &Path, runtime: Option<Runtime>, ctx: egui::Context) -> Self {
        log::info!("spawn requested");
        let (tx, events) = mpsc::channel();
        let (commands, rx) = mpsc::channel();
        let cancelled = Arc::new(AtomicBool::new(false));
        let child = Arc::new(Mutex::new(None));
        let worker_cancel = cancelled.clone();
        let worker_child = child.clone();
        let cwd = cwd.to_path_buf();
        std::thread::spawn(move || {
            worker::run(
                cwd,
                runtime,
                rx,
                Events { tx, ctx },
                worker_cancel,
                worker_child,
            )
        });
        Self {
            commands,
            events,
            next: 1,
            cancelled,
            child,
        }
    }
    pub(crate) fn request(
        &mut self,
        method: &str,
        path: String,
        body: Option<Value>,
    ) -> Result<u64, String> {
        log::debug!("request requested");
        let id = self.next;
        self.next += 1;
        self.commands
            .send(Request {
                id,
                method: method.into(),
                path,
                body,
            })
            .map_err(|_| "OpenCode connection closed".to_string())?;
        Ok(id)
    }
    pub(crate) fn drain(&self) -> Vec<Event> {
        self.events.try_iter().collect()
    }
    #[cfg(test)]
    pub(crate) fn capture() -> (Self, Receiver<Request>, Sender<Event>) {
        let (tx, events) = mpsc::channel();
        let (commands, rx) = mpsc::channel();
        (
            Self {
                commands,
                events,
                next: 1,
                cancelled: Arc::new(AtomicBool::new(false)),
                child: Arc::new(Mutex::new(None)),
            },
            rx,
            tx,
        )
    }
}
impl Drop for Server {
    fn drop(&mut self) {
        self.cancelled.store(true, Ordering::Relaxed);
        if let Some(mut child) = self.child.lock().unwrap().take() {
            kill(&mut child);
            std::thread::spawn(move || {
                let _ = child.wait();
            });
        }
    }
}

fn kill(child: &mut Child) {
    // Each owned server starts in its own process group, including tool children.
    #[cfg(unix)]
    unsafe {
        libc::kill(-(child.id() as libc::pid_t), libc::SIGKILL);
    }
    let _ = child.kill();
}

#[cfg(test)]
mod tests;
