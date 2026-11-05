//! Installed fonts are scoped to the terminal, leaving the rest of the UI unchanged.
use eframe::egui::{self, FontData, FontDefinitions, FontFamily, FontId};
use fontdb::{Database, Family, Query, Style, Weight};
use std::sync::{Arc, OnceLock};

pub(crate) fn font_id() -> FontId {
    FontId::new(14.0, FontFamily::Name("terminal".into()))
}
pub(crate) fn apply(ctx: &egui::Context) {
    static FONTS: OnceLock<FontDefinitions> = OnceLock::new();
    ctx.set_fonts(FONTS.get_or_init(discover).clone());
}
fn discover() -> FontDefinitions {
    let mut fonts = FontDefinitions::default();
    let mut db = Database::new();
    db.load_system_fonts();
    let regular =
        |face: &&fontdb::FaceInfo| face.style == Style::Normal && face.weight == Weight::NORMAL;
    let primary = system_monospace(&db)
        .or_else(|| {
            db.query(&Query {
                families: &[Family::Monospace],
                ..Default::default()
            })
        })
        .or_else(|| {
            db.faces()
                .filter(regular)
                .find(|face| face.monospaced)
                .map(|face| face.id)
        });
    let nerd = db
        .faces()
        .filter(regular)
        .find(|face| face.monospaced && face.families.iter().any(|(name, _)| name.contains("Nerd")))
        .map(|face| face.id);
    let symbols = db
        .faces()
        .filter(regular)
        .find(|face| {
            db.with_face_data(face.id, |data, index| {
                ttf_parser::Face::parse(data, index).is_ok_and(|font| {
                    ['✗', '➜', '─', '│']
                        .iter()
                        .all(|ch| font.glyph_index(*ch).is_some())
                })
            })
            .unwrap_or(false)
        })
        .map(|face| face.id);
    let mut ids = Vec::new();
    let mut family = Vec::new();
    for id in [primary, nerd, symbols].into_iter().flatten() {
        if ids.contains(&id) {
            continue;
        }
        ids.push(id);
        if let Some(data) = db.with_face_data(id, |bytes, index| {
            let mut data = FontData::from_owned(bytes.to_vec());
            data.index = index;
            data
        }) {
            let name = format!(
                "terminal-system-{}-{}",
                family.len(),
                db.face(id)
                    .map(|face| face.post_script_name.as_str())
                    .unwrap_or("font")
            );
            fonts.font_data.insert(name.clone(), Arc::new(data));
            family.push(name);
        }
    }
    family.extend(fonts.families[&FontFamily::Monospace].iter().cloned());
    fonts.families.insert(font_id().family, family);
    fonts
}

fn system_monospace(db: &Database) -> Option<fontdb::ID> {
    #[cfg(target_os = "linux")]
    {
        let output = std::process::Command::new("fc-match")
            .args(["-f", "%{postscriptname}", "monospace"])
            .output()
            .ok()?;
        if !output.status.success() {
            return None;
        }
        let name = std::str::from_utf8(&output.stdout).ok()?.trim();
        return db
            .faces()
            .find(|face| face.post_script_name == name)
            .map(|face| face.id);
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = db;
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn prompt_symbols_resolve_in_terminal_family_without_changing_ui_fonts() {
        let ctx = egui::Context::default();
        let definitions = discover();
        assert_eq!(
            definitions.families[&FontFamily::Monospace],
            FontDefinitions::default().families[&FontFamily::Monospace]
        );
        for ch in "✗➜─│".chars() {
            assert!(
                definitions.families[&font_id().family].iter().any(|name| {
                    let data = &definitions.font_data[name];
                    ttf_parser::Face::parse(data.font.as_ref(), data.index)
                        .is_ok_and(|font| font.glyph_index(ch).is_some())
                }),
                "missing prompt glyph {ch}"
            );
        }
        ctx.set_fonts(definitions);
        crate::test_frame(&ctx, Default::default(), |ui| {
            ui.fonts_mut(|fonts| {
                let text =
                    fonts.layout_no_wrap("✗➜─│\u{10ffff}".into(), font_id(), egui::Color32::WHITE);
                let glyphs = &text.rows[0].glyphs;
                for glyph in &glyphs[..4] {
                    assert_ne!(
                        glyph.uv_rect.min, glyphs[4].uv_rect.min,
                        "prompt symbol used replacement glyph"
                    );
                }
            });
        })
        .drop_without_applying_deltas();
    }
}
