use super::super::{
    actions::{Action, Pending, RefreshReply, Reply, Ticket},
    model::{Group, Selection, Side},
    presentation::Notice,
    repository::{Change, snapshot},
    review::{DiffView, ReviewStatus},
};
use super::{super::*, review::Repo};
use std::{
    sync::mpsc,
    time::{Duration, Instant},
};

fn pending<T>(ticket: Ticket) -> (mpsc::Sender<T>, Pending<T>) {
    let (tx, rx) = mpsc::channel();
    (
        tx,
        Pending {
            ticket,
            rx,
            started: Instant::now(),
            foreground: false,
        },
    )
}
fn wait(state: &mut GitUi) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while state.pending.is_some() || state.review_request.is_some() || state.refresh.is_some() {
        state.poll_reply();
        assert!(Instant::now() < deadline, "Git worker did not finish");
        std::thread::sleep(Duration::from_millis(5));
    }
}
fn initial() -> (Repo, GitUi) {
    let repo = Repo::new();
    repo.write("file", "original\n");
    repo.run(&["add", "."]);
    repo.run(&["-c", "core.hooksPath=/dev/null", "commit", "-qm", "initial"]);
    repo.write("file", "modified\n");
    let state = GitUi {
        workspace: Some(repo.0.clone()),
        snapshot: Some(snapshot(&repo.0).unwrap()),
        refreshed: Some(Instant::now()),
        ..Default::default()
    };
    (repo, state)
}

#[test]
fn one_model_classifies_partial_new_renamed_deleted_and_conflicted_files() {
    let (_, mut state) = initial();
    state.snapshot.as_mut().unwrap().files = vec![
        Change {
            path: "partial".into(),
            old: None,
            index: 'M',
            work: 'M',
        },
        Change {
            path: "new".into(),
            old: None,
            index: '?',
            work: '?',
        },
        Change {
            path: "renamed".into(),
            old: Some("old".into()),
            index: 'R',
            work: ' ',
        },
        Change {
            path: "deleted".into(),
            old: None,
            index: ' ',
            work: 'D',
        },
        Change {
            path: "conflict".into(),
            old: None,
            index: 'U',
            work: 'U',
        },
    ];
    let snapshot = state.snapshot.as_ref().unwrap();
    assert_eq!(snapshot.group(Group::Staged).len(), 2);
    assert_eq!(
        snapshot
            .group(Group::Changes)
            .iter()
            .map(|file| file.path.as_str())
            .collect::<Vec<_>>(),
        ["deleted", "new", "partial"]
    );
    assert_eq!(snapshot.group(Group::Conflicts).len(), 1);
    assert_eq!(snapshot.files[1].status(Side::Working), "New");
    assert_eq!(snapshot.files[2].paths(), ["renamed", "old"]);
    assert_eq!(
        state.commit_blocker(),
        Some("Resolve conflicts before committing")
    );
}

#[test]
fn unchanged_refresh_retains_snapshot_and_does_not_block_committing() {
    let (_repo, mut state) = initial();
    state.snapshot.as_mut().unwrap().files[0].index = 'M';
    state.commit = "Draft".into();
    let before = state.snapshot.as_ref().unwrap().files.as_ptr();
    let (tx, request) = pending(state.ticket());
    state.refresh = Some(request);
    assert!(!state.busy());
    assert!(state.commit_blocker().is_none());
    let snapshot = state.snapshot.clone().unwrap();
    tx.send(RefreshReply {
        snapshot: Ok(snapshot),
        invalidation: None,
    })
    .unwrap();
    state.poll_reply();
    assert_eq!(state.snapshot.as_ref().unwrap().files.as_ptr(), before);
    assert_eq!(state.commit, "Draft");
}

