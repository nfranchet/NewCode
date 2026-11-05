//! Concurrent draining keeps diagnostics live and prevents subprocess pipe deadlocks.
use std::io::{self, BufRead, BufReader, Read};
pub(crate) fn capture(
    reader: impl Read + Send + 'static,
    live: bool,
) -> std::thread::JoinHandle<io::Result<Vec<u8>>> {
    std::thread::spawn(move || {
        let mut result = Vec::new();
        for line in BufReader::new(reader).split(b'\n') {
            let mut line = line?;
            if live {
                super::process_line(
                    "focus_editor::dev_container::compose",
                    &String::from_utf8_lossy(&line),
                );
            }
            line.push(b'\n');
            result.extend_from_slice(&line);
            if live && result.len() > 64 * 1024 {
                result.drain(..result.len() - 64 * 1024);
            }
        }
        Ok(result)
    })
}
