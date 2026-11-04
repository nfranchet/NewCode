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
                let _ = ctx.run(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
                        ..Default::default()
                    },
                    |ctx| {
                        app.render(ctx);
                        assert!(
                            ctx.used_rect().right() <= size.x + 1.0,
                            "Window overflows horizontally: {:?}",
                            ctx.used_rect()
                        );
                        assert!(
                            ctx.used_rect().bottom() <= size.y + 1.0,
                            "Window overflows vertically: {:?}",
                            ctx.used_rect()
                        );
                    },
                );
            }
        }
    }
}
