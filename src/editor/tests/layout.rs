use super::test_app;
use crate::{editor::SidebarMode, ui_theme as theme};
use eframe::egui;

#[test]
fn themed_window_fits_compact_and_standard_sizes() {
    let ctx = egui::Context::default();
    theme::apply(&ctx);
    let mut app = test_app();
    for size in [egui::vec2(900.0, 600.0), egui::vec2(1440.0, 900.0)] {
        for sidebar in [SidebarMode::Explorer, SidebarMode::Search, SidebarMode::Git] {
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
