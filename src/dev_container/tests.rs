//! Failure logging happens once on state transitions, never during rendering.
use super::*;
use std::{cell::RefCell, sync::Once};
thread_local! {static RECORDS:RefCell<Option<Vec<(log::Level,String)>>> = const {RefCell::new(None)};}
struct Capture;
impl log::Log for Capture {
    fn enabled(&self, metadata: &log::Metadata<'_>) -> bool {
        metadata.target() == "focus_editor::dev_container"
    }
    fn log(&self, record: &log::Record<'_>) {
        if self.enabled(record.metadata()) {
            RECORDS.with(|records| {
                if let Some(records) = records.borrow_mut().as_mut() {
                    records.push((record.level(), record.args().to_string()));
                }
            });
        }
    }
    fn flush(&self) {}
}
fn capture() {
    static INIT: Once = Once::new();
    static LOGGER: Capture = Capture;
    INIT.call_once(|| {
        log::set_logger(&LOGGER).unwrap();
        log::set_max_level(log::LevelFilter::Trace);
    });
    RECORDS.with(|records| *records.borrow_mut() = Some(Vec::new()));
}
fn records() -> Vec<(log::Level, String)> {
    RECORDS.with(|records| records.borrow().as_ref().unwrap().clone())
}
fn deliver(container: &mut DevContainer, event: Event) {
    let (tx, rx) = mpsc::channel();
    container.events = Some(rx);
    tx.send(event).unwrap();
    assert!(container.tick());
    assert!(!container.tick());
}
#[test]
fn errors_are_logged_once_and_never_expand_the_menu() {
    capture();
    let configuration = fallback_configuration(&None);
    let mut container = DevContainer::default();
    deliver(
        &mut container,
        Event::Started(
            configuration.clone(),
            Err(format!(
                "Cannot connect to Docker: {}\nTOKEN=private-value",
                "diagnostic ".repeat(300)
            )),
            "rebuild",
        ),
    );
    let first = records();
    assert_eq!(first.len(), 2);
    assert_eq!(first[0].0, log::Level::Warn);
    assert!(first[0].1.contains("rebuild failed"));
    assert!(first[0].1.contains("Cannot connect to Docker"));
    assert!(
        first
            .iter()
            .all(|record| !record.1.contains("private-value") && !record.1.contains('\n'))
    );
    let ctx = egui::Context::default();
    for _ in 0..3 {
        let output = crate::test_frame(
            &ctx,
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(360.0, 300.0),
                )),
                ..Default::default()
            },
            |root| {
                egui::CentralPanel::default().show(root, |ui| container.menu(ui, &ctx));
                assert!(ctx.globally_used_rect().right() <= 361.0);
            },
        );
        let labels: Vec<_> = output
            .shapes
            .iter()
            .filter_map(|shape| match &shape.shape {
                egui::epaint::Shape::Text(text) => Some(text.galley.job.text.as_str()),
                _ => None,
            })
            .collect();
        assert!(
            labels
                .iter()
                .any(|text| text.contains("see application log"))
        );
        assert!(labels.iter().any(|text| text.contains("Retry")));
        assert!(
            !labels
                .iter()
                .any(|text| text.contains("Cannot connect") || text.contains("diagnostic"))
        );
    }
    assert_eq!(records().len(), 2);
    deliver(
        &mut container,
        Event::Stopped(
            configuration,
            Err("Compose service shutdown timed out".into()),
        ),
    );
    assert_eq!(records().len(), 3);
    assert!(
        records()[2]
            .1
            .contains("stop failed: Compose service shutdown timed out")
    );
}
#[test]
fn success_and_configuration_failures_use_their_expected_levels() {
    capture();
    let configuration = fallback_configuration(&None);
    let mut container = DevContainer::default();
    let runtime = Runtime {
        host_workspace: "/tmp".into(),
        container_id: "test".into(),
        workspace_folder: "/workspaces".into(),
        remote_user: None,
    };
    deliver(
        &mut container,
        Event::Started(configuration.clone(), Ok(runtime), "reconnect"),
    );
    assert!(container.runtime().is_some());
    deliver(&mut container, Event::Stopped(configuration, Ok(())));
    assert!(container.runtime().is_none());
    assert!(records().iter().all(|record| record.0 == log::Level::Info));
    let root =
        std::env::temp_dir().join(format!("focus-container-diagnostic-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(root.join(".devcontainer.json"), "{}").unwrap();
    container.sync_workspace(Some(&root));
    let count = records().len();
    container.sync_workspace(Some(&root));
    assert_eq!(records().len(), count);
    let warnings: Vec<_> = records()
        .into_iter()
        .filter(|record| record.0 == log::Level::Warn)
        .collect();
    assert_eq!(warnings.len(), 1);
    assert!(warnings[0].1.contains("configuration failed"));
    assert!(warnings[0].1.contains("missing field"));
    std::fs::remove_dir_all(root).unwrap();
}
