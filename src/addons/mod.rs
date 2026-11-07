//! Installed executable addons. Native UI and credential handling stay in the host.
mod catalog;
mod credentials;
mod installation;
mod manager;
mod packages;
mod process;
mod ui;
#[cfg(test)]
use installation::load_manifest;
mod services;
mod surface;
#[cfg(test)]
mod tests;
mod widgets;

use crate::dev_container::Runtime;
use eframe::egui;
use focus_addon_sdk::{
    Action, Context, Manifest, Message, Service, ServiceCall, ServiceReply, View,
};
use process::Process;
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
        mpsc,
    },
    time::Duration,
};

pub(crate) struct Addon {
    pub manifest: Manifest,
    directory: PathBuf,
    storage: PathBuf,
    enabled: bool,
    process: Option<Process>,
    error: Option<String>,
    view: View,
    values: BTreeMap<String, serde_json::Value>,
    icon: Option<egui::TextureHandle>,
    working: bool,
    incarnation: u64,
    surface: surface::Surface,
    seed: serde_json::Value,
    status: String,
}
struct Task {
    addon: String,
    process_generation: u64,
    request: u64,
    call: ServiceCall,
}
struct Completion {
    task: Task,
    reply: ServiceReply,
    changed: bool,
}
struct Credentials {
    addon: String,
    secret: String,
    result: String,
    pending: Option<mpsc::Receiver<Result<(), String>>>,
}

pub(crate) struct Addons {
    pub entries: Vec<Addon>,
    pub active: Option<String>,
    pub detail: bool,
    pub suppressed: Vec<String>,
    context: Context,
    runtime: Option<Runtime>,
    generation: Arc<AtomicU64>,
    process_generation: u64,
    tx: mpsc::Sender<Completion>,
    rx: mpsc::Receiver<Completion>,
    confirm: Option<Task>,
    worktree_jobs: usize,
    credentials: Option<Credentials>,
    pub notice: Option<String>,
    cache: crate::assistant_ui::content::CommonMarkCache,
    manager: manager::Manager,
}
impl Default for Addons {
    fn default() -> Self {
        let (tx, rx) = mpsc::channel();
        Self {
            entries: vec![],
            active: None,
            detail: false,
            suppressed: vec![],
            context: Context::default(),
            runtime: None,
            generation: Arc::new(AtomicU64::new(0)),
            process_generation: 0,
            tx,
            rx,
            confirm: None,
            worktree_jobs: 0,
            credentials: None,
            notice: None,
            cache: Default::default(),
            manager: Default::default(),
        }
    }
}
impl Drop for Addons {
    fn drop(&mut self) {
        for addon in &mut self.entries {
            if let Some(process) = addon.process.take() {
                process.stop_now();
            }
        }
    }
}

impl Addons {
    pub fn apply_settings(
        &mut self,
        enabled: &BTreeMap<String, bool>,
        state: &BTreeMap<String, serde_json::Value>,
    ) {
        for addon in &mut self.entries {
            addon.enabled = !self.suppressed.contains(&addon.manifest.id)
                && enabled
                    .get(&addon.manifest.id)
                    .copied()
                    .unwrap_or(addon.manifest.enabled_by_default);
            addon.seed = state.get(&addon.manifest.id).cloned().unwrap_or_default();
        }
    }

