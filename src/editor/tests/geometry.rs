use super::{EditorApp, test_app};
use crate::ui_theme as theme;
use eframe::egui;

fn text_shapes(output: &egui::FullOutput) -> Vec<&egui::epaint::TextShape> {
    fn visit<'a>(shape: &'a egui::Shape, out: &mut Vec<&'a egui::epaint::TextShape>) {
        match shape {
            egui::Shape::Text(text) => out.push(text),
            egui::Shape::Vec(shapes) => shapes.iter().for_each(|shape| visit(shape, out)),
            _ => {}
        }
    }
    let mut result = Vec::new();
    for clipped in &output.shapes {
        visit(&clipped.shape, &mut result);
    }
    result
}

fn frame(ctx: &egui::Context, width: f32, show: impl FnMut(&mut egui::Ui)) -> egui::FullOutput {
    crate::test_frame(
        ctx,
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(width, 600.0),
            )),
            ..Default::default()
        },
        show,
    )
}

#[test]
fn toolbar_centers_title_and_keeps_actions_on_the_right() {
    for width in [900.0, 1440.0] {
        let ctx = egui::Context::default();
        theme::apply(&ctx);
        let mut app = test_app();
        app.settings.workspace = Some(format!("/projects/{}", "long-project-".repeat(16)).into());
        let output = frame(&ctx, width, |root| app.toolbar(root));
        let texts = text_shapes(&output);
        let find = |label: &str| {
            texts
                .iter()
                .copied()
                .find(|text| text.galley.job.text == label)
                .unwrap()
        };
        let workspace = find("Workspace").visual_bounding_rect();
        let dev = find("Dev Container").visual_bounding_rect();
        let addons = find("Addons").visual_bounding_rect();
        let edit = find("Edit").visual_bounding_rect();
        let save = find("Save").visual_bounding_rect();
        let title = texts
            .iter()
            .find(|text| text.galley.job.text.starts_with("Focus Editor ·"))
            .unwrap()
            .visual_bounding_rect();
        assert!(workspace.right() < title.left());
        assert!(
            (title.center().x - width * 0.5).abs() <= 2.0,
            "title is not centered: {title:?}"
        );
        assert!(title.right() < dev.left());
        assert!(
            dev.right() < addons.left()
                && addons.right() < edit.left()
                && edit.right() < save.left()
        );
        assert!(
            save.right() <= width - 8.0 && save.right() > width - 35.0,
            "save={save:?} width={width} dev={dev:?}"
        );
        for control in [workspace, dev, addons, edit, save] {
            assert!(
                (control.center().y - 21.0).abs() <= 2.0,
                "misaligned control: {control:?}"
            );
        }
    }
}

#[test]
fn explorer_rows_keep_filename_columns_and_depth_independent_of_name_length() {
    let directory = tempfile::tempdir().unwrap();
    for name in ["a.rs", "a_very_long_source_file_name_for_truncation.rs"] {
        std::fs::write(directory.path().join(name), "").unwrap();
    }
    let nested = directory.path().join("nested");
    std::fs::create_dir(&nested).unwrap();
    std::fs::write(nested.join("child.rs"), "").unwrap();
    for width in [150.0, 232.0, 500.0] {
        let ctx = egui::Context::default();
        theme::apply(&ctx);
        let mut app = test_app();
        let output = frame(&ctx, width, |root| {
            egui::CentralPanel::default().show(root, |ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
                let id = ui.make_persistent_id(("folder", &nested));
                let mut state = egui::collapsing_header::CollapsingState::load_with_default_open(
                    &ctx, id, false,
                );
                state.set_open(true);
                state.store(&ctx);
                app.tree(ui, directory.path());
            });
        });
        let texts = text_shapes(&output);
        let find = |name: &str| {
            texts
                .iter()
                .copied()
                .find(|text| text.galley.job.text == name)
                .unwrap()
        };
        let short = find("a.rs");
        let long = find("a_very_long_source_file_name_for_truncation.rs");
        let folder = find("nested");
        let child = find("child.rs");
        assert_eq!(short.pos.x, long.pos.x);
        assert_eq!(folder.pos.x, short.pos.x);
        assert_eq!(child.pos.x - short.pos.x, 16.0);
        for name in ["a.rs", "a_very_long_source_file_name_for_truncation.rs"] {
            let row = ctx
                .read_response(egui::Id::new((
                    "explorer_file_row",
                    directory.path().join(name),
                )))
                .unwrap()
                .rect;
            assert_eq!(row.height(), 26.0);
            assert!(row.right() <= width);
            assert!(find(name).visual_bounding_rect().right() <= row.right());
        }
    }
}

