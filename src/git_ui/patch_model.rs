//! Parse unified patch bytes into aligned before/after rows without changing the patch.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum LineKind {
    Context,
    Removed,
    Added,
}

#[derive(Debug)]
pub(super) struct Line {
    pub(super) number: usize,
    pub(super) text: String,
    pub(super) kind: LineKind,
    pub(super) no_newline: bool,
}

#[derive(Debug)]
pub(super) struct Row {
    pub(super) before: Option<Line>,
    pub(super) after: Option<Line>,
}

pub(super) struct Hunk {
    pub(super) title: String,
    pub(super) new_start: usize,
    pub(super) rows: Vec<Row>,
    pub(super) patch: Vec<u8>,
    pub(super) staged: bool,
}

pub(super) struct ParsedPatch {
    pub(super) hunks: Vec<Hunk>,
    pub(super) hunk_actions: bool,
    pub(super) notice: Option<String>,
}

pub(super) fn parse(bytes: &[u8], staged: bool) -> ParsedPatch {
    let lines: Vec<&[u8]> = bytes.split_inclusive(|b| *b == b'\n').collect();
    let structural = lines.iter().any(|line| {
        [
            b"new file mode ".as_slice(),
            b"deleted file mode ",
            b"old mode ",
            b"new mode ",
            b"rename from ",
            b"rename to ",
            b"copy from ",
            b"copy to ",
        ]
        .iter()
        .any(|prefix| line.starts_with(prefix))
    });
    let binary = lines
        .iter()
        .any(|l| l.starts_with(b"Binary files ") || l.starts_with(b"GIT binary patch"));
    let combined = lines
        .iter()
        .any(|l| l.starts_with(b"diff --cc ") || l.starts_with(b"diff --combined "));
    let multiple = lines
        .iter()
        .filter(|l| l.starts_with(b"diff --git "))
        .count()
        > 1;
    let starts: Vec<usize> = lines
        .iter()
        .enumerate()
        .filter_map(|(i, line)| header(line).map(|_| i))
        .collect();
    let mut hunks = Vec::new();
    if !multiple && !combined && !binary {
        for (position, &start) in starts.iter().enumerate() {
            let end = starts.get(position + 1).copied().unwrap_or(lines.len());
            let (old, new) = header(lines[start]).unwrap();
            let mut patch: Vec<u8> = lines[..starts[0]].concat();
            patch.extend_from_slice(&lines[start..end].concat());
            hunks.push(Hunk {
                title: String::from_utf8_lossy(lines[start]).trim_end().into(),
                new_start: new,
                rows: aligned_rows(&lines[start + 1..end], old, new),
                patch,
                staged,
            });
        }
    }
    let notice = if binary {
        Some("Binary file: use the whole-file Stage/Unstage action.")
    } else if combined {
        Some("Resolve this conflict in the editor, then stage the whole file.")
    } else if structural || multiple {
        Some("This change requires whole-file Stage/Unstage actions.")
    } else if hunks.is_empty() {
        Some("No text differences. Untracked files can be staged from the sidebar.")
    } else {
        None
    };
    ParsedPatch {
        hunks,
        hunk_actions: !structural && !binary && !combined && !multiple && !starts.is_empty(),
        notice: notice.map(str::to_string),
    }
}

fn header(bytes: &[u8]) -> Option<(usize, usize)> {
    if !bytes.starts_with(b"@@ ") {
        return None;
    }
    let text = std::str::from_utf8(bytes).ok()?;
    let mut words = text.split_whitespace();
    words.next()?;
    let number = |word: &str, prefix| {
        word.strip_prefix(prefix)?
            .split(',')
            .next()?
            .parse::<usize>()
            .ok()
    };
    Some((number(words.next()?, '-')?, number(words.next()?, '+')?))
}

fn aligned_rows(lines: &[&[u8]], mut old: usize, mut new: usize) -> Vec<Row> {
    let mut rows: Vec<Row> = Vec::new();
    let mut removed: Vec<Line> = Vec::new();
    let mut added: Vec<Line> = Vec::new();
    let mut previous = b' ';
    for bytes in lines {
        let Some((&prefix, text)) = bytes.split_first() else {
            continue;
        };
        if prefix == b'\\' {
            match previous {
                b'-' => {
                    if let Some(line) = removed.last_mut() {
                        line.no_newline = true;
                    }
                }
                b'+' => {
                    if let Some(line) = added.last_mut() {
                        line.no_newline = true;
                    }
                }
                b' ' => {
                    if let Some(row) = rows.last_mut() {
                        if let Some(line) = &mut row.before {
                            line.no_newline = true;
                        }
                        if let Some(line) = &mut row.after {
                            line.no_newline = true;
                        }
                    }
                }
                _ => {}
            }
            continue;
        }
        let text = text.strip_suffix(b"\n").unwrap_or(text);
        let text = String::from_utf8_lossy(text.strip_suffix(b"\r").unwrap_or(text)).into_owned();
        let line = |number, kind, text| Line {
            number,
            text,
            kind,
            no_newline: false,
        };
        match prefix {
            b'-' => {
                removed.push(line(old, LineKind::Removed, text));
                old += 1;
            }
            b'+' => {
                added.push(line(new, LineKind::Added, text));
                new += 1;
            }
            b' ' => {
                flush(&mut rows, &mut removed, &mut added);
                rows.push(Row {
                    before: Some(line(old, LineKind::Context, text.clone())),
                    after: Some(line(new, LineKind::Context, text)),
                });
                old += 1;
                new += 1;
            }
            _ => {}
        }
        previous = prefix;
    }
    flush(&mut rows, &mut removed, &mut added);
    rows
}

fn flush(rows: &mut Vec<Row>, removed: &mut Vec<Line>, added: &mut Vec<Line>) {
    let mut before = removed.drain(..);
    let mut after = added.drain(..);
    loop {
        let (before, after) = (before.next(), after.next());
        if before.is_none() && after.is_none() {
            break;
        }
        rows.push(Row { before, after });
    }
}
