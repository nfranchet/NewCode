use super::{
    GitUi,
    actions::Reply,
    repository::{Snapshot, changes, conflict, git, snapshot},
};
use crate::ui_theme as theme;
use eframe::egui;
use std::{collections::HashMap, path::PathBuf, sync::mpsc, time::Instant};
#[test]
fn commit_focus_survives_refresh_and_failure() {
    let ctx = egui::Context::default();
    theme::apply(&ctx);
    let workspace = PathBuf::from("/tmp/focus-git-focus-test");
    let make_snapshot = || Snapshot {
        root: workspace.clone(),
        branch: "main".into(),
        files: Vec::new(),
        staged: String::new(),
        unstaged: String::new(),
        counts: [HashMap::new(), HashMap::new()],
    };
    let mut git_ui = GitUi {
        workspace: Some(workspace.clone()),
        snapshot: Some(make_snapshot()),
        refreshed: Some(Instant::now()),
        ..GitUi::default()
    };
    let id = egui::Id::new(("git_commit_message", &git_ui.workspace));
    let frame = |git_ui: &mut GitUi, events: Vec<egui::Event>| {
        let _ = ctx.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(350.0, 750.0),
                )),
                events,
                ..Default::default()
            },
            |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    git_ui.ui(ui, Some(&workspace));
                });
            },
        );
    };
    frame(&mut git_ui, vec![]);
    ctx.memory_mut(|memory| memory.request_focus(id));
    frame(&mut git_ui, vec![egui::Event::Text("Draft".into())]);
    let initial_rect = ctx.read_response(id).unwrap().rect;
    let (tx, rx) = mpsc::channel();
    git_ui.pending = Some(rx);
    frame(
        &mut git_ui,
        vec![egui::Event::Text(" during refresh".into())],
    );
    assert!(ctx.memory(|memory| memory.has_focus(id)));
    assert_eq!(ctx.read_response(id).unwrap().rect, initial_rect);
    tx.send(Reply {
        snapshot: Ok(make_snapshot()),
        message: None,
        committed: false,
        diff: None,
    })
    .unwrap();
    frame(
        &mut git_ui,
        vec![egui::Event::Text(" after refresh".into())],
    );
    assert!(ctx.memory(|memory| memory.has_focus(id)));
    assert_eq!(ctx.read_response(id).unwrap().rect, initial_rect);
    let (tx, rx) = mpsc::channel();
    git_ui.pending = Some(rx);
    tx.send(Reply {
        snapshot: Err("Temporary error".into()),
        message: None,
        committed: false,
        diff: None,
    })
    .unwrap();
    frame(
        &mut git_ui,
        vec![egui::Event::Text(" after failure".into())],
    );
    assert!(ctx.memory(|memory| memory.has_focus(id)));
    assert_eq!(
        git_ui.commit,
        "Draft during refresh after refresh after failure"
    );
    assert!(git_ui.snapshot.is_some());
}

#[test]
fn parses_partial_changes_renames_and_unusual_names() {
    let files = changes(b"MM partial.rs\0R  new name\0old name\0?? tab\tline\n.txt\0UU conflict\0");
    assert_eq!(files.len(), 4);
    assert_eq!((files[0].index, files[0].work), ('M', 'M'));
    assert_eq!(files[1].old.as_deref(), Some("old name"));
    assert_eq!(files[2].path, "tab\tline\n.txt");
    assert!(conflict(&files[3]));
}

#[test]
fn repository_staging_diff_commit_and_unstage() {
    let root = std::env::temp_dir().join(format!(
        "focus-git-test-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir(&root).unwrap();
    let run = || {
        git(&root, &["init", "-q"])?;
        git(&root, &["config", "user.name", "Test"])?;
        git(&root, &["config", "user.email", "test@example.invalid"])?;
        git(&root, &["config", "commit.gpgsign", "false"])?;
        std::fs::write(root.join("file [1].txt"), "one\n").unwrap();
        assert_eq!(snapshot(&root)?.files[0].index, '?');
        git(&root, &["add", "--", "file [1].txt"])?;
        assert_eq!(snapshot(&root)?.files[0].index, 'A');
        git(&root, &["reset", "-q", "--", "file [1].txt"])?;
        assert_eq!(snapshot(&root)?.files[0].index, '?');
        git(&root, &["add", "--", "file [1].txt"])?;
        git(
            &root,
            &["-c", "core.hooksPath=/dev/null", "commit", "-qm", "initial"],
        )?;
        std::fs::write(root.join("file [1].txt"), "one\ntwo\n").unwrap();
        git(&root, &["add", "--", "file [1].txt"])?;
        std::fs::write(root.join("file [1].txt"), "one\ntwo\nthree\n").unwrap();
        let state = snapshot(&root)?;
        assert_eq!((state.files[0].index, state.files[0].work), ('M', 'M'));
        assert_eq!(state.counts[0]["file [1].txt"], "+1 −0");
        assert_eq!(state.counts[1]["file [1].txt"], "+1 −0");
        git(&root, &["reset", "-q", "--", "file [1].txt"])?;
        assert_eq!(snapshot(&root)?.files[0].index, ' ');
        Ok::<_, String>(())
    };
    let result = run();
    std::fs::remove_dir_all(&root).unwrap();
    result.unwrap();
}

mod review;
