//! Background Git actions and applying completed results to panel state.

use super::{
    GitUi,
    repository::{Snapshot, conflict, git_with, snapshot_with},
};
use super::{
    fingerprint::FileVersion,
    patch::{self, PatchTask},
    review::{DiffView, ReviewUpdate},
};
use eframe::egui;
use std::{sync::mpsc, time::Instant};

pub(super) enum Action {
    Refresh,
    Index { paths: Vec<String>, stage: bool },
    Commit(String),
    Diff(String, bool),
    Patch(PatchTask),
}

impl Action {
    fn label(&self) -> &'static str {
        match self {
            Self::Refresh => "refresh",
            Self::Index { stage: true, .. } => "stage",
            Self::Index { .. } => "unstage",
            Self::Commit(_) => "commit",
            Self::Diff(..) => "diff",
            Self::Patch(_) => "hunk",
        }
    }
}

pub(super) struct Reply {
    pub(super) snapshot: Result<Snapshot, String>,
    pub(super) message: Option<Result<String, String>>,
    pub(super) committed: bool,
    pub(super) diff: Option<ReviewUpdate>,
}

impl GitUi {
    pub(super) fn start(&mut self, action: Action, ctx: &egui::Context) {
        log::debug!("Git action requested action={}", action.label());
        if self.pending.is_some() {
            return;
        }
        let review = self
            .diff
            .as_ref()
            .map(|v| (v.id, v.path.clone(), v.version.clone()));
        let mut anchor = None;
        if let Action::Diff(path, staged) = &action {
            anchor = self
                .diff
                .as_ref()
                .filter(|v| &v.path == path)
                .and_then(DiffView::anchor);
            if self.diff.as_ref().is_some_and(|v| &v.path != path) {
                self.diff = None;
            }
            self.review_generation += 1;
            self.diff_hidden = false;
            self.diff_target = Some((path.clone(), *staged));
        }
        let Some(workspace) = self.workspace.clone() else {
            return;
        };
        let root = self
            .snapshot
            .as_ref()
            .map(|s| s.root.clone())
            .unwrap_or(workspace.clone());
        let runtime = self.runtime.clone();
        let generation = self.review_generation;
        let (tx, rx) = mpsc::channel();
        self.pending = Some(rx);
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let mut diff = None;
            let mut committed = false;
            let message = match action {
                Action::Refresh => {
                    if let Some((id, path, expected)) = &review {
                        if FileVersion::read(&root, path, runtime.as_ref())
                            .map_or(true, |v| v != *expected)
                        {
                            diff = Some(ReviewUpdate::Invalidated {
                                id: *id,
                                reason: "File or index changed. Reload diff to continue.".into(),
                            });
                        }
                    }
                    None
                }
                Action::Index { paths, stage } => {
                    let mut args = if stage {
                        vec!["add", "--"]
                    } else {
                        vec!["reset", "-q", "--"]
                    };
                    args.extend(paths.iter().map(String::as_str));
                    Some(git_with(&root, &args, runtime.as_ref()).map(|_| "Index updated".into()))
                }
                Action::Commit(message) => {
                    let result = git_with(&root, &["commit", "-m", &message], runtime.as_ref());
                    committed = result.is_ok();
                    Some(result.map(|v| String::from_utf8_lossy(&v).trim().into()))
                }
                Action::Patch(task) => match patch::apply(&root, &task, runtime.as_ref()) {
                    Ok(applied) => {
                        diff = Some(ReviewUpdate::Applied(applied));
                        Some(Ok(if task.staged_after {
                            "Hunk staged"
                        } else {
                            "Hunk unstaged"
                        }
                        .into()))
                    }
                    Err(error) => {
                        diff = Some(ReviewUpdate::Invalidated {
                            id: task.review_id,
                            reason: format!("{error} Reload diff before retrying."),
                        });
                        Some(Err(error))
                    }
                },
                Action::Diff(path, staged) => {
                    match DiffView::load(&root, &path, staged, generation, anchor, runtime.as_ref())
                    {
                        Ok(view) => {
                            diff = Some(ReviewUpdate::Loaded(Box::new(view)));
                            None
                        }
                        Err(error) => Some(Err(error)),
                    }
                }
            };
            if diff.is_none() && message.as_ref().is_some_and(Result::is_ok) {
                if let Some((id, _, _)) = review {
                    diff = Some(ReviewUpdate::Invalidated {
                        id,
                        reason: "Index updated. Reload diff to review the current changes.".into(),
                    });
                }
            }
            let _ = tx.send(Reply {
                committed,
                snapshot: snapshot_with(&workspace, runtime.as_ref()),
                message,
                diff,
            });
            ctx.request_repaint();
        });
    }
    pub(super) fn poll_reply(&mut self) {
        if let Some(reply) = self.pending.as_ref().and_then(|rx| rx.try_recv().ok()) {
            log::debug!(
                "Git reply snapshot_success={} action_success={}",
                reply.snapshot.is_ok(),
                reply.message.as_ref().is_none_or(|m| m.is_ok())
            );
            self.pending = None;
            self.refreshed = Some(Instant::now());
            match reply.snapshot {
                Ok(s) => self.snapshot = Some(s),
                Err(e) => {
                    log::warn!("Git status refresh failed; details available in Source Control");
                    self.message = e;
                }
            }
            if let Some(message) = reply.message {
                match message {
                    Ok(m) => {
                        if reply.committed {
                            log::info!("Git commit completed");
                            self.commit.clear();
                        }
                        self.message = m;
                    }
                    Err(e) => {
                        log::warn!("Git action failed; details available in Source Control");
                        self.message = e;
                    }
                }
            }
            if let (Some(snapshot), Some((path, staged))) = (&self.snapshot, &mut self.diff_target)
            {
                if let Some(change) = snapshot.files.iter().find(|c| &c.path == path) {
                    let has_staged =
                        change.index != ' ' && change.index != '?' && !conflict(change);
                    let has_unstaged =
                        change.work != ' ' || change.index == '?' || conflict(change);
                    if *staged && !has_staged && has_unstaged {
                        *staged = false;
                    } else if !*staged && !has_unstaged && has_staged {
                        *staged = true;
                    }
                }
            }
            if let Some(update) = reply.diff {
                self.accept_review(update);
            }
        }
    }
}
