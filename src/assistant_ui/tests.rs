use super::*;
use composer::{Choice, Selector};

fn frame(
    ctx: &egui::Context,
    size: egui::Vec2,
    events: Vec<egui::Event>,
    mut show: impl FnMut(&mut egui::Ui),
) -> egui::FullOutput {
    crate::test_frame(
        ctx,
        egui::RawInput {
            events,
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
            ..Default::default()
        },
        |root| {
            egui::CentralPanel::default().show(root, |ui| {
                configure(ui);
                show(ui);
            });
        },
    )
}
fn texts(output: &egui::FullOutput) -> Vec<&egui::epaint::TextShape> {
    fn collect<'a>(shape: &'a egui::Shape, out: &mut Vec<&'a egui::epaint::TextShape>) {
        match shape {
            egui::Shape::Text(text) => out.push(text),
            egui::Shape::Vec(shapes) => {
                for shape in shapes {
                    collect(shape, out);
                }
            }
            _ => {}
        }
    }
    let mut out = Vec::new();
    for shape in &output.shapes {
        collect(&shape.shape, &mut out);
    }
    out
}
fn key(key: egui::Key) -> egui::Event {
    egui::Event::Key {
        key,
        physical_key: Some(key),
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::CTRL,
    }
}

#[test]
fn long_drafts_scroll_to_both_ends_without_moving_controls_or_losing_text() {
    for size in [
        egui::vec2(260.0, 420.0),
        egui::vec2(390.0, 600.0),
        egui::vec2(700.0, 800.0),
    ] {
        let ctx = egui::Context::default();
        theme::apply(&ctx);
        ctx.style_mut_of(egui::Theme::Light, |style| {
            style.scroll_animation = egui::style::ScrollAnimation::none()
        });
        let mut input = (0..400).map(|i| format!("draft {i}\n")).collect::<String>();
        let original = input.clone();
        let id = egui::Id::new("long_draft");
        let mut sent = 0;
        let mut draw = |ui: &mut egui::Ui| {
            let (conversation, footer, input_height) = body(ui, &input);
            assert!(conversation.bottom() <= footer.top());
            ui.scope_builder(
                egui::UiBuilder::new().id_salt("footer").max_rect(footer),
                |ui| {
                    let selectors = ["model", "thinking", "mode"].map(|title| Selector {
                        id: title,
                        title,
                        label: "Long choice label".into(),
                        selected: Some(0),
                        options: vec![Choice {
                            label: "Choice".into(),
                            detail: String::new(),
                        }],
                        enabled: true,
                    });
                    let action = composer::show(
                        ui,
                        id,
                        &mut input,
                        input_height,
                        true,
                        true,
                        true,
                        selectors,
                    );
                    sent += usize::from(action.send);
                    assert!(ui.min_rect().right() <= size.x);
                    assert!(ui.min_rect().bottom() <= size.y);
                },
            );
        };
        frame(&ctx, size, vec![], &mut draw);
        ctx.memory_mut(|m| m.request_focus(id));
        frame(&ctx, size, vec![key(egui::Key::Home)], &mut draw);
        frame(&ctx, size, vec![key(egui::Key::End)], &mut draw);
        let output = frame(&ctx, size, vec![], &mut draw);
        let viewport = ctx
            .data(|d| d.get_temp::<egui::Rect>(id.with("viewport")))
            .unwrap();
        assert!(viewport.height() <= 121.0);
        let scroll_id = ctx
            .data(|d| d.get_temp::<egui::Id>(id.with("scroll_id")))
            .unwrap();
        assert!(
            egui::scroll_area::State::load(&ctx, scroll_id)
                .unwrap()
                .offset
                .y
                > 100.0
        );
        let send = texts(&output)
            .into_iter()
            .find(|text| text.galley.job.text == "Send")
            .unwrap()
            .visual_bounding_rect();
        assert!(send.top() >= viewport.bottom());
        frame(&ctx, size, vec![key(egui::Key::Home)], &mut draw);
        frame(&ctx, size, vec![], &mut draw);
        assert!(
            egui::scroll_area::State::load(&ctx, scroll_id)
                .unwrap()
                .offset
                .y
                < 1.0
        );
        frame(&ctx, size, vec![key(egui::Key::Enter)], &mut draw);
        drop(draw);
        assert_eq!(sent, 1);
        assert_eq!(input, original);
    }
}

