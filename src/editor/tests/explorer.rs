use super::test_app;
use crate::{
    editor::{EditorApp, SidebarMode, deletion::Deletion},
    ui_theme as theme,
};
use eframe::egui;
use std::fs;

#[test]
fn folder_context_menu_opens_delete_confirmation_when_closed_or_open() {
    let root = std::env::temp_dir().join(format!(
        "focus-folder-menu-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let folder = root.join("folder");
    fs::create_dir_all(&folder).unwrap();
    fs::write(folder.join("child.txt"), "keep").unwrap();
    for expanded in [false, true] {
        let ctx = egui::Context::default();
        theme::apply(&ctx);
        let mut app = test_app();
        let frame = |app: &mut EditorApp, events: Vec<egui::Event>| {
            let _ = ctx.run(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(800.0, 600.0),
                    )),
                    events,
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        let id = ui.make_persistent_id(("folder", &folder));
                        let mut state =
                            egui::collapsing_header::CollapsingState::load_with_default_open(
                                ctx, id, false,
                            );
                        state.set_open(expanded);
                        state.store(ctx);
                        app.tree(ui, &root);
                    });
                    app.deletion_dialog(ctx);
                },
            );
        };
        let click = |app: &mut EditorApp, pos: egui::Pos2, button| {
            frame(
                app,
                vec![
                    egui::Event::PointerMoved(pos),
                    egui::Event::PointerButton {
                        pos,
                        button,
                        pressed: true,
                        modifiers: egui::Modifiers::NONE,
                    },
                ],
            );
            frame(
                app,
                vec![egui::Event::PointerButton {
                    pos,
                    button,
                    pressed: false,
                    modifiers: egui::Modifiers::NONE,
                }],
            );
            frame(app, vec![]);
        };
        frame(&mut app, vec![]);
        frame(&mut app, vec![]);
        let row = ctx
            .read_response(egui::Id::new(("explorer_folder_name", &folder)))
            .unwrap()
            .rect;
        click(&mut app, row.center(), egui::PointerButton::Secondary);
        let delete = ctx
            .read_response(egui::Id::new(("explorer_delete_action", folder.as_path())))
            .expect("Folder context menu must offer Delete")
            .rect;
        click(&mut app, delete.center(), egui::PointerButton::Primary);
        assert_eq!(app.deletion.as_ref().map(|d| &d.path), Some(&folder));
        assert!(
            folder.join("child.txt").exists(),
            "Choosing Delete must only open confirmation"
        );
        frame(
            &mut app,
            vec![egui::Event::Key {
                key: egui::Key::Escape,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            }],
        );
        assert!(app.deletion.is_none());
        assert!(folder.join("child.txt").exists());
    }
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn deletion_confirmation_cancels_and_preserves_symlink_targets() {
    let root = std::env::temp_dir().join(format!(
        "focus-delete-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir(&root).unwrap();
    let file = root.join("file.txt");
    fs::write(&file, "saved").unwrap();
    let mut app = test_app();
    app.active_file = Some(file.clone());
    app.open_files = vec![file.clone()];
    app.buffer = "unsaved".into();
    let ctx = egui::Context::default();
    theme::apply(&ctx);
    let frame = |app: &mut EditorApp, events: Vec<egui::Event>| {
        let _ = ctx.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(900.0, 600.0),
                )),
                events,
                ..Default::default()
            },
            |ctx| app.deletion_dialog(ctx),
        );
    };
    app.deletion = Some(Deletion {
        path: file.clone(),
        error: String::new(),
        needs_focus: true,
    });
    frame(&mut app, vec![]);
    assert!(file.exists(), "Opening confirmation must not delete");
    let key = |key| egui::Event::Key {
        key,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::NONE,
    };
    frame(&mut app, vec![key(egui::Key::Enter)]);
    assert!(file.exists(), "Enter must not confirm destructive action");
    if app.deletion.is_none() {
        app.deletion = Some(Deletion {
            path: file.clone(),
            error: String::new(),
            needs_focus: true,
        });
        frame(&mut app, vec![]);
    }
    frame(&mut app, vec![key(egui::Key::Escape)]);
    assert!(app.deletion.is_none());
    assert_eq!(app.buffer, "unsaved");
    assert!(app.confirm_delete(&root.join("missing")).is_err());
    assert_eq!(app.open_files, vec![file.clone()]);
    app.confirm_delete(&file).unwrap();
    assert!(!file.exists());
    assert!(app.active_file.is_none() && app.open_files.is_empty());
    assert!(!app.dirty);
    let target = root.join("target");
    fs::create_dir(&target).unwrap();
    fs::write(target.join("keep.txt"), "keep").unwrap();
    let link = root.join("link");
    std::os::unix::fs::symlink(&target, &link).unwrap();
    app.confirm_delete(&link).unwrap();
    assert!(!link.exists());
    assert_eq!(fs::read_to_string(target.join("keep.txt")).unwrap(), "keep");
    app.open_files = vec![target.join("keep.txt")];
    app.active_file = app.open_files.first().cloned();
    app.confirm_delete(&target).unwrap();
    assert!(!target.exists());
    assert!(app.open_files.is_empty());
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn inline_creation_submits_cancels_and_never_overwrites() {
    let root = std::env::temp_dir().join(format!(
        "focus-inline-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir(&root).unwrap();
    let ctx = egui::Context::default();
    theme::apply(&ctx);
    let mut app = test_app();
    app.settings.workspace = Some(root.clone());
    app.active_file = None;
    let frame = |app: &mut EditorApp, events: Vec<egui::Event>| {
        let _ = ctx.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1200.0, 800.0),
                )),
                events,
                ..Default::default()
            },
            |ctx| app.render(ctx),
        );
    };
    let key = |key| egui::Event::Key {
        key,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::NONE,
    };
    for is_dir in [false, true] {
        let name = if is_dir {
            "new folder"
        } else {
            ".new file.txt"
        };
        app.create_entry(is_dir);
        let id = app.creation.as_ref().unwrap().id();
        frame(&mut app, vec![]);
        assert!(ctx.memory(|m| m.has_focus(id)));
        frame(&mut app, vec![egui::Event::Text(name.into())]);
        assert!(!root.join(name).exists());
        frame(&mut app, vec![key(egui::Key::Enter)]);
        assert!(app.creation.is_none());
        assert_eq!(root.join(name).is_dir(), is_dir);
        assert!(root.join(name).exists());
    }
    let nested = root.join("nested").join("deep");
    fs::create_dir_all(&nested).unwrap();
    let displayed = nested.join("displayed.rs");
    fs::write(&displayed, "// open file").unwrap();
    app.active_file = Some(displayed);
    for is_dir in [false, true] {
        app.create_entry(is_dir);
        assert_eq!(app.creation.as_ref().unwrap().parent, nested);
        let id = app.creation.as_ref().unwrap().id();
        for _ in 0..4 {
            frame(&mut app, vec![]);
        }
        assert!(
            ctx.memory(|m| m.has_focus(id)),
            "Nested creation must reveal and focus its row"
        );
        let name = if is_dir {
            "beside-folder"
        } else {
            "beside.txt"
        };
        frame(&mut app, vec![egui::Event::Text(name.into())]);
        frame(&mut app, vec![key(egui::Key::Enter)]);
        assert!(nested.join(name).exists());
        assert!(!root.join(name).exists());
    }
    app.settings.workspace = Some(root.join("new folder"));
    app.create_entry(false);
    let id = app.creation.as_ref().unwrap().id();
    frame(&mut app, vec![]);
    assert!(
        ctx.memory(|m| m.has_focus(id)),
        "Outside-workspace creation must remain visible"
    );
    frame(&mut app, vec![egui::Event::Text("outside.txt".into())]);
    frame(&mut app, vec![key(egui::Key::Enter)]);
    assert!(nested.join("outside.txt").is_file());
    app.active_file = None;
    app.settings.workspace = Some(root.clone());
    for cancel_with_escape in [true, false] {
        app.create_entry(false);
        frame(&mut app, vec![]);
        frame(&mut app, vec![egui::Event::Text("cancelled.txt".into())]);
        if cancel_with_escape {
            frame(&mut app, vec![key(egui::Key::Escape)]);
        } else {
            ctx.memory_mut(|m| m.request_focus(egui::Id::new("main_editor_text")));
            frame(&mut app, vec![]);
        }
        assert!(app.creation.is_none());
        assert!(!root.join("cancelled.txt").exists());
    }
    fs::write(root.join("existing.txt"), "keep me").unwrap();
    app.create_entry(false);
    frame(&mut app, vec![]);
    frame(&mut app, vec![egui::Event::Text("existing.txt".into())]);
    frame(&mut app, vec![key(egui::Key::Enter)]);
    assert!(!app.creation.as_ref().unwrap().error.is_empty());
    assert_eq!(
        fs::read_to_string(root.join("existing.txt")).unwrap(),
        "keep me"
    );
    for name in ["", ".", "..", "../escape", "a/b", "bad\0name", "a\\b"] {
        app.creation.as_mut().unwrap().name = name.into();
        assert!(app.creation.as_ref().unwrap().create().is_err());
    }
    app.sidebar_mode = SidebarMode::Search;
    frame(&mut app, vec![]);
    assert!(app.creation.is_none());
    app.sidebar_mode = SidebarMode::Explorer;
    app.create_entry(false);
    app.settings.workspace = None;
    frame(&mut app, vec![]);
    assert!(app.creation.is_none());
    fs::remove_dir_all(root).unwrap();
}
