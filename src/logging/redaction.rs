//! Shared diagnostic redaction before any UI or file sink.
use regex::Regex;
use std::sync::LazyLock;
pub(crate) fn redact(line: &str, secrets: &[String]) -> String {
    static PROCESS: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r#"\bprocess\s+"(?:\\.|[^"\\])*""#).unwrap());
    static SHELL: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"\[(?:/bin/)?(?:ba)?sh\s+-c\b[^\]]*\]").unwrap());
    static INSTRUCTION: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?i)(?:^\s*(?:RUN|CMD|ENTRYPOINT|ENV|ARG)\s+|\]\s+(?:RUN|CMD|ENTRYPOINT|ENV|ARG)\s+|>>>\s*).*").unwrap()
    });
    static CREDENTIAL: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r#"(?i)(\b(?:[a-z0-9_]*(?:password|passwd|token|secret|api_key|api-key|credential)[a-z0-9_]*|authorization)\b["']?\s*[:=]\s*)(?:"(?:\\.|[^"\\])*"|'[^']*'|[^\s,;]+)"#).unwrap()
    });
    static AUTHORIZATION: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?i)(authorization\s*[:=]\s*)(?:bearer|basic)\s+[^\s,;]+").unwrap()
    });
    static URL_AUTH: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"([a-zA-Z][a-zA-Z0-9+.-]*://)[^\s/@]+@").unwrap());
    let mut text = line.to_owned();
    for secret in secrets {
        text = text.replace(secret, "[redacted]");
    }
    text = PROCESS
        .replace_all(&text, "process \"[command omitted]\"")
        .into_owned();
    text = SHELL.replace_all(&text, "[command omitted]").into_owned();
    text = INSTRUCTION
        .replace_all(&text, "[build instruction omitted]")
        .into_owned();
    text = AUTHORIZATION
        .replace_all(&text, "${1}[redacted]")
        .into_owned();
    text = CREDENTIAL.replace_all(&text, "${1}[redacted]").into_owned();
    URL_AUTH.replace_all(&text, "${1}[redacted]@").into_owned()
}
pub(super) fn environment_secrets() -> Vec<String> {
    std::env::vars_os()
        .filter_map(|(key, value)| {
            let key = key.to_string_lossy().to_ascii_uppercase();
            [
                "TOKEN",
                "PASSWORD",
                "SECRET",
                "API_KEY",
                "CREDENTIAL",
                "AUTH",
            ]
            .iter()
            .any(|word| key.contains(word))
            .then(|| value.to_string_lossy().into_owned())
            .filter(|value| value.len() >= 4)
        })
        .collect()
}
pub(crate) fn sanitize(detail: &str) -> Vec<String> {
    let secrets = environment_secrets();
    let mut lines = Vec::new();
    let mut size = 0;
    for line in detail.lines().filter(|line| !line.trim().is_empty()) {
        let line = redact(line, &secrets);
        let mut text: String = line
            .chars()
            .filter(|ch| !ch.is_control() || *ch == '\t')
            .take(1000)
            .collect();
        if line.chars().count() > 1000 {
            text.push_str(" [line truncated]");
        }
        if size + text.len() > 16_000 {
            lines.push("[diagnostics truncated]".into());
            break;
        }
        size += text.len();
        lines.push(text);
    }
    if lines.is_empty() {
        lines.push("No diagnostic details supplied".into());
    }
    lines
}
