//! Native viewer of the same authoritative session used by browser viewers.
use super::{
    Host,
    protocol::{Frame, Input},
    registry,
};
use base64::{Engine, engine::general_purpose::STANDARD};
use eframe::egui;
use std::{
    collections::HashMap,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    time::Duration,
};
use tiny_http::Server;

struct Painter {
    textures: HashMap<String, egui::TextureHandle>,
    meshes: Vec<(egui::Rect, String, egui::Mesh)>,
    size: egui::Vec2,
    free: Vec<String>,
}
impl Default for Painter {
    fn default() -> Self {
        Self {
            textures: HashMap::new(),
            meshes: vec![],
            free: vec![],
            size: egui::vec2(1440.0, 900.0),
        }
    }
}
impl Painter {
    fn update(&mut self, ctx: &egui::Context, frame: &Frame) -> Result<(), String> {
        for id in self.free.drain(..) {
            self.textures.remove(&id);
        }
        if frame.reset {
            self.textures.clear();
        }
        for texture in &frame.textures {
            let bytes = STANDARD
                .decode(&texture.pixels)
                .map_err(|_| "Invalid texture")?;
            let image = egui::ColorImage::new(
                texture.size,
                bytes
                    .chunks_exact(4)
                    .map(|pixel| {
                        egui::Color32::from_rgba_premultiplied(
                            pixel[0], pixel[1], pixel[2], pixel[3],
                        )
                    })
                    .collect(),
            );
            if let Some(pos) = texture.pos {
                if let Some(handle) = self.textures.get_mut(&texture.id) {
                    handle.set_partial(pos, image, texture.options);
                }
            } else if let Some(handle) = self.textures.get_mut(&texture.id) {
                handle.set(image, texture.options);
            } else {
                self.textures.insert(
                    texture.id.clone(),
                    ctx.load_texture("editor-session", image, texture.options),
                );
            }
        }
        self.meshes.clear();
        for mesh in &frame.meshes {
            let vertices = STANDARD
                .decode(&mesh.vertices)
                .map_err(|_| "Invalid vertices")?;
            let indices = STANDARD
                .decode(&mesh.indices)
                .map_err(|_| "Invalid indices")?;
            let vertices = vertices
                .chunks_exact(20)
                .map(|bytes| {
                    let float = |at| f32::from_le_bytes(bytes[at..at + 4].try_into().unwrap());
                    egui::epaint::Vertex {
                        pos: egui::pos2(float(0), float(4)),
                        uv: egui::pos2(float(8), float(12)),
                        color: egui::Color32::from_rgba_premultiplied(
                            bytes[16], bytes[17], bytes[18], bytes[19],
                        ),
                    }
                })
                .collect();
            let indices = indices
                .chunks_exact(4)
                .map(|bytes| u32::from_le_bytes(bytes.try_into().unwrap()))
                .collect();
            self.meshes.push((
                egui::Rect::from_min_max(
                    egui::pos2(mesh.clip[0], mesh.clip[1]),
                    egui::pos2(mesh.clip[2], mesh.clip[3]),
                ),
                mesh.texture.clone(),
                egui::Mesh {
                    vertices,
                    indices,
                    ..Default::default()
                },
            ));
        }
        self.free = frame.free.clone();
        self.size = egui::vec2(frame.size[0], frame.size[1]);
        Ok(())
    }
    fn draw(&self, ui: &mut egui::Ui) -> egui::emath::TSTransform {
        let rect = ui.available_rect_before_wrap();
        let scale = (rect.width() / self.size.x).min(rect.height() / self.size.y);
        let offset = rect.center() - self.size * scale * 0.5;
        let transform = egui::emath::TSTransform::new(offset.to_vec2(), scale);
        for (clip, texture, mesh) in &self.meshes {
            if let Some(texture) = self.textures.get(texture) {
                let mut mesh = mesh.clone();
                mesh.texture_id = texture.id();
                mesh.transform(transform);
                ui.painter()
                    .with_clip_rect((transform * *clip).intersect(rect))
                    .add(egui::Shape::mesh(mesh));
            }
        }
        transform
    }
}

