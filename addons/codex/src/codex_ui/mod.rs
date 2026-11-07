//! Codex panel state and lifecycle; rendering and protocol handling are separate modules.
use crate::{app_server::AppServer, dev_container::Runtime};
use egui;
use messages::{Message, PendingRequest};
use serde_json::{Value, json};
use std::{collections::HashMap, path::Path, time::Instant};
mod catalog;
mod composer;
mod conversation;
mod events;
mod header;
mod messages;
mod plan;
mod reasoning;
mod requests;
mod responses;
mod usage;

#[cfg(test)]
mod tests;

pub(crate) struct CodexUi {
    server: Option<AppServer>,
    input: String,
    thread_id: Option<String>,
    turn_id: Option<String>,
    initializing_id: Option<u64>,
    requests: HashMap<u64, String>,
    sessions: Vec<Value>,
    sessions_cursor: Option<String>,
    resume_latest: bool,
    resume_thread: Option<String>,
    models: Vec<Value>,
    modes: Vec<Value>,
    model: String,
    effort: Option<String>,
    mode: Option<Value>,
    usage: Value,
    token_usage: Value,
    usage_error: String,
    last_usage: Option<Instant>,
    composer_input_height: f32,
    busy: bool,
    status: String,
    messages: Vec<Message>,
    pending: Vec<PendingRequest>,
    answer: String,
    plan: plan::PlanState,
    markdown: crate::assistant_ui::content::CommonMarkCache,
    runtime: Option<Runtime>,
}

impl Default for CodexUi {
    fn default() -> Self {
        Self {
            server: None,
            input: String::new(),
            thread_id: None,
            turn_id: None,
            initializing_id: None,
            requests: HashMap::new(),
            sessions: Vec::new(),
            sessions_cursor: None,
            resume_latest: false,
            resume_thread: None,
            models: Vec::new(),
            modes: Vec::new(),
            model: String::new(),
            effort: None,
            mode: None,
            usage: Value::Null,
            token_usage: Value::Null,
            usage_error: String::new(),
            last_usage: None,
            composer_input_height: 80.0,
            busy: false,
            status: "Codex is offline".into(),
            messages: Vec::new(),
            pending: Vec::new(),
            answer: String::new(),
            plan: plan::PlanState::default(),
            markdown: Default::default(),
            runtime: None,
        }
    }
}

impl CodexUi {
    pub(crate) fn saved_session(&self) -> Option<String> {
        self.thread_id
            .clone()
            .or_else(|| self.resume_thread.clone())
    }
    pub(crate) fn restore_session(&mut self, id: String) {
        self.resume_thread = Some(id);
    }

    pub(crate) fn start(&mut self, cwd: Option<&Path>, ctx: egui::Context) {
        log::info!("start requested");
        if self.server.is_some() {
            return;
        }
        match AppServer::spawn(cwd, self.runtime.as_ref(), ctx) {
            Ok(mut server) => {
                self.initializing_id = match server.request("initialize", json!({
                    "clientInfo":{"name":"focus_editor","title":"Focus Editor","version":env!("CARGO_PKG_VERSION")},
                    "capabilities":{"experimentalApi":true}
                })) { Ok(id) => Some(id), Err(e) => { self.status=e; return; } };
                self.clear_plan();
                self.requests.clear();
                self.sessions.clear();
                self.sessions_cursor = None;
                self.resume_latest = true;
                self.models.clear();
                self.modes.clear();
                self.last_usage = None;
                self.server = Some(server);
                self.status = "Connecting to Codex…".into();
                self.thread_id = None;
                self.messages.clear();
            }
            Err(error) => self.status = error,
        }
    }
    pub(crate) fn set_runtime(&mut self, runtime: Option<&Runtime>) -> bool {
        if self.runtime.as_ref() != runtime {
            self.stop();
            self.runtime = runtime.cloned();
            true
        } else {
            false
        }
    }
    pub(super) fn remote_cwd(&self, cwd: Option<&Path>) -> Option<std::path::PathBuf> {
        cwd.and_then(|path| {
            self.runtime.as_ref().map_or_else(
                || Some(path.to_path_buf()),
                |runtime| runtime.container_path(path).ok(),
            )
        })
    }

    pub(crate) fn stop(&mut self) {
        log::info!("stop requested");
        if self.thread_id.is_some() {
            self.resume_thread = self.thread_id.clone();
        }
        self.clear_plan();
        self.server = None;
        self.thread_id = None;
        self.busy = false;
        self.turn_id = None;
        self.pending.clear();
        self.initializing_id = None;
        self.requests.clear();
        self.status = "Codex is offline".into();
    }

    pub(crate) fn state(&self) -> &'static str {
        if !self.pending.is_empty() {
            "Waiting for you"
        } else if self.busy {
            "Running"
        } else if self.thread_id.is_some() {
            "Ready"
        } else if self.server.is_some() {
            "Connecting"
        } else {
            "Offline"
        }
    }
    pub(crate) fn tick(&mut self, cwd: Option<&Path>, ctx: &egui::Context) {
        self.process_events(cwd);
        self.schedule_usage(ctx);
    }

    pub(crate) fn show(&mut self, ui: &mut egui::Ui, cwd: Option<&Path>, ctx: &egui::Context) {
        self.tick(cwd, ctx);
        crate::assistant_ui::configure(ui);
        egui::ScrollArea::vertical()
            .id_salt("assistant_header")
            .max_height(ui.available_height() * 0.35)
            .auto_shrink([false, true])
            .show(ui, |ui| {
                self.show_header(ui, cwd, ctx);
                self.show_usage(ui, ctx);
            });
        let full = ui.available_rect_before_wrap();
        let (conversation, composer, input_height) = crate::assistant_ui::body(ui, &self.input);
        self.composer_input_height = input_height;
        ui.scope_builder(
            egui::UiBuilder::new()
                .id(ui.make_persistent_id("assistant_composer_area"))
                .max_rect(composer),
            |ui| self.show_composer(ui, ctx),
        );
        ui.scope_builder(
            egui::UiBuilder::new()
                .id(ui.make_persistent_id("assistant_conversation_area"))
                .max_rect(conversation),
            |ui| self.show_conversation(ui),
        );
        ui.advance_cursor_after_rect(full);
    }
}
