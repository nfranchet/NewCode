//! Debug-only Codex wire diagnostics, sanitized before the logging facade.
use serde_json::Value;
use std::sync::atomic::{AtomicU64, Ordering};
pub(super) const TARGET: &str = "focus_addon_codex::protocol";

pub(crate) fn trace(direction: &str, payload: &Value) {
    if !log::log_enabled!(target: TARGET, log::Level::Debug) {
        return;
    }
    static NEXT: AtomicU64 = AtomicU64::new(1);
    let sequence = NEXT.fetch_add(1, Ordering::Relaxed);
    let secrets = super::redaction::environment_secrets();
    for line in records(direction, sequence, payload, &secrets) {
        log::debug!(target: TARGET, "{line}");
    }
}

fn clean(value: &Value, secrets: &[String]) -> Value {
    match value {
        Value::Object(fields) => Value::Object(
            fields
                .iter()
                .map(|(key, value)| {
                    let normalized = key.to_ascii_lowercase().replace(['_', '-'], "");
                    let credential = [
                        "password",
                        "passwd",
                        "secret",
                        "apikey",
                        "credential",
                        "authorization",
                    ]
                    .iter()
                    .any(|word| normalized.contains(word))
                        || (normalized.contains("token") && value.is_string());
                    let key = super::redaction::redact(key, secrets);
                    (
                        key,
                        if credential {
                            Value::String("[redacted]".into())
                        } else {
                            clean(value, secrets)
                        },
                    )
                })
                .collect(),
        ),
        Value::Array(values) => {
            Value::Array(values.iter().map(|value| clean(value, secrets)).collect())
        }
        Value::String(text) => Value::String(super::redaction::redact(text, secrets)),
        _ => value.clone(),
    }
}

fn records(direction: &str, sequence: u64, payload: &Value, secrets: &[String]) -> Vec<String> {
    let payload = clean(payload, secrets);
    let short = |value: Option<&Value>| {
        value
            .map(Value::to_string)
            .unwrap_or("-".into())
            .chars()
            .take(80)
            .collect::<String>()
    };
    let metadata = format!(
        "Codex {direction} #{sequence} id={} method={}",
        short(payload.get("id")),
        short(payload.get("method"))
    );
    // Chunk only after complete redaction: secrets spanning a boundary cannot leak.
    let text = serde_json::to_string(&payload).unwrap();
    let chars: Vec<_> = text.chars().collect();
    let parts = chars.len().div_ceil(700);
    chars
        .chunks(700)
        .enumerate()
        .map(|(index, chars)| {
            format!(
                "{metadata} part={}/{parts} {}",
                index + 1,
                chars.iter().collect::<String>()
            )
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn chunks_preserve_payload_and_redact_before_splitting_without_mutating_wire_data() {
        let text = format!(
            "{}secret-across-boundary{}",
            "é".repeat(690),
            "tail".repeat(500)
        );
        let original = json!({"id":42,"method":"turn/start","params":{"input":text,"nested":{"api_key":"unlisted-key"},"tokenUsage":{"inputTokens":123}}});
        let lines = records("OUT", 7, &original, &["secret-across-boundary".into()]);
        assert!(lines.len() > 3);
        assert!(
            lines
                .iter()
                .all(|line| line.contains("Codex OUT #7 id=42") && line.chars().count() < 1000)
        );
        let combined = lines
            .iter()
            .map(|line| {
                line.split_once(" part=")
                    .unwrap()
                    .1
                    .split_once(' ')
                    .unwrap()
                    .1
            })
            .collect::<String>();
        let payload: Value = serde_json::from_str(&combined).unwrap();
        assert!(!combined.contains("secret-across-boundary") && !combined.contains("unlisted-key"));
        assert_eq!(payload["params"]["tokenUsage"]["inputTokens"], 123);
        assert_eq!(original["params"]["nested"]["api_key"], "unlisted-key");
        assert!(
            records("IN", 8, &json!({"id":42,"result":{"text":"reply"}}), &[])[0].contains("reply")
        );
    }
}
