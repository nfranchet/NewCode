use super::{EditorApp, test_app};
use crate::{terminal::Terminal, ui_theme};
use eframe::egui::{self, Event, Key, Modifiers};
use std::{
    io::Write,
    sync::{Arc, Mutex},
};

#[derive(Clone, Default)]
struct Capture(Arc<Mutex<Vec<u8>>>);
impl Write for Capture {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
struct Harness {
    app: EditorApp,
    ctx: egui::Context,
    capture: Capture,
}
impl Harness {
    fn new() -> Self {
        let ctx = egui::Context::default();
        ui_theme::apply(&ctx);
        let mut app = test_app();
        let capture = Capture::default();
        let mut terminal = Terminal::spawn("exec /bin/cat", None, ctx.clone()).unwrap();
        terminal.input = Box::new(capture.clone());
        app.top = Some(terminal);
        Self { app, ctx, capture }
    }
    fn frame(&mut self, events: Vec<Event>) {
        let modifiers = events
            .iter()
            .rev()
            .find_map(|event| match event {
                Event::Key { modifiers, .. } => Some(*modifiers),
                _ => None,
            })
            .unwrap_or_default();
        let mut input = egui::RawInput {
            modifiers,
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1000.0, 700.0),
            )),
            events,
            ..Default::default()
        };
        eframe::App::raw_input_hook(&mut self.app, &self.ctx, &mut input);
        let _ = self.ctx.run(input, |ctx| self.app.render(ctx));
    }
    fn rect(&self) -> egui::Rect {
        self.ctx
            .data(|data| data.get_temp(super::super::terminal_input::terminal_id().with("rect")))
            .unwrap()
    }
    fn click(&mut self, pos: egui::Pos2) {
        self.frame(vec![Event::PointerMoved(pos), pointer(pos, true)]);
        self.frame(vec![pointer(pos, false)]);
    }
    fn focused(&self) -> bool {
        self.ctx
            .memory(|m| m.has_focus(super::super::terminal_input::terminal_id()))
    }
}
impl Drop for Harness {
    fn drop(&mut self) {
        if let Some(terminal) = &mut self.app.top {
            let _ = terminal._child.kill();
            let _ = terminal._child.wait();
        }
    }
}
fn key(key: Key, modifiers: Modifiers) -> Event {
    Event::Key {
        key,
        physical_key: Some(key),
        pressed: true,
        repeat: false,
        modifiers,
    }
}
fn pointer(pos: egui::Pos2, pressed: bool) -> Event {
    Event::PointerButton {
        pos,
        pressed,
        button: egui::PointerButton::Primary,
        modifiers: Modifiers::NONE,
    }
}

#[test]
fn terminal_captures_keys_from_first_tab_without_editor_actions() {
    let mut h = Harness::new();
    h.frame(vec![]);
    h.click(h.rect().center());
    assert!(h.focused());
    for event in [
        key(Key::Tab, Modifiers::NONE),
        key(Key::Tab, Modifiers::SHIFT),
        key(Key::Escape, Modifiers::NONE),
        key(Key::ArrowUp, Modifiers::NONE),
        key(Key::ArrowLeft, Modifiers::NONE),
    ] {
        h.frame(vec![event]);
        assert!(h.focused(), "terminal lost focus");
    }
    let ctrl = Modifiers::CTRL | Modifiers::COMMAND;
    h.frame(vec![
        key(Key::F, ctrl),
        key(Key::W, ctrl),
        key(Key::S, ctrl),
    ]);
    assert!(!h.app.find_open);
    assert!(h.app.close_confirmation.is_none());
    assert_eq!(h.app.open_files.len(), 1);
    h.frame(vec![
        Event::Text("abc é".into()),
        Event::Copy,
        Event::Cut,
        Event::Paste("pasted".into()),
    ]);
    assert_eq!(
        *h.capture.0.lock().unwrap(),
        b"\t\x1b[Z\x1b\x1b[A\x1b[D\x06\x17\x13abc \xc3\xa9\x03\x18pasted"
    );
    let sent = h.capture.0.lock().unwrap().clone();
    h.frame(vec![]);
    assert_eq!(
        *h.capture.0.lock().unwrap(),
        sent,
        "an idle frame resent input"
    );
}

