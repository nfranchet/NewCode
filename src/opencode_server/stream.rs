//! SSE framing and bounded reconnects; reconnects trigger authoritative UI snapshots.
use super::{Event, Events, worker};
use reqwest::blocking::Client;
use std::{
    io::{BufRead, BufReader, Read},
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

pub(super) fn listen(
    client: Client,
    base: String,
    cwd: PathBuf,
    events: Events,
    cancel: Arc<AtomicBool>,
) {
    let mut delay = 1;
    while !cancel.load(Ordering::Relaxed) {
        let result = (|| -> Result<(), String> {
            let url = worker::endpoint(&base, "/event", &cwd)?;
            let response = worker::authorize(client.get(url))
                .send()
                .map_err(|e| e.to_string())?;
            if !response.status().is_success() {
                return Err(format!("OpenCode event stream: {}", response.status()));
            }
            events.send(Event::Reconnected);
            read(
                BufReader::new(response),
                |value| {
                    events.send(Event::Data(value));
                },
                &cancel,
            )
        })();
        if cancel.load(Ordering::Relaxed) {
            break;
        }
        events.send(Event::Disconnected(
            result
                .err()
                .unwrap_or_else(|| "OpenCode stream disconnected".into()),
        ));
        for _ in 0..delay * 10 {
            if cancel.load(Ordering::Relaxed) {
                return;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        delay = (delay * 2).min(10);
    }
}
pub(super) fn read(
    mut reader: impl BufRead,
    mut receive: impl FnMut(serde_json::Value),
    cancel: &AtomicBool,
) -> Result<(), String> {
    let mut data = String::new();
    let mut bytes = Vec::new();
    loop {
        if cancel.load(Ordering::Relaxed) {
            return Ok(());
        }
        bytes.clear();
        let count = Read::take(&mut reader, 2_097_153)
            .read_until(b'\n', &mut bytes)
            .map_err(|e| e.to_string())?;
        if count == 0 {
            return Ok(());
        }
        if bytes.len() > 2_097_152 {
            return Err("OpenCode event exceeded the size limit".into());
        }
        let line = std::str::from_utf8(&bytes)
            .map_err(|e| e.to_string())?
            .trim_end_matches(['\r', '\n']);
        if line.is_empty() {
            if !data.is_empty() {
                receive(
                    serde_json::from_str(&data)
                        .map_err(|e| format!("Invalid OpenCode event: {e}"))?,
                );
                data.clear();
            }
        } else if let Some(value) = line.strip_prefix("data:") {
            if !data.is_empty() {
                data.push('\n');
            }
            data.push_str(value.strip_prefix(' ').unwrap_or(value));
            if data.len() > 2_097_152 {
                return Err("OpenCode event exceeded the size limit".into());
            }
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn sse_handles_comments_multiline_crlf_and_utf8() {
        let bytes = b":heartbeat\r\nid: 1\r\ndata: {\"type\":\"test\",\r\ndata: \"text\":\"\xc3\xa9\"}\r\n\r\n";
        let mut events = Vec::new();
        read(
            BufReader::with_capacity(1, &bytes[..]),
            |v| events.push(v),
            &AtomicBool::new(false),
        )
        .unwrap();
        assert_eq!(events[0]["text"], "é");
        assert_eq!(events.len(), 1);
    }
}
