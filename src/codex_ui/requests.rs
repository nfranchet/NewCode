//! Outgoing App Server requests for turns, sessions, and user responses.

use super::{CodexUi, messages::Message};
use serde_json::{Value, json};
use std::path::Path;

impl CodexUi {
    pub(super) fn send(&mut self, method: &str, params: Value) -> Result<u64, String> {
        let id = self
            .server
            .as_mut()
            .ok_or("Codex is offline".to_string())?
            .request(method, params)?;
        self.requests.insert(id, method.into());
        Ok(id)
    }

    pub(super) fn send_turn(&mut self) {
        let text = self.input.trim().to_string();
        let Some(thread_id) = self.thread_id.clone() else {
            return;
        };
        if text.is_empty() {
            return;
        }
        self.messages.push(Message::new("You", text.clone()));
        self.input.clear();
        let params = self.turn_params(&thread_id, &text);
        match self.send("turn/start", params) {
            Ok(_) => {
                self.busy = true;
                self.status = "Codex is running…".into();
            }
            Err(e) => self.status = e,
        }
    }

    pub(super) fn turn_params(&self, thread_id: &str, text: &str) -> Value {
        let mut params =
            json!({"threadId":thread_id,"summary":"auto","input":[{"type":"text","text":text}]});
        if !self.model.is_empty() {
            params["model"] = json!(self.model);
        }
        params["effort"] = json!(self.effort);
        if let Some(mode) = &self.mode {
            if !self.model.is_empty() {
                params["collaborationMode"] = json!({"mode":mode["mode"],"settings":{"model":self.model,"reasoning_effort":self.effort,"developer_instructions":null}});
            }
        }
        params
    }

    pub(super) fn list_sessions(&mut self, cwd: Option<&Path>, cursor: Option<&str>) {
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

    pub(super) fn open_thread(&mut self, cwd: Option<&Path>, previous: Option<&str>) {
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

    pub(super) fn respond_decision(&mut self, index: usize, decision: &str) {
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

    pub(super) fn respond_user_input(
        &mut self,
        id: Value,
        answer_key: Option<String>,
        answer: String,
    ) {
        self.pending.retain(|item| item.id != id);
        if let Some(server) = self.server.as_mut() {
            let key = answer_key.unwrap_or_else(|| "response".into());
            if let Err(e) = server.respond(id, json!({"answers":{key:{"answers":[answer]}}})) {
                self.status = e;
            }
        }
    }
}