#[test]
fn reasoning_is_visible_initially_and_stays_collapsed_when_text_streams() {
    let ctx = egui::Context::default();
    theme::apply(&ctx);
    ctx.style_mut_of(egui::Theme::Light, |style| style.animation_time = 0.0);
    let mut cache = content::CommonMarkCache::default();
    let mut text = "A visible reasoning paragraph".to_owned();
    let size = egui::vec2(390.0, 600.0);
    let mut show = |ui: &mut egui::Ui| content::reasoning(ui, &mut cache, "reasoning-1", &text);
    let output = frame(&ctx, size, vec![], &mut show);
    assert!(
        texts(&output)
            .iter()
            .any(|t| t.galley.job.text.contains("visible reasoning"))
    );
    let pos = texts(&output)
        .into_iter()
        .find(|t| t.galley.job.text == "Reasoning")
        .unwrap()
        .visual_bounding_rect()
        .center();
    for pressed in [true, false] {
        frame(
            &ctx,
            size,
            vec![
                egui::Event::PointerMoved(pos),
                egui::Event::PointerButton {
                    pos,
                    pressed,
                    button: egui::PointerButton::Primary,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
            &mut show,
        );
    }
    drop(show);
    text.push_str(" and more streamed content");
    let output = frame(&ctx, size, vec![], |ui| {
        content::reasoning(ui, &mut cache, "reasoning-1", &text)
    });
    assert!(
        !texts(&output)
            .iter()
            .any(|t| t.galley.job.text.contains("visible reasoning"))
    );
    let output = frame(&ctx, size, vec![], |ui| {
        ui.push_id("other_session", |ui| {
            content::reasoning(ui, &mut cache, "reasoning-1", &text)
        })
        .inner
    });
    assert!(
        texts(&output)
            .iter()
            .any(|t| t.galley.job.text.contains("visible reasoning"))
    );
}

#[test]
fn markdown_formats_headings_and_incomplete_fences_and_copies_exact_code() {
    let ctx = egui::Context::default();
    theme::apply(&ctx);
    let mut cache = content::CommonMarkCache::default();
    let size = egui::vec2(390.0, 800.0);
    let source = "# Heading\n\n**Bold** and `inline`\n\n```shell\necho hello\n  echo indented\n```\n\n| Feature | Status |\n|---|---|\n| Shared panels | Ready |";
    let output = frame(&ctx, size, vec![], |ui| {
        content::markdown(ui, &mut cache, source)
    });
    let text = texts(&output);
    assert!(text.iter().any(|t| t.galley.job.text == "Heading"));
    for word in ["Feature", "Status", "Shared panels", "Ready"] {
        assert!(
            text.iter()
                .any(|t| t.galley.job.text == word && t.galley.rows.len() == 1),
            "table cell wrapped unnecessarily: {word}"
        );
    }
    assert!(!text.iter().any(|t| t.galley.job.text.contains("```shell")));
    assert!(
        text.iter()
            .any(|t| t.galley.job.text.contains("echo hello"))
    );
    let pos = text
        .iter()
        .find(|t| t.galley.job.text == "🗐")
        .unwrap()
        .visual_bounding_rect()
        .center();
    let mut copied = false;
    for pressed in [true, false] {
        let output = frame(
            &ctx,
            size,
            vec![
                egui::Event::PointerMoved(pos),
                egui::Event::PointerButton {
                    pos,
                    pressed,
                    button: egui::PointerButton::Primary,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
            |ui| content::markdown(ui, &mut cache, source),
        );
        copied |= output.platform_output.commands.iter().any(|command| matches!(command,egui::OutputCommand::CopyText(text) if text == "echo hello\n  echo indented"));
    }
    assert!(copied);
    let output = frame(&ctx, size, vec![], |ui| {
        content::markdown(ui, &mut cache, "```unknown\nunfinished code")
    });
    assert!(
        texts(&output)
            .iter()
            .any(|t| t.galley.job.text.contains("unfinished code"))
    );
}
