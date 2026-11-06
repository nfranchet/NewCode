use super::Project;
use crate::editor::close_dialog::{CloseChoice, CloseTabs};
use std::fs;

#[test]
fn close_others_targets_the_clicked_tab_and_preserves_its_unsaved_contents() {
    let project = Project::new();
    let mut app = project.app();
    for name in ["a.txt", "b.txt", "c.txt"] {
        app.open_file(project.file(name));
    }
    app.activate_file(project.file("b.txt"));
    app.buffer = "unsaved b".into();
    app.mark_modified();
    app.activate_file(project.file("a.txt"));
    app.close_tabs(CloseTabs::Others(project.file("b.txt")));
    assert_eq!(app.open_files, [project.file("b.txt")]);
    assert_eq!(app.active_file, Some(project.file("b.txt")));
    assert_eq!(app.buffer, "unsaved b");
    assert!(app.dirty);
    assert!(app.close_confirmation.is_none());
}

#[test]
fn bulk_close_saves_discards_and_cancels_in_tab_order() {
    let project = Project::new();
    let mut app = project.app();
    app.open_file(project.file("a.txt"));
    for name in ["b.txt", "c.txt", "d.txt"] {
        app.open_file(project.file(name));
        app.buffer = format!("unsaved {name}");
        app.mark_modified();
    }
    app.close_tabs(CloseTabs::All);
    assert!(!app.open_files.contains(&project.file("a.txt")));
    assert_eq!(app.close_confirmation, Some(project.file("b.txt")));
    app.close_file(project.file("d.txt"));
    assert_eq!(
        app.close_confirmation,
        Some(project.file("b.txt")),
        "another close replaced the question"
    );
    app.resolve_close(CloseChoice::Save);
    assert_eq!(
        fs::read_to_string(project.file("b.txt")).unwrap(),
        "unsaved b.txt"
    );
    assert_eq!(app.close_confirmation, Some(project.file("c.txt")));
    app.resolve_close(CloseChoice::Discard);
    assert_eq!(fs::read_to_string(project.file("c.txt")).unwrap(), "c.txt");
    assert_eq!(app.close_confirmation, Some(project.file("d.txt")));
    app.resolve_close(CloseChoice::Cancel);
    assert_eq!(app.open_files, [project.file("d.txt")]);
    assert_eq!(app.buffer, "unsaved d.txt");
    assert!(app.dirty);
    assert!(app.close_queue.is_empty());
    app.close_tabs(CloseTabs::All);
    app.resolve_close(CloseChoice::Save);
    assert!(app.open_files.is_empty());
    assert!(app.active_file.is_none());
}

#[test]
fn bulk_close_pauses_on_failed_save_and_preserves_remaining_tabs() {
    let project = Project::new();
    let mut app = project.app();
    for name in ["a.txt", "b.txt"] {
        app.open_file(project.file(name));
        app.buffer = format!("modified {name}");
        app.mark_modified();
    }
    fs::remove_file(project.file("a.txt")).unwrap();
    fs::create_dir(project.file("a.txt")).unwrap();
    app.close_tabs(CloseTabs::All);
    app.resolve_close(CloseChoice::Save);
    assert_eq!(app.close_confirmation, Some(project.file("a.txt")));
    assert_eq!(app.open_files.len(), 2);
    assert_eq!(app.unsaved_files[&project.file("a.txt")], "modified a.txt");
    app.resolve_close(CloseChoice::Cancel);
    assert!(app.close_queue.is_empty());
    assert_eq!(app.open_files.len(), 2);
}
