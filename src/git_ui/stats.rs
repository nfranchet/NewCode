//! Numeric Git diff statistics and bounded-memory line counting for new files.
use std::{
    collections::HashMap,
    fs::{self, File},
    io::{ErrorKind, Read},
    path::Path,
};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum LineStats {
    Text {
        added: u64,
        deleted: u64,
    },
    Binary,
    #[default]
    Unavailable,
}

pub(super) fn numstat(bytes: &[u8]) -> HashMap<String, LineStats> {
    let mut fields = bytes.split(|byte| *byte == 0);
    let mut counts = HashMap::new();
    while let Some(record) = fields.next() {
        let mut columns = record.splitn(3, |byte| *byte == b'\t');
        let (Some(added), Some(deleted), Some(path)) =
            (columns.next(), columns.next(), columns.next())
        else {
            continue;
        };
        let path = if path.is_empty() {
            // Rename/copy records contain a separate old path and destination path.
            let (Some(_old), Some(destination)) = (fields.next(), fields.next()) else {
                break;
            };
            destination
        } else {
            path
        };
        let stats = if added == b"-" && deleted == b"-" {
            LineStats::Binary
        } else {
            let number = |value| std::str::from_utf8(value).ok()?.parse::<u64>().ok();
            match (number(added), number(deleted)) {
                (Some(added), Some(deleted)) => LineStats::Text { added, deleted },
                _ => LineStats::Unavailable,
            }
        };
        counts.insert(String::from_utf8_lossy(path).into_owned(), stats);
    }
    counts
}

pub(super) fn untracked(path: &Path) -> LineStats {
    // Do not follow links or open special files such as pipes.
    if !fs::symlink_metadata(path).is_ok_and(|metadata| metadata.is_file()) {
        return LineStats::Unavailable;
    }
    let Ok(mut file) = File::open(path) else {
        return LineStats::Unavailable;
    };
    let mut buffer = [0; 32 * 1024];
    let mut lines = 0;
    let mut last = None;
    loop {
        let size = match file.read(&mut buffer) {
            Ok(0) => break,
            Ok(size) => size,
            Err(error) if error.kind() == ErrorKind::Interrupted => continue,
            Err(_) => return LineStats::Unavailable,
        };
        let bytes = &buffer[..size];
        if bytes.contains(&0) {
            return LineStats::Binary;
        }
        lines += bytes.iter().filter(|byte| **byte == b'\n').count() as u64;
        last = bytes.last().copied();
    }
    LineStats::Text {
        added: lines + u64::from(last.is_some_and(|byte| byte != b'\n')),
        deleted: 0,
    }
}
