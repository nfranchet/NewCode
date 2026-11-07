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