#[test]
fn outside_click_and_keyboard_focus_release_restore_editor_shortcuts() {
    let mut h = Harness::new();
    h.frame(vec![]);
    h.click(h.rect().center());
    let outside = egui::pos2(h.rect().left(), h.rect().top() - 50.0);
    let ctrl = Modifiers::CTRL | Modifiers::COMMAND;
    let mut raw = egui::RawInput {
        events: vec![
            key(Key::Tab, Modifiers::NONE),
            pointer(outside, true),
            key(Key::F, ctrl),
        ],
        ..Default::default()
    };
    h.app.route_terminal_input(&h.ctx, &mut raw);
    assert_eq!(*h.capture.0.lock().unwrap(), b"\t");
    assert_eq!(raw.events.len(), 2);
    let mut opened_find = false;
    raw.modifiers = ctrl;
    let _ = h
        .ctx
        .run(raw, |ctx| opened_find = h.app.editor_shortcuts(ctx));
    assert!(opened_find);
    h.ctx
        .memory_mut(|m| m.request_focus(egui::Id::new("main_editor_text")));
    let mut raw = egui::RawInput {
        modifiers: ctrl,
        events: vec![key(Key::W, ctrl)],
        ..Default::default()
    };
    h.app.route_terminal_input(&h.ctx, &mut raw);
    assert_eq!(raw.events.len(), 1);
    let _ = h.ctx.run(raw, |ctx| {
        h.app.editor_shortcuts(ctx);
    });
    assert!(h.app.close_confirmation.is_some());
}

#[test]
fn terminal_output_updates_input_modes_even_across_reads() {
    let mut h = Harness::new();
    let terminal = h.app.top.as_mut().unwrap();
    let (tx, rx) = std::sync::mpsc::channel();
    terminal.rx = rx;
    for chunk in [b"\x1b[?1;20".as_slice(), b"04h"] {
        tx.send(chunk.to_vec()).unwrap();
        terminal.poll();
    }
    assert!(terminal.input_modes.application_cursor);
    assert!(terminal.input_modes.bracketed_paste);
    tx.send(b"\x1b[?1;2004l".to_vec()).unwrap();
    terminal.poll();
    assert!(!terminal.input_modes.application_cursor);
    assert!(!terminal.input_modes.bracketed_paste);
}

#[test]
fn clicking_editor_transfers_typing_back_to_it() {
    let mut h = Harness::new();
    h.frame(vec![]);
    h.click(h.rect().center());
    assert!(h.focused());
    let editor_id = egui::Id::new("main_editor_text");
    let pos = h.ctx.read_response(editor_id).unwrap().rect.center();
    h.click(pos);
    assert!(!h.focused());
    assert!(h.ctx.memory(|m| m.has_focus(editor_id)));
    h.frame(vec![Event::Text("typed in editor".into())]);
    assert!(h.app.buffer.contains("typed in editor"));
    assert!(h.capture.0.lock().unwrap().is_empty());
}

#[test]
fn logs_tab_preserves_process_drains_output_and_does_not_capture_keys() {
    let mut h = Harness::new();
    h.frame(vec![]);
    h.click(h.rect().center());
    let process = h.app.top.as_ref().unwrap()._child.process_id();
    let (tx, rx) = std::sync::mpsc::channel();
    h.app.top.as_mut().unwrap().rx = rx;
    h.app.bottom_tab = super::super::logs::Tab::Logs;
    let mut raw = egui::RawInput {
        events: vec![
            Event::Text("search logs".into()),
            key(Key::Tab, Modifiers::NONE),
        ],
        ..Default::default()
    };
    h.app.route_terminal_input(&h.ctx, &mut raw);
    assert_eq!(raw.events.len(), 2);
    assert!(h.capture.0.lock().unwrap().is_empty());
    tx.send(b"background output".to_vec()).unwrap();
    h.frame(vec![]);
    assert!(!h.focused());
    let terminal = h.app.top.as_ref().unwrap();
    assert_eq!(terminal._child.process_id(), process);
    let text: String = terminal
        .cells
        .iter()
        .flatten()
        .map(|cell| cell.ch)
        .collect();
    assert!(text.contains("background output"));
    h.app.bottom_tab = super::super::logs::Tab::Terminal;
    h.frame(vec![]);
    h.click(h.rect().center());
    h.frame(vec![Event::Text("sent".into())]);
    assert_eq!(*h.capture.0.lock().unwrap(), b"sent");
}