struct NativeApp {
    host: Host,
    incoming: mpsc::Receiver<tiny_http::Request>,
    stopped: Arc<AtomicBool>,
    reader: Option<std::thread::JoinHandle<()>>,
    painter: Painter,
    frame: Option<Frame>,
    input: Option<Input>,
    close_confirmation: bool,
    confirmed: bool,
}
impl NativeApp {
    fn new(
        ctx: &egui::Context,
        args: &crate::logging::Arguments,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        ctx.options_mut(|options| options.zoom_with_keyboard = false);
        let server = Server::http("127.0.0.1:0").map_err(std::io::Error::other)?;
        let origin = format!(
            "http://{}",
            server.server_addr().to_ip().ok_or("Invalid listener")?
        );
        let mut host = Host::new(args, &origin, &registry::random_id()?, "native")?;
        host.runtime.connect("native-window", false);
        host.runtime.renderer.set_waker(ctx.clone());
        let (sender, incoming) = mpsc::sync_channel(64);
        let stopped = Arc::new(AtomicBool::new(false));
        let reader_stopped = stopped.clone();
        let wake = ctx.clone();
        let reader = std::thread::spawn(move || {
            while !reader_stopped.load(Ordering::Acquire) {
                if let Ok(Some(request)) = server.recv_timeout(Duration::from_millis(100)) {
                    if let Err(
                        mpsc::TrySendError::Full(request)
                        | mpsc::TrySendError::Disconnected(request),
                    ) = sender.try_send(request)
                    {
                        super::respond(request, 503, "text/plain", "Editor is busy", false);
                    }
                    wake.request_repaint();
                }
            }
        });
        Ok(Self {
            host,
            incoming,
            stopped,
            reader: Some(reader),
            painter: Painter::default(),
            frame: None,
            input: None,
            close_confirmation: false,
            confirmed: false,
        })
    }
}
impl eframe::App for NativeApp {
    fn raw_input_hook(&mut self, ctx: &egui::Context, raw: &mut egui::RawInput) {
        if raw.viewport().close_requested()
            && !self.confirmed
            && self.frame.as_ref().is_some_and(|frame| frame.dirty)
        {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.close_confirmation = true;
        }
        let control = self.host.runtime.control("native-window");
        let size = raw
            .screen_rect
            .unwrap_or_else(|| ctx.viewport_rect())
            .size();
        self.input = Some(Input {
            size: [size.x.max(1.0), size.y.max(1.0)],
            pixels_per_point: raw
                .viewport()
                .native_pixels_per_point
                .unwrap_or(1.0)
                .clamp(0.5, 4.0),
            focused: raw.focused,
            events: if control.owned && !self.close_confirmation {
                raw.events.clone()
            } else {
                vec![]
            },
            epoch: if !self.close_confirmation {
                Some(control.epoch)
            } else {
                None
            },
        });
        if control.owned && !self.close_confirmation {
            raw.events.clear();
        }
    }
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        for request in self.incoming.try_iter().take(64) {
            self.host.handle(request);
        }
        if !self.host.runtime.known("native-window") {
            self.host.runtime.connect("native-window", false);
        }
        let input = self.input.take().unwrap_or(Input {
            size: [1440.0, 900.0],
            pixels_per_point: 1.0,
            focused: false,
            events: vec![],
            epoch: None,
        });
        match self.host.runtime.frame("native-window", input) {
            Ok(Some(frame)) => {
                if let Some(title) = &frame.title {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Title(title.clone()));
                }
                if let Err(error) = self.painter.update(ctx, &frame) {
                    log::warn!("Native viewer: {error}");
                }
                if frame.control.owned {
                    for command in &frame.output.commands {
                        match command {
                            egui::OutputCommand::CopyText(text) => ctx.copy_text(text.clone()),
                            egui::OutputCommand::OpenUrl(url) => ctx.open_url(url.clone()),
                            egui::OutputCommand::CopyImage(image) => ctx.copy_image(image.clone()),
                        }
                    }
                    if frame.paste {
                        ctx.send_viewport_cmd(egui::ViewportCommand::RequestPaste);
                    }
                }
                self.frame = Some(frame);
            }
            Ok(None) => {}
            Err(error) => log::warn!("Native viewer request failed: {error}"),
        }
        self.host.tick();
        if self.host.stop {
            self.confirmed = true;
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
        ctx.request_repaint_after(Duration::from_millis(100));
    }
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        let owned = self.host.runtime.control("native-window").owned;
        if !owned {
            egui::Panel::top("viewer_control").show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label("This editor is controlled remotely");
                    if ui.button("Take control").clicked() {
                        self.host.runtime.claim("native-window");
                    }
                });
            });
        }
        let transform = egui::CentralPanel::default()
            .frame(egui::Frame::NONE)
            .show(ui, |ui| self.painter.draw(ui))
            .inner;
        if owned {
            if let Some(frame) = &self.frame {
                ctx.set_cursor_icon(frame.output.cursor_icon);
                ctx.output_mut(|output| {
                    output.ime = frame.output.ime.map(|mut ime| {
                        ime.rect = transform * ime.rect;
                        ime.cursor_rect = transform * ime.cursor_rect;
                        ime
                    })
                });
            }
        }
        if self.close_confirmation {
            egui::Modal::new(egui::Id::new("close_native_editor")).show(&ctx, |ui| {
                ui.heading("Unsaved changes");
                ui.label("Save changes before closing this editor?");
                ui.horizontal(|ui| {
                    if ui.button("Save all").clicked()
                        && self.host.runtime.renderer.save_documents()
                    {
                        self.confirmed = true;
                    }
                    if ui.button("Discard").clicked() {
                        self.confirmed = true;
                    }
                    if ui.button("Cancel").clicked() {
                        self.close_confirmation = false;
                    }
                });
            });
            if self.confirmed {
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        }
    }
    fn persist_egui_memory(&self) -> bool {
        false
    }
    fn save(&mut self, _storage: &mut dyn eframe::Storage) {
        self.host.runtime.renderer.save();
    }
}
impl Drop for NativeApp {
    fn drop(&mut self) {
        self.stopped.store(true, Ordering::Release);
        if let Some(reader) = self.reader.take() {
            let _ = reader.join();
        }
    }
}

pub(super) fn run(arguments: &crate::logging::Arguments) -> Result<(), Box<dyn std::error::Error>> {
    let options = eframe::NativeOptions {
        persist_window: false,
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1440.0, 900.0])
            .with_min_inner_size([900.0, 600.0]),
        ..Default::default()
    };
    eframe::run_native(
        "Focus Editor",
        options,
        Box::new(|cc| {
            Ok(Box::new(NativeApp::new(&cc.egui_ctx, arguments).map_err(
                |error| std::io::Error::other(error.to_string()),
            )?))
        }),
    )?;
    Ok(())
}
