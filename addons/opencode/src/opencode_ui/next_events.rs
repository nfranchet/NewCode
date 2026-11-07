//! OpenCode's newer session event format, normalized into the same visible parts.
use super::{Message, OpenCodeUi, string};
use serde_json::{Value, json};
impl OpenCodeUi {
    pub(super) fn next_event(&mut self, kind: &str, props: &Value) {
        if kind == "session.next.step.failed" {
            self.turn_cancelled = true;
            self.busy = false;
            self.error = Some(
                props["error"]["message"]
                    .as_str()
                    .unwrap_or("OpenCode step failed")
                    .into(),
            );
            self.refresh();
            return;
        }
        if !(kind.starts_with("session.next.step.")
            || kind.starts_with("session.next.text.")
            || kind.starts_with("session.next.reasoning.")
            || kind.starts_with("session.next.tool."))
        {
            if kind == "session.next.prompted" {
                self.refresh();
            }
            return;
        }
        let id = props["assistantMessageID"]
            .as_str()
            .or_else(|| props["messageID"].as_str())
            .unwrap_or("");
        if id.is_empty() {
            return;
        }
        if !self.messages.iter().any(|m| m.id == id) {
            self.messages.push(Message{id:id.into(),info:json!({"id":id,"role":"assistant","sessionID":self.session,"agent":self.agent,"time":{"created":props["timestamp"]}}),parts:vec![]});
        }
        let message = self.messages.iter_mut().find(|m| m.id == id).unwrap();
        match kind {
            "session.next.step.started" => {
                self.busy = true;
                message.info["agent"] = props["agent"].clone();
            }
            "session.next.step.ended" => {
                message.info["time"]["completed"] = props["timestamp"].clone();
                message.info["tokens"] = props["tokens"].clone();
                message.info["cost"] = props["cost"].clone();
                self.refresh();
            }
            k if k.starts_with("session.next.text.")
                || k.starts_with("session.next.reasoning.") =>
            {
                let reasoning = k.starts_with("session.next.reasoning.");
                let part_id = props[if reasoning { "reasoningID" } else { "textID" }]
                    .as_str()
                    .unwrap_or("");
                if part_id.is_empty() {
                    return;
                }
                if !message.parts.iter().any(|p| string(p, "id") == part_id) {
                    message.parts.push(json!({"id":part_id,"type":if reasoning{"reasoning"}else{"text"},"text":""}));
                }
                let part = message
                    .parts
                    .iter_mut()
                    .find(|p| string(p, "id") == part_id)
                    .unwrap();
                if k.ends_with(".delta") {
                    part["text"] =
                        Value::String(string(part, "text").to_string() + string(props, "delta"));
                } else if k.ends_with(".ended") {
                    part["text"] = props["text"].clone();
                }
            }
            k if k.starts_with("session.next.tool.") => {
                let call = string(props, "callID");
                if call.is_empty() {
                    return;
                }
                if !message.parts.iter().any(|p| string(p, "id") == call) {
                    message.parts.push(json!({"id":call,"type":"tool","tool":props["tool"].as_str().or_else(||props["name"].as_str()).unwrap_or("Tool"),"state":{"status":"running"}}));
                }
                let part = message
                    .parts
                    .iter_mut()
                    .find(|p| string(p, "id") == call)
                    .unwrap();
                if !props["input"].is_null() {
                    part["state"]["input"] = props["input"].clone();
                }
                if k.ends_with(".success") {
                    part["state"]["status"] = json!("completed");
                    let content = props["content"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .filter_map(|p| p["text"].as_str())
                        .collect::<Vec<_>>()
                        .join("\n");
                    part["state"]["output"] = props["output"]
                        .as_str()
                        .map(|s| json!(s))
                        .unwrap_or_else(|| json!(content));
                } else if k.ends_with(".failed") {
                    part["state"]["status"] = json!("error");
                    part["state"]["error"] = props["error"]["message"]
                        .as_str()
                        .map(|s| json!(s))
                        .unwrap_or_else(|| json!("Tool failed"));
                }
            }
            _ => {}
        }
    }
}
