//! Git command execution, status parsing, snapshots, and conflict classification.

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    process::Command,
};

pub(super) fn git(root: &Path, args: &[&str]) -> Result<Vec<u8>, String> {
    let output = Command::new("git")
        .current_dir(root)
        .env("GIT_LITERAL_PATHSPECS", "1")
        .args(args)
        .output()
        .map_err(|e| e.to_string())?;
    if output.status.success() {
        Ok(output.stdout)
    } else {
        Err(String::from_utf8_lossy(&output.stderr).trim().to_owned())
    }
}

#[derive(Clone, Debug)]
pub(super) struct Change {
    pub(super) path: String,
    pub(super) old: Option<String>,
    pub(super) index: char,
    pub(super) work: char,
}

pub(super) fn changes(bytes: &[u8]) -> Vec<Change> {
    let mut fields = bytes.split(|b| *b == 0).filter(|s| !s.is_empty());
    let mut result = Vec::new();
    while let Some(field) = fields.next() {
        if field.len() < 4 {
            continue;
        }
        let index = field[0] as char;
        let work = field[1] as char;
        let old = if matches!(index, 'R' | 'C') || matches!(work, 'R' | 'C') {
            fields
                .next()
                .map(|s| String::from_utf8_lossy(s).into_owned())
        } else {
            None
        };
        result.push(Change {
            path: String::from_utf8_lossy(&field[3..]).into_owned(),
            old,
            index,
            work,
        });
    }
    result
}

pub(super) struct Snapshot {
    pub(super) root: PathBuf,
    pub(super) branch: String,
    pub(super) files: Vec<Change>,
    pub(super) staged: String,
    pub(super) unstaged: String,
    pub(super) counts: [HashMap<String, String>; 2],
}

pub(super) fn snapshot(workspace: &Path) -> Result<Snapshot, String> {
    let root = git(workspace, &["rev-parse", "--show-toplevel"])?;
    let root = PathBuf::from(String::from_utf8_lossy(&root).trim_end());
    let branch = String::from_utf8_lossy(&git(&root, &["status", "--short", "--branch"])?)
        .lines()
        .next()
        .unwrap_or("")
        .trim_start_matches("## ")
        .to_owned();
    let files = changes(&git(
        &root,
        &["status", "--porcelain=v1", "-z", "--untracked-files=all"],
    )?);
    let stats = |cached: bool| -> Result<String, String> {
        let args = if cached {
            vec!["diff", "--cached", "--shortstat"]
        } else {
            vec!["diff", "--shortstat"]
        };
        Ok(String::from_utf8_lossy(&git(&root, &args)?)
            .trim()
            .to_owned())
    };
    let counts = |cached: bool| -> Result<HashMap<String, String>, String> {
        let mut args = vec!["diff", "--numstat", "-z", "--no-renames"];
        if cached {
            args.push("--cached");
        }
        let bytes = git(&root, &args)?;
        Ok(bytes
            .split(|b| *b == 0)
            .filter_map(|entry| {
                let mut parts = entry.splitn(3, |b| *b == b'\t');
                let added = String::from_utf8_lossy(parts.next()?);
                let removed = String::from_utf8_lossy(parts.next()?);
                let path = String::from_utf8_lossy(parts.next()?).into_owned();
                Some((
                    path,
                    if added == "-" {
                        "Binary".into()
                    } else {
                        format!("+{added} −{removed}")
                    },
                ))
            })
            .collect())
    };
    Ok(Snapshot {
        counts: [counts(true)?, counts(false)?],
        staged: stats(true)?,
        unstaged: stats(false)?,
        root,
        branch,
        files,
    })
}

pub(super) fn conflict(c: &Change) -> bool {
    c.index == 'U'
        || c.work == 'U'
        || (c.index == 'A' && c.work == 'A')
        || (c.index == 'D' && c.work == 'D')
}
