//! A compact WebGL transport for the same meshes used by the desktop renderer.
use base64::{Engine, engine::general_purpose::STANDARD};
use eframe::egui::{self, ColorImage, TextureId, TextureOptions, epaint::ImageDelta};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Clone, Serialize, Deserialize)]
pub(super) struct Input {
    pub size: [f32; 2],
    pub pixels_per_point: f32,
    pub focused: bool,
    pub events: Vec<egui::Event>,
    #[serde(default)]
    pub epoch: Option<u64>,
}

impl Input {
    pub fn raw(self, time: f64, zoom: f32) -> Result<egui::RawInput, &'static str> {
        if self
            .size
            .iter()
            .any(|v| !v.is_finite() || *v < 1.0 || *v > 16384.0)
            || !self.pixels_per_point.is_finite()
            || !(0.5..=4.0).contains(&self.pixels_per_point)
            || self.events.len() > 4096
        {
            return Err("Invalid browser input dimensions or event count");
        }
        let rect = egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(self.size[0], self.size[1]) / zoom,
        );
        let events = self
            .events
            .into_iter()
            .map(|mut event| {
                match &mut event {
                    egui::Event::PointerMoved(pos)
                    | egui::Event::PointerButton { pos, .. }
                    | egui::Event::Touch { pos, .. } => {
                        *pos = egui::Pos2::new(pos.x / zoom, pos.y / zoom);
                    }
                    egui::Event::MouseWheel {
                        unit: egui::MouseWheelUnit::Point,
                        delta,
                        ..
                    } => *delta /= zoom,
                    _ => {}
                }
                event
            })
            .collect();
        let mut raw = egui::RawInput {
            screen_rect: Some(rect),
            time: Some(time),
            focused: self.focused,
            events,
            max_texture_side: Some(4096),
            ..Default::default()
        };
        let viewport = raw.viewports.get_mut(&egui::ViewportId::ROOT).unwrap();
        viewport.native_pixels_per_point = Some(self.pixels_per_point);
        viewport.inner_rect = Some(rect);
        viewport.focused = Some(self.focused);
        Ok(raw)
    }
}

fn texture_id(id: TextureId) -> String {
    match id {
        TextureId::Managed(id) => format!("m{id}"),
        TextureId::User(id) => format!("u{id}"),
    }
}

#[derive(Clone, Serialize, Deserialize, PartialEq)]
pub(super) struct Mesh {
    pub clip: [f32; 4],
    pub texture: String,
    /// Little-endian x/y/u/v float32 values followed by four premultiplied RGBA bytes.
    pub vertices: String,
    /// Little-endian uint32 triangle indices.
    pub indices: String,
}

#[derive(Clone, Serialize, Deserialize, PartialEq)]
pub(super) struct Texture {
    pub id: String,
    pub size: [usize; 2],
    pub pos: Option<[usize; 2]>,
    pub options: TextureOptions,
    pub pixels: String,
}

impl Texture {
    fn new(
        id: TextureId,
        image: &ColorImage,
        pos: Option<[usize; 2]>,
        options: TextureOptions,
    ) -> Self {
        Self {
            id: texture_id(id),
            size: image.size,
            pos,
            options,
            pixels: STANDARD.encode(
                image
                    .pixels
                    .iter()
                    .flat_map(|p| p.to_array())
                    .collect::<Vec<_>>(),
            ),
        }
    }
}

/// Keep a CPU copy so a page reload can upload every live texture again.
#[derive(Default)]
pub(super) struct Textures(HashMap<TextureId, (ColorImage, TextureOptions)>);

impl Textures {
    pub fn full(&self) -> Vec<Texture> {
        self.0
            .iter()
            .map(|(&id, (image, options))| Texture::new(id, image, None, *options))
            .collect()
    }
    fn update(&mut self, id: TextureId, delta: ImageDelta) -> Texture {
        let egui::ImageData::Color(image) = delta.image;
        let texture = Texture::new(id, &image, delta.pos, delta.options);
        if let Some([x, y]) = delta.pos {
            if let Some((target, options)) = self.0.get_mut(&id) {
                for row in 0..image.height() {
                    let start = (y + row) * target.width() + x;
                    target.pixels[start..start + image.width()].copy_from_slice(
                        &image.pixels[row * image.width()..(row + 1) * image.width()],
                    );
                }
                *options = delta.options;
            }
        } else {
            self.0.insert(id, ((*image).clone(), delta.options));
        }
        texture
    }
}

#[derive(Clone, Default, Serialize, Deserialize, PartialEq)]
pub(super) struct Control {
    pub owned: bool,
    pub owner: Option<String>,
    pub epoch: u64,
}

#[derive(Clone, Serialize, Deserialize, PartialEq)]
pub(super) struct Frame {
    pub meshes: Vec<Mesh>,
    pub textures: Vec<Texture>,
    pub free: Vec<String>,
    pub output: egui::PlatformOutput,
    pub title: Option<String>,
    pub paste: bool,
    pub dirty: bool,
    pub pixels_per_point: f32,
    pub size: [f32; 2],
    pub revision: u64,
    pub reset: bool,
    pub control: Control,
}

