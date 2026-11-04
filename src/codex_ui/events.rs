//! Incoming event dispatch, server requests, notifications, and turn lifecycle.

use super::{
    CodexUi,
    messages::{Message, PendingRequest},
};
use crate::app_server::AppServer;
use serde_json::Value;
use std::path::Path;

impl CodexUi {
    pub(super) fn process_events(&mut self, cwd: Option<&Path>) {
        let events = self
            .server
            .as_ref()
            .map(AppServer::drain)
            .unwrap_or_default();
        for event in events {
            self.handle_event(event, cwd);
        }
    }
    pub(super) fn handle_event(&mut self, event: Value, cwd: Option<&Path>) {
        if let Some(error) = event.get("_error").and_then(Value::as_str) {
            self.status = error.into();
            return;
        }
        if let Some(log) = event.get("_log").and_then(Value::as_str) {
            let _ = log; // Diagnostic logs should not replace the user-facing activity.
            return;
        }
        if event.get("_closed").is_some() {
            self.status = "Codex App Server stopped".into();
            self.busy = false;
            self.finish_reasoning();
            self.server = None;
            self.thread_id = None;
            self.turn_id = None;
            self.pending.clear();
            self.requests.clear();
            return;
        }
        if let Some(id) = event.get("id") {
            if let Some(method) = event.get("method").and_then(Value::as_str) {
                self.handle_server_request(id, method, &event);
            } else {
                self.handle_response(id, &event, cwd);
            }
            return;
        }
        let method = event
            .get("method")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let params = event.get("params").cloned().unwrap_or(Value::Null);
        self.handle_notification(method, params);
    }
    fn handle_server_request(&mut self, id: &Value, method: &str, event: &Value) {
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
    }
    fn handle_notification(&mut self, method: &str, params: Value) {
        if let Some(thread) = params.get("threadId").and_then(Value::as_str) {
            if Some(thread) != self.thread_id.as_deref() {
                return;
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
            "item/reasoning/summaryPartAdded" => self.add_reasoning_part(&params),
            "item/agentMessage/delta"
            | "item/plan/delta"
            | "item/reasoning/summaryTextDelta"
            | "item/reasoning/textDelta"
            | "item/commandExecution/outputDelta" => self.append_delta(method, &params),
            "turn/started" => {
                self.turn_id = params
                    .pointer("/turn/id")
                    .and_then(Value::as_str)
                    .map(str::to_string);
                self.busy = true;
                self.status = "Codex is running…".into();
            }
            "turn/completed" => {
                self.finish_reasoning();
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
            "item/started" | "item/completed" => self.update_item(method, &params),
            _ => {}
        }
    }
}
