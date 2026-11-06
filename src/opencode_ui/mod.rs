//! Native OpenCode panel; backend events and rendering remain separate.
use crate::{dev_container::Runtime, opencode_server::Server};
use eframe::egui;
use serde_json::Value;
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    time::Instant,
};
mod approvals;
mod catalog;
mod composer;
mod conversation;
mod events;
mod header;
mod next_events;
mod requests;
mod responses;
mod usage;

#[derive(Clone)]
enum Operation {
    Providers,
    Agents,
    Sessions(bool),
    Create,
    Session,
    History(bool),
    Status,
    Permissions,
    Questions,
    PermissionsV2,
    QuestionsV2,
    Send {
        text: String,
        draft: bool,
        previous_agent: Option<String>,
    },
    Abort,
    Answer {
        question: bool,
        id: String,
    },
}
struct Pending {
    operation: Operation,
    epoch: u64,
}
#[derive(Default)]
struct Message {
    id: String,
    info: Value,
    parts: Vec<Value>,
}
#[derive(Default)]
struct Answers {
    selected: Vec<Vec<String>>,
    custom: Vec<String>,
}

pub(crate) struct OpenCodeUi {
    server: Option<Server>,
    workspace: PathBuf,
    input: String,
    composer_input_height: f32,
    session: Option<String>,
    epoch: u64,
    requests: HashMap<u64, Pending>,
    sessions: Vec<Value>,
    session_info: Value,
    models: Vec<catalog::Model>,
    agents: Vec<String>,
    model: Option<(String, String)>,
    variant: String,
    agent: String,
    messages: Vec<Message>,
    permissions: Vec<Value>,
    questions: Vec<Value>,
    answers: HashMap<String, Answers>,
    busy: bool,
    turn_cancelled: bool,
    online: bool,
    stream_online: bool,
    status: String,
    version: String,
    error: Option<String>,
    seen_events: std::collections::HashSet<String>,
    next_stream: bool,
    session_limit: usize,
    history_before: Option<String>,
    last_refresh: Option<Instant>,
    markdown: crate::assistant_ui::content::CommonMarkCache,
    runtime: Option<Runtime>,
}
impl Default for OpenCodeUi {
    fn default() -> Self {
        Self {
            server: None,
            workspace: PathBuf::new(),
            input: String::new(),
            composer_input_height: 80.0,
            session: None,
            epoch: 0,
            requests: HashMap::new(),
            sessions: Vec::new(),
            session_info: Value::Null,
            models: Vec::new(),
            agents: Vec::new(),
            model: None,
            variant: String::new(),
            agent: String::new(),
            messages: Vec::new(),
            permissions: Vec::new(),
            questions: Vec::new(),
            answers: HashMap::new(),
            busy: false,
            turn_cancelled: false,
            online: false,
            stream_online: false,
            status: "OpenCode is offline".into(),
            version: String::new(),
            error: None,
            seen_events: Default::default(),
            next_stream: false,
            session_limit: 50,
            history_before: None,
            last_refresh: None,
            markdown: Default::default(),
            runtime: None,
        }
    }
}
impl OpenCodeUi {
    pub(crate) fn start(&mut self, cwd: Option<&Path>, ctx: egui::Context) {
        log::info!("start requested");
        if self.server.is_some() {
            return;
        }
        self.workspace = cwd
            .unwrap_or(Path::new("."))
            .canonicalize()
            .unwrap_or_else(|_| cwd.unwrap_or(Path::new(".")).to_path_buf());
        self.server = Some(Server::spawn(&self.workspace, self.runtime.clone(), ctx));
        self.error = None;
        self.online = false;
        self.stream_online = false;
        self.status = "Connecting to OpenCode…".into();
    }
    pub(crate) fn session_id(&self) -> Option<&str> {
        self.session.as_deref()
    }
    pub(crate) fn remember_session(&mut self, id: String) {
        self.session = Some(id);
    }
    pub(crate) fn set_directory(&mut self, path: &Path) {
        if self.server.is_none() {
            self.workspace = path.to_path_buf();
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
    pub(crate) fn stop(&mut self) {
        log::info!("stop requested");
        self.server = None;
        self.online = false;
        self.stream_online = false;
        self.busy = false;
        self.requests.clear();
        self.permissions.clear();
        self.questions.clear();
        self.answers.clear();
        self.status = "OpenCode is offline".into();
    }
    pub(crate) fn state(&self) -> &'static str {
        if !self.permissions.is_empty() || !self.questions.is_empty() {
            "Waiting for you"
        } else if self.busy {
            "Running"
        } else if self.online && self.stream_online {
            "Ready"
        } else if self.server.is_some() {
            "Connecting"
        } else {
            "Offline"
        }
    }
    fn locked(&self) -> bool {
        self.busy
            || !self.permissions.is_empty()
            || !self.questions.is_empty()
            || self.requests.values().any(|p| {
                matches!(
                    p.operation,
                    Operation::Create
                        | Operation::History(_)
                        | Operation::Send { .. }
                        | Operation::Status
                        | Operation::Session
                        | Operation::Permissions
                        | Operation::Questions
                        | Operation::Abort
                )
            })
    }
    pub(crate) fn tick(&mut self, ctx: &egui::Context) {
        self.process_events();
        if self.online {
            ctx.request_repaint_after(std::time::Duration::from_secs(1));
            if self.session.is_some()
                && self
                    .last_refresh
                    .is_none_or(|t| t.elapsed().as_secs() >= 60)
            {
                self.refresh();
            }
        }
    }
    pub(crate) fn show(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        crate::assistant_ui::configure(ui);
        egui::ScrollArea::vertical()
            .id_salt("assistant_header")
            .max_height(ui.available_height() * 0.35)
            .auto_shrink([false, true])
            .show(ui, |ui| {
                self.header(ui, ctx);
                self.usage(ui);
            });
        let full = ui.available_rect_before_wrap();
        let (conversation, composer, input_height) = crate::assistant_ui::body(ui, &self.input);
        self.composer_input_height = input_height;
        ui.scope_builder(
            egui::UiBuilder::new()
                .id(ui.make_persistent_id("assistant_composer_area"))
                .max_rect(composer),
            |ui| self.composer(ui),
        );
        ui.scope_builder(
            egui::UiBuilder::new()
                .id(ui.make_persistent_id("assistant_conversation_area"))
                .max_rect(conversation),
            |ui| self.conversation(ui),
        );
        ui.advance_cursor_after_rect(full);
    }
}
fn string<'a>(value: &'a Value, key: &str) -> &'a str {
    value.get(key).and_then(Value::as_str).unwrap_or("")
}
fn short(text: &str, limit: usize) -> String {
    let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut result: String = text.chars().take(limit).collect();
    if text.chars().count() > limit {
        result.push('…');
    }
    result
}
#[cfg(test)]
mod tests;
