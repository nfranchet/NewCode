use super::test_app;
#[test]
fn empty_addon_installation_reclaims_right_panel() {
    let mut app = test_app();
    let ctx = eframe::egui::Context::default();
    let width = app.settings.right_width;
    let _ = crate::test_frame(&ctx, Default::default(), |ui| {
        app.addons.right_panel(
            ui,
            &mut app.settings.right_width,
            &mut app.settings.selected_right_addon,
        )
    });
    assert_eq!(app.settings.right_width, width);
    assert!(app.settings.selected_right_addon.is_none());
}
