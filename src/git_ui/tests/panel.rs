use super::super::{
    GitUi,
    actions::{Pending, RefreshReply},
    model::{Selection, Side},
    repository::{Change, Snapshot},
    review::ReviewStatus,
    stats::LineStats,
};
use crate::ui_theme as theme;
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
    crate::test_frame(
        ctx,
        egui::RawInput {
            events,
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(width, 600.0),
            )),
            ..Default::default()
        },
        |root| {
            egui::CentralPanel::default().show(root, |ui| {
                state.ui(ui, workspace.as_deref(), None);
            });
            assert!(
                root.ctx().globally_used_rect().right() <= width + 1.0,
                "panel exceeds width: {:?}",
                root.ctx().globally_used_rect()
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
        let _ = crate::test_frame(&ctx, Default::default(), |root| {
            egui::CentralPanel::default().show(root, |ui| {
                assert!(state.show_diff(ui));
            });
        });
    }
    state.dismiss_diff();
    let _ = crate::test_frame(&ctx, Default::default(), |root| {
        egui::CentralPanel::default().show(root, |ui| {
            assert!(!state.show_diff(ui));
        });
    });
}

fn text_shapes<'a>(shape: &'a egui::Shape, texts: &mut Vec<&'a egui::epaint::TextShape>) {
    match shape {
        egui::Shape::Text(text) => texts.push(text),
        egui::Shape::Vec(shapes) => {
            for shape in shapes {
                text_shapes(shape, texts);
            }
        }
        _ => {}
    }
}

