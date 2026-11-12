use crate::{editor::persistence::restore_open_files, settings::Settings};

#[test]
fn restores_order_active_file_and_skips_missing_files() {
    let root = std::env::temp_dir().join(format!("focus-session-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let first = root.join("first.txt");
    let second = root.join("second.txt");
    std::fs::write(&first, "first").unwrap();
    std::fs::write(&second, "second").unwrap();
    let mut settings = Settings {
        open_files: vec![first.clone(), root.join("missing.txt"), second.clone()],
        active_file: Some(second.clone()),
        ..Settings::default()
    };
    assert_eq!(
        restore_open_files(&settings),
        (
            vec![first.clone(), second.clone()],
            Some(second),
            "second".into()
        )
    );
    settings.active_file = Some(root.join("missing.txt"));
    assert_eq!(restore_open_files(&settings).1, Some(first));
    settings.workspace = Some(root.clone());
    assert!(restore_open_files(&settings).0.is_empty());
    let old: Settings = toml::from_str("").unwrap();
    assert!(restore_open_files(&old).0.is_empty());
    std::fs::remove_dir_all(root).unwrap();
}

// Run the full transition in a child so its application directories are isolated
// without changing process-wide environment variables underneath parallel tests.
#[cfg(target_os = "linux")]
#[test]
fn workspace_switch_restores_state_and_handles_unsaved_choices() {
    use super::super::close_dialog::CloseChoice;
    let marker = "FOCUS_WORKSPACE_TEST_ROOT";
    let Ok(root) = std::env::var(marker) else {
        let root = tempfile::tempdir().unwrap();
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "editor::tests::persistence::workspace_switch_restores_state_and_handles_unsaved_choices", "--nocapture"])
            .env(marker, root.path())
            .env("XDG_DATA_HOME", root.path().join("data"))
            .env("XDG_CONFIG_HOME", root.path().join("config"))
            .output().unwrap();
        assert!(
            output.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        return;
    };
    let root = std::path::PathBuf::from(root);
    let a = root.join("a");
    let b = root.join("b");
    std::fs::create_dir_all(&a).unwrap();
    std::fs::create_dir_all(&b).unwrap();
    let file_a = a.join("a.txt");
    let file_b = b.join("b.txt");
    std::fs::write(&file_a, "a disk").unwrap();
    std::fs::write(&file_b, "b disk").unwrap();
    // An old global record must never seed a workspace, even when its workspace
    // matches or is the ambiguous relative path used by previous builds.
    let global = dirs::config_dir()
        .unwrap()
        .join("focus-editor/settings.toml");
    for recorded in [std::path::PathBuf::from("."), a.clone()] {
        let obsolete = Settings {
            workspace: Some(recorded.clone()),
            session_workspace: Some(recorded),
            open_files: vec![file_b.clone()],
            active_file: Some(file_b.clone()),
            explorer_width: 777.0,
            addon_enabled: [("codex".into(), false)].into(),
            ..Default::default()
        };
        crate::settings::write_workspace(&global, &obsolete).unwrap();
        let (fresh, _) = crate::settings::load_workspace(Some(&a)).unwrap();
        assert!(fresh.open_files.is_empty());
        assert!(fresh.active_file.is_none());
        assert!(fresh.addon_enabled.is_empty());
        assert_eq!(fresh.explorer_width, Settings::default().explorer_width);
        assert!(
            crate::settings::load_workspace(None)
                .unwrap()
                .0
                .workspace
                .is_none()
        );
    }
    for (workspace, file, width, enabled) in
        [(&a, &file_a, 250.0, false), (&b, &file_b, 360.0, true)]
    {
        let settings = Settings {
            workspace: Some(workspace.clone()),
            session_workspace: Some(workspace.clone()),
            open_files: vec![file.clone()],
            active_file: Some(file.clone()),
            explorer_width: width,
            addon_enabled: [("codex".into(), enabled)].into(),
            ..Default::default()
        };
        crate::settings::write_workspace(
            &crate::app_paths::workspace_dir(workspace)
                .unwrap()
                .join("settings.toml"),
            &settings,
        )
        .unwrap();
    }
    let ctx = eframe::egui::Context::default();
    let mut app = super::test_app();
    app.dirty = false;
    app.request_workspace(a.clone());
    app.resolve_workspace_switch(CloseChoice::Discard, &ctx);
    assert_eq!(app.open_files, [file_a.clone()]);
    assert_eq!(app.buffer, "a disk");
    assert!(!app.settings.addon_enabled["codex"]);
    app.request_workspace(a.join("."));
    assert!(app.pending_workspace.is_none());
    app.buffer = "edited a".into();
    app.mark_modified();
    app.search_query = "old search".into();
    app.request_workspace(b.clone());
    app.resolve_workspace_switch(CloseChoice::Cancel, &ctx);
    assert_eq!(app.settings.workspace.as_ref(), Some(&a));
    assert_eq!(app.buffer, "edited a");
    assert!(app.dirty);
    // A failed save leaves the workspace and unsaved buffer available for retry.
    std::fs::remove_file(&file_a).unwrap();
    std::fs::create_dir(&file_a).unwrap();
    app.request_workspace(b.clone());
    app.resolve_workspace_switch(CloseChoice::Save, &ctx);
    assert_eq!(app.settings.workspace.as_ref(), Some(&a));
    assert!(app.pending_workspace.is_some());
    assert_eq!(app.buffer, "edited a");
    std::fs::remove_dir(&file_a).unwrap();
    std::fs::write(&file_a, "a disk").unwrap();
    app.resolve_workspace_switch(CloseChoice::Save, &ctx);
    assert_eq!(std::fs::read_to_string(&file_a).unwrap(), "edited a");
    assert_eq!(app.open_files, [file_b.clone()]);
    assert_eq!(app.buffer, "b disk");
    assert!(app.settings.addon_enabled["codex"]);
    assert_eq!(app.settings.explorer_width, 360.0);
    assert!(app.search_query.is_empty());
    app.buffer = "discarded b".into();
    app.mark_modified();
    app.request_workspace(a.clone());
    app.resolve_workspace_switch(CloseChoice::Discard, &ctx);
    assert_eq!(app.buffer, "edited a");
    assert_eq!(app.settings.explorer_width, 250.0);
    assert!(!app.settings.addon_enabled["codex"]);
    assert!(app.unsaved_files.is_empty());
    assert!(!app.dirty);
    app.request_workspace(b);
    app.resolve_workspace_switch(CloseChoice::Discard, &ctx);
    assert_eq!(app.buffer, "b disk");
    assert_eq!(app.open_files, [file_b]);
}