#[test]
fn mutation_supersedes_status_and_late_results_cannot_roll_back_the_index() {
    let (repo, mut state) = initial();
    let stale_ticket = state.ticket();
    let stale_snapshot = state.snapshot.clone().unwrap();
    let (tx, request) = pending(state.ticket());
    state.refresh = Some(request);
    state.start(
        Action::Index {
            paths: vec!["file".into()],
            stage: true,
        },
        &egui::Context::default(),
    );
    assert!(
        tx.send(RefreshReply {
            snapshot: Ok(stale_snapshot.clone()),
            invalidation: None
        })
        .is_err()
    );
    wait(&mut state);
    assert_eq!(state.snapshot.as_ref().unwrap().staged_count(), 1);
    let (tx, request) = pending(stale_ticket);
    state.refresh = Some(request);
    tx.send(RefreshReply {
        snapshot: Ok(stale_snapshot),
        invalidation: None,
    })
    .unwrap();
    state.poll_reply();
    assert_eq!(state.snapshot.as_ref().unwrap().staged_count(), 1);
    assert_eq!(repo.run(&["show", ":file"]), b"modified\n");
}

#[test]
fn switching_repository_rejects_old_status_and_review_replies() {
    let (repo, mut state) = initial();
    let old = state.ticket();
    let stale_snapshot = state.snapshot.clone().unwrap();
    let view = DiffView::load(&repo.0, "file", false, 1, None, None).unwrap();
    let next = Repo::new();
    state.sync_workspace(Some(&next.0), None);
    let (tx, request) = pending(old);
    state.refresh = Some(request);
    tx.send(RefreshReply {
        snapshot: Ok(stale_snapshot),
        invalidation: None,
    })
    .unwrap();
    let (tx, request) = pending(old);
    state.review_request = Some(request);
    tx.send(Ok(view)).unwrap();
    state.poll_reply();
    assert!(state.snapshot.is_none());
    assert!(state.diff.is_none());
    assert_eq!(state.workspace.as_ref(), Some(&next.0));
}

#[test]
fn refreshing_failure_retains_data_and_retry_is_coalesced() {
    let (_repo, mut state) = initial();
    let before = state.snapshot.clone().unwrap();
    let (tx, request) = pending(state.ticket());
    state.refresh = Some(request);
    state.schedule_refresh(&egui::Context::default(), true);
    state.schedule_refresh(&egui::Context::default(), true);
    assert!(!state.refresh_requested);
    tx.send(RefreshReply {
        snapshot: Err("Temporary read failure".into()),
        invalidation: None,
    })
    .unwrap();
    state.poll_reply();
    assert_eq!(state.snapshot.as_ref(), Some(&before));
    assert!(state.notice.as_ref().unwrap().error);
    assert!(!state.busy());
}

#[test]
fn commit_completion_preserves_a_new_draft_and_failed_actions_keep_the_old_one() {
    let (_repo, mut state) = initial();
    state.commit = "Next draft".into();
    let (tx, request) = pending(state.ticket());
    state.pending = Some(request);
    tx.send(Reply {
        snapshot: Ok(state.snapshot.clone().unwrap()),
        notice: Notice::success("Commit created"),
        committed: Some("Submitted draft".into()),
        diff: None,
    })
    .unwrap();
    state.poll_reply();
    assert_eq!(state.commit, "Next draft");
    let (tx, request) = pending(state.ticket());
    state.pending = Some(request);
    tx.send(Reply {
        snapshot: Ok(state.snapshot.clone().unwrap()),
        notice: Notice::error("Could not create commit", "hook rejected it"),
        committed: None,
        diff: None,
    })
    .unwrap();
    state.poll_reply();
    assert_eq!(state.commit, "Next draft");
}

#[test]
fn selecting_a_new_file_cancels_the_old_diff_load() {
    let (repo, mut state) = initial();
    repo.write("second", "new file\n");
    let (tx, request) = pending(state.ticket());
    state.review_request = Some(request);
    let view = DiffView::load(&repo.0, "file", false, 0, None, None).unwrap();
    state.start(
        Action::Diff(Selection::new("second", Side::Working)),
        &egui::Context::default(),
    );
    assert!(tx.send(Ok(view)).is_err());
    assert!(matches!(state.review_status, ReviewStatus::Loading));
    wait(&mut state);
    assert_eq!(state.diff.as_ref().unwrap().path, "second");
    assert!(
        state
            .diff
            .as_ref()
            .unwrap()
            .notice
            .as_ref()
            .unwrap()
            .contains("untracked")
    );
}
