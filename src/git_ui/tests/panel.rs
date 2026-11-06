use super::super::{
    GitUi,
    actions::{Pending, RefreshReply},
    model::{Selection, Side},
    repository::{Change, Snapshot},
    review::ReviewStatus,
};
use eframe::egui;
use std::{collections::HashMap, path::PathBuf, sync::mpsc, time::Instant};

fn panel() -> GitUi {
    let root = PathBuf::from("/tmp/focus-layout-repository");
    GitUi {
        workspace: Some(root.clone()),
        snapshot: Some(Snapshot {
            root,
            branch: "main...origin/main".into(),
            files: vec![Change {
                path: "nested/a_very_long_filename_with_changes.rs".into(),
                old: None,
                index: 'M',
                work: 'M',
            }],
            counts: [HashMap::new(), HashMap::new()],
        }),
        commit: "Draft".into(),
        refreshed: Some(Instant::now()),
        ..Default::default()
    }
}
fn frame(state: &mut GitUi, ctx: &egui::Context, width: f32) -> egui::FullOutput {
    frame_events(state, ctx, width, Vec::new())
}
fn frame_events(
    state: &mut GitUi,
    ctx: &egui::Context,
    width: f32,
    events: Vec<egui::Event>,
) -> egui::FullOutput {
    let workspace = state.workspace.clone();
    ctx.run(
        egui::RawInput {
            events,
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(width, 600.0),
            )),
            ..Default::default()
        },
        |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                state.ui(ui, workspace.as_deref(), None);
            });
            assert!(
                ctx.used_rect().right() <= width + 1.0,
                "panel exceeds width: {:?}",
                ctx.used_rect()
            );
        },
    )
}
#[test]
fn narrow_layout_and_commit_position_stay_stable_during_refresh_and_selection() {
    for width in [150.0, 250.0, 500.0] {
        let ctx = egui::Context::default();
        crate::ui_theme::apply(&ctx);
        let mut state = panel();
        frame(&mut state, &ctx, width);
        let id = egui::Id::new(("git_commit_message", &state.workspace));
        ctx.memory_mut(|memory| memory.request_focus(id));
        frame(&mut state, &ctx, width);
        let before = ctx.read_response(id).unwrap().rect;
        let (tx, rx) = mpsc::channel();
        state.refresh = Some(Pending {
            ticket: state.ticket(),
            rx,
            started: Instant::now(),
            foreground: false,
        });
        state.diff_target = Some(Selection::new(
            "nested/a_very_long_filename_with_changes.rs",
            Side::Working,
        ));
        for _ in 0..4 {
            frame(&mut state, &ctx, width);
        }
        assert_eq!(ctx.read_response(id).unwrap().rect, before);
        assert!(ctx.memory(|memory| memory.has_focus(id)));
        assert!(state.commit_blocker().is_none());
        tx.send(RefreshReply {
            snapshot: Ok(state.snapshot.clone().unwrap()),
            invalidation: None,
        })
        .unwrap();
        frame(&mut state, &ctx, width);
        assert_eq!(ctx.read_response(id).unwrap().rect, before);
        assert_eq!(state.commit, "Draft");
    }
}
#[test]
fn loading_and_failed_reviews_keep_the_center_pane_owned() {
    let mut state = panel();
    state.diff_target = Some(Selection::new("file.rs", Side::Working));
    let ctx = egui::Context::default();
    for status in [
        ReviewStatus::Loading,
        ReviewStatus::Failed("Diff failed; retry loading it".into()),
    ] {
        state.review_status = status;
        let _ = ctx.run(Default::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                assert!(state.show_diff(ui));
            });
        });
    }
    state.dismiss_diff();
    let _ = ctx.run(Default::default(), |ctx| {
        egui::CentralPanel::default().show(ctx, |ui| {
            assert!(!state.show_diff(ui));
        });
    });
}

#[test]
fn filename_clicks_load_reviews_and_space_stages_without_repeating() {
    use super::super::repository::snapshot;
    use super::review::Repo;
    use std::time::Duration;
    let repo = Repo::new();
    repo.write("file.rs", "old\n");
    repo.run(&["add", "."]);
    repo.run(&["-c", "core.hooksPath=/dev/null", "commit", "-qm", "fixture"]);
    repo.write("file.rs", "new\n");
    let ctx = egui::Context::default();
    crate::ui_theme::apply(&ctx);
    let mut state = GitUi {
        workspace: Some(repo.0.clone()),
        snapshot: Some(snapshot(&repo.0).unwrap()),
        refreshed: Some(Instant::now()),
        ..Default::default()
    };
    frame(&mut state, &ctx, 250.0);
    let selection = Selection::new("file.rs", Side::Working);
    let row_id = egui::Id::new(("git_file_row", &state.workspace, &selection));
    let pos = ctx.read_response(row_id).unwrap().rect.min + egui::vec2(12.0, 9.0);
    let pointer = |pressed| egui::Event::PointerButton {
        pos,
        pressed,
        button: egui::PointerButton::Primary,
        modifiers: egui::Modifiers::NONE,
    };
    frame_events(
        &mut state,
        &ctx,
        250.0,
        vec![egui::Event::PointerMoved(pos), pointer(true)],
    );
    frame_events(&mut state, &ctx, 250.0, vec![pointer(false)]);
    assert_eq!(state.diff_target, Some(selection));
    let wait = |state: &mut GitUi| {
        let deadline = Instant::now() + Duration::from_secs(3);
        while state.pending.is_some() || state.review_request.is_some() {
            state.poll_reply();
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(5));
        }
    };
    wait(&mut state);
    assert_eq!(state.diff.as_ref().unwrap().path, "file.rs");
    let space = |repeat| egui::Event::Key {
        key: egui::Key::Space,
        physical_key: Some(egui::Key::Space),
        pressed: true,
        repeat,
        modifiers: egui::Modifiers::NONE,
    };
    frame_events(&mut state, &ctx, 250.0, vec![space(false)]);
    assert!(state.pending.is_some());
    wait(&mut state);
    assert_eq!(state.snapshot.as_ref().unwrap().staged_count(), 1);
    frame(&mut state, &ctx, 250.0);
    let staged = Selection::new("file.rs", Side::Staged);
    let row_id = egui::Id::new(("git_file_row", &state.workspace, &staged));
    ctx.memory_mut(|memory| memory.request_focus(row_id));
    frame_events(&mut state, &ctx, 250.0, vec![space(true)]);
    assert!(state.pending.is_none(), "key repeat toggled the file back");
    frame_events(
        &mut state,
        &ctx,
        250.0,
        vec![egui::Event::Key {
            key: egui::Key::Space,
            physical_key: Some(egui::Key::Space),
            pressed: false,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        }],
    );
    frame_events(&mut state, &ctx, 250.0, vec![space(false)]);
    wait(&mut state);
    assert_eq!(state.snapshot.as_ref().unwrap().staged_count(), 0);
}
