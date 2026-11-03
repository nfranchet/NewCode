use crate::app_server::AppServer;
use eframe::egui;
use serde_json::{Value, json};
use std::path::Path;

struct PendingRequest {
    id: Value,
    method: String,
    prompt: String,
    answer_key: Option<String>,
}

pub(crate) struct CodexUi {
    server: Option<AppServer>,
    input: String,
    thread_id: Option<String>,
    initializing_id: Option<u64>,
    starting_thread: bool,
    busy: bool,
    status: String,
    messages: Vec<String>,
    pending: Vec<PendingRequest>,
    answer: String,
}

impl Default for CodexUi {
    fn default() -> Self {
        Self {
            server: None,
            input: String::new(),
            thread_id: None,
            initializing_id: None,
            starting_thread: false,
            busy: false,
            status: "App Server is stopped".into(),
            messages: Vec::new(),
            pending: Vec::new(),
            answer: String::new(),
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
                    "capabilities":{"experimentalApi":false}
                })) { Ok(id) => Some(id), Err(e) => { self.status=e; return; } };
                self.server = Some(server);
                self.status = "Connecting to Codex…".into();
                self.thread_id = None;
                self.messages.clear();
            }
            Err(error) => self.status = error,
        }
    }

    pub(crate) fn stop(&mut self) {
        self.server = None;
        self.thread_id = None;
        self.busy = false;
        self.status = "App Server is stopped".into();
    }

    fn send(&mut self, method: &str, params: Value) -> Result<u64, String> {
        self.server
            .as_mut()
            .ok_or("App Server is stopped".to_string())?
            .request(method, params)
    }

    pub(crate) fn show(&mut self, ui: &mut egui::Ui, cwd: Option<&Path>, ctx: &egui::Context) {
        self.process_events(cwd);
        ui.horizontal(|ui| {
            ui.strong("Codex");
            ui.label(
                egui::RichText::new(&self.status)
                    .small()
                    .color(egui::Color32::GRAY),
            );
            if self.server.is_some() {
                if ui.small_button("Stop").clicked() {
                    self.stop();
                }
            } else if ui.small_button("Start").clicked() {
                self.start(cwd, ctx.clone());
            }
        });
        ui.separator();
        egui::ScrollArea::vertical()
            .stick_to_bottom(true)
            .show(ui, |ui| {
                for message in &self.messages {
                    ui.add(egui::Label::new(message).wrap());
                    ui.add_space(6.0);
                }
                for index in 0..self.pending.len() {
                    let pending_id = self.pending[index].id.clone();
                    let pending_method = self.pending[index].method.clone();
                    let pending_prompt = self.pending[index].prompt.clone();
                    let pending_answer_key = self.pending[index].answer_key.clone();
                    ui.group(|ui| {
                        ui.label(egui::RichText::new("Codex needs your response").strong());
                        ui.label(&pending_prompt);
                        if pending_method.contains("requestUserInput") {
                            ui.text_edit_multiline(&mut self.answer);
                            if ui.button("Send response").clicked() {
                                let answer = self.answer.clone();
                                self.answer.clear();
                                self.respond_user_input(
                                    pending_id.clone(),
                                    pending_answer_key,
                                    answer,
                                );
                            }
                        } else {
                            ui.horizontal(|ui| {
                                if ui.button("Allow").clicked() {
                                    self.respond_decision(index, "accept");
                                }
                                if ui.button("Allow this session").clicked() {
                                    self.respond_decision(index, "acceptForSession");
                                }
                                if ui.button("Decline").clicked() {
                                    self.respond_decision(index, "decline");
                                }
                            });
                        }
                    });
                }
            });
        ui.separator();
        ui.add_enabled_ui(
            self.thread_id.is_some() && !self.busy && self.server.is_some(),
            |ui| {
                ui.add(
                    egui::TextEdit::multiline(&mut self.input)
                        .hint_text("Message Codex… (Ctrl+Enter to send)")
                        .desired_rows(3),
                );
                ui.horizontal(|ui| {
                    if ui.button("Send").clicked() {
                        self.send_turn();
                    }
                    if ui.input(|i| i.modifiers.ctrl && i.key_pressed(egui::Key::Enter)) {
                        self.send_turn();
                    }
                    if self.busy && ui.button("Interrupt").clicked() {
                        if let Some(thread_id) = &self.thread_id {
                            let _ = self.send("turn/interrupt", json!({"threadId":thread_id}));
                        }
                    }
                });
            },
        );
    }

    fn process_events(&mut self, cwd: Option<&Path>) {
        let events = self
            .server
            .as_ref()
            .map(AppServer::drain)
            .unwrap_or_default();
        for event in events {
            if let Some(error) = event.get("_error").and_then(Value::as_str) {
                self.status = error.into();
                continue;
            }
            if let Some(log) = event.get("_log").and_then(Value::as_str) {
                self.status = log.into();
                continue;
            }
            if event.get("_closed").is_some() {
                self.status = "Codex App Server stopped".into();
                self.busy = false;
                self.server = None;
                continue;
            }
            if let Some(id) = event.get("id") {
                if let Some(method) = event.get("method").and_then(Value::as_str) {
                    let prompt = event
                        .pointer("/params/questions/0/question")
                        .and_then(Value::as_str)
                        .or_else(|| event.pointer("/params/message").and_then(Value::as_str))
                        .or_else(|| event.pointer("/params/reason").and_then(Value::as_str))
                        .or_else(|| {
                            event
                                .pointer("/params/command/command")
                                .and_then(Value::as_str)
                        })
                        .or_else(|| {
                            event
                                .pointer("/params/changes/0/path")
                                .and_then(Value::as_str)
                        })
                        .unwrap_or(method)
                        .to_string();
                    self.pending.push(PendingRequest {
                        id: id.clone(),
                        method: method.into(),
                        prompt,
                        answer_key: event
                            .pointer("/params/questions/0/id")
                            .and_then(Value::as_str)
                            .map(str::to_string),
                    });
                    continue;
                }
                let result = event.get("result").cloned().unwrap_or(Value::Null);
                let error = event
                    .pointer("/error/message")
                    .and_then(Value::as_str)
                    .map(str::to_string);
                if self.initializing_id == id.as_u64() {
                    self.initializing_id = None;
                    if let Some(error) = error {
                        self.status = format!("Initialize failed: {error}");
                        continue;
                    }
                    if let Some(server) = self.server.as_mut() {
                        let _ = server.notify("initialized", json!({}));
                    }
                    self.starting_thread = true;
                    match self.send(
                        "thread/start",
                        json!({"cwd":cwd.map(|p|p.to_string_lossy().to_string())}),
                    ) {
                        Ok(_) => self.status = "Starting conversation…".into(),
                        Err(e) => self.status = e,
                    }
                } else if self.starting_thread {
                    self.starting_thread = false;
                    if let Some(error) = error {
                        self.status = format!("Could not start conversation: {error}");
                        continue;
                    }
                    self.thread_id = result
                        .pointer("/thread/id")
                        .or_else(|| result.get("id"))
                        .and_then(Value::as_str)
                        .map(str::to_string);
                    self.status = if self.thread_id.is_some() {
                        "Ready".into()
                    } else {
                        "No thread returned by Codex".into()
                    };
                } else if let Some(error) = error {
                    self.messages.push(format!("Codex error: {error}"));
                    self.busy = false;
                }
                continue;
            }
            let method = event
                .get("method")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let params = event.get("params").cloned().unwrap_or(Value::Null);
            match method {
                "item/agentMessage/delta" => {
                    if let Some(delta) = params.get("delta").and_then(Value::as_str) {
                        if let Some(last) = self
                            .messages
                            .last_mut()
                            .filter(|m| m.starts_with("Codex: "))
                        {
                            last.push_str(delta);
                        } else {
                            self.messages.push(format!("Codex: {delta}"));
                        }
                    }
                }
                "turn/started" => {
                    self.busy = true;
                    self.status = "Codex is working…".into();
                }
                "turn/completed" => {
                    self.busy = false;
                    self.status = "Ready".into();
                }
                "item/started" | "item/completed" => {
                    let item = params.get("item").cloned().unwrap_or(Value::Null);
                    let kind = item
                        .get("type")
                        .and_then(Value::as_str)
                        .unwrap_or("activity");
                    if kind != "agentMessage" {
                        let detail = item
                            .get("command")
                            .and_then(Value::as_str)
                            .or_else(|| item.get("text").and_then(Value::as_str))
                            .unwrap_or("");
                        self.messages.push(format!("{kind}: {detail}"));
                    }
                }
                _ => {}
            }
        }
    }

    fn send_turn(&mut self) {
        let text = self.input.trim().to_string();
        let Some(thread_id) = self.thread_id.clone() else {
            return;
        };
        if text.is_empty() {
            return;
        }
        self.messages.push(format!("You: {text}"));
        self.input.clear();
        match self.send(
            "turn/start",
            json!({"threadId":thread_id,"input":[{"type":"text","text":text}]}),
        ) {
            Ok(_) => {
                self.busy = true;
                self.status = "Sending…".into();
            }
            Err(e) => self.status = e,
        }
    }

    fn respond_decision(&mut self, index: usize, decision: &str) {
        let pending = self.pending.remove(index);
        let response = if pending.method.contains("fileChange") {
            json!({"decision":decision})
        } else {
            json!({"decision":decision})
        };
        if let Some(server) = self.server.as_mut() {
            if let Err(e) = server.respond(pending.id, response) {
                self.status = e;
            }
        }
    }

    fn respond_user_input(&mut self, id: Value, answer_key: Option<String>, answer: String) {
        self.pending.retain(|item| item.id != id);
        if let Some(server) = self.server.as_mut() {
            let key = answer_key.unwrap_or_else(|| "response".into());
            if let Err(e) = server.respond(id, json!({"answers":{key:{"answers":[answer]}}})) {
                self.status = e;
            }
        }
    }
}
