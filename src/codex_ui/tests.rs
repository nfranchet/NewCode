mod plan;
mod reasoning;

use super::{
    CodexUi,
    catalog::{resolve_effort, startup_mode},
    messages::{Message, history_message},
    usage::usage_title,
};
use crate::ui_theme as theme;
use eframe::egui;
use serde_json::{Value, json};

#[test]
fn thinking_levels_follow_model_capabilities() {
    let model = json!({"defaultReasoningEffort":"medium","supportedReasoningEfforts":[{"reasoningEffort":"medium"},{"reasoningEffort":"high"}]});
    assert_eq!(
        resolve_effort(Some(&model), Some("high")),
        Some("high".into())
    );
    assert_eq!(
        resolve_effort(Some(&model), Some("low")),
        Some("medium".into())
    );
    assert_eq!(resolve_effort(Some(&model), None), Some("medium".into()));
    assert_eq!(
        resolve_effort(Some(&json!({"supportedReasoningEfforts":[]})), Some("high")),
        None
    );
}

#[test]
fn startup_prefers_plan_then_first_available_mode() {
    let agent = json!({"name":"Agent","mode":"default"});
    let plan = json!({"name":"Plan","mode":"plan"});
    assert_eq!(startup_mode(&[agent.clone(), plan.clone()]), Some(plan));
    assert_eq!(
        startup_mode(&[agent.clone(), json!({"mode":"other"})]),
        Some(agent)
    );
    assert_eq!(startup_mode(&[]), None);
}

#[test]
fn long_messages_fit_narrow_panel() {
    let ctx = egui::Context::default();
    theme::apply(&ctx);
    let mut codex = CodexUi::default();
    codex.model = "long-model-name-".repeat(20);
    codex.messages.push(Message::new(
        "Codex",
        "long/path/without/spaces/".repeat(100),
    ));
    codex
        .messages
        .push(Message::new("Command", "command ".repeat(100)));
    codex.messages.push(Message::new(
        "Proposed plan",
        "A long proposed plan with several steps. ".repeat(60),
    ));
    for size in [
        egui::vec2(260.0, 420.0),
        egui::vec2(390.0, 600.0),
        egui::vec2(700.0, 800.0),
    ] {
        for busy in [false, true] {
            codex.busy = busy;
            for _ in 0..3 {
                let _ = crate::test_frame(
                    &ctx,
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
                        ..Default::default()
                    },
                    |root| {
                        egui::CentralPanel::default().show(root, |ui| {
                            codex.show(ui, None, &ctx);
                            assert!(
                                ui.min_rect().right() <= ui.clip_rect().right() + 1.0,
                                "Codex content exceeds panel: {:?}",
                                ui.min_rect()
                            );
                            let composer_id = ui
                                .make_persistent_id("assistant_composer_area")
                                .with("codex_composer");
                            let composer = ctx
                                .data(|data| {
                                    data.get_temp::<egui::Rect>(composer_id.with("viewport"))
                                })
                                .unwrap();
                            assert!(
                                composer.bottom() <= ui.clip_rect().bottom() + 1.0,
                                "Composer hidden below panel: {:?}",
                                composer
                            );
                        });
                    },
                );
            }
        }
    }
}

#[test]
fn restores_user_and_command_history() {
    let user = history_message(&json!({"type":"userMessage","id":"u1","content":[{"type":"text","text":"Fix this"},{"type":"text","text":"Keep tests"}]})).unwrap();
    assert_eq!(user.text, "Fix this\nKeep tests");
    assert_eq!(user.kind, "You");
    let command = history_message(&json!({"type":"commandExecution","id":"c1","command":"cargo test","aggregatedOutput":"passed"})).unwrap();
    assert_eq!(command.id, "c1");
    assert_eq!(command.output, "passed");
    assert!(history_message(&json!({"type":"unknown"})).is_none());
}

#[test]
fn usage_title_reports_remaining_quota_without_inventing_limits() {
    assert_eq!(
        usage_title(&json!({"rateLimits":{"primary":{"usedPercent":35}}})),
        "Usage / status · 65% remaining"
    );
    assert_eq!(usage_title(&Value::Null), "Usage / status");
}

