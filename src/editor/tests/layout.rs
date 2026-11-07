use super::test_app;
use crate::{editor::SidebarMode, ui_theme as theme};
use eframe::egui;

#[test]
fn addon_manager_is_the_fourth_activity_and_preserves_file_tabs() {
    let ctx = egui::Context::default();
    theme::apply(&ctx);
    let mut app = test_app();
    let files = app.open_files.clone();
    let buffer = app.buffer.clone();
    let mut frame = |events: Vec<egui::Event>| {
        let _ = crate::test_frame(
            &ctx,
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(900.0, 600.0),
                )),
                events,
                ..Default::default()
            },
            |ui| app.activity_bar(ui),
        );
    };
    frame(vec![]);
    for pressed in [true, false] {
        frame(vec![
            egui::Event::PointerMoved(egui::pos2(24.0, 173.0)),
            egui::Event::PointerButton {
                pos: egui::pos2(24.0, 173.0),
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: Default::default(),
            },
        ]);
    }
    assert!(matches!(app.sidebar_mode, SidebarMode::AddonManager));
    assert_eq!(app.open_files, files);
    assert_eq!(app.buffer, buffer);
}

#[test]
fn themed_window_fits_compact_and_standard_sizes() {
    let ctx = egui::Context::default();
    theme::apply(&ctx);
    let mut app = test_app();
    for size in [egui::vec2(900.0, 600.0), egui::vec2(1440.0, 900.0)] {
        for sidebar in [
            SidebarMode::Explorer,
            SidebarMode::Search,
            SidebarMode::Git,
            SidebarMode::AddonManager,
        ] {
            app.sidebar_mode = sidebar;
            for _ in 0..3 {
                let _ = crate::test_frame(
                    &ctx,
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
                        ..Default::default()
                    },
                    |root| {
                        app.render(root);
                        assert!(
                            ctx.globally_used_rect().right() <= size.x + 1.0,
                            "Window overflows horizontally: {:?}",
                            ctx.globally_used_rect()
                        );
                        assert!(
                            ctx.globally_used_rect().bottom() <= size.y + 1.0,
                            "Window overflows vertically: {:?}",
                            ctx.globally_used_rect()
                        );
                    },
                );
            }
        }
    }
}

#[test]
fn native_title_tracks_workspace_changes_without_redundant_commands() {
    let ctx = egui::Context::default();
    let mut app = test_app();
    for (workspace, expected) in [
        (Some("/projects/first"), Some("Focus Editor — first")),
        (Some("/projects/first"), None),
        (Some("/projects/second"), Some("Focus Editor — second")),
        (None, Some("Focus Editor")),
    ] {
        app.settings.workspace = workspace.map(Into::into);
        let output = crate::test_frame(&ctx, Default::default(), |root| {
            app.update_window_title(root.ctx())
        });
        let titles: Vec<_> = output
            .viewport_output
            .values()
            .flat_map(|viewport| viewport.commands.iter())
            .filter_map(|command| match command {
                egui::ViewportCommand::Title(title) => Some(title.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(titles, expected.into_iter().collect::<Vec<_>>());
    }
}
