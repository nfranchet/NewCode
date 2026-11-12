use super::protocol::{Frame, Input, Textures};
use crate::{editor::EditorApp, logging::Arguments};
use eframe::{App, egui};
use std::time::{Duration, Instant};

pub(super) struct Renderer {
    ctx: egui::Context,
    app: EditorApp,
    textures: Textures,
    pending: egui::FullOutput,
    input: egui::RawInput,
    start: Instant,
    last_render: Instant,
    last_body: Vec<u8>,
}

impl Renderer {
    pub fn new(arguments: &Arguments) -> Self {
        let ctx = egui::Context::default();
        let app = EditorApp::new(&ctx, arguments);
        let start = Instant::now();
        Self {
            ctx,
            app,
            textures: Textures::default(),
            pending: egui::FullOutput::default(),
            input: egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1440.0, 900.0),
                )),
                ..Default::default()
            },
            start,
            last_render: start,
            last_body: Vec::new(),
        }
    }

    fn render(&mut self, mut input: egui::RawInput) {
        self.input = input.clone();
        self.input.events.clear();
        self.app.raw_input_hook(&self.ctx, &mut input);
        self.pending
            .append(self.ctx.run_ui(input, |ui| self.app.render(ui)));
        self.last_render = Instant::now();
    }

    pub fn release_input(&mut self) {
        let mut raw = self.input.clone();
        raw.time = Some(self.start.elapsed().as_secs_f64());
        raw.focused = false;
        raw.events = vec![egui::Event::WindowFocused(false), egui::Event::PointerGone];
        self.render(raw);
        self.ctx
            .input_mut(|input| input.pointer = Default::default());
    }

    pub fn background(&mut self) {
        if self.last_render.elapsed() < Duration::from_millis(100) {
            return;
        }
        // Continue processing addon events and PTY output while a tab is hidden or reloading.
        let mut raw = self.input.clone();
        raw.time = Some(self.start.elapsed().as_secs_f64());
        self.render(raw);
    }

    pub fn frame(&mut self, input: Input, reset: bool) -> Result<Option<Vec<u8>>, String> {
        let raw = input.raw(self.start.elapsed().as_secs_f64(), self.ctx.zoom_factor())?;
        self.render(raw);
        let output = std::mem::take(&mut self.pending);
        let mut frame = Frame::new(&self.ctx, output, &mut self.textures, reset);
        frame.dirty = self.app.has_unsaved_changes();
        frame.title = Some(self.app.window_title());
        // AccessKit is native-specific; serializing a tree adds no browser accessibility support.
        frame.output.accesskit_update = None;
        let body = serde_json::to_vec(&frame).map_err(|e| e.to_string())?;
        if !reset && frame.output.commands.is_empty() && !frame.paste && body == self.last_body {
            Ok(None)
        } else {
            self.last_body = body.clone();
            Ok(Some(body))
        }
    }

    pub fn save(&self) {
        self.app.save_settings();
    }
}

impl Drop for Renderer {
    fn drop(&mut self) {
        // No GPU consumes pending background texture deltas when the session shuts down.
        self.pending.textures_delta.clear();
    }
}
