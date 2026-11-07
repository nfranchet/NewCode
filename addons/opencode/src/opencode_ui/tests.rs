use super::*;
use serde_json::json;
use std::sync::mpsc;
fn connected() -> (
    OpenCodeUi,
    mpsc::Receiver<crate::opencode_server::Request>,
    mpsc::Sender<crate::opencode_server::Event>,
) {
    let (server, rx, tx) = Server::capture();
    let ui = OpenCodeUi {
        server: Some(server),
        online: true,
        stream_online: true,
        session: Some("session1".into()),
        workspace: std::env::current_dir().unwrap(),
        agent: "plan".into(),
        agents: vec!["plan".into(), "build".into()],
        model: Some(("provider".into(), "model".into())),
        ..Default::default()
    };
    (ui, rx, tx)
}
#[test]
fn prompt_go_and_failure_preserve_draft_and_provider_choices() {
    let (mut ui, rx, _) = connected();
    ui.input = "unsent draft".into();
    ui.variant = "high".into();
    ui.merge_message(json!({"id":"msg1","sessionID":"session1","role":"assistant","agent":"plan","time":{"created":1,"completed":2}}),vec![json!({"id":"part1","type":"text","text":"The plan"})]);
    assert!(ui.go_ready());
    ui.submit(true);
    let request = rx.recv().unwrap();
    assert_eq!(request.body.as_ref().unwrap()["agent"], "build");
    assert_eq!(request.body.as_ref().unwrap()["variant"], "high");
    assert_eq!(request.body.as_ref().unwrap()["parts"][0]["text"], "Go");
    ui.submit(true);
    assert!(rx.try_recv().is_err());
    ui.reply(request.id, Err("failed".into()));
    assert_eq!(ui.agent, "plan");
    assert_eq!(ui.input, "unsent draft");
    assert!(ui.go_ready());
}
#[test]
fn stream_merges_identity_and_rejects_other_sessions_and_stale_replies() {
    let (mut ui, _, _) = connected();
    let info = json!({"id":"msg","sessionID":"session1","role":"assistant","time":{"created":1}});
    ui.event(json!({"type":"message.updated","properties":{"info":info}}));
    let part =
        json!({"id":"part","messageID":"msg","sessionID":"session1","type":"text","text":"hello"});
    ui.event(json!({"type":"message.part.updated","properties":{"part":part}}));
    ui.event(json!({"type":"message.part.delta","properties":{"sessionID":"session1","messageID":"msg","partID":"part","field":"text","delta":" world"}}));
    ui.event(json!({"type":"message.part.delta","properties":{"sessionID":"other","messageID":"msg","partID":"part","field":"text","delta":" wrong"}}));
    assert_eq!(ui.messages[0].parts[0]["text"], "hello world");
    assert_eq!(ui.messages.len(), 1);
    ui.requests.insert(
        99,
        Pending {
            epoch: 0,
            operation: Operation::History(false),
        },
    );
    ui.epoch = 1;
    ui.reply(99, Ok(json!([{ "info":{"id":"stale"},"parts":[]}])));
    assert_eq!(ui.messages.len(), 1);
}
#[test]
fn catalog_defaults_are_plan_first_and_variants_follow_connected_model() {
    let mut ui = OpenCodeUi::default();
    ui.load_agents(json!([{"name":"build","mode":"primary"},{"name":"plan","mode":"primary"},{"name":"hidden","mode":"primary","hidden":true},{"name":"explore","mode":"subagent"}]));
    assert_eq!(ui.agent, "plan");
    assert_eq!(ui.agents.len(), 2);
    ui.load_models(json!({"connected":["p"],"all":[{"id":"p","name":"Provider","models":{"m":{"name":"Model","variants":{"high":{},"disabled":{"disabled":true}},"limit":{"context":1000}}}},{"id":"other","models":{"bad":{}}}],"default":{"p":"m"}}));
    assert_eq!(ui.models.len(), 1);
    assert_eq!(ui.selected_model().unwrap().variants, ["high"]);
    assert_eq!(ui.model, Some(("p".into(), "m".into())));
}

impl OpenCodeUi {
    pub(crate) fn test_connection() -> (
        Self,
        mpsc::Receiver<crate::opencode_server::Request>,
        mpsc::Sender<crate::opencode_server::Event>,
    ) {
        let (mut ui, rx, tx) = connected();
        ui.input = "OpenCode draft".into();
        (ui, rx, tx)
    }
    pub(crate) fn test_draft(&self) -> &str {
        &self.input
    }
}

#[test]
fn next_stream_and_replayed_events_do_not_duplicate_text() {
    let (mut ui, _, _) = connected();
    ui.event(json!({"id":"start","type":"session.next.step.started","properties":{"sessionID":"session1","assistantMessageID":"assistant","timestamp":1,"agent":"plan"}}));
    for event in [
        json!({"id":"text","type":"session.next.text.started","properties":{"sessionID":"session1","assistantMessageID":"assistant","textID":"text1"}}),
        json!({"id":"delta","type":"session.next.text.delta","properties":{"sessionID":"session1","assistantMessageID":"assistant","textID":"text1","delta":"A plan"}}),
        json!({"id":"delta","type":"session.next.text.delta","properties":{"sessionID":"session1","assistantMessageID":"assistant","textID":"text1","delta":"A plan"}}),
    ] {
        ui.event(event);
    }
    assert_eq!(ui.messages[0].parts[0]["text"], "A plan");
    assert!(ui.busy);
    ui.event(json!({"type":"permission.v2.asked","properties":{"sessionID":"session1","id":"p1","action":"edit","resources":["file.rs"]}}));
    assert_eq!(ui.state(), "Waiting for you");
    assert_eq!(ui.permissions[0]["_v2"], true);
}

