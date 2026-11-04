use super::*;
fn frame(ui: &mut OpenCodeUi, ctx: &egui::Context, events: Vec<egui::Event>) -> egui::FullOutput {
    ctx.run(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(390.0, 750.0),
            )),
            events,
            ..Default::default()
        },
        |ctx| {
            egui::CentralPanel::default().show(ctx, |panel| ui.show(panel, ctx));
        },
    )
}
fn click(ui: &mut OpenCodeUi, ctx: &egui::Context, label: &str) {
    let output = frame(ui, ctx, vec![]);
    let pos = output
        .shapes
        .iter()
        .find_map(|shape| match &shape.shape {
            egui::epaint::Shape::Text(text) if text.galley.job.text == label => {
                Some(text.pos + text.galley.size() * 0.5)
            }
            _ => None,
        })
        .unwrap_or_else(|| panic!("Missing button {label}"));
    for pressed in [true, false] {
        frame(
            ui,
            ctx,
            vec![
                egui::Event::PointerMoved(pos),
                egui::Event::PointerButton {
                    pos,
                    pressed,
                    button: egui::PointerButton::Primary,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
        );
    }
}
#[test]
fn permission_and_question_buttons_use_the_matching_protocol_paths() {
    let ctx = egui::Context::default();
    let (mut ui, rx, _) = connected();
    ui.permissions
        .push(json!({"id":"p","sessionID":"session1","permission":"edit","patterns":["file.rs"]}));
    click(&mut ui, &ctx, "Allow once");
    let request = rx.recv_timeout(std::time::Duration::from_secs(1)).unwrap();
    assert_eq!(request.path, "/permission/p/reply");
    assert_eq!(request.body.unwrap()["reply"], "once");
    ui.reply(request.id, Ok(Value::Bool(true)));
    assert!(ui.permissions.is_empty());
    ui.questions.push(json!({"id":"q","_v2":true,"sessionID":"session1","questions":[{"header":"Choose","question":"Choose a color","options":[{"label":"Blue","description":"A color"}],"multiple":false}]}));
    click(&mut ui, &ctx, "Blue");
    click(&mut ui, &ctx, "Submit answer");
    let request = rx.recv_timeout(std::time::Duration::from_secs(1)).unwrap();
    assert_eq!(request.path, "/api/session/session1/question/q/reply");
    assert_eq!(request.body.unwrap()["answers"], json!([["Blue"]]));
    ui.reply(request.id, Ok(Value::Bool(true)));
    assert!(ui.questions.is_empty());
}
#[test]
fn composer_cycles_agents_and_ctrl_enter_submits_once() {
    let ctx = egui::Context::default();
    let (mut ui, rx, _) = connected();
    ui.input = "hello".into();
    let mut id = egui::Id::NULL;
    let draw = |ui: &mut OpenCodeUi, events: Vec<egui::Event>, id: &mut egui::Id| {
        let _ = ctx.run(
            egui::RawInput {
                events,
                ..Default::default()
            },
            |ctx| {
                egui::CentralPanel::default().show(ctx, |panel| {
                    *id = panel.make_persistent_id("opencode_composer");
                    ui.composer(panel);
                });
            },
        );
    };
    draw(&mut ui, vec![], &mut id);
    ctx.memory_mut(|m| m.request_focus(id));
    draw(&mut ui, vec![], &mut id);
    let key = |key, modifiers| egui::Event::Key {
        key,
        physical_key: Some(key),
        pressed: true,
        repeat: false,
        modifiers,
    };
    draw(
        &mut ui,
        vec![key(egui::Key::Tab, egui::Modifiers::NONE)],
        &mut id,
    );
    assert_eq!(ui.agent, "build");
    draw(
        &mut ui,
        vec![key(egui::Key::Tab, egui::Modifiers::SHIFT)],
        &mut id,
    );
    assert_eq!(ui.agent, "plan");
    draw(
        &mut ui,
        vec![key(egui::Key::Enter, egui::Modifiers::CTRL)],
        &mut id,
    );
    let request = rx.recv_timeout(std::time::Duration::from_secs(1)).unwrap();
    assert_eq!(request.body.unwrap()["parts"][0]["text"], "hello");
    draw(&mut ui, vec![], &mut id);
    assert!(rx.try_recv().is_err());
    ui.reply(request.id, Ok(Value::Null));
    assert!(ui.input.is_empty());
}
#[test]
fn go_button_selects_build_and_preserves_unsent_draft() {
    let ctx = egui::Context::default();
    let (mut ui, rx, _) = connected();
    ui.input = "draft".into();
    ui.merge_message(
        json!({"id":"plan","role":"assistant","agent":"plan","time":{"created":1,"completed":2}}),
        vec![json!({"id":"text","type":"text","text":"A completed plan"})],
    );
    click(&mut ui, &ctx, "Go");
    let request = rx.recv_timeout(std::time::Duration::from_secs(1)).unwrap();
    assert_eq!(request.body.unwrap()["agent"], "build");
    assert_eq!(ui.input, "draft");
}
