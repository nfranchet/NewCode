use crate::{
    codex_ui::{
        CodexUi,
        messages::{Message, history_message},
    },
    ui_theme as theme,
};
use egui;
use serde_json::{Value, json};

fn notification(codex: &mut CodexUi, method: &str, mut params: Value) {
    codex.thread_id = Some("current".into());
    params["threadId"] = json!("current");
    codex.handle_event(json!({"method": method, "params": params}), None);
}

#[test]
fn history_and_live_snapshots_use_the_same_readable_reasoning() {
    for (fields, expected) in [
        (
            json!({"summary":["Summary one", "Summary two"],"content":["Other text"]}),
            "Summary one\n\nSummary two",
        ),
        (
            json!({"summary":[],"content":["Supplied content", "Next section"]}),
            "Supplied content\n\nNext section",
        ),
        (
            json!({"content":["Content without a summary field"]}),
            "Content without a summary field",
        ),
        (
            json!({"summary":[{"type":"summary_text","text":"Readable summary"}]}),
            "Readable summary",
        ),
        (
            json!({"content":[{"text":"Readable content"},{"encrypted_content":"opaque"}]}),
            "Readable content",
        ),
        (json!({"summary":["  "],"content":[]}), ""),
    ] {
        let mut item = fields;
        item["type"] = json!("reasoning");
        item["id"] = json!("r1");
        let restored = history_message(&item).unwrap();
        assert_eq!(restored.text, expected);
        assert!(restored.complete);
        let mut codex = CodexUi::default();
        notification(&mut codex, "item/completed", json!({"item":item}));
        assert_eq!(codex.messages[0].text, restored.text);
    }
}

#[test]
fn indexed_streams_preserve_sections_and_prefer_summary_without_duplicates() {
    let mut codex = CodexUi::default();
    notification(
        &mut codex,
        "item/started",
        json!({"item":{"type":"reasoning","id":"r1","summary":[]}}),
    );
    notification(
        &mut codex,
        "item/reasoning/textDelta",
        json!({"itemId":"r1","contentIndex":0,"delta":"Content fallback"}),
    );
    assert_eq!(codex.messages[0].text, "Content fallback");
    notification(
        &mut codex,
        "item/reasoning/textDelta",
        json!({"itemId":"r1","contentIndex":1,"delta":"Another block"}),
    );
    assert_eq!(codex.messages[0].text, "Content fallback\n\nAnother block");
    notification(
        &mut codex,
        "item/reasoning/summaryPartAdded",
        json!({"itemId":"r1","summaryIndex":1}),
    );
    notification(
        &mut codex,
        "item/reasoning/summaryTextDelta",
        json!({"itemId":"r1","summaryIndex":1,"delta":"Second"}),
    );
    notification(
        &mut codex,
        "item/reasoning/summaryTextDelta",
        json!({"itemId":"r1","summaryIndex":0,"delta":"First"}),
    );
    notification(
        &mut codex,
        "item/reasoning/summaryTextDelta",
        json!({"itemId":"r1","summaryIndex":0,"delta":" section"}),
    );
    notification(
        &mut codex,
        "item/reasoning/summaryPartAdded",
        json!({"itemId":"r1","summaryIndex":1}),
    );
    assert_eq!(codex.messages[0].text, "First section\n\nSecond");
    assert!(!codex.messages[0].complete);
    notification(
        &mut codex,
        "item/completed",
        json!({"item":{"type":"reasoning","id":"r1","summary":[],"content":[]}}),
    );
    assert_eq!(codex.messages[0].text, "First section\n\nSecond");
    assert!(codex.messages[0].complete);
    notification(
        &mut codex,
        "item/completed",
        json!({"item":{"type":"reasoning","id":"r1","summary":["Final summary"],"content":["Final content"]}}),
    );
    assert_eq!(codex.messages.len(), 1);
    assert_eq!(codex.messages[0].text, "Final summary");
}

#[test]
fn reasoning_part_events_respect_thread_isolation_and_shutdown() {
    let mut codex = CodexUi::default();
    codex.thread_id = Some("current".into());
    codex.busy = true;
    codex.handle_event(json!({"method":"item/reasoning/summaryPartAdded","params":{"threadId":"other","itemId":"r1","summaryIndex":0}}), None);
    assert!(codex.messages.is_empty());
    notification(
        &mut codex,
        "item/reasoning/summaryPartAdded",
        json!({"itemId":"r1","summaryIndex":0}),
    );
    assert!(!codex.messages[0].complete);
    codex.handle_event(json!({"_closed":true}), None);
    assert!(codex.messages[0].complete);
    assert!(!codex.busy);
}

fn painted_text(codex: &mut CodexUi) -> Vec<String> {
    fn collect(shape: &egui::Shape, text: &mut Vec<String>) {
        match shape {
            egui::Shape::Text(shape) => text.push(shape.galley.job.text.clone()),
            egui::Shape::Vec(shapes) => {
                for shape in shapes {
                    collect(shape, text);
                }
            }
            _ => {}
        }
    }
    let ctx = egui::Context::default();
    theme::apply(&ctx);
    let mut text = Vec::new();
    for _ in 0..2 {
        text.clear();
        let output = crate::test_frame(
            &ctx,
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(450.0, 800.0),
                )),
                ..Default::default()
            },
            |root| {
                egui::CentralPanel::default().show(root, |ui| codex.show_conversation(ui));
            },
        );
        for shape in output.shapes {
            collect(&shape.shape, &mut text);
        }
    }
    text
}

#[test]
fn empty_reasoning_shows_thinking_only_until_completion_and_keeps_the_plan() {
    let mut codex = CodexUi::default();
    codex.busy = true;
    notification(
        &mut codex,
        "item/started",
        json!({"item":{"type":"reasoning","id":"r1","summary":[],"content":[]}}),
    );
    let text = painted_text(&mut codex);
    assert!(text.iter().any(|s| s == "Thinking…"));
    assert!(!text.iter().any(|s| s.contains("No reasoning summary")));
    codex
        .messages
        .push(Message::new("Proposed plan", "Visible final plan".into()));
    notification(
        &mut codex,
        "turn/completed",
        json!({"turn":{"id":"turn1","status":"completed"}}),
    );
    let text = painted_text(&mut codex);
    assert!(
        !text.iter().any(|s| s.contains("Thinking")
            || s.contains("No reasoning summary")
            || s == "Reasoning")
    );
    assert!(text.iter().any(|s| s == "Visible final plan"));
    assert!(text.iter().any(|s| s == "Plan ready"));
}

#[test]
fn new_turns_request_summaries_without_changing_model_effort_or_mode() {
    let mut codex = CodexUi::default();
    codex.model = "selected-model".into();
    codex.effort = Some("high".into());
    for mode in ["plan", "default"] {
        codex.mode = Some(json!({"mode":mode,"reasoning_effort":"low"}));
        let params = codex.turn_params("thread1", "Do the work");
        assert_eq!(params["summary"], "auto");
        assert_eq!(params["model"], "selected-model");
        assert_eq!(params["effort"], "high");
        assert_eq!(params["collaborationMode"]["mode"], mode);
        assert_eq!(
            params["collaborationMode"]["settings"]["reasoning_effort"],
            "high"
        );
        assert_eq!(params["threadId"], "thread1");
        assert_eq!(params["input"][0]["text"], "Do the work");
    }
}
