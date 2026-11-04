//! Bounded, line-oriented diagnostics with targeted credential/command redaction.
use regex::Regex;
use std::{process::Output, sync::LazyLock};

pub(super) fn failure(operation: &str, detail: &str) {
    for line in sanitize(detail) {
        log::warn!(target: "focus_editor::dev_container", "Dev Container {operation} failed: {line}");
    }
}

/// Keep both streams and their tails: the actual build error often follows progress.
pub(super) fn compose_failure(output: &Output) -> String {
    let mut detail = format!("Docker Compose failed ({})", output.status);
    for (name, bytes) in [
        ("stdout", output.stdout.as_slice()),
        ("stderr", output.stderr.as_slice()),
    ] {
        let text = String::from_utf8_lossy(bytes);
        if !text.trim().is_empty() {
            detail.push_str(&format!("\n{name}:\n{}", bounded_stream(text.trim())));
        }
    }
    detail
}
fn bounded_stream(text: &str) -> String {
    if text.len() <= 6000 {
        return text.into();
    }
    let mut head = 1000;
    while !text.is_char_boundary(head) {
        head -= 1;
    }
    let mut tail = text.len() - 5000;
    while !text.is_char_boundary(tail) {
        tail += 1;
    }
    format!("{}\n[output truncated]\n{}", &text[..head], &text[tail..])
}

fn redact(line: &str, secrets: &[String]) -> String {
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
fn sanitize(detail: &str) -> Vec<String> {
    let secrets: Vec<String> = std::env::vars_os()
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
        .collect();
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

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn buildkit_failure_keeps_exit_code_but_not_command_payload() {
        let line = r#"failed to solve: process "/bin/sh -c sudo apt update && echo private" did not complete successfully: exit code: 100"#;
        let result = redact(line, &[]);
        assert!(result.contains("failed to solve"));
        assert!(result.contains("exit code: 100"));
        assert!(!result.contains("sudo apt"));
        assert!(!result.contains("private"));
        assert!(
            redact(
                "executor failed running [/bin/sh -c private]: exit code: 2",
                &[]
            )
            .contains("exit code: 2")
        );
    }
    #[test]
    fn secrets_are_redacted_without_hiding_the_surrounding_error() {
        let result = redact("ERROR TOKEN=hidden: access denied", &[]);
        assert!(!result.contains("hidden"));
        assert!(result.contains("access denied"));
        assert!(!redact("Authorization: Bearer opaque-value", &[]).contains("opaque-value"));
        assert!(!redact("https://user:pass@example.test denied", &[]).contains("user:pass"));
        assert!(
            !redact("unexpected opaque-value denied", &["opaque-value".into()])
                .contains("opaque-value")
        );
        assert_eq!(
            redact("authentication token expired", &[]),
            "authentication token expired"
        );
        assert!(!redact("3 | >>> RUN echo private", &[]).contains("echo private"));
        assert!(!redact("4 | >>> sudo apt install private", &[]).contains("install private"));
    }
    #[test]
    fn diagnostics_are_line_oriented_and_bounded() {
        let result = sanitize("first error\nsecond error");
        assert_eq!(result, ["first error", "second error"]);
        assert!(
            sanitize(&"a".repeat(100_000))
                .iter()
                .map(String::len)
                .sum::<usize>()
                <= 16_100
        );
        let output = bounded_stream(&format!("{}\nactual failure at end", "é".repeat(40_000)));
        assert!(output.contains("actual failure at end"));
        assert!(output.contains("truncated"));
    }
    #[cfg(unix)]
    #[test]
    fn compose_failure_keeps_both_streams_and_status_including_stdout_only_errors() {
        use std::os::unix::process::ExitStatusExt;
        let mut output = Output {
            status: std::process::ExitStatus::from_raw(1 << 8),
            stdout: b"Unable to locate package example".to_vec(),
            stderr: b"Dockerfile:3".to_vec(),
        };
        let detail = compose_failure(&output);
        assert!(detail.contains("exit status: 1"));
        assert!(detail.contains("stdout:\nUnable to locate"));
        assert!(detail.contains("stderr:\nDockerfile:3"));
        output.stderr.clear();
        assert!(compose_failure(&output).contains("Unable to locate"));
    }
}
