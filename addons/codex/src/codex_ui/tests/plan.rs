use crate::{
    app_server::AppServer,
    codex_ui::{CodexUi, messages::PendingRequest},
    ui_theme as theme,
};
use egui;
use serde_json::{Value, json};
use std::{sync::mpsc::Receiver, time::Duration};

fn event(codex: &mut CodexUi, method: &str, params: Value) {
    codex.handle_event(json!({"method":method,"params":params}), None);
}

fn start_plan(codex: &mut CodexUi, status: &str) {
    event(
        codex,
        "turn/started",
        json!({"threadId":"current","turn":{"id":"turn1"}}),
    );
    event(
        codex,
        "item/completed",
        json!({"threadId":"current","turnId":"turn1","item":{"type":"plan","id":"plan1","text":"The finished plan"}}),
    );
    event(
        codex,
        "turn/completed",
        json!({"threadId":"current","turn":{"id":"turn1","status":status}}),
    );
}

fn ready_panel() -> (CodexUi, Receiver<Value>) {
    let (server, requests) = AppServer::capture_requests();
    let mut codex = CodexUi::default();
    codex.server = Some(server);
    codex.thread_id = Some("current".into());
    codex.model = "chosen-model".into();
    codex.effort = Some("high".into());
    codex.modes = vec![
        json!({"mode":"plan","name":"Plan"}),
        json!({"mode":"default","name":"Default","reasoning_effort":"low"}),
    ];
    codex.mode = Some(codex.modes[0].clone());
    codex.input = "My unsent draft".into();
    start_plan(&mut codex, "completed");
    (codex, requests)
}

#[test]
fn go_switches_mode_sends_once_and_preserves_settings_and_draft() {
    let (mut codex, requests) = ready_panel();
    assert_eq!(codex.go_plan_id(), Some("plan1"));
    let id = codex.go().unwrap();
    let request = requests.recv_timeout(Duration::from_secs(2)).unwrap();
    assert_eq!(request["id"], id);
    assert_eq!(request["method"], "turn/start");
    assert_eq!(request["params"]["threadId"], "current");
    assert_eq!(
        request["params"]["input"],
        json!([{"type":"text","text":"Go"}])
    );
    assert_eq!(request["params"]["collaborationMode"]["mode"], "default");
    assert_eq!(request["params"]["model"], "chosen-model");
    assert_eq!(request["params"]["effort"], "high");
    assert_eq!(
        request["params"]["collaborationMode"]["settings"]["reasoning_effort"],
        "high"
    );
    assert_eq!(codex.mode.as_ref().unwrap()["mode"], "default");
    assert_eq!(codex.input, "My unsent draft");
    assert!(codex.busy);
    assert!(codex.go().is_err());
    assert!(requests.try_recv().is_err());
    assert_eq!(codex.requests.len(), 1);
    assert_eq!(
        codex
            .messages
            .iter()
            .filter(|m| m.kind == "You" && m.text == "Go")
            .count(),
        1
    );
}

#[test]
fn failed_submission_restores_plan_and_allows_only_explicit_retry() {
    let (mut codex, requests) = ready_panel();
    let id = codex.go().unwrap();
    requests.recv_timeout(Duration::from_secs(2)).unwrap();
    codex.handle_event(json!({"id":id,"error":{"message":"Not accepted"}}), None);
    assert!(!codex.busy);
    assert_eq!(codex.mode.as_ref().unwrap()["mode"], "plan");
    assert_eq!(codex.go_plan_id(), Some("plan1"));
    assert_eq!(codex.input, "My unsent draft");
    assert!(codex.messages.last().unwrap().text.contains("Not accepted"));
    assert!(requests.try_recv().is_err());
    assert!(codex.go().is_ok());
    assert_eq!(
        requests.recv_timeout(Duration::from_secs(2)).unwrap()["params"]["input"][0]["text"],
        "Go"
    );

    let (mut codex, _) = ready_panel();
    codex.server.as_mut().unwrap().disconnect_capture();
    assert!(codex.go().is_err());
    assert_eq!(codex.mode.as_ref().unwrap()["mode"], "plan");
    assert_eq!(codex.go_plan_id(), Some("plan1"));
    assert_eq!(codex.input, "My unsent draft");
    assert!(!codex.busy);
    assert!(!codex.messages.iter().any(|m| m.kind == "You"));
}

#[test]
fn go_is_unavailable_for_missing_modes_busy_pending_or_disconnected_sessions() {
    let (mut codex, requests) = ready_panel();
    codex.modes.retain(|m| m["mode"] == "plan");
    assert_eq!(
        codex.go_unavailable_reason(),
        Some("Default mode is not available")
    );
    assert!(codex.go().is_err());
    codex.modes.push(json!({"mode":"default","name":"Default"}));
    codex.busy = true;
    assert!(codex.go().is_err());
    codex.busy = false;
    codex.pending.push(PendingRequest {
        id: json!(9),
        method: "requestUserInput".into(),
        prompt: "Question".into(),
        answer_key: None,
    });
    assert!(codex.go().is_err());
    codex.pending.clear();
    codex.requests.insert(99, "thread/resume".into());
    assert!(codex.go().is_err());
    codex.requests.clear();
    codex.server = None;
    assert!(codex.go().is_err());
    assert!(requests.try_recv().is_err());
    assert_eq!(codex.mode.as_ref().unwrap()["mode"], "plan");
}

