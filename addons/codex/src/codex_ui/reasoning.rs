//! Readable reasoning summaries and content, kept in separate indexed sections.

use serde_json::Value;
use std::collections::BTreeMap;

#[derive(Default)]
pub(super) struct ReasoningText {
    summary: BTreeMap<i64, String>,
    content: BTreeMap<i64, String>,
}

impl ReasoningText {
    pub(super) fn add_summary_part(&mut self, params: &Value) {
        let index = params
            .get("summaryIndex")
            .and_then(Value::as_i64)
            .unwrap_or(0);
        self.summary.entry(index).or_default();
    }

    pub(super) fn append(&mut self, summary: bool, params: &Value) {
        let Some(delta) = params.get("delta").and_then(Value::as_str) else {
            return;
        };
        let (parts, key) = if summary {
            (&mut self.summary, "summaryIndex")
        } else {
            (&mut self.content, "contentIndex")
        };
        let index = params.get(key).and_then(Value::as_i64).unwrap_or(0);
        parts.entry(index).or_default().push_str(delta);
    }

    pub(super) fn update(&mut self, item: &Value) {
        for (key, parts) in [
            ("summary", &mut self.summary),
            ("content", &mut self.content),
        ] {
            let Some(values) = item.get(key).and_then(Value::as_array) else {
                continue;
            };
            let snapshot: BTreeMap<_, _> = values
                .iter()
                .enumerate()
                .filter_map(|(index, value)| {
                    // Only readable text is accepted, never encrypted/opaque fields.
                    let text = value
                        .as_str()
                        .or_else(|| value.get("text").and_then(Value::as_str))?;
                    Some((index as i64, text.to_string()))
                })
                .collect();
            // Empty completion snapshots must not erase text already streamed.
            if snapshot.values().any(|text| !text.trim().is_empty()) {
                *parts = snapshot;
            }
        }
    }

    pub(super) fn text(&self) -> String {
        let summary = join_sections(&self.summary);
        if summary.is_empty() {
            join_sections(&self.content)
        } else {
            summary
        }
    }
}

fn join_sections(parts: &BTreeMap<i64, String>) -> String {
    parts
        .values()
        .filter(|text| !text.trim().is_empty())
        .map(String::as_str)
        .collect::<Vec<_>>()
        .join("\n\n")
}