impl Frame {
    pub fn new(
        ctx: &egui::Context,
        mut output: egui::FullOutput,
        textures: &mut Textures,
        reset: bool,
    ) -> Self {
        let updates: Vec<_> = output
            .textures_delta
            .set
            .drain()
            .flat_map(|(id, deltas)| deltas.into_iter().map(move |delta| (id, delta)))
            .map(|(id, delta)| textures.update(id, delta))
            .collect();
        let uploads = if reset {
            textures
                .0
                .iter()
                .map(|(&id, (image, options))| Texture::new(id, image, None, *options))
                .collect()
        } else {
            updates
        };
        let meshes = ctx
            .tessellate(output.shapes, output.pixels_per_point)
            .into_iter()
            .filter_map(|primitive| {
                let egui::epaint::Primitive::Mesh(mesh) = primitive.primitive else {
                    return None;
                };
                let mut vertices = Vec::with_capacity(mesh.vertices.len() * 20);
                for v in mesh.vertices {
                    for value in [v.pos.x, v.pos.y, v.uv.x, v.uv.y] {
                        vertices.extend(value.to_le_bytes());
                    }
                    vertices.extend(v.color.to_array());
                }
                Some(Mesh {
                    clip: [
                        primitive.clip_rect.min.x,
                        primitive.clip_rect.min.y,
                        primitive.clip_rect.max.x,
                        primitive.clip_rect.max.y,
                    ],
                    texture: texture_id(mesh.texture_id),
                    vertices: STANDARD.encode(vertices),
                    indices: STANDARD.encode(
                        mesh.indices
                            .iter()
                            .flat_map(|i| i.to_le_bytes())
                            .collect::<Vec<_>>(),
                    ),
                })
            })
            .collect();
        let free = output
            .textures_delta
            .free
            .drain()
            .map(|id| {
                textures.0.remove(&id);
                texture_id(id)
            })
            .collect();
        let mut frame = Self {
            meshes,
            textures: uploads,
            free,
            output: output.platform_output,
            title: None,
            paste: false,
            dirty: false,
            pixels_per_point: output.pixels_per_point,
            size: [ctx.viewport_rect().width(), ctx.viewport_rect().height()],
            revision: 0,
            reset,
            control: Control::default(),
        };
        if let Some(viewport) = output.viewport_output.get(&egui::ViewportId::ROOT) {
            for command in &viewport.commands {
                match command {
                    egui::ViewportCommand::Title(title) => frame.title = Some(title.clone()),
                    egui::ViewportCommand::RequestPaste => frame.paste = true,
                    _ => {}
                }
            }
        }
        frame
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reload_replays_patched_textures_and_free_removes_them() {
        let ctx = egui::Context::default();
        ctx.run_ui(egui::RawInput::default(), |ui| {
            ui.label("Texture test");
        })
        .drop_without_applying_deltas();
        let id = TextureId::Managed(7);
        let mut textures = Textures::default();
        textures.update(
            id,
            ImageDelta::full(
                ColorImage::filled([2, 2], egui::Color32::RED),
                TextureOptions::LINEAR,
            ),
        );
        textures.update(
            id,
            ImageDelta::partial(
                [1, 1],
                ColorImage::filled([1, 1], egui::Color32::BLUE),
                TextureOptions::NEAREST,
            ),
        );
        let replay = Frame::new(
            &ctx,
            egui::FullOutput {
                pixels_per_point: 1.0,
                ..Default::default()
            },
            &mut textures,
            true,
        );
        assert_eq!(replay.textures.len(), 1);
        let bytes = STANDARD.decode(&replay.textures[0].pixels).unwrap();
        assert_eq!(&bytes[12..], &egui::Color32::BLUE.to_array());
        assert_eq!(replay.textures[0].pos, None);
        assert_eq!(replay.textures[0].options, TextureOptions::NEAREST);
        let mut output = egui::FullOutput {
            pixels_per_point: 1.0,
            ..Default::default()
        };
        output.textures_delta.free.insert(id);
        let frame = Frame::new(&ctx, output, &mut textures, false);
        assert_eq!(frame.free, ["m7"]);
        assert!(textures.0.is_empty());
    }

    #[test]
    fn browser_input_preserves_unicode_shortcuts_and_display_scale() {
        let input: Input = serde_json::from_value(serde_json::json!({
            "size": [1440, 900], "pixels_per_point": 2, "focused": true,
            "events": [{"Text": "héllo 世界"}, {"ModifiersChanged": {
                "alt": false, "ctrl": true, "shift": false, "mac_cmd": false, "command": true
            }}]
        }))
        .unwrap();
        let raw = input.raw(10.0, 1.0).unwrap();
        assert_eq!(raw.events[0], egui::Event::Text("héllo 世界".into()));
        assert_eq!(raw.viewport().native_pixels_per_point, Some(2.0));
        assert_eq!(raw.screen_rect.unwrap().size(), egui::vec2(1440.0, 900.0));
        assert_eq!(raw.time, Some(10.0));
        let invalid = Input {
            size: [0.0, 900.0],
            pixels_per_point: 1.0,
            focused: true,
            events: vec![],
            epoch: None,
        };
        assert!(invalid.raw(0.0, 1.0).is_err());
    }
}
