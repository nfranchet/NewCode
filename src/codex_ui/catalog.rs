//! Session labels and model/mode catalog selection helpers.

use serde_json::Value;

pub(super) fn session_label(session: &Value, limit: usize, show_id: bool) -> String {
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

pub(super) fn startup_mode(modes: &[Value]) -> Option<Value> {
    modes
        .iter()
        .find(|mode| mode.get("mode").and_then(Value::as_str) == Some("plan"))
        .or_else(|| modes.first())
        .cloned()
}

pub(super) fn resolve_effort(model: Option<&Value>, selected: Option<&str>) -> Option<String> {
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