fn source_frame(ctx: &egui::Context, app: &mut EditorApp) -> egui::FullOutput {
    frame(ctx, 340.0, |root| {
        egui::CentralPanel::default().show(root, |ui| app.text_view(ui, ctx));
    })
}

#[test]
fn editor_gutter_stays_fixed_and_aligned_when_source_scrolls() {
    let ctx = egui::Context::default();
    theme::apply(&ctx);
    ctx.style_mut_of(egui::Theme::Light, |style| {
        style.scroll_animation = egui::style::ScrollAnimation::none()
    });
    let mut app = test_app();
    app.buffer = format!("{}\n{}", "x".repeat(200), "let value = 1;\n".repeat(100));
    let output = source_frame(&ctx, &mut app);
    let id = app.editor_id();
    let gutter_before = ctx.read_response(id.with("gutter")).unwrap().rect;
    let texts = text_shapes(&output);
    let source = texts
        .iter()
        .find(|text| text.galley.job.text == app.buffer)
        .unwrap();
    let first = texts
        .iter()
        .find(|text| text.galley.job.text == "1")
        .unwrap();
    assert!(gutter_before.width() >= 42.0 && gutter_before.width() < 48.0);
    assert!((source.pos.x - gutter_before.right() - 8.0).abs() < 1.0);
    assert!((source.pos.y - 23.0).abs() < 1.0);
    assert!((first.pos.y - source.pos.y).abs() < 1.0);
    assert!((source.galley.rows[1].pos.y - source.galley.rows[0].pos.y - 21.0).abs() < 1.0);
    let source_before = source.pos;
    let scroll = ctx
        .data(|data| data.get_temp::<egui::Id>(id.with("scroll_state_id")))
        .unwrap();
    let mut state = egui::scroll_area::State::load(&ctx, scroll).unwrap();
    state.offset = egui::vec2(160.0, 42.0);
    state.store(&ctx, scroll);
    let output = source_frame(&ctx, &mut app);
    let gutter_after = ctx.read_response(id.with("gutter")).unwrap().rect;
    assert_eq!(gutter_after.left(), gutter_before.left());
    let texts = text_shapes(&output);
    let source = texts
        .iter()
        .find(|text| text.galley.job.text == app.buffer)
        .unwrap();
    assert!((source.pos.x - source_before.x + 160.0).abs() < 1.0);
    let third = texts
        .iter()
        .find(|text| text.galley.job.text == "3")
        .unwrap();
    assert!((third.pos.y - source.pos.y - 42.0).abs() < 1.0);
    // Changing appearance must invalidate the source layout cache too.
    let mut appearance = theme::current(&ctx);
    appearance.metrics.editor_line_height = 28.0;
    theme::apply_theme(&ctx, &appearance);
    let output = source_frame(&ctx, &mut app);
    let texts = text_shapes(&output);
    let source = texts
        .iter()
        .find(|text| text.galley.job.text == app.buffer)
        .unwrap();
    assert!((source.galley.rows[1].pos.y - source.galley.rows[0].pos.y - 28.0).abs() < 1.0);
    app.buffer = "\n".repeat(10_000);
    source_frame(&ctx, &mut app);
    source_frame(&ctx, &mut app);
    assert!(ctx.read_response(id.with("gutter")).unwrap().rect.width() > 56.0);
}

#[test]
fn tabs_are_contiguous_and_meet_the_minimum_width() {
    let ctx = egui::Context::default();
    theme::apply(&ctx);
    let mut app = test_app();
    let files: Vec<std::path::PathBuf> = ["a.rs", "b.rs"].into_iter().map(Into::into).collect();
    app.open_files = files.clone();
    app.active_file = Some(files[0].clone());
    app.dirty = false;
    for _ in 0..3 {
        frame(&ctx, 900.0, |root| app.document_panel(root, false));
    }
    let mut previous_right = None;
    for path in &files {
        let tab = egui::Id::new(("file_tab", path));
        let label = ctx.read_response(tab).unwrap().rect;
        let close = ctx.read_response(tab.with("close")).unwrap().rect;
        assert_eq!(label.height(), 37.0);
        assert_eq!(label.top(), 0.0, "tab header must start at the pane edge");
        assert!(close.right() - label.left() >= 130.0);
        if let Some(right) = previous_right {
            assert_eq!(label.left(), right);
        }
        previous_right = Some(close.right());
    }
}
