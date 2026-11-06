use super::{EditorApp, Project};
use eframe::egui::{self, Event};

struct Harness {
    project: Project,
    app: EditorApp,
    ctx: egui::Context,
    time: f64,
}
impl Harness {
    fn new() -> Self {
        let project = Project::new();
        let app = project.app();
        let ctx = egui::Context::default();
        crate::ui_theme::apply(&ctx);
        Self {
            project,
            app,
            ctx,
            time: 0.0,
        }
    }
    fn frame(&mut self, events: Vec<Event>) -> egui::FullOutput {
        self.time += 0.03;
        crate::test_frame(
            &self.ctx,
            egui::RawInput {
                time: Some(self.time),
                events,
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1100.0, 750.0),
                )),
                ..Default::default()
            },
            |root| self.app.render(root),
        )
    }
    fn explorer(&self, name: &str) -> egui::Id {
        egui::Id::new(("explorer_file_row", self.project.file(name)))
    }
    fn tab(&self, name: &str) -> egui::Id {
        egui::Id::new(("file_tab", self.project.file(name)))
    }
    fn click(&mut self, id: egui::Id, delay: f64) {
        self.time += delay;
        let pos = self.ctx.read_response(id).unwrap().rect.center();
        self.frame(vec![
            Event::PointerMoved(pos),
            Event::PointerButton {
                pos,
                pressed: true,
                button: egui::PointerButton::Primary,
                modifiers: egui::Modifiers::NONE,
            },
        ]);
        self.frame(vec![Event::PointerButton {
            pos,
            pressed: false,
            button: egui::PointerButton::Primary,
            modifiers: egui::Modifiers::NONE,
        }]);
    }
}

fn italic_label(output: &egui::FullOutput, name: &str) -> bool {
    output.shapes.iter().any(|shape| match &shape.shape {
        egui::epaint::Shape::Text(text) => {
            text.galley.job.text.contains(name)
                && text
                    .galley
                    .job
                    .sections
                    .iter()
                    .any(|section| section.format.italics)
        }
        _ => false,
    })
}

#[test]
fn explorer_single_click_replaces_preview_and_double_click_keeps_it() {
    let mut h = Harness::new();
    h.frame(vec![]);
    h.click(h.explorer("a.txt"), 0.5);
    assert_eq!(h.app.preview_file, Some(h.project.file("a.txt")));
    let output = h.frame(vec![]);
    assert!(italic_label(&output, "a.txt"));
    h.click(h.explorer("b.txt"), 0.5);
    assert_eq!(h.app.open_files, [h.project.file("b.txt")]);
    h.click(h.explorer("b.txt"), 0.5);
    h.click(h.explorer("b.txt"), 0.03);
    assert!(h.app.preview_file.is_none());
    let output = h.frame(vec![]);
    assert!(!italic_label(&output, "b.txt"));
    h.click(h.explorer("c.txt"), 0.5);
    assert_eq!(
        h.app.open_files,
        [h.project.file("b.txt"), h.project.file("c.txt")]
    );
    h.click(h.tab("c.txt"), 0.5);
    assert_eq!(h.app.preview_file, Some(h.project.file("c.txt")));
    h.click(h.tab("c.txt"), 0.03);
    assert!(h.app.preview_file.is_none());
}

#[test]
fn typing_pins_preview_and_browsing_retains_the_unsaved_buffer() {
    let mut h = Harness::new();
    h.frame(vec![]);
    h.click(h.explorer("a.txt"), 0.5);
    h.ctx
        .memory_mut(|memory| memory.request_focus(h.app.editor_id()));
    h.frame(vec![Event::Text("edited ".into())]);
    assert!(h.app.preview_file.is_none());
    assert!(h.app.dirty);
    let edited = h.app.buffer.clone();
    h.click(h.explorer("b.txt"), 0.5);
    assert_eq!(h.app.open_files.len(), 2);
    assert!(h.app.file_is_dirty(&h.project.file("a.txt")));
    let output = h.frame(vec![]);
    assert!(output.shapes.iter().any(|shape| match &shape.shape {
        egui::epaint::Shape::Text(text) => text.galley.job.text.contains("a.txt •"),
        _ => false,
    }));
    h.click(h.tab("a.txt"), 0.5);
    assert_eq!(h.app.buffer, edited);
    assert!(h.app.dirty);
    assert_eq!(h.app.preview_file, Some(h.project.file("b.txt")));
}

