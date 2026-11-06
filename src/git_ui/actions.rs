//! Independent status, review-loading, and serialized mutation requests.
use super::{
    GitUi,
    fingerprint::FileVersion,
    model::Selection,
    patch::{self, PatchTask},
    presentation::Notice,
    repository::{Snapshot, git_with, snapshot_with},
    review::{DiffView, ReviewStatus, ReviewUpdate},
};
use eframe::egui;
use std::{
    sync::mpsc,
    time::{Duration, Instant},
};

const REFRESH_INTERVAL: Duration = Duration::from_secs(3);
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Ticket {
    pub repository: u64,
    pub data: u64,
}
pub(super) struct Pending<T> {
    pub(super) ticket: Ticket,
    pub(super) rx: mpsc::Receiver<T>,
    pub(super) started: Instant,
    pub(super) foreground: bool,
}
impl<T: Send + 'static> Pending<T> {
    fn spawn(
        ticket: Ticket,
        foreground: bool,
        ctx: &egui::Context,
        job: impl FnOnce() -> T + Send + 'static,
    ) -> Self {
        let (tx, rx) = mpsc::channel();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let _ = tx.send(job());
            ctx.request_repaint();
        });
        Self {
            ticket,
            rx,
            started: Instant::now(),
            foreground,
        }
    }
}
fn receive<T>(slot: &mut Option<Pending<T>>) -> Option<(Ticket, Result<T, String>)> {
    let request = slot.as_ref()?;
    let result = match request.rx.try_recv() {
        Ok(value) => Ok(value),
        Err(mpsc::TryRecvError::Empty) => return None,
        Err(mpsc::TryRecvError::Disconnected) => {
            Err("The Git worker stopped before returning a result. Retry the action.".into())
        }
    };
    let ticket = request.ticket;
    *slot = None;
    Some((ticket, result))
}

pub(super) enum Action {
    Index { paths: Vec<String>, stage: bool },
    Commit(String),
    Diff(Selection),
    Patch(PatchTask),
}
pub(super) struct RefreshReply {
    pub(super) snapshot: Result<Snapshot, String>,
    pub(super) invalidation: Option<ReviewUpdate>,
}
pub(super) struct Reply {
    pub(super) snapshot: Result<Snapshot, String>,
    pub(super) notice: Notice,
    pub(super) committed: Option<String>,
    pub(super) diff: Option<ReviewUpdate>,
}
pub(super) type ReviewReply = Result<DiffView, String>;

