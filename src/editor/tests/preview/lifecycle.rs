use super::{Project, fs};
use crate::editor::{close_dialog::CloseChoice, persistence::restore_open_files};

#[test]
fn preview_replaces_same_slot_and_permanent_tabs_stay_permanent() {
    let project = Project::new();
    let mut app = project.app();
    let [a, b, c, d] = ["a.txt", "b.txt", "c.txt", "d.txt"].map(|name| project.file(name));
    app.open_file(a.clone());
    app.preview_file(b.clone());
    app.open_file(c.clone());
    app.preview_file(d.clone());
    assert_eq!(app.open_files, [a.clone(), d.clone(), c]);
    assert_eq!(app.preview_file, Some(d.clone()));
    app.preview_file(a.clone());
    assert_eq!(app.preview_file, Some(d.clone()));
    assert_eq!(app.open_files.len(), 3);
    app.activate_file(d.clone());
    assert_eq!(app.preview_file, Some(d.clone()));
    fs::write(&d, "external update").unwrap();
    app.open_file(d.clone()); // Pin without reloading the active preview.
    assert_eq!(app.buffer, "d.txt");
    assert!(app.preview_file.is_none());
    app.preview_file(b.clone());
    assert_eq!(app.open_files, [a, d, project.file("c.txt"), b]);
}

#[test]
fn unsaved_files_survive_browsing_and_save_targets_the_inactive_document() {
    let project = Project::new();
    let mut app = project.app();
    let a = project.file("a.txt");
    let b = project.file("b.txt");
    app.preview_file(a.clone());
    app.buffer = "unsaved a".into();
    app.mark_modified();
    assert!(app.preview_file.is_none());
    app.preview_file(b.clone());
    assert!(app.file_is_dirty(&a));
    assert_eq!(app.unsaved_files[&a], "unsaved a");
    app.buffer = "unsaved b".into();
    app.mark_modified();
    app.preview_file(project.file("c.txt"));
    app.close_file(a.clone());
    app.resolve_close(CloseChoice::Cancel);
    assert!(app.file_is_dirty(&a));
    app.close_file(a.clone());
    app.resolve_close(CloseChoice::Save);
    assert_eq!(fs::read_to_string(&a).unwrap(), "unsaved a");
    assert!(!app.open_files.contains(&a));
    assert_eq!(app.buffer, "c.txt");
    app.activate_file(b.clone());
    assert_eq!(app.buffer, "unsaved b");
    assert!(app.dirty);
    app.open_file(b.clone());
    assert_eq!(app.buffer, "unsaved b");
    app.activate_file(project.file("c.txt"));
    app.close_file(b.clone());
    app.resolve_close(CloseChoice::Discard);
    assert_eq!(fs::read_to_string(&b).unwrap(), "b.txt");
    assert!(!app.unsaved_files.contains_key(&b));
    assert_eq!(app.preview_file, Some(project.file("c.txt")));
}

#[test]
fn failed_reads_and_saves_preserve_preview_and_unsaved_text() {
    let project = Project::new();
    let mut app = project.app();
    let a = project.file("a.txt");
    app.preview_file(a.clone());
    let invalid = project.file("invalid");
    fs::write(&invalid, [255]).unwrap();
    for path in [invalid, project.file("missing")] {
        assert!(!app.preview_file(path));
        assert_eq!(app.preview_file, Some(a.clone()));
        assert_eq!(app.open_files, [a.clone()]);
        assert_eq!(app.buffer, "a.txt");
    }
    app.buffer = "do not lose this".into();
    app.mark_modified();
    app.preview_file(project.file("b.txt"));
    fs::remove_file(&a).unwrap();
    fs::create_dir(&a).unwrap();
    app.close_file(a.clone());
    app.resolve_close(CloseChoice::Save);
    assert_eq!(app.close_confirmation, Some(a.clone()));
    assert!(app.open_files.contains(&a));
    assert_eq!(app.unsaved_files[&a], "do not lose this");
    app.activate_file(a);
    assert_eq!(app.buffer, "do not lose this");
}

#[test]
fn directory_moves_and_deletions_update_preview_and_inactive_unsaved_buffers() {
    let project = Project::new();
    let mut app = project.app();
    let folder = project.file("folder");
    fs::create_dir(&folder).unwrap();
    let a = folder.join("a");
    let b = folder.join("b");
    fs::write(&a, "a").unwrap();
    fs::write(&b, "b").unwrap();
    app.preview_file(a.clone());
    app.buffer = "edited a".into();
    app.mark_modified();
    app.preview_file(b.clone());
    app.close_file(a.clone());
    let moved = project.file("moved");
    fs::rename(&folder, &moved).unwrap();
    app.update_open_path(&folder, &moved);
    assert_eq!(app.active_file, Some(moved.join("b")));
    assert_eq!(app.preview_file, Some(moved.join("b")));
    assert_eq!(app.close_confirmation, Some(moved.join("a")));
    assert_eq!(app.unsaved_files[&moved.join("a")], "edited a");
    assert!(app.deletion_has_unsaved(&moved));
    app.confirm_delete(&moved.join("b")).unwrap();
    assert!(app.preview_file.is_none());
    assert_eq!(app.active_file, Some(moved.join("a")));
    assert_eq!(app.buffer, "edited a");
    assert!(app.dirty);
    app.preview_file(project.file("c.txt"));
    app.confirm_delete(&moved).unwrap();
    assert!(app.unsaved_files.is_empty());
    assert!(app.close_confirmation.is_none());
    assert_eq!(app.open_files, [project.file("c.txt")]);
}

#[test]
fn restoring_tabs_keeps_preview_path_but_uses_saved_disk_contents() {
    let project = Project::new();
    let mut app = project.app();
    let a = project.file("a.txt");
    let b = project.file("b.txt");
    app.open_file(a.clone());
    app.buffer = "unsaved a".into();
    app.mark_modified();
    app.preview_file(b.clone());
    app.save_settings();
    let settings = toml::from_str(&fs::read_to_string(&app.settings_path).unwrap()).unwrap();
    let (files, active, buffer) = restore_open_files(&settings);
    assert_eq!(files, [a, b.clone()]);
    assert_eq!(active, Some(b));
    assert_eq!(buffer, "b.txt");
    assert!(
        !fs::read_to_string(&app.settings_path)
            .unwrap()
            .contains("unsaved a")
    );
}
