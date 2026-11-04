//! Message data, restored-history conversion, and streamed item updates.

use super::{CodexUi, reasoning::ReasoningText};
use serde_json::Value;

pub(super) struct PendingRequest {
    pub(super) id: Value,
    pub(super) method: String,
    pub(super) prompt: String,
    pub(super) answer_key: Option<String>,
}

pub(super) struct Message {
    pub(super) id: String,
    pub(super) kind: String,
    pub(super) text: String,
    pub(super) output: String,
    pub(super) complete: bool,
    reasoning: Option<ReasoningText>,
}

impl Message {
    pub(super) fn new(kind: &str, text: String) -> Self {
        Self {
            id: String::new(),
            kind: kind.into(),
            text,
            output: String::new(),
            complete: true,
            reasoning: None,
        }
    }
}

pub(super) fn history_message(item: &Value) -> Option<Message> {
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
        "reasoning" => ("Reasoning", String::new()),
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
    if kind == "reasoning" {
        let mut reasoning = ReasoningText::default();
        reasoning.update(item);
        message.text = reasoning.text();
        message.reasoning = Some(reasoning);
    }
    Some(message)
}

impl CodexUi {
    fn item_message(&mut self, id: &str, kind: &str) -> &mut Message {
        let index = self
            .messages
            .iter()
            .rposition(|m| m.id == id && m.kind == kind)
            .unwrap_or_else(|| {
                let mut message = Message::new(kind, String::new());
                message.id = id.into();
                message.complete = false;
                self.messages.push(message);
                self.messages.len() - 1
            });
        &mut self.messages[index]
    }

    pub(super) fn add_reasoning_part(&mut self, params: &Value) {
        let id = params.get("itemId").and_then(Value::as_str).unwrap_or("");
        self.item_message(id, "Reasoning")
            .reasoning
            .get_or_insert_with(ReasoningText::default)
            .add_summary_part(params);
    }

    pub(super) fn finish_reasoning(&mut self) {
        for message in &mut self.messages {
            if message.kind == "Reasoning" {
                message.complete = true;
            }
        }
    }

    pub(super) fn append_delta(&mut self, method: &str, params: &Value) {
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
            let message = self.item_message(id, kind);
            if kind == "Command" {
                message.output.push_str(delta);
            } else if kind == "Reasoning" {
                let reasoning = message.reasoning.get_or_insert_with(ReasoningText::default);
                reasoning.append(method == "item/reasoning/summaryTextDelta", params);
                message.text = reasoning.text();
            } else {
                message.text.push_str(delta);
            }
        }
    }
    pub(super) fn update_item(&mut self, method: &str, params: &Value) {
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
            _ => return,
        };
        let id = item.get("id").and_then(Value::as_str).unwrap_or("");
        let message = self.item_message(id, label);
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
            let reasoning = message.reasoning.get_or_insert_with(ReasoningText::default);
            reasoning.update(&item);
            message.text = reasoning.text();
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
}
