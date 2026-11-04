//! Stable review snapshots, hunk state, and generation-checked background updates.

use super::{
    GitUi,
    fingerprint::FileVersion,
    patch::{AppliedPatch, PatchTask},
    patch_model::{self, Hunk},
    repository::git,
};
use std::path::Path;

pub(super) struct DiffView {
    pub(super) id: u64,
    pub(super) path: String,
    pub(super) original_staged: bool,
    pub(super) version: FileVersion,
    pub(super) hunks: Vec<Hunk>,
    pub(super) hunk_actions: bool,
    pub(super) notice: Option<String>,
    pub(super) stale: Option<String>,
    pub(super) selected_hunk: Option<usize>,
    pub(super) scroll_to: Option<usize>,
}

pub(super) enum ReviewUpdate {
    Loaded(Box<DiffView>),
    Applied(AppliedPatch),
    Invalidated { id: u64, reason: String },
}

impl DiffView {
    pub(super) fn load(
        root: &Path,
        path: &str,
        staged: bool,
        id: u64,
        anchor: Option<usize>,
    ) -> Result<Self, String> {
        let version = FileVersion::read(root, path)?;
        let mut args = vec![
            "diff",
            "--no-color",
            "--no-ext-diff",
            "--no-textconv",
            "--src-prefix=a/",
            "--dst-prefix=b/",
            "--no-relative",
        ];
        if staged {
            args.push("--cached");
        }
        args.extend(["--", path]);
        let bytes = git(root, &args)?;
        if FileVersion::read(root, path)? != version {
            return Err("The file changed while loading its diff. Reload diff.".into());
        }
        let mut parsed = patch_model::parse(&bytes, staged);
        if version.is_symlink() {
            parsed.hunk_actions = false;
            parsed.notice = Some("Symbolic link: use whole-file Stage/Unstage actions.".into());
        }
        let selected_hunk = anchor.and_then(|line| {
            parsed
                .hunks
                .iter()
                .enumerate()
                .min_by_key(|(_, h)| h.new_start.abs_diff(line))
                .map(|(i, _)| i)
        });
        Ok(Self {
            id,
            path: path.into(),
            original_staged: staged,
            version,
            hunks: parsed.hunks,
            hunk_actions: parsed.hunk_actions,
            notice: parsed.notice,
            stale: None,
            selected_hunk,
            scroll_to: selected_hunk,
        })
    }

    pub(super) fn patch_task(&self, hunk: usize) -> Result<PatchTask, String> {
        if let Some(error) = &self.stale {
            return Err(error.clone());
        }
        if !self.hunk_actions {
            return Err("Use the whole-file Stage/Unstage action for this change.".into());
        }
        let change = self.hunks.get(hunk).ok_or("Hunk is no longer available")?;
        Ok(PatchTask {
            review_id: self.id,
            path: self.path.clone(),
            hunk,
            patch: change.patch.clone(),
            reverse: change.staged,
            staged_after: !change.staged,
            expected: self.version.clone(),
        })
    }

    pub(super) fn anchor(&self) -> Option<usize> {
        self.selected_hunk
            .and_then(|i| self.hunks.get(i))
            .map(|h| h.new_start)
    }
}

impl GitUi {
    pub(super) fn accept_review(&mut self, update: ReviewUpdate) {
        match update {
            ReviewUpdate::Loaded(view) if view.id == self.review_generation => {
                self.diff = Some(*view)
            }
            ReviewUpdate::Applied(applied) => {
                if let Some(view) = self
                    .diff
                    .as_mut()
                    .filter(|view| view.id == applied.review_id)
                {
                    if let Some(hunk) = view.hunks.get_mut(applied.hunk) {
                        hunk.staged = applied.staged;
                    }
                    if let Some(version) = applied.version {
                        view.version = version;
                    }
                    view.stale = applied.warning;
                    view.selected_hunk = Some(applied.hunk);
                }
            }
            ReviewUpdate::Invalidated { id, reason } => {
                if let Some(view) = self.diff.as_mut().filter(|view| view.id == id) {
                    view.stale = Some(reason);
                }
            }
            _ => {}
        }
    }
}
