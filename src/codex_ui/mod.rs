//! Codex panel state and lifecycle; rendering and protocol handling are separate modules.
use crate::app_server::AppServer;
use eframe::egui;
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
    composer_height: f32,
    busy: bool,
    status: String,
    messages: Vec<Message>,
    pending: Vec<PendingRequest>,
    answer: String,
    plan: plan::PlanState,
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
            composer_height: 190.0,
            busy: false,
            status: "Codex is offline".into(),
            messages: Vec::new(),
            pending: Vec::new(),
            answer: String::new(),
            plan: plan::PlanState::default(),
        }
    }
}

impl CodexUi {
    pub(crate) fn start(&mut self, cwd: Option<&Path>, ctx: egui::Context) {
        if self.server.is_some() {
            return;
        }
        match AppServer::spawn(cwd, ctx) {
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

    pub(crate) fn stop(&mut self) {
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
        ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Wrap);
        let cursor_color = if ui.visuals().dark_mode {
            egui::Color32::WHITE
        } else {
            egui::Color32::BLACK
        };
        ui.visuals_mut().text_cursor.stroke = egui::Stroke::new(3.0_f32, cursor_color);
        ui.visuals_mut().text_cursor.blink = false;
        self.show_header(ui, cwd, ctx);
        self.show_usage(ui, ctx);
        ui.separator();
        self.show_conversation(ui);
        self.show_composer(ui, ctx);
    }
}
