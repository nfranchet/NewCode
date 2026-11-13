use super::protocol::{Frame, Input, Textures};
use crate::{editor::EditorApp, logging::Arguments};
use eframe::{App, egui};
use std::time::Instant;

pub(super) struct Renderer {
    ctx: egui::Context,
    app: EditorApp,
    textures: Textures,
    pending: egui::FullOutput,
    input: egui::RawInput,
    start: Instant,
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
        }
    }

    fn render(&mut self, mut input: egui::RawInput) {
        self.input = input.clone();
        self.input.events.clear();
        self.app.raw_input_hook(&self.ctx, &mut input);
        self.pending
            .append(self.ctx.run_ui(input, |ui| self.app.render(ui)));
    }

    pub fn release_input(&mut self) {
        let mut raw = self.input.clone();
        raw.time = Some(self.start.elapsed().as_secs_f64());
        raw.focused = false;
        raw.events = vec![
            egui::Event::ModifiersChanged(Default::default()),
            egui::Event::WindowFocused(false),
            egui::Event::PointerGone,
        ];
        self.render(raw);
        self.ctx.input_mut(|input| {
            input.pointer = Default::default();
            input.keys_down.clear();
            input.modifiers = Default::default();
        });
        self.pending.platform_output.commands.clear();
        for viewport in self.pending.viewport_output.values_mut() {
            viewport
                .commands
                .retain(|command| !matches!(command, egui::ViewportCommand::RequestPaste));
        }
    }

    pub fn save(&self) {
        self.app.save_settings();
    }

    pub fn title(&self) -> String {
        self.app.window_title()
    }
    pub fn set_waker(&self, viewer: egui::Context) {
        self.ctx
            .set_request_repaint_callback(move |info| viewer.request_repaint_after(info.delay));
    }
    pub fn save_documents(&mut self) -> bool {
        self.app.save_documents()
    }
    pub fn workspace(&self) -> Option<std::path::PathBuf> {
        self.app.workspace()
    }
    pub fn atlas(&self) -> Vec<super::protocol::Texture> {
        self.textures.full()
    }
    pub fn render_frame(&mut self, input: Option<Input>, remote: bool) -> Result<Frame, String> {
        self.ctx
            .data_mut(|data| data.insert_temp(egui::Id::new("remote_controller"), remote));
        let raw = match input {
            Some(input) => input.raw(self.start.elapsed().as_secs_f64(), self.ctx.zoom_factor())?,
            None => {
                let mut raw = self.input.clone();
                raw.time = Some(self.start.elapsed().as_secs_f64());
                raw
            }
        };
        self.render(raw);
        let mut frame = Frame::new(
            &self.ctx,
            std::mem::take(&mut self.pending),
            &mut self.textures,
            false,
        );
        frame.dirty = self.app.has_unsaved_changes();
        frame.title = Some(self.app.window_title());
        frame.output.accesskit_update = None;
        Ok(frame)
    }
}

impl Drop for Renderer {
    fn drop(&mut self) {
        // No GPU consumes pending background texture deltas when the session shuts down.
        self.pending.textures_delta.clear();
    }
}