#[test]
fn compact_rows_have_four_columns_with_aligned_green_counts() {
    for width in [150.0, 250.0, 500.0] {
        let ctx = egui::Context::default();
        crate::ui_theme::apply(&ctx);
        let mut state = panel();
        let path = state.snapshot.as_ref().unwrap().files[0].path.clone();
        let mut same_name = state.snapshot.as_ref().unwrap().files[0].clone();
        same_name.path = path.replacen("nested/", "other/", 1);
        let other_path = same_name.path.clone();
        state.snapshot.as_mut().unwrap().files.push(same_name);
        for (side, added, deleted) in [(Side::Staged, 12, 3), (Side::Working, 1, 22)] {
            state.snapshot.as_mut().unwrap().counts[side.counts_index()]
                .insert(path.clone(), LineStats::Text { added, deleted });
        }
        frame(&mut state, &ctx, width);
        let output = frame(&mut state, &ctx, width);
        let mut texts = Vec::new();
        for shape in &output.shapes {
            text_shapes(&shape.shape, &mut texts);
        }
        assert!(texts.iter().any(|text| text.galley.job.text == other_path));
        let mut previous_rights = None;
        for (side, added, deleted) in [(Side::Staged, "+12", "-3"), (Side::Working, "+1", "-22")] {
            let id = egui::Id::new((
                "git_file_row",
                &state.workspace,
                Selection::new(&path, side),
            ));
            let row = ctx.read_response(id).unwrap().rect;
            assert_eq!(row.height(), 28.0);
            let labels: Vec<_> = texts
                .iter()
                .copied()
                .filter(|text| row.contains(text.visual_bounding_rect().center()))
                .collect();
            assert_eq!(labels.len(), 4);
            assert_eq!(
                labels
                    .iter()
                    .map(|text| text.galley.job.text.as_str())
                    .collect::<Vec<_>>(),
                ["M", path.as_str(), added, deleted]
            );
            for label in &labels {
                let bounds = label.visual_bounding_rect();
                assert!(bounds.left() >= row.left() && bounds.right() <= row.right());
            }
            for label in &labels[2..] {
                assert!(
                    label
                        .galley
                        .job
                        .sections
                        .iter()
                        .all(|section| section.format.color == theme::palette(&ctx).success)
                );
            }
            let rights = [
                labels[2].pos.x + labels[2].galley.rect.right(),
                labels[3].pos.x + labels[3].galley.rect.right(),
            ];
            if let Some(previous) = previous_rights {
                assert_eq!(rights, previous);
            }
            previous_rights = Some(rights);
        }
        // Huge counts must also stay inside the narrowest supported panel.
        for counts in &mut state.snapshot.as_mut().unwrap().counts {
            counts.insert(
                path.clone(),
                LineStats::Text {
                    added: u64::MAX,
                    deleted: u64::MAX,
                },
            );
        }
        frame(&mut state, &ctx, width);
    }
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
    let pos = ctx.read_response(row_id).unwrap().rect.min + egui::vec2(40.0, 9.0);
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
    let count_pos = ctx.read_response(row_id).unwrap().rect.right_center() - egui::vec2(12.0, 0.0);
    for pressed in [true, false] {
        frame_events(
            &mut state,
            &ctx,
            250.0,
            vec![
                egui::Event::PointerMoved(count_pos),
                egui::Event::PointerButton {
                    pos: count_pos,
                    pressed,
                    button: egui::PointerButton::Primary,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
        );
    }
    wait(&mut state);
    assert_eq!(
        state.snapshot.as_ref().unwrap().staged_count(),
        0,
        "clicking a count staged the file"
    );
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

#[test]
fn double_click_moves_new_and_partial_files_between_the_correct_groups() {
    use super::super::{model::Group, repository::snapshot};
    use super::review::Repo;
    use std::time::Duration;
    let repo = Repo::new();
    repo.write("partial", "one\ntwo\n");
    repo.run(&["add", "."]);
    repo.run(&["-c", "core.hooksPath=/dev/null", "commit", "-qm", "fixture"]);
    repo.write("partial", "staged\ntwo\n");
    repo.run(&["add", "partial"]);
    repo.write("partial", "staged\nworking\n");
    repo.write("new", "new content\n");
    let mut state = GitUi {
        workspace: Some(repo.0.clone()),
        snapshot: Some(snapshot(&repo.0).unwrap()),
        refreshed: Some(Instant::now()),
        ..Default::default()
    };
    let wait = |state: &mut GitUi| {
        let deadline = Instant::now() + Duration::from_secs(3);
        while state.pending.is_some() || state.review_request.is_some() {
            state.poll_reply();
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(5));
        }
    };
    for (path, side, expected_group) in [
        ("new", Side::Working, Group::Staged),
        ("new", Side::Staged, Group::Untracked),
        ("partial", Side::Working, Group::Staged),
        ("partial", Side::Staged, Group::Changes),
    ] {
        let ctx = egui::Context::default();
        crate::ui_theme::apply(&ctx);
        frame(&mut state, &ctx, 350.0);
        let id = egui::Id::new(("git_file_row", &state.workspace, Selection::new(path, side)));
        let pos = ctx.read_response(id).unwrap().rect.min + egui::vec2(40.0, 9.0);
        for click in 0..2 {
            for pressed in [true, false] {
                frame_events(
                    &mut state,
                    &ctx,
                    350.0,
                    vec![
                        egui::Event::PointerMoved(pos),
                        egui::Event::PointerButton {
                            pos,
                            pressed,
                            button: egui::PointerButton::Primary,
                            modifiers: egui::Modifiers::NONE,
                        },
                    ],
                );
            }
            if click == 0 {
                assert!(state.pending.is_none(), "single click changed the index");
            }
        }
        assert!(
            state.pending.is_some(),
            "double click did not start an index operation: {path} {side:?}"
        );
        wait(&mut state);
        let snapshot = state.snapshot.as_ref().unwrap();
        assert!(
            snapshot
                .group(expected_group)
                .iter()
                .any(|file| file.path == path)
        );
        assert!(
            !snapshot
                .group(if expected_group == Group::Staged {
                    Group::Changes
                } else {
                    Group::Staged
                })
                .iter()
                .any(|file| file.path == path)
        );
    }
}
