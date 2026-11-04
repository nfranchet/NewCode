//! Identity-based streaming updates; snapshots repair any stream interruptions.
use super::{Message, OpenCodeUi, Operation, string};
use crate::opencode_server::Event;
use serde_json::Value;
impl OpenCodeUi {
    pub(super) fn process_events(&mut self) {
        for event in self.server.as_ref().map(|s| s.drain()).unwrap_or_default() {
            match event {
                Event::Ready(version) => {
                    self.version = version;
                    self.online = true;
                    self.status = "Loading OpenCode…".into();
                    self.request(Operation::Providers, "GET", "/provider".into(), None);
                    self.request(Operation::Agents, "GET", "/agent".into(), None);
                    self.list_sessions(true);
                }
                Event::Reply(id, result) => self.reply(id, result),
                Event::Data(value) => self.event(value),
                Event::Reconnected => {
                    self.stream_online = true;
                    self.refresh();
                }
                Event::Disconnected(error) => {
                    self.stream_online = false;
                    self.status = format!("Reconnecting: {error}");
                }
                Event::Closed(error) => {
                    self.stop();
                    self.error = Some(error.clone());
                    self.status = error;
                }
            }
        }
    }
    pub(super) fn merge_message(&mut self, info: Value, parts: Vec<Value>) {
        let id = string(&info, "id").to_owned();
        if id.is_empty() {
            return;
        }
        let position = self.messages.iter().position(|m| m.id == id);
        let message = if let Some(index) = position {
            &mut self.messages[index]
        } else {
            self.messages.push(Message {
                id,
                ..Default::default()
            });
            self.messages.last_mut().unwrap()
        };
        message.info = info;
        for part in parts {
            merge_part(&mut message.parts, part);
        }
        self.messages.sort_by(|a, b| {
            a.info
                .pointer("/time/created")
                .and_then(Value::as_u64)
                .unwrap_or(0)
                .cmp(
                    &b.info
                        .pointer("/time/created")
                        .and_then(Value::as_u64)
                        .unwrap_or(0),
                )
                .then(a.id.cmp(&b.id))
        });
    }
    pub(super) fn event(&mut self, event: Value) {
        log::trace!("OpenCode stream event received");
        let kind = string(&event, "type");
        let props = event
            .get("properties")
            .or_else(|| event.get("data"))
            .unwrap_or(&Value::Null);
        if matches!(
            kind,
            "session.created" | "session.updated" | "session.deleted"
        ) {
            let info = &props["info"];
            let id = string(info, "id");
            if let Some(entry) = self.sessions.iter_mut().find(|s| string(s, "id") == id) {
                *entry = info.clone();
            }
            if self.session.as_deref() == Some(id) && kind == "session.updated" {
                self.session_info = info.clone();
            }
            return;
        }
        let session = props["sessionID"]
            .as_str()
            .or_else(|| props.pointer("/part/sessionID").and_then(Value::as_str))
            .or_else(|| props.pointer("/info/sessionID").and_then(Value::as_str));
        if session != self.session.as_deref() || session.is_none() {
            return;
        }
        if let Some(id) = event["id"].as_str() {
            if self.seen_events.len() > 20_000 {
                self.seen_events.clear();
            }
            if !self.seen_events.insert(id.to_string()) {
                return;
            }
        }
        if kind.starts_with("session.next.") {
            self.next_stream = true;
            self.next_event(kind, props);
            return;
        }
        if matches!(kind, "permission.v2.asked" | "question.v2.asked") {
            let mut request = props.clone();
            request["_v2"] = Value::Bool(true);
            if kind == "permission.v2.asked" {
                upsert(&mut self.permissions, request);
            } else {
                upsert(&mut self.questions, request);
            }
            return;
        }
        match kind {
            "message.updated" => self.merge_message(props["info"].clone(), vec![]),
            "message.part.updated" => {
                let part = &props["part"];
                let id = string(part, "messageID");
                if let Some(message) = self.messages.iter_mut().find(|m| m.id == id) {
                    merge_part(&mut message.parts, part.clone());
                } else {
                    self.refresh();
                }
            }
            "message.part.delta" if !self.next_stream => {
                let id = string(props, "messageID");
                let part_id = string(props, "partID");
                if let Some(part) = self
                    .messages
                    .iter_mut()
                    .find(|m| m.id == id)
                    .and_then(|m| m.parts.iter_mut().find(|p| string(p, "id") == part_id))
                {
                    let field = string(props, "field");
                    let previous = string(part, field).to_owned();
                    part[field] = Value::String(previous + string(props, "delta"));
                } else {
                    self.refresh();
                }
            }
            "message.removed" => self.messages.retain(|m| m.id != string(props, "messageID")),
            "message.part.removed" => {
                for message in &mut self.messages {
                    message
                        .parts
                        .retain(|p| string(p, "id") != string(props, "partID"));
                }
            }
            "session.status" => self.busy = string(&props["status"], "type") != "idle",
            "session.idle" => {
                self.status = "Ready".into();
                self.busy = false;
                self.refresh();
            }
            "session.error" => {
                self.turn_cancelled = true;
                self.busy = false;
                self.status = props
                    .pointer("/error/data/message")
                    .and_then(Value::as_str)
                    .unwrap_or("OpenCode session failed")
                    .into();
                self.error = Some(self.status.clone());
                self.refresh();
            }
            "permission.asked" => upsert(&mut self.permissions, props.clone()),
            "permission.replied" | "permission.v2.replied" => self
                .permissions
                .retain(|p| string(p, "id") != string(props, "requestID")),
            "question.asked" => upsert(&mut self.questions, props.clone()),
            "question.replied"
            | "question.rejected"
            | "question.v2.replied"
            | "question.v2.rejected" => self
                .questions
                .retain(|p| string(p, "id") != string(props, "requestID")),
            _ => {}
        }
    }
}
fn merge_part(parts: &mut Vec<Value>, part: Value) {
    if let Some(old) = parts
        .iter_mut()
        .find(|p| string(p, "id") == string(&part, "id"))
    {
        *old = part;
    } else {
        parts.push(part);
    }
}
fn upsert(items: &mut Vec<Value>, value: Value) {
    if let Some(old) = items
        .iter_mut()
        .find(|p| string(p, "id") == string(&value, "id"))
    {
        *old = value;
    } else {
        items.push(value);
    }
}
