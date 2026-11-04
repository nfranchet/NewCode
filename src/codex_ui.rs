use crate::{
    app_server::AppServer,
    ui_theme::{self as theme, Icon},
};
use eframe::egui;
use serde_json::{Value, json};
use std::{
    collections::HashMap,
    path::Path,
    time::{Duration, Instant},
};

struct PendingRequest {
    id: Value,
    method: String,
    prompt: String,
    answer_key: Option<String>,
}

struct Message {
    id: String,
    kind: String,
    text: String,
    output: String,
    complete: bool,
}

impl Message {
    fn new(kind: &str, text: String) -> Self {
        Self {
            id: String::new(),
            kind: kind.into(),
            text,
            output: String::new(),
            complete: true,
        }
    }
}

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
                self.requests.clear();
                self.sessions.clear();
                self.sessions_cursor = None;
                self.resume_latest = true;
                self.models.clear();
                self.modes.clear();
                self.mode = None;
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
        self.server = None;
        self.thread_id = None;
        self.busy = false;
        self.turn_id = None;
        self.pending.clear();
        self.initializing_id = None;
        self.requests.clear();
        self.status = "Codex is offline".into();
    }

    fn send(&mut self, method: &str, params: Value) -> Result<u64, String> {
        let id = self
            .server
            .as_mut()
            .ok_or("Codex is offline".to_string())?
            .request(method, params)?;
        self.requests.insert(id, method.into());
        Ok(id)
    }

    pub(crate) fn show(&mut self, ui: &mut egui::Ui, cwd: Option<&Path>, ctx: &egui::Context) {
        self.process_events(cwd);
        ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Wrap);
        let cursor_color = if ui.visuals().dark_mode {
            egui::Color32::WHITE
        } else {
            egui::Color32::BLACK
        };
        ui.visuals_mut().text_cursor.stroke = egui::Stroke::new(3.0_f32, cursor_color);
        ui.visuals_mut().text_cursor.blink = false;
        ui.horizontal(|ui| {
            theme::icon(ui, Icon::Spark, theme::TEAL);
            ui.label(egui::RichText::new("Codex").strong().color(theme::TEAL));
            let status = if !self.pending.is_empty() {
                "Waiting for you"
            } else if self.busy {
                "Running"
            } else if self.thread_id.is_some() {
                "Ready"
            } else {
                "Offline"
            };
            let (color, fill) = match status {
                "Running" => (theme::TEAL, theme::TEAL_SOFT),
                "Ready" => (theme::GREEN, egui::Color32::from_rgb(235, 247, 239)),
                "Waiting for you" => (theme::AMBER, egui::Color32::from_rgb(255, 246, 223)),
                _ => (theme::MUTED, theme::SURFACE),
            };
            theme::badge(ui, status, color, fill).on_hover_text(&self.status);
            if self.busy {
                ui.spinner();
            }
            egui::ScrollArea::horizontal()
                .id_salt("codex_header")
                .auto_shrink([false, true])
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        if self.server.is_some() {
                            if theme::button(ui, Icon::Stop, "", theme::MUTED, false)
                                .on_hover_text("Stop Codex")
                                .clicked()
                            {
                                self.stop();
                            }
                        } else if theme::button(ui, Icon::Spark, "Start", theme::TEAL, true)
                            .clicked()
                        {
                            self.start(cwd, ctx.clone());
                        }
                        ui.add_enabled_ui(
                            self.server.is_some()
                                && self.initializing_id.is_none()
                                && !self.busy
                                && self.pending.is_empty()
                                && !self.requests.values().any(|m| {
                                    matches!(
                                        m.as_str(),
                                        "thread/list" | "thread/start" | "thread/resume"
                                    )
                                }),
                            |ui| {
                                ui.horizontal(|ui| {
                                    if ui.small_button("+").on_hover_text("New session").clicked() {
                                        self.open_thread(cwd, None);
                                    }
                                    let selected = self.sessions.iter().find(|session| {
                                        session.get("id").and_then(Value::as_str)
                                            == self.thread_id.as_deref()
                                    });
                                    let label = selected
                                        .map(|session| session_label(session, 18, false))
                                        .unwrap_or_else(|| "Choose session…".into());
                                    let mut chosen = None;
                                    egui::ComboBox::from_id_salt("codex_session")
                                        .truncate()
                                        .selected_text(label)
                                        .width(95.0)
                                        .show_ui(ui, |ui| {
                                            ui.weak("Recent sessions in this workspace");
                                            if self.sessions.is_empty() {
                                                ui.label("No saved sessions");
                                            }
                                            for session in &self.sessions {
                                                if let Some(id) =
                                                    session.get("id").and_then(Value::as_str)
                                                {
                                                    if ui
                                                        .selectable_label(
                                                            self.thread_id.as_deref() == Some(id),
                                                            session_label(session, 100, true),
                                                        )
                                                        .on_hover_text(format!(
                                                            "{}\n{id}",
                                                            session
                                                                .get("name")
                                                                .and_then(Value::as_str)
                                                                .or_else(|| session
                                                                    .get("preview")
                                                                    .and_then(Value::as_str))
                                                                .unwrap_or("Untitled session")
                                                        ))
                                                        .clicked()
                                                        && self.thread_id.as_deref() != Some(id)
                                                    {
                                                        chosen = Some(id.to_string());
                                                    }
                                                }
                                            }
                                        });
                                    if let Some(id) = chosen {
                                        self.open_thread(cwd, Some(&id));
                                    }
                                    if ui
                                        .small_button("↻")
                                        .on_hover_text("Refresh sessions")
                                        .clicked()
                                    {
                                        self.list_sessions(cwd, None);
                                    }
                                    if let Some(cursor) = self.sessions_cursor.clone() {
                                        if ui
                                            .small_button("…")
                                            .on_hover_text("Load older sessions")
                                            .clicked()
                                        {
                                            self.list_sessions(cwd, Some(&cursor));
                                        }
                                    }
                                });
                            },
                        );
                    });
                });
        });
        if self.server.is_some() && self.initializing_id.is_none() {
            ctx.request_repaint_after(Duration::from_secs(1));
            if self
                .last_usage
                .is_none_or(|t| t.elapsed() >= Duration::from_secs(60))
                && !self
                    .requests
                    .values()
                    .any(|m| m == "account/rateLimits/read")
            {
                self.refresh_usage();
            }
        }
        egui::CollapsingHeader::new(usage_title(&self.usage)).show(ui, |ui| {
            if let Some(id) = &self.thread_id {
                ui.small(format!("Session: {id}"));
            }
            ui.small(format!("Model: {}", self.model));
            if !self.usage_error.is_empty() {
                ui.small(&self.usage_error);
            }
            if self.usage.is_null() {
                ui.small("Account usage unavailable");
            } else {
                show_limits(ui, &self.usage);
            }
            if let Some(total) = self
                .token_usage
                .pointer("/total/totalTokens")
                .and_then(Value::as_i64)
            {
                ui.small(format!("Session tokens: {total}"));
            }
            if let (Some(tokens), Some(window)) = (
                self.token_usage
                    .pointer("/last/totalTokens")
                    .and_then(Value::as_f64),
                self.token_usage
                    .get("modelContextWindow")
                    .and_then(Value::as_f64),
            ) {
                ui.small(format!(
                    "Context: {:.0}% used",
                    tokens / window.max(1.0) * 100.0
                ));
            }
            if ui
                .add_enabled(self.server.is_some(), egui::Button::new("Refresh usage"))
                .clicked()
            {
                self.refresh_usage();
            }
        });
        ui.separator();
        // Reserve the composer before laying out the scrollable conversation.
        let composer_height = self.composer_height;
        egui::ScrollArea::vertical()
            .id_salt("codex_conversation")
            .max_height((ui.available_height() - composer_height).max(0.0))
            .auto_shrink([false, false])
            .stick_to_bottom(true)
            .show(ui, |ui| {
                if self.messages.is_empty() {
                    ui.vertical_centered(|ui| {
                        theme::empty_art(ui, Icon::Spark, theme::TEAL);
                        ui.label(
                            egui::RichText::new("A little help, a lot of progress")
                                .strong()
                                .color(theme::TEAL),
                        );
                        ui.label("Ask a question, plan a change, or work through a bug.");
                        ui.add_space(8.0);
                        ui.small("Pick a model, thinking level, and mode below.");
                    });
                }
                for (index, message) in self.messages.iter().enumerate() {
                    ui.push_id(index, |ui| match message.kind.as_str() {
                        "Command" | "Reasoning" | "Files" => {
                            let title = if message.kind == "Command" {
                                format!(
                                    "{} · {}",
                                    if message.complete {
                                        "Executed"
                                    } else {
                                        "Running"
                                    },
                                    "Command"
                                )
                            } else {
                                message.kind.clone()
                            };
                            egui::CollapsingHeader::new(title).show(ui, |ui| {
                                if message.kind == "Reasoning" && message.text.is_empty() {
                                    ui.weak("No reasoning summary provided");
                                }
                                ui.add(
                                    egui::Label::new(
                                        egui::RichText::new(&message.text).monospace(),
                                    )
                                    .wrap(),
                                );
                                if !message.output.is_empty() {
                                    ui.separator();
                                    ui.label("Output");
                                    ui.add(
                                        egui::Label::new(
                                            egui::RichText::new(&message.output).monospace(),
                                        )
                                        .wrap(),
                                    );
                                }
                            });
                        }
                        _ => {
                            let (accent, fill) = match message.kind.as_str() {
                                "You" => (theme::BLUE, theme::BLUE_SOFT),
                                "Proposed plan" => (theme::TEAL, theme::TEAL_SOFT),
                                "Error" => (theme::RED, egui::Color32::from_rgb(255, 241, 243)),
                                "Status" => (theme::GREEN, theme::SURFACE),
                                _ => (theme::TEAL, egui::Color32::WHITE),
                            };
                            egui::Frame::new()
                                .fill(fill)
                                .stroke(egui::Stroke::new(1.0_f32, theme::BORDER))
                                .corner_radius(7)
                                .inner_margin(10)
                                .show(ui, |ui| {
                                    ui.label(
                                        egui::RichText::new(&message.kind)
                                            .small()
                                            .strong()
                                            .color(accent),
                                    );
                                    ui.add(egui::Label::new(&message.text).wrap().selectable(true));
                                });
                        }
                    });
                    ui.add_space(10.0);
                }
                for index in (0..self.pending.len()).rev() {
                    let pending_id = self.pending[index].id.clone();
                    let pending_method = self.pending[index].method.clone();
                    let pending_prompt = self.pending[index].prompt.clone();
                    let pending_answer_key = self.pending[index].answer_key.clone();
                    egui::Frame::group(ui.style())
                        .fill(egui::Color32::from_rgb(255, 248, 231))
                        .stroke(egui::Stroke::new(1.0_f32, theme::AMBER))
                        .show(ui, |ui| {
                            theme::heading(
                                ui,
                                Icon::Warning,
                                "Your response is needed",
                                theme::AMBER,
                            );
                            ui.label(&pending_prompt);
                            if pending_method.contains("requestUserInput") {
                                ui.add(
                                    egui::TextEdit::multiline(&mut self.answer)
                                        .desired_width(ui.available_width()),
                                );
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
                                ui.horizontal_wrapped(|ui| {
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
        ui.add_space((ui.available_height() - composer_height).max(0.0));
        ui.separator();
        let composer_top = ui.cursor().top();
        let connected = self.thread_id.is_some()
            && self.server.is_some()
            && !self.requests.values().any(|m| {
                matches!(
                    m.as_str(),
                    "thread/start" | "thread/resume" | "collaborationMode/list"
                )
            });
        let composer_id = ui.make_persistent_id("codex_composer");
        if connected && ui.memory(|m| m.has_focus(composer_id)) {
            let direction = ui.input_mut(|input| {
                if input.consume_key(egui::Modifiers::SHIFT, egui::Key::Tab) {
                    Some(true)
                } else if input.consume_key(egui::Modifiers::NONE, egui::Key::Tab) {
                    Some(false)
                } else {
                    None
                }
            });
            if let Some(backward) = direction {
                if !self.busy && self.pending.is_empty() && !self.modes.is_empty() {
                    let current = self
                        .modes
                        .iter()
                        .position(|mode| self.mode.as_ref() == Some(mode))
                        .or_else(|| {
                            self.modes.iter().position(|mode| {
                                mode.get("mode").and_then(Value::as_str) == Some("default")
                            })
                        })
                        .unwrap_or(0);
                    let next = if backward {
                        (current + self.modes.len() - 1) % self.modes.len()
                    } else {
                        (current + 1) % self.modes.len()
                    };
                    self.mode = Some(self.modes[next].clone());
                }
            }
        }
        let composer = ui.add_enabled(
            connected,
            egui::TextEdit::multiline(&mut self.input)
                .id(composer_id)
                .lock_focus(true)
                .hint_text("Message Codex…")
                .desired_width(f32::INFINITY)
                .desired_rows(3),
        );
        ui.horizontal_wrapped(|ui| {
            let can_send = connected && !self.busy && !self.input.trim().is_empty();
            if ui
                .add_enabled_ui(can_send, |ui| {
                    theme::button(ui, Icon::Send, "Send", theme::TEAL, true)
                })
                .inner
                .clicked()
                || (can_send
                    && composer.has_focus()
                    && ui.input_mut(|i| i.consume_key(egui::Modifiers::CTRL, egui::Key::Enter)))
            {
                self.send_turn();
            }
            if self.busy {
                if theme::button(ui, Icon::Stop, "Interrupt", theme::AMBER, false).clicked() {
                    if let (Some(thread_id), Some(turn_id)) = (&self.thread_id, &self.turn_id) {
                        let _ = self.send(
                            "turn/interrupt",
                            json!({"threadId":thread_id,"turnId":turn_id}),
                        );
                    }
                }
            }
            let selectors_enabled = connected && !self.busy && self.pending.is_empty();
            theme::wrap_control(ui, 95.0);
            ui.allocate_ui_with_layout(
                egui::vec2(95.0, 28.0),
                egui::Layout::top_down(egui::Align::Min),
                |ui| {
                    if !selectors_enabled {
                        ui.disable();
                    }
                    egui::ComboBox::from_id_salt("codex_model")
                        .width(95.0)
                        .truncate()
                        .selected_text(if self.model.is_empty() {
                            "Default model".to_string()
                        } else {
                            self.model.chars().take(14).collect::<String>()
                        })
                        .show_ui(ui, |ui| {
                            for model in &self.models {
                                if let Some(id) = model.get("model").and_then(Value::as_str) {
                                    ui.selectable_value(
                                        &mut self.model,
                                        id.to_string(),
                                        model
                                            .get("displayName")
                                            .and_then(Value::as_str)
                                            .unwrap_or(id),
                                    );
                                }
                            }
                        });
                },
            );
            let catalog = self
                .models
                .iter()
                .find(|m| m.get("model").and_then(Value::as_str) == Some(self.model.as_str()));
            self.effort = resolve_effort(catalog, self.effort.as_deref());
            let options = catalog
                .and_then(|m| m.get("supportedReasoningEfforts"))
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            theme::wrap_control(ui, 80.0);
            ui.allocate_ui_with_layout(
                egui::vec2(80.0, 28.0),
                egui::Layout::top_down(egui::Align::Min),
                |ui| {
                    if !selectors_enabled || options.is_empty() {
                        ui.disable();
                    }
                    egui::ComboBox::from_id_salt("codex_effort")
                        .width(80.0)
                        .truncate()
                        .selected_text(self.effort.as_deref().unwrap_or("Default"))
                        .show_ui(ui, |ui| {
                            for option in &options {
                                if let Some(level) =
                                    option.get("reasoningEffort").and_then(Value::as_str)
                                {
                                    ui.selectable_value(
                                        &mut self.effort,
                                        Some(level.to_string()),
                                        level,
                                    )
                                    .on_hover_text(
                                        option
                                            .get("description")
                                            .and_then(Value::as_str)
                                            .unwrap_or(""),
                                    );
                                }
                            }
                        })
                        .response
                        .on_hover_text("Thinking level");
                },
            );
            theme::wrap_control(ui, 95.0);
            ui.allocate_ui_with_layout(
                egui::vec2(95.0, 28.0),
                egui::Layout::top_down(egui::Align::Min),
                |ui| {
                    if !selectors_enabled {
                        ui.disable();
                    }
                    egui::ComboBox::from_id_salt("codex_mode")
                        .width(95.0)
                        .truncate()
                        .selected_text(
                            self.mode
                                .as_ref()
                                .and_then(|v| v.get("name"))
                                .and_then(Value::as_str)
                                .unwrap_or("Codex default"),
                        )
                        .show_ui(ui, |ui| {
                            for mode in &self.modes {
                                ui.selectable_value(
                                    &mut self.mode,
                                    Some(mode.clone()),
                                    mode.get("name").and_then(Value::as_str).unwrap_or("Mode"),
                                );
                            }
                        });
                },
            );
            ui.weak("Ctrl+Enter to send");
        });
        let measured_height = (ui.cursor().top() - composer_top + 10.0).max(120.0);
        if (measured_height - self.composer_height).abs() > 1.0 {
            self.composer_height = measured_height;
            ctx.request_repaint();
        }
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
                let _ = log; // Diagnostic logs should not replace the user-facing activity.
                continue;
            }
            if event.get("_closed").is_some() {
                self.status = "Codex App Server stopped".into();
                self.busy = false;
                self.server = None;
                self.thread_id = None;
                self.turn_id = None;
                self.pending.clear();
                self.requests.clear();
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
                    let _ = self.send("model/list", json!({}));
                    let _ = self.send("collaborationMode/list", json!({}));
                    self.refresh_usage();
                    self.list_sessions(cwd, None);
                    self.status = "Finding previous session…".into();
                } else if let Some(method) = id.as_u64().and_then(|id| self.requests.remove(&id)) {
                    if let Some(error) = error {
                        if method == "account/rateLimits/read" {
                            self.usage_error = error;
                        } else {
                            self.messages
                                .push(Message::new("Error", format!("{method}: {error}")));
                            self.status = "Request failed".into();
                        }
                        if method == "turn/start" {
                            self.busy = false;
                        }
                        continue;
                    }
                    match method.as_str() {
                        "thread/list" => {
                            if let Some(sessions) = result.get("data").and_then(Value::as_array) {
                                for session in sessions {
                                    if let Some(existing) = self
                                        .sessions
                                        .iter_mut()
                                        .find(|s| s.get("id") == session.get("id"))
                                    {
                                        *existing = session.clone();
                                    } else {
                                        self.sessions.push(session.clone());
                                    }
                                }
                            }
                            self.sessions_cursor = result
                                .get("nextCursor")
                                .and_then(Value::as_str)
                                .map(str::to_string);
                            if self.resume_latest {
                                self.resume_latest = false;
                                let previous = self
                                    .sessions
                                    .first()
                                    .and_then(|s| s.get("id"))
                                    .and_then(Value::as_str)
                                    .map(str::to_string);
                                self.open_thread(cwd, previous.as_deref());
                            }
                        }
                        "thread/start" | "thread/resume" => {
                            self.thread_id = result
                                .pointer("/thread/id")
                                .and_then(Value::as_str)
                                .map(str::to_string);
                            if let Some(model) = result.get("model").and_then(Value::as_str) {
                                self.model = model.into();
                            }
                            self.effort = result
                                .get("reasoningEffort")
                                .and_then(Value::as_str)
                                .map(str::to_string);
                            if let Some(thread) = result.get("thread") {
                                self.sessions.retain(|s| s.get("id") != thread.get("id"));
                                self.sessions.insert(0, thread.clone());
                            }
                            self.busy = false;
                            self.turn_id = None;
                            self.messages.clear();
                            self.token_usage = Value::Null;
                            if let Some(turns) =
                                result.pointer("/thread/turns").and_then(Value::as_array)
                            {
                                for turn in turns {
                                    if let Some(items) = turn.get("items").and_then(Value::as_array)
                                    {
                                        for item in items {
                                            if let Some(message) = history_message(item) {
                                                self.messages.push(message);
                                            }
                                        }
                                    }
                                    if turn.get("status").and_then(Value::as_str)
                                        == Some("inProgress")
                                    {
                                        self.busy = true;
                                        self.turn_id = turn
                                            .get("id")
                                            .and_then(Value::as_str)
                                            .map(str::to_string);
                                    }
                                }
                            }
                            self.status = if self.busy {
                                "Codex is running…"
                            } else {
                                "Ready"
                            }
                            .into();
                        }
                        "model/list" => {
                            self.models.extend(
                                result
                                    .get("data")
                                    .and_then(Value::as_array)
                                    .cloned()
                                    .unwrap_or_default(),
                            );
                            if let Some(cursor) = result.get("nextCursor").and_then(Value::as_str) {
                                let _ = self.send("model/list", json!({"cursor":cursor}));
                            }
                        }
                        "collaborationMode/list" => {
                            self.modes = result
                                .get("data")
                                .and_then(Value::as_array)
                                .cloned()
                                .unwrap_or_default();
                            self.mode = startup_mode(&self.modes);
                        }
                        "account/rateLimits/read" => {
                            self.usage = result;
                            self.usage_error.clear();
                        }
                        _ => {}
                    }
                }
                continue;
            }
            let method = event
                .get("method")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let params = event.get("params").cloned().unwrap_or(Value::Null);
            if let Some(thread) = params.get("threadId").and_then(Value::as_str) {
                if Some(thread) != self.thread_id.as_deref() {
                    continue;
                }
            }
            match method {
                "account/rateLimits/updated" => {
                    self.usage = params;
                    self.usage_error.clear();
                }
                "thread/tokenUsage/updated" => {
                    if params.get("threadId").and_then(Value::as_str) == self.thread_id.as_deref() {
                        self.token_usage = params.get("tokenUsage").cloned().unwrap_or(Value::Null);
                    }
                }
                "item/agentMessage/delta"
                | "item/plan/delta"
                | "item/reasoning/summaryTextDelta"
                | "item/reasoning/textDelta"
                | "item/commandExecution/outputDelta" => {
                    if let Some(delta) = params.get("delta").and_then(Value::as_str) {
                        let id = params.get("itemId").and_then(Value::as_str).unwrap_or("");
                        let kind = if method == "item/plan/delta" {
                            "Proposed plan"
                        } else if method.contains("reasoning") {
                            "Reasoning"
                        } else if method.contains("commandExecution") {
                            "Command"
                        } else {
                            "Codex"
                        };
                        let index = self
                            .messages
                            .iter()
                            .rposition(|m| m.id == id && m.kind == kind);
                        let index = index.unwrap_or_else(|| {
                            let mut message = Message::new(kind, String::new());
                            message.id = id.into();
                            self.messages.push(message);
                            self.messages.len() - 1
                        });
                        if kind == "Command" {
                            self.messages[index].output.push_str(delta);
                        } else {
                            self.messages[index].text.push_str(delta);
                        }
                    }
                }
                "turn/started" => {
                    self.turn_id = params
                        .pointer("/turn/id")
                        .and_then(Value::as_str)
                        .map(str::to_string);
                    self.busy = true;
                    self.status = "Codex is running…".into();
                }
                "turn/completed" => {
                    self.busy = false;
                    self.turn_id = None;
                    self.status = "Ready".into();
                    let status = params
                        .pointer("/turn/status")
                        .and_then(Value::as_str)
                        .unwrap_or("completed");
                    let marker = match status {
                        "failed" => "Response failed",
                        "interrupted" => "Response interrupted",
                        _ if self
                            .messages
                            .iter()
                            .rev()
                            .take_while(|m| m.kind != "You")
                            .any(|m| m.kind == "Proposed plan") =>
                        {
                            "Plan ready"
                        }
                        _ => "Completed !",
                    };
                    self.messages.push(Message::new("Status", marker.into()));
                    if let Some(error) = params
                        .pointer("/turn/error/message")
                        .and_then(Value::as_str)
                    {
                        self.messages.push(Message::new("Error", error.into()));
                    }
                }
                "item/started" | "item/completed" => {
                    let item = params.get("item").cloned().unwrap_or(Value::Null);
                    let kind = item
                        .get("type")
                        .and_then(Value::as_str)
                        .unwrap_or("activity");
                    let label = match kind {
                        "agentMessage" => "Codex",
                        "plan" => "Proposed plan",
                        "reasoning" => "Reasoning",
                        "commandExecution" => "Command",
                        "fileChange" => "Files",
                        _ => continue,
                    };
                    let id = item.get("id").and_then(Value::as_str).unwrap_or("");
                    let index = self
                        .messages
                        .iter()
                        .rposition(|m| m.id == id && m.kind == label);
                    let index = index.unwrap_or_else(|| {
                        let mut message = Message::new(label, String::new());
                        message.id = id.into();
                        self.messages.push(message);
                        self.messages.len() - 1
                    });
                    let message = &mut self.messages[index];
                    message.complete = method == "item/completed";
                    if let Some(text) = item
                        .get("command")
                        .or_else(|| item.get("text"))
                        .and_then(Value::as_str)
                    {
                        message.text = text.into();
                    }
                    if let Some(output) = item.get("aggregatedOutput").and_then(Value::as_str) {
                        message.output = output.into();
                    }
                    if kind == "reasoning" {
                        if let Some(summary) = item.get("summary").and_then(Value::as_array) {
                            let text = summary
                                .iter()
                                .filter_map(Value::as_str)
                                .collect::<Vec<_>>()
                                .join("\n");
                            if !text.is_empty() {
                                message.text = text;
                            }
                        }
                    }
                    if kind == "fileChange" {
                        if let Some(changes) = item.get("changes").and_then(Value::as_array) {
                            message.text = changes
                                .iter()
                                .filter_map(|c| c.get("path").and_then(Value::as_str))
                                .collect::<Vec<_>>()
                                .join("\n");
                        }
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
        self.messages.push(Message::new("You", text.clone()));
        self.input.clear();
        let mut params = json!({"threadId":thread_id,"input":[{"type":"text","text":text}]});
        if !self.model.is_empty() {
            params["model"] = json!(self.model);
        }
        params["effort"] = json!(self.effort);
        if let Some(mode) = &self.mode {
            if !self.model.is_empty() {
                params["collaborationMode"] = json!({"mode":mode["mode"],"settings":{"model":self.model,"reasoning_effort":self.effort,"developer_instructions":null}});
            }
        }
        match self.send("turn/start", params) {
            Ok(_) => {
                self.busy = true;
                self.status = "Codex is running…".into();
            }
            Err(e) => self.status = e,
        }
    }

    fn list_sessions(&mut self, cwd: Option<&Path>, cursor: Option<&str>) {
        let directory = cwd
            .map(|p| p.to_path_buf())
            .or_else(|| std::env::current_dir().ok());
        match self.send(
            "thread/list",
            json!({"cwd":directory,"limit":30,"sortKey":"updated_at","cursor":cursor}),
        ) {
            Ok(_) => {
                if cursor.is_none() {
                    self.sessions.clear();
                    self.sessions_cursor = None;
                }
            }
            Err(error) => self.status = error,
        }
    }

    fn open_thread(&mut self, cwd: Option<&Path>, previous: Option<&str>) {
        let mut params = json!({"cwd":cwd});
        if !self.model.is_empty() {
            params["model"] = json!(self.model);
        }
        let method = if let Some(id) = previous {
            params["threadId"] = json!(id);
            "thread/resume"
        } else {
            "thread/start"
        };
        match self.send(method, params) {
            Ok(_) => {
                self.status = "Opening session…".into();
            }
            Err(e) => self.status = e,
        }
    }

    fn refresh_usage(&mut self) {
        self.last_usage = Some(Instant::now());
        if let Err(e) = self.send("account/rateLimits/read", json!({})) {
            self.usage_error = e;
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

fn history_message(item: &Value) -> Option<Message> {
    let kind = item.get("type")?.as_str()?;
    let (label, text) = match kind {
        "userMessage" => (
            "You",
            item.get("content")?
                .as_array()?
                .iter()
                .filter_map(|c| c.get("text").and_then(Value::as_str))
                .collect::<Vec<_>>()
                .join("\n"),
        ),
        "agentMessage" => ("Codex", item.get("text")?.as_str()?.into()),
        "plan" => ("Proposed plan", item.get("text")?.as_str()?.into()),
        "commandExecution" => ("Command", item.get("command")?.as_str()?.into()),
        "reasoning" => (
            "Reasoning",
            item.get("summary")?
                .as_array()?
                .iter()
                .filter_map(Value::as_str)
                .collect::<Vec<_>>()
                .join("\n"),
        ),
        "fileChange" => (
            "Files",
            item.get("changes")?
                .as_array()?
                .iter()
                .filter_map(|c| c.get("path").and_then(Value::as_str))
                .collect::<Vec<_>>()
                .join("\n"),
        ),
        _ => return None,
    };
    let mut message = Message::new(label, text);
    message.id = item
        .get("id")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .into();
    message.output = item
        .get("aggregatedOutput")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .into();
    Some(message)
}

fn show_limits(ui: &mut egui::Ui, usage: &Value) {
    let snapshots: Vec<&Value> = usage
        .get("rateLimitsByLimitId")
        .and_then(Value::as_object)
        .map(|m| m.values().collect())
        .unwrap_or_else(|| usage.get("rateLimits").into_iter().collect());
    for snapshot in snapshots {
        ui.small(
            snapshot
                .get("limitName")
                .and_then(Value::as_str)
                .unwrap_or("Account limits"),
        );
        for key in ["primary", "secondary"] {
            if let Some(window) = snapshot.get(key).filter(|v| !v.is_null()) {
                let used = window
                    .get("usedPercent")
                    .and_then(Value::as_f64)
                    .unwrap_or_default();
                let minutes = window.get("windowDurationMins").and_then(Value::as_i64);
                ui.small(format!(
                    "{}: {:.0}% remaining",
                    minutes
                        .map(|m| format!("{m} min window"))
                        .unwrap_or(key.into()),
                    (100.0 - used).max(0.0)
                ));
                if let Some(reset) = window.get("resetsAt").and_then(Value::as_u64) {
                    let now = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_secs();
                    ui.small(format!(
                        "Resets in {} min",
                        reset.saturating_sub(now).div_ceil(60)
                    ));
                }
            }
        }
    }
}

fn usage_title(usage: &Value) -> String {
    let snapshot = usage.get("rateLimits");
    if let Some(used) = snapshot
        .and_then(|v| v.pointer("/primary/usedPercent"))
        .and_then(Value::as_f64)
    {
        format!("Usage / status · {:.0}% remaining", (100.0 - used).max(0.0))
    } else {
        "Usage / status".into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn thinking_levels_follow_model_capabilities() {
        let model = json!({"defaultReasoningEffort":"medium","supportedReasoningEfforts":[{"reasoningEffort":"medium"},{"reasoningEffort":"high"}]});
        assert_eq!(
            resolve_effort(Some(&model), Some("high")),
            Some("high".into())
        );
        assert_eq!(
            resolve_effort(Some(&model), Some("low")),
            Some("medium".into())
        );
        assert_eq!(resolve_effort(Some(&model), None), Some("medium".into()));
        assert_eq!(
            resolve_effort(Some(&json!({"supportedReasoningEfforts":[]})), Some("high")),
            None
        );
    }

    #[test]
    fn startup_prefers_plan_then_first_available_mode() {
        let agent = json!({"name":"Agent","mode":"default"});
        let plan = json!({"name":"Plan","mode":"plan"});
        assert_eq!(startup_mode(&[agent.clone(), plan.clone()]), Some(plan));
        assert_eq!(
            startup_mode(&[agent.clone(), json!({"mode":"other"})]),
            Some(agent)
        );
        assert_eq!(startup_mode(&[]), None);
    }

    #[test]
    fn long_messages_fit_narrow_panel() {
        let ctx = egui::Context::default();
        theme::apply(&ctx);
        let mut codex = CodexUi::default();
        codex.model = "long-model-name-".repeat(20);
        codex.messages.push(Message::new(
            "Codex",
            "long/path/without/spaces/".repeat(100),
        ));
        codex
            .messages
            .push(Message::new("Command", "command ".repeat(100)));
        codex.messages.push(Message::new(
            "Proposed plan",
            "A long proposed plan with several steps. ".repeat(60),
        ));
        for size in [
            egui::vec2(260.0, 420.0),
            egui::vec2(390.0, 600.0),
            egui::vec2(700.0, 800.0),
        ] {
            for busy in [false, true] {
                codex.busy = busy;
                for _ in 0..3 {
                    let _ = ctx.run(
                        egui::RawInput {
                            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
                            ..Default::default()
                        },
                        |ctx| {
                            egui::CentralPanel::default().show(ctx, |ui| {
                                codex.show(ui, None, ctx);
                                assert!(
                                    ui.min_rect().right() <= ui.clip_rect().right() + 1.0,
                                    "Codex content exceeds panel: {:?}",
                                    ui.min_rect()
                                );
                                let composer_id = ui.make_persistent_id("codex_composer");
                                let composer = ctx.read_response(composer_id).unwrap();
                                assert!(
                                    composer.rect.bottom() <= ui.clip_rect().bottom() + 1.0,
                                    "Composer hidden below panel: {:?}",
                                    composer.rect
                                );
                            });
                        },
                    );
                }
            }
        }
    }

    #[test]
    fn restores_user_and_command_history() {
        let user = history_message(&json!({"type":"userMessage","id":"u1","content":[{"type":"text","text":"Fix this"},{"type":"text","text":"Keep tests"}]})).unwrap();
        assert_eq!(user.text, "Fix this\nKeep tests");
        assert_eq!(user.kind, "You");
        let command = history_message(&json!({"type":"commandExecution","id":"c1","command":"cargo test","aggregatedOutput":"passed"})).unwrap();
        assert_eq!(command.id, "c1");
        assert_eq!(command.output, "passed");
        assert!(history_message(&json!({"type":"unknown"})).is_none());
    }

    #[test]
    fn usage_title_reports_remaining_quota_without_inventing_limits() {
        assert_eq!(
            usage_title(&json!({"rateLimits":{"primary":{"usedPercent":35}}})),
            "Usage / status · 65% remaining"
        );
        assert_eq!(usage_title(&Value::Null), "Usage / status");
    }
}

fn session_label(session: &Value, limit: usize, show_id: bool) -> String {
    let title = session
        .get("name")
        .and_then(Value::as_str)
        .filter(|s| !s.trim().is_empty())
        .or_else(|| {
            session
                .get("preview")
                .and_then(Value::as_str)
                .filter(|s| !s.trim().is_empty())
        })
        .unwrap_or("Untitled session");
    let title = title.split_whitespace().collect::<Vec<_>>().join(" ");
    let short: String = title.chars().take(limit).collect();
    let suffix = if title.chars().count() > limit {
        "…"
    } else {
        ""
    };
    let id: String = session
        .get("id")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .chars()
        .rev()
        .take(8)
        .collect::<String>()
        .chars()
        .rev()
        .collect();
    if show_id {
        format!("{short}{suffix} · {id}")
    } else {
        format!("{short}{suffix}")
    }
}

fn startup_mode(modes: &[Value]) -> Option<Value> {
    modes
        .iter()
        .find(|mode| mode.get("mode").and_then(Value::as_str) == Some("plan"))
        .or_else(|| modes.first())
        .cloned()
}

fn resolve_effort(model: Option<&Value>, selected: Option<&str>) -> Option<String> {
    let model = model?;
    let options = model.get("supportedReasoningEfforts")?.as_array()?;
    let supported = |value: &str| {
        options
            .iter()
            .any(|o| o.get("reasoningEffort").and_then(Value::as_str) == Some(value))
    };
    selected
        .filter(|value| supported(value))
        .or_else(|| {
            model
                .get("defaultReasoningEffort")
                .and_then(Value::as_str)
                .filter(|value| supported(value))
        })
        .or_else(|| options.first()?.get("reasoningEffort")?.as_str())
        .map(str::to_string)
}
