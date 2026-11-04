//! Validate and apply one retained hunk to the index, including its exact inverse.

use super::fingerprint::FileVersion;
use std::{
    io::Write,
    path::Path,
    process::{Command, Stdio},
};

pub(super) struct PatchTask {
    pub(super) review_id: u64,
    pub(super) path: String,
    pub(super) hunk: usize,
    pub(super) patch: Vec<u8>,
    pub(super) reverse: bool,
    pub(super) staged_after: bool,
    pub(super) expected: FileVersion,
}

pub(super) struct AppliedPatch {
    pub(super) review_id: u64,
    pub(super) hunk: usize,
    pub(super) staged: bool,
    pub(super) version: Option<FileVersion>,
    pub(super) warning: Option<String>,
}

pub(super) fn apply(root: &Path, task: &PatchTask) -> Result<AppliedPatch, String> {
    if FileVersion::read(root, &task.path)? != task.expected {
        return Err(
            "The file or index changed outside this review. Reload diff before continuing.".into(),
        );
    }
    let mut command = Command::new("git");
    command.current_dir(root).args(["apply", "--cached"]);
    if task.reverse {
        command.arg("--reverse");
    }
    let mut child = command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| e.to_string())?;
    let write_result = child
        .stdin
        .take()
        .ok_or("Could not open Git patch input")?
        .write_all(&task.patch);
    let output = child.wait_with_output().map_err(|e| e.to_string())?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).trim().into());
    }
    write_result.map_err(|e| e.to_string())?;
    let (version, warning) = match FileVersion::read(root, &task.path) {
        Ok(version) => {
            let warning = (!version.same_sources(&task.expected)).then(|| {
                "The file changed during the action. Reload diff before continuing.".into()
            });
            (Some(version), warning)
        }
        Err(error) => (
            None,
            Some(format!(
                "Hunk applied, but the file could not be checked: {error}. Reload diff."
            )),
        ),
    };
    Ok(AppliedPatch {
        review_id: task.review_id,
        hunk: task.hunk,
        staged: task.staged_after,
        version,
        warning,
    })
}