    pub fn sync(&mut self, workspace: Option<&Path>, runtime: Option<&Runtime>) {
        let workspace = workspace.map(|p| p.canonicalize().unwrap_or_else(|_| p.to_path_buf()));
        if self.context.workspace == workspace && self.runtime.as_ref() == runtime {
            return;
        }
        for addon in &mut self.entries {
            if let Some(process) = &addon.process {
                addon.surface.flush_input(process, self.context.generation);
            }
        }
        self.context.workspace = workspace;
        self.context.runtime = runtime.map(Into::into);
        self.runtime = runtime.cloned();
        self.context.generation = self.context.generation.wrapping_add(1);
        self.generation
            .store(self.context.generation, Ordering::SeqCst);
        self.detail = false;
        if let Some(task) = self.confirm.take() {
            self.respond(task, ServiceReply::error("context", "Workspace changed"));
        }
        for addon in &mut self.entries {
            addon.view = View::default();
            addon.values.clear();
            addon.surface.workspace_changed();
            addon.status.clear();
            if let Some(process) = &addon.process {
                let mut context = self.context.clone();
                context.storage = addon.storage.clone();
                context.state = addon.seed.clone();
                let _ = process.send(Message::notification("context.changed", context));
            }
        }
    }
    pub fn activate(&mut self, id: String, ctx: &egui::Context) {
        if self
            .entries
            .iter()
            .any(|a| a.manifest.id == id && a.manifest.panel == focus_addon_sdk::Panel::Left)
        {
            self.active = Some(id.clone());
            self.detail = false;
        }
        let Some(addon) = self
            .entries
            .iter_mut()
            .find(|a| a.manifest.id == id && a.enabled)
        else {
            return;
        };
        if addon.process.is_none() {
            let mut context = self.context.clone();
            context.storage = addon.storage.clone();
            context.state = addon.seed.clone();
            addon.surface = Default::default();
            match Process::start(&addon.directory, &addon.manifest, &context, ctx) {
                Ok(process) => {
                    addon.process = Some(process);
                    addon.error = None;
                    self.process_generation += 1;
                    addon.incarnation = self.process_generation;
                }
                Err(error) => addon.error = Some(error),
            }
        }
        if let Some(process) = &addon.process {
            let _ = process.send(Message::notification(
                "view.activated",
                serde_json::json!({"generation":self.context.generation}),
            ));
        }
    }
    fn respond(&mut self, task: Task, reply: ServiceReply) {
        if let Some(addon) = self
            .entries
            .iter_mut()
            .find(|a| a.manifest.id == task.addon)
        {
            if task.process_generation != addon.incarnation {
                return;
            }
            if let Some(process) = &addon.process {
                let _ = process.send(Message::response(task.request, reply));
            }
        }
    }
    fn spawn(&mut self, task: Task, ctx: &egui::Context) {
        if task.call.generation != self.context.generation {
            self.respond(task, ServiceReply::error("context", "Workspace changed"));
            return;
        }
        let Some(addon) = self
            .entries
            .iter_mut()
            .find(|a| a.manifest.id == task.addon)
        else {
            return;
        };
        if addon.process.is_none() || addon.incarnation != task.process_generation {
            return;
        }
        addon.working = true;
        let manifest = addon.manifest.clone();
        let workspace = self.context.workspace.clone();
        let runtime = self.runtime.clone();
        let tx = self.tx.clone();
        let ctx = ctx.clone();
        let generation = self.generation.clone();
        let changing = matches!(&task.call.service, Service::Git { operation } if operation.changes_worktree());
        if changing {
            self.worktree_jobs += 1;
        }
        std::thread::spawn(move || {
            let reply = if generation.load(Ordering::SeqCst) != task.call.generation {
                ServiceReply::error("context", "Workspace changed")
            } else {
                services::execute(
                    &manifest,
                    workspace.as_deref(),
                    runtime.as_ref(),
                    task.call.service.clone(),
                )
            };
            let changed =
                matches!(&task.call.service, Service::Git { operation } if operation.mutation());
            let _ = tx.send(Completion {
                task,
                reply,
                changed,
            });
            ctx.request_repaint();
        });
    }
    /// Return true after any Git mutation, including partially completed failures.
    pub fn tick(&mut self, ctx: &egui::Context, dirty: bool, visible: bool) -> bool {
        let starters: Vec<_> = self
            .entries
            .iter()
            .filter(|a| {
                a.enabled
                    && a.manifest.panel == focus_addon_sdk::Panel::Right
                    && a.process.is_none()
                    && a.error.is_none()
            })
            .map(|a| a.manifest.id.clone())
            .collect();
        for id in starters {
            self.activate(id, ctx);
        }
        let mut changed = false;
        while let Ok(completion) = self.rx.try_recv() {
            if matches!(&completion.task.call.service, Service::Git { operation } if operation.changes_worktree())
            {
                self.worktree_jobs = self.worktree_jobs.saturating_sub(1);
            }
            changed |=
                completion.changed && completion.task.call.generation == self.context.generation;
            self.respond(completion.task, completion.reply);
        }
        let mut tasks = Vec::new();
        for addon in &mut self.entries {
            let mut failure = None;
            if let Some(process) = &mut addon.process {
                if process.ready && process.last_tick.elapsed() > Duration::from_secs(30) {
                    let _ = process.send(Message::notification("tick", serde_json::json!({"visible": visible && self.active.as_ref() == Some(&addon.manifest.id)})));
                    process.last_tick = std::time::Instant::now();
                }
                for _ in 0..64 {
                    let event = match process.receiver.try_recv() {
                        Ok(event) => event,
                        Err(_) => break,
                    };
                    let message = match event {
                        Ok(message) => message,
                        Err(error) => {
                            failure = Some(error);
                            break;
                        }
                    };
                    match message.method.as_deref() {
                        None if message.id == Some(0) => {
                            if message
                                .result
                                .as_ref()
                                .and_then(|v| v.get("protocol"))
                                .and_then(|v| v.as_u64())
                                == Some(focus_addon_sdk::PROTOCOL_VERSION as u64)
                            {
                                if addon.manifest.ui == focus_addon_sdk::Presentation::Surface
                                    && message
                                        .result
                                        .as_ref()
                                        .and_then(|v| v.get("surface"))
                                        .and_then(|v| v.as_u64())
                                        != Some(1)
                                {
                                    failure = Some("Unsupported addon UI surface version".into());
                                } else {
                                    process.ready = true;
                                }
                            } else {
                                failure = Some("Addon protocol handshake failed".into());
                            }
                        }
                        Some("panel.status") => {
                            if let Some(params) = message.params {
                                if params["generation"].as_u64() == Some(self.context.generation) {
                                    addon.status =
                                        params["state"].as_str().unwrap_or_default().into();
                                }
                            }
                        }
                        Some("surface.frame") => {
                            match serde_json::from_value::<focus_addon_support::surface::Frame>(
                                message.params.unwrap_or_default(),
                            ) {
                                Ok(frame) if frame.generation == self.context.generation => {
                                    addon.status = frame.state.clone();
                                    if let Err(error) = addon.surface.accept(ctx, frame) {
                                        failure = Some(error);
                                    }
                                }
                                Ok(frame) => {
                                    if let Err(error) = addon.surface.resources(ctx, &frame) {
                                        failure = Some(error);
                                    }
                                }
                                Err(_) => failure = Some("Invalid addon surface frame".into()),
                            }
                        }
                        Some("view.update") => {
                            match serde_json::from_value::<View>(message.params.unwrap_or_default())
                            {
                                Ok(view) if view.generation == self.context.generation => {
                                    if view.reveal_detail
                                        && !view.detail.is_empty()
                                        && self.active.as_ref() == Some(&addon.manifest.id)
                                    {
                                        self.detail = true;
                                    }
                                    if addon.view.form != view.form {
                                        addon.values.clear();
                                        if self.active.as_ref() == Some(&addon.manifest.id) {
                                            self.detail = !view.detail.is_empty();
                                        }
                                    }
                                    addon.working = view.busy;
                                    addon.view = view;
                                }
                                Ok(_) => (),
                                Err(_) => failure = Some("Addon sent an invalid view".into()),
                            }
                        }
                        Some("host.call") => {
                            if let Some(request) = message.id {
                                match serde_json::from_value::<ServiceCall>(
                                    message.params.unwrap_or_default(),
                                ) {
                                    Ok(call) => tasks.push(Task {
                                        addon: addon.manifest.id.clone(),
                                        process_generation: addon.incarnation,
                                        request,
                                        call,
                                    }),
                                    Err(_) => {
                                        let _ = process.send(Message::response(
                                            request,
                                            ServiceReply::error(
                                                "invalid_request",
                                                "Unsupported host service request",
                                            ),
                                        ));
                                    }
                                }
                            }
                        }
                        _ => (),
                    }
                }
                if process.expired() && failure.is_none() {
                    failure =
                        Some("Addon stopped or initialization timed out. Use Restart.".into());
                }
            }
            if let Some(error) = failure {
                log::warn!("Addon failed id={}", addon.manifest.id);
                addon.error = Some(error);
                addon.process = None;
                addon.working = false;
            }
        }
        for task in tasks {
            if task.call.generation != self.context.generation {
                self.respond(task, ServiceReply::error("context", "Workspace changed"));
            } else if dirty
                && matches!(&task.call.service, Service::Git { operation } if operation.changes_worktree())
            {
                if self.confirm.is_none() {
                    self.confirm = Some(task);
                } else {
                    self.respond(
                        task,
                        ServiceReply::error(
                            "busy",
                            "Another checkout is waiting for unsaved files",
                        ),
                    );
                }
            } else {
                self.spawn(task, ctx);
            }
        }
        if self.entries.iter().any(|a| a.process.is_some()) {
            ctx.request_repaint_after(Duration::from_millis(100));
        }
        changed
    }
    pub fn checkout_waiting(&self) -> bool {
        self.confirm.is_some()
    }
    pub fn worktree_busy(&self) -> bool {
        self.worktree_jobs > 0
    }
    pub fn confirm_checkout(&mut self, allow: bool, ctx: &egui::Context) {
        if let Some(task) = self.confirm.take() {
            if allow {
                self.spawn(task, ctx);
            } else {
                self.respond(
                    task,
                    ServiceReply::error("cancelled", "Checkout cancelled; unsaved files were kept"),
                );
            }
        }
    }
}
