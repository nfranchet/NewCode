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
    output: std::sync::mpsc::Sender<Vec<u8>>,
}
impl Harness {
    fn new() -> Self {
        let ctx = egui::Context::default();
        ui_theme::apply(&ctx);
        let mut app = test_app();
        let capture = Capture::default();
        let mut terminal = Terminal::spawn("exec /bin/cat", None, ctx.clone()).unwrap();
        let output = terminal.capture_output(Box::new(capture.clone()));
        app.top = Some(terminal);
        Self {
            app,
            ctx,
            capture,
            output,
        }
    }
    fn frame(&mut self, events: Vec<Event>) -> egui::FullOutput {
        let modifiers = events
            .iter()
            .rev()
            .find_map(|event| match event {
                Event::Key { modifiers, .. } => Some(*modifiers),
                _ => None,
            })
            .unwrap_or_default();
        let mut input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1000.0, 700.0),
            )),
            events: std::iter::once(Event::ModifiersChanged(modifiers))
                .chain(events)
                .collect(),
            ..Default::default()
        };
        eframe::App::raw_input_hook(&mut self.app, &self.ctx, &mut input);
        crate::test_frame(&self.ctx, input, |root| self.app.render(root))
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
        super::super::terminal_input::focused(&self.ctx)
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
        let description = format!("{event:?}");
        h.frame(vec![event]);
        assert!(h.focused(), "terminal lost focus for {description}");
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
    h.frame(vec![Event::Text("abc é".into())]);
    h.frame(vec![Event::Copy]);
    h.frame(vec![Event::Cut]);
    h.frame(vec![Event::Paste("pasted".into())]);
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
fn outside_click_restores_editor_shortcuts() {
    let mut h = Harness::new();
    h.frame(vec![]);
    h.click(h.rect().center());
    let editor = h
        .ctx
        .read_response(h.app.editor_id())
        .unwrap()
        .rect
        .center();
    h.click(editor);
    assert!(!h.focused());
    h.frame(vec![key(Key::F, Modifiers::CTRL | Modifiers::COMMAND)]);
    assert!(h.app.find_open);
}

#[test]
fn terminal_output_updates_input_modes_even_across_reads() {
    let mut h = Harness::new();
    h.frame(vec![]);
    h.click(h.rect().center());
    h.output.send(b"\x1b[?1;20".to_vec()).unwrap();
    h.frame(vec![]);
    h.output.send(b"04h".to_vec()).unwrap();
    h.frame(vec![
        key(Key::ArrowUp, Modifiers::NONE),
        Event::Paste("hello".into()),
    ]);
    assert_eq!(
        *h.capture.0.lock().unwrap(),
        b"\x1bOA\x1b[200~hello\x1b[201~"
    );
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
    let process = h.app.top.as_ref().unwrap().process_id();
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
    h.output.send(b"background output".to_vec()).unwrap();
    h.frame(vec![]);
    assert!(!h.focused());
    let terminal = h.app.top.as_mut().unwrap();
    assert_eq!(terminal.process_id(), process);
    assert!(
        terminal
            .view
            .visible_text()
            .unwrap()
            .contains("background output")
    );
    h.app.bottom_tab = super::super::logs::Tab::Terminal;
    h.frame(vec![]);
    h.click(h.rect().center());
    h.frame(vec![Event::Text("sent".into())]);
    assert_eq!(*h.capture.0.lock().unwrap(), b"sent");
}

#[test]
fn scrollback_stays_put_when_background_output_arrives() {
    let mut h = Harness::new();
    h.frame(vec![]);
    let lines: String = (0..100)
        .map(|number| format!("line {number}\r\n"))
        .collect();
    h.output.send(lines.into_bytes()).unwrap();
    h.frame(vec![]);
    let latest = h.app.top.as_mut().unwrap().view.visible_text().unwrap();
    h.frame(vec![
        Event::PointerMoved(h.rect().center()),
        Event::MouseWheel {
            unit: egui::MouseWheelUnit::Line,
            delta: egui::vec2(0.0, 12.0),
            phase: egui::TouchPhase::Move,
            modifiers: Modifiers::NONE,
        },
    ]);
    // Let egui finish smoothing this wheel gesture before comparing the viewport.
    for _ in 0..20 {
        h.frame(vec![]);
    }
    let scrolled = h.app.top.as_mut().unwrap().view.visible_text().unwrap();
    assert_ne!(scrolled.lines().next(), latest.lines().next());
    h.output
        .send(b"more background output\r\n".to_vec())
        .unwrap();
    h.frame(vec![]);
    let after = h.app.top.as_mut().unwrap().view.visible_text().unwrap();
    assert_eq!(after.lines().next(), scrolled.lines().next());
}

#[test]
fn restarting_and_switching_workspace_replace_the_owned_session() {
    let mut h = Harness::new();
    h.frame(vec![]);
    let old = h.app.top.as_ref().unwrap().process_id().unwrap();
    h.app.top.as_mut().unwrap().command = "exec /bin/cat".into();
    h.app.restart_terminal(&h.ctx, None, None);
    assert_ne!(h.app.top.as_ref().unwrap().process_id(), Some(old));
    // SAFETY: signal 0 only checks whether our reaped child still exists.
    assert_eq!(unsafe { libc::kill(old as i32, 0) }, -1);
    let restarted = h.app.top.as_ref().unwrap().process_id().unwrap();
    h.app.settings.workspace = Some(std::env::temp_dir());
    h.frame(vec![]);
    assert_ne!(h.app.top.as_ref().unwrap().process_id(), Some(restarted));
    assert!(
        h.app
            .top
            .as_ref()
            .unwrap()
            .matches_context(h.app.settings.workspace.as_deref(), None)
    );
}

#[test]
fn selecting_text_copies_with_shift_and_ctrl_c_still_interrupts() {
    let mut h = Harness::new();
    h.frame(vec![]);
    h.output.send(b"copy me".to_vec()).unwrap();
    h.frame(vec![]);
    let font = crate::terminal_fonts::font_id();
    let (width, height) = h
        .ctx
        .fonts_mut(|fonts| (fonts.glyph_width(&font, 'M'), fonts.row_height(&font)));
    let start = h.rect().min + egui::vec2(6.0 + width * 0.1, 6.0 + height * 0.5);
    let end = start + egui::vec2(width * 7.0, 0.0);
    h.frame(vec![Event::PointerMoved(start), pointer(start, true)]);
    h.frame(vec![Event::PointerMoved(end)]);
    h.frame(vec![pointer(end, false)]);
    assert_eq!(
        h.app
            .top
            .as_ref()
            .unwrap()
            .view
            .selected_text()
            .as_deref()
            .map(str::trim),
        Some("copy me")
    );
    let output = h.frame(vec![
        Event::ModifiersChanged(Modifiers::CTRL | Modifiers::SHIFT),
        Event::Copy,
    ]);
    assert!(output.platform_output.commands.iter().any(
        |command| matches!(command, egui::OutputCommand::CopyText(text) if text.trim() == "copy me")
    ));
    assert!(h.capture.0.lock().unwrap().is_empty());
    h.frame(vec![Event::Copy]);
    assert_eq!(*h.capture.0.lock().unwrap(), b"\x03");
}