impl GitUi {
    pub(super) fn schedule_refresh(&mut self, ctx: &egui::Context, manual: bool) {
        if let Some(refresh) = &mut self.refresh {
            refresh.foreground |= manual;
            return;
        }
        self.refresh_requested |= manual;
        ctx.request_repaint_after(REFRESH_INTERVAL);
        if self.refresh.is_some()
            || self.busy()
            || self.review_request.is_some()
            || self.workspace.is_none()
        {
            return;
        }
        if !self.refresh_requested
            && self
                .refreshed
                .is_some_and(|time| time.elapsed() < REFRESH_INTERVAL)
        {
            return;
        }
        let foreground = std::mem::take(&mut self.refresh_requested) || self.snapshot.is_none();
        let workspace = self.workspace.clone().unwrap();
        let root = self
            .snapshot
            .as_ref()
            .map_or(workspace.clone(), |snapshot| snapshot.root.clone());
        let runtime = self.runtime.clone();
        let review = self
            .diff
            .as_ref()
            .map(|view| (view.id, view.path.clone(), view.version.clone()));
        self.refresh = Some(Pending::spawn(self.ticket(), foreground, ctx, move || {
            let invalidation = review.and_then(|(id, path, version)| {
                (FileVersion::read(&root, &path, runtime.as_ref())
                    .map_or(true, |current| current != version))
                .then(|| ReviewUpdate::Invalidated {
                    id,
                    reason: "File or index changed. Reload diff to continue.".into(),
                })
            });
            RefreshReply {
                snapshot: snapshot_with(&workspace, runtime.as_ref()),
                invalidation,
            }
        }));
    }
    pub(super) fn start(&mut self, action: Action, ctx: &egui::Context) {
        if self.busy() {
            return;
        }
        if let Action::Diff(selection) = action {
            self.load_review(selection, ctx);
            return;
        }
        let Some(workspace) = self.workspace.clone() else {
            return;
        };
        // Older status reads and diff loads cannot overwrite a mutation's resulting state.
        self.data_generation = self.data_generation.wrapping_add(1);
        self.refresh = None;
        if self.review_request.take().is_some() {
            self.review_status =
                ReviewStatus::Failed("Index changing. Reload diff to continue.".into());
        }
        let root = self
            .snapshot
            .as_ref()
            .map_or(workspace.clone(), |snapshot| snapshot.root.clone());
        let runtime = self.runtime.clone();
        let review_id = self.diff.as_ref().map(|view| view.id);
        let Ok(lease) = crate::repository_jobs::acquire(&workspace) else {
            return;
        };
        self.pending = Some(Pending::spawn(self.ticket(), true, ctx, move || {
            let _lease = lease;
            let mut diff = None;
            let mut committed = None;
            let (result, success, failure) = match action {
                Action::Index { paths, stage } => {
                    let mut args = if stage {
                        vec!["add", "--"]
                    } else {
                        vec!["reset", "-q", "--"]
                    };
                    args.extend(paths.iter().map(String::as_str));
                    (
                        git_with(&root, &args, runtime.as_ref()).map(|_| ()),
                        if stage {
                            "Files staged"
                        } else {
                            "Files unstaged"
                        },
                        if stage {
                            "Could not stage files"
                        } else {
                            "Could not unstage files"
                        },
                    )
                }
                Action::Commit(message) => {
                    let result =
                        git_with(&root, &["commit", "-m", &message], runtime.as_ref()).map(|_| ());
                    if result.is_ok() {
                        committed = Some(message);
                    }
                    (result, "Commit created", "Could not create commit")
                }
                Action::Patch(task) => {
                    let result = match patch::apply(&root, &task, runtime.as_ref()) {
                        Ok(applied) => {
                            diff = Some(ReviewUpdate::Applied(applied));
                            Ok(())
                        }
                        Err(error) => {
                            diff = Some(ReviewUpdate::Invalidated {
                                id: task.review_id,
                                reason: format!("{error} Reload diff before retrying."),
                            });
                            Err(error)
                        }
                    };
                    (
                        result,
                        if task.staged_after {
                            "Hunk staged"
                        } else {
                            "Hunk unstaged"
                        },
                        "Could not update hunk",
                    )
                }
                Action::Diff(_) => unreachable!(),
            };
            if result.is_ok() && diff.is_none() {
                diff = review_id.map(|id| ReviewUpdate::Invalidated {
                    id,
                    reason: "Index updated. Reload diff to review the current changes.".into(),
                });
            }
            let notice = match result {
                Ok(()) => Notice::success(success),
                Err(error) => Notice::error(failure, error),
            };
            Reply {
                snapshot: snapshot_with(&workspace, runtime.as_ref()),
                notice,
                committed,
                diff,
            }
        }));
    }
    fn load_review(&mut self, selection: Selection, ctx: &egui::Context) {
        let Some(workspace) = self.workspace.clone() else {
            return;
        };
        let root = self
            .snapshot
            .as_ref()
            .map_or(workspace, |snapshot| snapshot.root.clone());
        let runtime = self.runtime.clone();
        let same = self.diff_target.as_ref() == Some(&selection);
        let anchor = same
            .then(|| self.diff.as_ref().and_then(DiffView::anchor))
            .flatten();
        if !same {
            self.diff = None;
        }
        self.review_generation = self.review_generation.wrapping_add(1);
        let generation = self.review_generation;
        self.diff_hidden = false;
        self.review_status = ReviewStatus::Loading;
        self.diff_target = Some(selection.clone());
        // Replacing the receiver drops obsolete loads; their results cannot reach the current view.
        self.review_request = Some(Pending::spawn(self.ticket(), true, ctx, move || {
            DiffView::load(
                &root,
                &selection.path,
                selection.side.is_staged(),
                generation,
                anchor,
                runtime.as_ref(),
            )
        }));
    }
    fn apply_snapshot(&mut self, result: Result<Snapshot, String>) {
        self.refreshed = Some(Instant::now());
        match result {
            Ok(snapshot) => {
                if self.snapshot.as_ref() != Some(&snapshot) {
                    self.snapshot = Some(snapshot);
                }
                if let (Some(snapshot), Some(selection)) = (&self.snapshot, &mut self.diff_target) {
                    if let Some(change) = snapshot
                        .files
                        .iter()
                        .find(|change| change.path == selection.path)
                    {
                        if selection.side.is_staged()
                            && !change.has_staged()
                            && change.has_working()
                        {
                            selection.side = super::model::Side::Working;
                        } else if !selection.side.is_staged()
                            && !change.has_working()
                            && change.has_staged()
                        {
                            selection.side = super::model::Side::Staged;
                        }
                    }
                }
                if self
                    .notice
                    .as_ref()
                    .is_some_and(|notice| notice.title == "Could not refresh repository")
                {
                    self.notice = None;
                }
            }
            Err(error) => self.notice = Some(Notice::error("Could not refresh repository", error)),
        }
    }
    pub(super) fn poll_reply(&mut self) {
        if let Some((ticket, result)) = receive(&mut self.refresh) {
            if ticket == self.ticket() {
                match result {
                    Ok(reply) => {
                        self.apply_snapshot(reply.snapshot);
                        if let Some(update) = reply.invalidation {
                            self.accept_review(update);
                        }
                    }
                    Err(error) => self.apply_snapshot(Err(error)),
                }
            }
        }
        if let Some((ticket, result)) = receive(&mut self.review_request) {
            if ticket == self.ticket() {
                match result.and_then(|result| result) {
                    Ok(view) => self.accept_review(ReviewUpdate::Loaded(Box::new(view))),
                    Err(error) => self.review_status = ReviewStatus::Failed(error),
                }
            }
        }
        if let Some((ticket, result)) = receive(&mut self.pending) {
            if ticket == self.ticket() {
                match result {
                    Ok(reply) => {
                        let status_error = reply.snapshot.as_ref().err().cloned();
                        self.apply_snapshot(reply.snapshot);
                        if reply
                            .committed
                            .as_ref()
                            .is_some_and(|message| message == &self.commit)
                        {
                            self.commit.clear();
                        }
                        let mut notice = reply.notice;
                        if let Some(error) = status_error {
                            notice.details = Some(format!(
                                "{}\n\nStatus refresh failed: {error}",
                                notice.details.unwrap_or_default()
                            ));
                            if !notice.error {
                                notice.title.push_str("; status refresh failed");
                            }
                            notice.error = true;
                        }
                        self.notice = Some(notice);
                        if let Some(update) = reply.diff {
                            self.accept_review(update);
                        }
                    }
                    Err(error) => self.notice = Some(Notice::error("Git operation failed", error)),
                }
            }
        }
    }
}