#[test]
fn permissions_questions_and_session_resume_are_isolated() {
    let (mut ui, rx, _) = connected();
    ui.event(json!({"type":"question.asked","properties":{"id":"q1","sessionID":"other","questions":[]}}));
    assert!(ui.questions.is_empty());
    ui.event(json!({"type":"question.asked","properties":{"id":"q1","sessionID":"session1","questions":[]}}));
    assert_eq!(ui.state(), "Waiting for you");
    ui.stop();
    assert_eq!(ui.state(), "Offline");
    assert_eq!(ui.session.as_deref(), Some("session1"));
    assert!(ui.questions.is_empty());
    assert!(rx.try_recv().is_err());
}

#[test]
fn history_is_merged_without_duplicates_and_usage_is_reported_once() {
    let (mut ui, _, _) = connected();
    let row = json!({"info":{"id":"m1","role":"assistant","time":{"created":1},"cost":0.01,"tokens":{"input":4,"output":6}},"parts":[{"id":"t1","type":"text","text":"answer"}]});
    for id in [1, 2] {
        ui.requests.insert(
            id,
            Pending {
                epoch: 0,
                operation: Operation::History(false),
            },
        );
        ui.reply(id, Ok(json!([row])));
    }
    assert_eq!(ui.messages.len(), 1);
    assert_eq!(ui.messages[0].parts.len(), 1);
}

#[test]
fn opencode_ui_fits_narrow_and_standard_panels() {
    for width in [260.0, 390.0, 650.0] {
        let ctx = egui::Context::default();
        crate::ui_theme::apply(&ctx);
        let (mut ui, _, _) = connected();
        ui.input = "Long draft line\n".repeat(300);
        ui.models.push(catalog::Model {
            provider: "provider".into(),
            id: "model".into(),
            label: "A long provider / model name".into(),
            variants: vec!["high".into()],
            context: Some(100000),
        });
        ui.messages.push(Message {
            id: "message".into(),
            info: json!({"role":"assistant"}),
            parts: vec![json!({"id":"text","type":"text","text":"a_very_long_line_".repeat(80)})],
        });
        for _ in 0..3 {
            let _ = crate::test_frame(
                &ctx,
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(width, 650.0),
                    )),
                    ..Default::default()
                },
                |root| {
                    egui::CentralPanel::default().show(root, |panel| ui.show(panel, &ctx));
                    assert!(
                        ctx.globally_used_rect().right() <= width + 1.0,
                        "OpenCode overflows: {:?}",
                        ctx.globally_used_rect()
                    );
                    assert!(
                        ctx.globally_used_rect().bottom() <= 651.0,
                        "OpenCode overflows vertically: {:?}",
                        ctx.globally_used_rect()
                    );
                },
            );
        }
    }
}

mod interactions;

#[test]
fn bootstrap_restores_only_current_workspace_and_reconnect_keeps_history() {
    let (mut ui, rx, tx) = connected();
    ui.session = None;
    ui.online = false;
    tx.send(crate::opencode_server::Event::Ready("1.19.5".into()))
        .unwrap();
    ui.process_events();
    while let Ok(request) = rx.try_recv() {
        let operation = ui.requests[&request.id].operation.clone();
        let result = match operation {
            Operation::Providers => {
                json!({"connected":["p"],"default":{"p":"m"},"all":[{"id":"p","name":"Provider","models":{"m":{"name":"Model"}}}]})
            }
            Operation::Agents => {
                json!([{"name":"build","mode":"primary"},{"name":"plan","mode":"primary"}])
            }
            Operation::Sessions(_) => {
                json!([{"id":"wrong","directory":"/other/workspace","time":{"updated":9}},{"id":"saved","directory":ui.workspace,"title":"Saved session","time":{"updated":1}}])
            }
            Operation::Session => json!({"id":"saved"}),
            Operation::History(_) => {
                json!([{"info":{"id":"answer","role":"assistant","agent":"plan","time":{"created":1,"completed":2}},"parts":[{"id":"text","type":"text","text":"A plan"}]}])
            }
            Operation::Status => json!({"saved":{"type":"idle"}}),
            Operation::Permissions
            | Operation::Questions
            | Operation::PermissionsV2
            | Operation::QuestionsV2 => json!([]),
            _ => panic!("unexpected bootstrap operation"),
        };
        ui.reply(request.id, Ok(result));
    }
    assert_eq!(ui.session.as_deref(), Some("saved"));
    assert_eq!(ui.sessions.len(), 1);
    assert_eq!(ui.state(), "Ready");
    assert!(ui.can_send());
    tx.send(crate::opencode_server::Event::Disconnected(
        "temporary".into(),
    ))
    .unwrap();
    ui.process_events();
    assert!(!ui.can_send());
    assert_eq!(ui.messages.len(), 1);
    tx.send(crate::opencode_server::Event::Reconnected).unwrap();
    ui.process_events();
    assert_eq!(ui.messages.len(), 1);
    assert!(rx.try_recv().is_ok());
}

#[test]
fn transport_queue_failure_does_not_leave_go_running() {
    let (mut ui, rx, _) = connected();
    drop(rx);
    ui.input = "draft".into();
    ui.merge_message(
        json!({"id":"plan","role":"assistant","agent":"plan","time":{"created":1,"completed":2}}),
        vec![json!({"id":"text","type":"text","text":"A plan"})],
    );
    ui.submit(true);
    assert!(!ui.busy);
    assert_eq!(ui.agent, "plan");
    assert_eq!(ui.input, "draft");
    assert!(ui.error.is_some());
}
