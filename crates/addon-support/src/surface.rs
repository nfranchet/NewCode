//! Generic egui surface protocol. Serialized meshes and compressed textures, never a Rust ABI.
use base64::Engine;
use egui::{
    self, PlatformOutput, RawInput, Rect, TextureId, TextureOptions,
    epaint::{ImageDelta, Mesh},
};
use serde::{Deserialize, Serialize};

pub const SURFACE_VERSION: u32 = 1;
#[derive(Serialize, Deserialize)]
pub struct Input {
    pub generation: u64,
    pub sequence: u64,
    pub input: RawInput,
    pub pixels_per_point: f32,
    /// Resolved host appearance; absent for older hosts.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub theme: Option<crate::ui_theme::Theme>,
}
#[derive(Serialize, Deserialize)]
pub struct Texture {
    pub id: TextureId,
    pub size: [usize; 2],
    pub pos: Option<[usize; 2]>,
    pub options: TextureOptions,
    pub pixels: String,
}
impl Texture {
    pub fn encode(id: TextureId, delta: ImageDelta) -> Self {
        let egui::ImageData::Color(image) = delta.image;
        let bytes: Vec<u8> = image.pixels.iter().flat_map(|p| p.to_array()).collect();
        Self {
            id,
            size: image.size,
            pos: delta.pos,
            options: delta.options,
            pixels: base64::engine::general_purpose::STANDARD
                .encode(miniz_oxide::deflate::compress_to_vec_zlib(&bytes, 3)),
        }
    }
    pub fn decode(&self) -> Result<egui::ColorImage, String> {
        let length = self.size[0]
            .checked_mul(self.size[1])
            .and_then(|v| v.checked_mul(4))
            .filter(|v| *v <= 64 * 1024 * 1024)
            .ok_or("Addon texture exceeds size limit")?;
        let compressed = base64::engine::general_purpose::STANDARD
            .decode(&self.pixels)
            .map_err(|_| "Invalid addon texture")?;
        let bytes = miniz_oxide::inflate::decompress_to_vec_zlib_with_limit(&compressed, length)
            .map_err(|_| "Invalid addon texture")?;
        if bytes.len() != length {
            return Err("Invalid addon texture dimensions".into());
        }
        // egui pixels are premultiplied; preserve them exactly on the receiving side.
        Ok(egui::ColorImage::new(
            self.size,
            bytes
                .chunks_exact(4)
                .map(|p| egui::Color32::from_rgba_premultiplied(p[0], p[1], p[2], p[3]))
                .collect(),
        ))
    }
}
#[derive(Serialize, Deserialize)]
pub struct Frame {
    pub generation: u64,
    pub sequence: u64,
    pub meshes: Vec<(Rect, Mesh)>,
    pub textures: Vec<Texture>,
    pub free: Vec<TextureId>,
    pub output: PlatformOutput,
    pub state: String,
}
pub fn render(
    ctx: &egui::Context,
    input: Input,
    state: String,
    show: impl FnMut(&mut egui::Ui),
) -> Frame {
    if let Some(theme) = &input.theme {
        crate::ui_theme::apply_theme(ctx, theme);
    }
    ctx.set_pixels_per_point(input.pixels_per_point.clamp(0.5, 4.0));
    let mut output = ctx.run_ui(input.input, show);
    let meshes = ctx
        .tessellate(output.shapes, output.pixels_per_point)
        .into_iter()
        .filter_map(|p| match p.primitive {
            egui::epaint::Primitive::Mesh(mesh) => Some((p.clip_rect, mesh)),
            _ => None,
        })
        .collect();
    Frame {
        generation: input.generation,
        sequence: input.sequence,
        meshes,
        textures: output
            .textures_delta
            .set
            .drain()
            .flat_map(|(id, deltas)| deltas.into_iter().map(move |d| Texture::encode(id, d)))
            .collect(),
        free: output.textures_delta.free.drain().collect(),
        output: output.platform_output,
        state,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn host_theme_round_trips_and_is_applied_without_resetting_surface_state() {
        use crate::ui_theme::{self, Theme};
        let ctx = egui::Context::default();
        ui_theme::apply(&ctx);
        let mut theme = Theme::from_style((*ctx.global_style()).clone());
        theme.colors.accent = egui::Color32::from_rgb(17, 93, 147);
        theme.colors.raised = egui::Color32::from_rgb(241, 232, 219);
        theme.style.visuals.window_corner_radius = egui::CornerRadius::same(2);
        theme.style.spacing.interact_size.y = 25.0;
        let id = egui::Id::new("persistent_draft");
        let mut draft = "keep this draft".to_owned();
        for sequence in [1, 2] {
            if sequence == 2 {
                theme.colors.accent = egui::Color32::from_rgb(141, 51, 97);
                theme.style.spacing.interact_size.y = 29.0;
            }
            let input = Input {
                generation: 1,
                sequence,
                pixels_per_point: 1.0,
                theme: Some(theme.clone()),
                input: egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(400.0, 300.0),
                    )),
                    ..Default::default()
                },
            };
            let encoded = serde_json::to_value(&input).unwrap();
            let decoded: Input = serde_json::from_value(encoded).unwrap();
            assert_eq!(
                serde_json::to_value(decoded.theme.as_ref().unwrap()).unwrap(),
                serde_json::to_value(&theme).unwrap()
            );
            let frame = render(&ctx, decoded, "Ready".into(), |ui| {
                assert_eq!(ui_theme::colors(ui).raised, theme.colors.raised);
                assert_eq!(
                    ui.spacing().interact_size.y,
                    theme.style.spacing.interact_size.y
                );
                assert_eq!(
                    ui.visuals().window_corner_radius,
                    egui::CornerRadius::same(2)
                );
                ui.add(egui::TextEdit::singleline(&mut draft).id(id));
                ui_theme::button(
                    ui,
                    ui_theme::Icon::Send,
                    "Send",
                    ui_theme::colors(ui).accent,
                    true,
                );
            });
            assert!(
                frame
                    .meshes
                    .iter()
                    .any(|(_, mesh)| mesh.vertices.iter().any(|v| v.color == theme.colors.accent))
            );
            assert_eq!(draft, "keep this draft");
            if sequence == 1 {
                ctx.memory_mut(|memory| memory.request_focus(id));
            } else {
                assert!(ctx.memory(|memory| memory.has_focus(id)));
            }
        }
    }

    #[test]
    fn older_surface_input_without_theme_remains_supported() {
        let encoded = serde_json::json!({
            "generation": 1, "sequence": 1, "pixels_per_point": 1.0,
            "input": egui::RawInput::default(),
        });
        let input: Input = serde_json::from_value(encoded).unwrap();
        assert!(input.theme.is_none());
    }

    #[test]
    fn textures_preserve_premultiplied_pixels() {
        let image = egui::ColorImage::new(
            [2, 1],
            vec![
                egui::Color32::from_rgba_premultiplied(10, 20, 30, 50),
                egui::Color32::WHITE,
            ],
        );
        let texture = Texture::encode(
            TextureId::Managed(0),
            ImageDelta::full(image.clone(), TextureOptions::LINEAR),
        );
        assert_eq!(texture.decode().unwrap().pixels, image.pixels);
    }
}