#[test]
fn routes_streamed_items_without_duplicates_or_cross_thread_updates() {
    let mut codex = CodexUi::default();
    codex.thread_id = Some("current".into());
    let notification = |method: &str, mut params: Value| {
        params["threadId"] = json!("current");
        json!({"method":method,"params":params})
    };
    codex.handle_event(
        notification("turn/started", json!({"turn":{"id":"turn1"}})),
        None,
    );
    assert!(codex.busy);
    assert_eq!(codex.turn_id.as_deref(), Some("turn1"));
    codex.handle_event(json!({"method":"item/plan/delta","params":{"threadId":"other","itemId":"p1","delta":"ignore"}}), None);
    assert!(codex.messages.is_empty());
    codex.handle_event(
        notification(
            "item/started",
            json!({"item":{"id":"p1","type":"plan","text":""}}),
        ),
        None,
    );
    codex.handle_event(
        notification(
            "item/plan/delta",
            json!({"itemId":"p1","delta":"Draft plan"}),
        ),
        None,
    );
    assert_eq!(codex.messages[0].text, "Draft plan");
    codex.handle_event(
        notification(
            "item/completed",
            json!({"item":{"id":"p1","type":"plan","text":"Final plan"}}),
        ),
        None,
    );
    assert_eq!(codex.messages.len(), 1);
    assert_eq!(codex.messages[0].text, "Final plan");
    assert!(codex.messages[0].complete);
    codex.handle_event(
        notification(
            "item/reasoning/summaryTextDelta",
            json!({"itemId":"r1","delta":"Summary"}),
        ),
        None,
    );
    codex.handle_event(
        notification(
            "item/completed",
            json!({"item":{"id":"r1","type":"reasoning","summary":[]}}),
        ),
        None,
    );
    assert_eq!(codex.messages[1].text, "Summary");
    codex.handle_event(
        notification(
            "item/started",
            json!({"item":{"id":"skip","type":"unsupported"}}),
        ),
        None,
    );
    assert_eq!(codex.messages.len(), 2);
    codex.handle_event(
        notification(
            "turn/completed",
            json!({"turn":{"id":"turn1","status":"completed"}}),
        ),
        None,
    );
    assert!(!codex.busy);
    assert!(codex.turn_id.is_none());
    assert_eq!(codex.messages.last().unwrap().text, "Plan ready");
}

#[test]
fn routes_responses_requests_and_shutdown_independently() {
    let mut codex = CodexUi::default();
    codex.requests.insert(7, "collaborationMode/list".into());
    let plan = json!({"name":"Plan","mode":"plan"});
    codex.handle_event(
        json!({"id":7,"result":{"data":[{"name":"Default","mode":"default"},plan.clone()]}}),
        None,
    );
    assert_eq!(codex.mode, Some(plan));
    assert!(codex.requests.is_empty());
    codex.handle_event(json!({"id":9,"method":"item/tool/requestUserInput","params":{"questions":[{"id":"choice","question":"Choose one"}]}}), None);
    assert_eq!(codex.pending.len(), 1);
    assert_eq!(codex.pending[0].id, json!(9));
    assert_eq!(codex.pending[0].prompt, "Choose one");
    assert_eq!(codex.pending[0].answer_key.as_deref(), Some("choice"));
    codex.busy = true;
    codex.requests.insert(10, "turn/start".into());
    codex.handle_event(json!({"id":10,"error":{"message":"Rejected"}}), None);
    assert!(!codex.busy);
    assert_eq!(codex.messages.last().unwrap().text, "turn/start: Rejected");
    let status = codex.status.clone();
    codex.handle_event(json!({"_log":"debug output"}), None);
    assert_eq!(codex.status, status);
    codex.thread_id = Some("current".into());
    codex.turn_id = Some("turn1".into());
    codex.busy = true;
    codex.requests.insert(11, "account/rateLimits/read".into());
    codex.handle_event(json!({"_closed":true}), None);
    assert!(!codex.busy);
    assert!(codex.thread_id.is_none() && codex.turn_id.is_none());
    assert!(codex.pending.is_empty() && codex.requests.is_empty());
}