#[test]
fn undo_in_another_file_does_not_restore_the_previous_files_contents() {
    let mut h = Harness::new();
    h.frame(vec![]);
    h.click(h.explorer("a.txt"), 0.5);
    h.ctx
        .memory_mut(|memory| memory.request_focus(h.app.editor_id()));
    h.frame(vec![Event::Text("edited ".into())]);
    h.frame(vec![]);
    h.click(h.explorer("b.txt"), 0.5);
    h.ctx
        .memory_mut(|memory| memory.request_focus(h.app.editor_id()));
    let modifiers = egui::Modifiers::CTRL | egui::Modifiers::COMMAND;
    h.frame(vec![Event::Key {
        key: egui::Key::Z,
        physical_key: Some(egui::Key::Z),
        pressed: true,
        repeat: false,
        modifiers,
    }]);
    assert_eq!(h.app.buffer, "b.txt");
    h.frame(vec![Event::Key {
        key: egui::Key::Z,
        physical_key: Some(egui::Key::Z),
        pressed: false,
        repeat: false,
        modifiers,
    }]);
    h.click(h.tab("a.txt"), 0.5);
    h.ctx
        .memory_mut(|memory| memory.request_focus(h.app.editor_id()));
    h.frame(vec![Event::Key {
        key: egui::Key::Z,
        physical_key: Some(egui::Key::Z),
        pressed: true,
        repeat: false,
        modifiers,
    }]);
    assert_eq!(
        h.app.buffer, "a.txt",
        "the original file should retain its own Undo history"
    );
}

#[test]
fn tab_context_menu_closes_clicked_others_and_all_with_unsaved_confirmation() {
    fn choose(h: &mut Harness, name: &str, action: &str) {
        h.time += 0.5;
        h.frame(vec![]);
        h.frame(vec![]);
        let pos = h.ctx.read_response(h.tab(name)).unwrap().rect.center();
        for pressed in [true, false] {
            h.frame(vec![
                Event::PointerMoved(pos),
                Event::PointerButton {
                    pos,
                    pressed,
                    button: egui::PointerButton::Secondary,
                    modifiers: egui::Modifiers::NONE,
                },
            ]);
        }
        let output = h.frame(vec![]);
        assert!(
            h.ctx
                .read_response(h.tab(name))
                .unwrap()
                .context_menu_opened(),
            "wrong context target {name} at {pos:?}"
        );
        let pos = output
            .shapes
            .iter()
            .find_map(|shape| match &shape.shape {
                egui::Shape::Text(text) if text.galley.job.text == action => {
                    Some(text.visual_bounding_rect().center())
                }
                _ => None,
            })
            .unwrap_or_else(|| panic!("missing context action {action}"));
        for pressed in [true, false] {
            h.frame(vec![
                Event::PointerMoved(pos),
                Event::PointerButton {
                    pos,
                    pressed,
                    button: egui::PointerButton::Primary,
                    modifiers: egui::Modifiers::NONE,
                },
            ]);
        }
    }
    let mut h = Harness::new();
    for name in ["a.txt", "b.txt", "c.txt"] {
        h.app.open_file(h.project.file(name));
    }
    h.frame(vec![]);
    choose(&mut h, "a.txt", "Close file");
    assert!(!h.app.open_files.contains(&h.project.file("a.txt")));
    assert_eq!(h.app.active_file, Some(h.project.file("c.txt")));
    choose(&mut h, "b.txt", "Close others");
    assert_eq!(h.app.open_files, [h.project.file("b.txt")]);
    assert_eq!(h.app.active_file, Some(h.project.file("b.txt")));
    h.app.buffer = "unsaved b".into();
    h.app.mark_modified();
    h.frame(vec![]);
    choose(&mut h, "b.txt", "Close all");
    assert_eq!(h.app.close_confirmation, Some(h.project.file("b.txt")));
    assert_eq!(h.app.open_files, [h.project.file("b.txt")]);
    let output = h.frame(vec![]);
    assert!(output.shapes.iter().any(|shape| matches!(&shape.shape, egui::Shape::Text(text) if text.galley.job.text.contains("Save changes to") && text.galley.job.text.contains("b.txt"))));
}