#[test]
fn incomplete_failed_and_old_plans_are_not_ready() {
    let (mut codex, _) = ready_panel();
    event(
        &mut codex,
        "turn/completed",
        json!({"threadId":"current","turn":{"id":"different-turn","status":"completed"}}),
    );
    assert!(
        codex.go_plan_id().is_none(),
        "A completion from another turn must not reuse the old plan"
    );
    for status in ["failed", "interrupted"] {
        start_plan(&mut codex, status);
        assert!(codex.go_plan_id().is_none());
    }
    event(
        &mut codex,
        "turn/started",
        json!({"threadId":"current","turn":{"id":"turn2"}}),
    );
    event(
        &mut codex,
        "item/started",
        json!({"threadId":"current","turnId":"turn2","item":{"type":"plan","id":"partial","text":"Still writing"}}),
    );
    event(
        &mut codex,
        "turn/completed",
        json!({"threadId":"current","turn":{"id":"turn2","status":"completed"}}),
    );
    assert!(codex.go_plan_id().is_none());
    start_plan(&mut codex, "completed");
    assert_eq!(codex.go_plan_id(), Some("plan1"));
    event(
        &mut codex,
        "turn/started",
        json!({"threadId":"current","turn":{"id":"turn3"}}),
    );
    event(
        &mut codex,
        "item/completed",
        json!({"threadId":"current","turnId":"turn1","item":{"type":"plan","id":"late","text":"Old plan"}}),
    );
    event(
        &mut codex,
        "turn/completed",
        json!({"threadId":"current","turn":{"id":"turn3","status":"completed"}}),
    );
    assert!(codex.go_plan_id().is_none());
}

#[test]
fn readiness_is_restored_only_for_the_latest_completed_plan_and_cleared_on_switch() {
    let (mut codex, requests) = ready_panel();
    for (status, items, expected) in [
        (
            "completed",
            json!([{"id":"restored","type":"plan","text":"Restored plan"}]),
            Some("restored"),
        ),
        (
            "completed",
            json!([{"id":"answer","type":"agentMessage","text":"Work done"}]),
            None,
        ),
        (
            "failed",
            json!([{"id":"failed","type":"plan","text":"Failed plan"}]),
            None,
        ),
    ] {
        codex.requests.insert(99, "thread/resume".into());
        codex.handle_event(json!({"id":99,"result":{"model":"chosen-model","thread":{"id":"current","turns":[{"id":"old","status":"completed","items":[{"id":"old-plan","type":"plan","text":"Old"}]},{"id":"latest","status":status,"items":items}]}}}), None);
        assert_eq!(codex.go_plan_id(), expected);
    }
    start_plan(&mut codex, "completed");
    let id = codex.go().unwrap();
    requests.recv_timeout(Duration::from_secs(2)).unwrap();
    codex.open_thread(None, Some("different"));
    assert_eq!(
        requests.recv_timeout(Duration::from_secs(2)).unwrap()["method"],
        "thread/resume"
    );
    codex.handle_event(json!({"id":id,"error":{"message":"Late failure"}}), None);
    assert!(codex.go_plan_id().is_none());
    codex.stop();
    assert!(codex.go_plan_id().is_none());
}

#[test]
fn go_button_can_be_clicked_in_narrow_and_standard_conversation_panels() {
    fn go_position(shape: &egui::Shape) -> Option<egui::Pos2> {
        match shape {
            egui::Shape::Text(text) if text.galley.job.text == "Go" => {
                Some(text.pos + text.galley.size() * 0.5)
            }
            egui::Shape::Vec(shapes) => shapes.iter().find_map(go_position),
            _ => None,
        }
    }
    for width in [260.0, 390.0] {
        let (mut codex, requests) = ready_panel();
        let ctx = egui::Context::default();
        theme::apply(&ctx);
        let frame = |codex: &mut CodexUi, events: Vec<egui::Event>| {
            crate::test_frame(
                &ctx,
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(width, 700.0),
                    )),
                    events,
                    ..Default::default()
                },
                |root| {
                    egui::CentralPanel::default().show(root, |ui| {
                        codex.show_conversation(ui);
                        assert!(ui.min_rect().right() <= ui.clip_rect().right() + 1.0);
                    });
                },
            )
        };
        frame(&mut codex, vec![]);
        let output = frame(&mut codex, vec![]);
        let position = output
            .shapes
            .iter()
            .find_map(|shape| go_position(&shape.shape))
            .expect("Ready plan must have a Go button");
        frame(
            &mut codex,
            vec![
                egui::Event::PointerMoved(position),
                egui::Event::PointerButton {
                    pos: position,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
        );
        frame(
            &mut codex,
            vec![egui::Event::PointerButton {
                pos: position,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: egui::Modifiers::NONE,
            }],
        );
        let request = requests.recv_timeout(Duration::from_secs(2)).unwrap();
        assert_eq!(request["params"]["input"][0]["text"], "Go");
        assert_eq!(request["params"]["collaborationMode"]["mode"], "default");
    }
}
