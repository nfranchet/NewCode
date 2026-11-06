//! Git command execution, status parsing, snapshots, and conflict classification.

use super::stats::{self, LineStats};
use crate::dev_container::Runtime;
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    process::Command,
};

#[cfg(test)]
pub(super) fn git(root: &Path, args: &[&str]) -> Result<Vec<u8>, String> {
    git_with(root, args, None)
}

pub(super) fn git_with(
    root: &Path,
    args: &[&str],
    runtime: Option<&Runtime>,
) -> Result<Vec<u8>, String> {
    let mut command = if let Some(runtime) = runtime {
        let remote_root = runtime.container_path(root)?;
        let mut command = runtime.command("git");
        command.arg("-C").arg(remote_root);
        command
    } else {
        let mut command = Command::new("git");
        command.current_dir(root);
        command
    };
    let output = command
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

#[derive(Clone, Debug, PartialEq, Eq)]
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

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Snapshot {
    pub(super) root: PathBuf,
    pub(super) branch: String,
    pub(super) files: Vec<Change>,
    pub(super) counts: [HashMap<String, LineStats>; 2],
}

#[cfg(test)]
pub(super) fn snapshot(workspace: &Path) -> Result<Snapshot, String> {
    snapshot_with(workspace, None)
}

pub(super) fn snapshot_with(
    workspace: &Path,
    runtime: Option<&Runtime>,
) -> Result<Snapshot, String> {
    let root = git_with(workspace, &["rev-parse", "--show-toplevel"], runtime)?;
    let root = PathBuf::from(String::from_utf8_lossy(&root).trim_end());
    let root = runtime.map_or(Ok(root.clone()), |runtime| runtime.host_path(&root))?;
    let branch = String::from_utf8_lossy(&git_with(
        &root,
        &["status", "--short", "--branch"],
        runtime,
    )?)
    .lines()
    .next()
    .unwrap_or("")
    .trim_start_matches("## ")
    .to_owned();
    let files = changes(&git_with(
        &root,
        &["status", "--porcelain=v1", "-z", "--untracked-files=all"],
        runtime,
    )?);
    let counts = |cached: bool| -> Result<HashMap<String, LineStats>, String> {
        let mut args = vec!["diff", "--numstat", "-z", "--find-renames"];
        if cached {
            args.push("--cached");
        }
        Ok(stats::numstat(&git_with(&root, &args, runtime)?))
    };
    let mut counts = [counts(true)?, counts(false)?];
    for change in &files {
        if change.index == '?' {
            counts[1].insert(
                change.path.clone(),
                stats::untracked(&root.join(&change.path)),
            );
        }
    }
    Ok(Snapshot {
        counts,
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
