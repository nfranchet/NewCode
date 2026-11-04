//! Correlated response handling for initialization, sessions, catalogs, and usage.

use super::{
    CodexUi,
    catalog::startup_mode,
    messages::{Message, history_message},
};
use serde_json::{Value, json};
use std::path::Path;

impl CodexUi {
    pub(super) fn handle_response(&mut self, id: &Value, event: &Value, cwd: Option<&Path>) {
        let result = event.get("result").cloned().unwrap_or(Value::Null);
        let error = event
            .pointer("/error/message")
            .and_then(Value::as_str)
            .map(str::to_string);
        if self.initializing_id == id.as_u64() {
            self.initializing_id = None;
            if let Some(error) = error {
                self.status = format!("Initialize failed: {error}");
                return;
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
                return;
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
                    if let Some(turns) = result.pointer("/thread/turns").and_then(Value::as_array) {
                        for turn in turns {
                            if let Some(items) = turn.get("items").and_then(Value::as_array) {
                                for item in items {
                                    if let Some(message) = history_message(item) {
                                        self.messages.push(message);
                                    }
                                }
                            }
                            if turn.get("status").and_then(Value::as_str) == Some("inProgress") {
                                self.busy = true;
                                self.turn_id =
                                    turn.get("id").and_then(Value::as_str).map(str::to_string);
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
    }
}
