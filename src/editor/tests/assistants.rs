use super::test_app;
use crate::{
    opencode_server::Event,
    settings::{Assistant, Settings},
};
use eframe::egui;
use serde_json::json;

#[test]
fn disabled_panels_reclaim_editor_width_and_preserve_saved_pane_width() {
    let ctx = egui::Context::default();
    crate::ui_theme::apply(&ctx);
    let mut app = test_app();
    let width = app.settings.right_width;
    let mut right = Vec::new();
    for (codex, open, selected) in [
        (true, false, Assistant::Codex),
        (true, true, Assistant::OpenCode),
        (false, true, Assistant::OpenCode),
        (false, false, Assistant::Codex),
    ] {
        app.settings.codex_enabled = codex;
        app.settings.opencode_enabled = open;
        app.settings.selected_assistant = selected;
        for _ in 0..3 {
            let _ = ctx.run(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1000.0, 750.0),
                    )),
                    ..Default::default()
                },
                |ctx| {
                    app.render(ctx);
                    assert!(ctx.used_rect().right() <= 1001.0);
                    assert!(ctx.used_rect().bottom() <= 751.0);
                },
            );
        }
        right.push(ctx.read_response(app.editor_id()).unwrap().rect.width());
    }
    assert!(
        right[3] > right[0] + 200.0,
        "disabled pane did not release its width: {right:?}"
    );
    assert_eq!(app.settings.right_width, width);
}

#[test]
fn hidden_opencode_processes_events_and_disabling_stops_it() {
    let ctx = egui::Context::default();
    let mut app = test_app();
    let root = std::env::temp_dir().join(format!("focus-assistant-disable-{}", std::process::id()));
    app.settings_path = root.join("settings.toml");
    let (open, _requests, events) = crate::opencode_ui::OpenCodeUi::test_connection();
    app.opencode_ui = open;
    app.settings.opencode_enabled = true;
    app.settings.selected_assistant = Assistant::Codex;
    events.send(Event::Data(json!({"type":"permission.asked","properties":{"id":"approval","sessionID":"session1","permission":"edit"}}))).unwrap();
    app.update_assistants(&ctx);
    assert_eq!(app.opencode_ui.state(), "Waiting for you");
    app.set_assistant_enabled(Assistant::OpenCode, false, &ctx);
    assert_eq!(app.opencode_ui.state(), "Offline");
    assert!(!app.settings.opencode_enabled);
    assert_eq!(app.settings.selected_assistant, Assistant::Codex);
    let stored: Settings =
        toml::from_str(&std::fs::read_to_string(&app.settings_path).unwrap()).unwrap();
    assert!(!stored.opencode_enabled);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn old_settings_keep_codex_startup_preference_and_new_preferences_roundtrip() {
    let old: Settings = toml::from_str("codex_app_server = false\n").unwrap();
    assert!(old.codex_enabled);
    assert!(!old.opencode_enabled);
    assert!(!old.codex_app_server);
    let settings = Settings {
        codex_enabled: false,
        opencode_enabled: true,
        selected_assistant: Assistant::OpenCode,
        ..Default::default()
    };
    let saved: Settings = toml::from_str(&toml::to_string(&settings).unwrap()).unwrap();
    assert!(!saved.codex_enabled);
    assert!(saved.opencode_enabled);
    assert_eq!(saved.selected_assistant, Assistant::OpenCode);
}

#[test]
fn workspace_switch_keeps_separate_drafts_and_drops_old_event_channels() {
    let ctx = egui::Context::default();
    let mut app = test_app();
    let root =
        std::env::temp_dir().join(format!("focus-assistant-workspaces-{}", std::process::id()));
    let one = root.join("one");
    let two = root.join("two");
    std::fs::create_dir_all(&one).unwrap();
    std::fs::create_dir_all(&two).unwrap();
    app.settings_path = root.join("settings.toml");
    app.settings.opencode_enabled = true;
    app.settings.codex_enabled = false;
    app.skip_opencode_start = true;
    app.settings.workspace = Some(one.clone());
    let (open, _requests, events) = crate::opencode_ui::OpenCodeUi::test_connection();
    app.opencode_ui = open;
    app.update_assistants(&ctx);
    assert_eq!(app.opencode_ui.test_draft(), "OpenCode draft");
    app.settings.workspace = Some(two);
    app.update_assistants(&ctx);
    assert_eq!(app.opencode_ui.test_draft(), "");
    assert!(
        events
            .send(Event::Data(
                json!({"type":"session.idle","properties":{"sessionID":"session1"}})
            ))
            .is_err()
    );
    app.settings.workspace = Some(one);
    app.update_assistants(&ctx);
    assert_eq!(app.opencode_ui.test_draft(), "OpenCode draft");
    assert_eq!(app.opencode_ui.state(), "Offline");
    std::fs::remove_dir_all(root).unwrap();
}
