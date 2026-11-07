//! Hosting an addon-owned UI surface, independent of its application or provider.
use super::*;
use egui::{Event, Rect, TextureHandle, TextureId};
use focus_addon_support::surface::{Frame, Input};
use std::{collections::HashMap, time::Instant};

#[derive(Default)]
pub(super) struct Surface {
    frame: Option<Frame>,
    textures: HashMap<TextureId, TextureHandle>,
    free: Vec<TextureId>,
    rect: Option<Rect>,
    events: Vec<Event>,
    focused: bool,
    sequence: u64,
    waiting: Option<Instant>,
}

#[cfg(test)]
mod tests {
    use super::*;
    fn addon(id: &str) -> Addon {
        Addon {
            storage: Default::default(),
            manifest: Manifest {
                id: id.into(),
                name: id.into(),
                panel: focus_addon_sdk::Panel::Right,
                ..Default::default()
            },
            directory: "/unused".into(),
            enabled: true,
            process: None,
            error: None,
            view: View {
                sidebar: vec![focus_addon_sdk::Widget::text("Generic addon")],
                ..Default::default()
            },
            values: Default::default(),
            icon: None,
            working: false,
            incarnation: 0,
            surface: Default::default(),
            seed: Default::default(),
            status: String::new(),
        }
    }
    #[test]
    fn right_dock_supports_any_one_both_or_no_addons() {
        let mut host = Addons::default();
        host.entries = vec![addon("first-tool"), addon("second-tool")];
        let ctx = egui::Context::default();
        let mut width = 360.0;
        let mut selected = None;
        let mut remaining = Vec::new();
        let mut visible_width = width;
        for enabled in [[true, false], [true, true], [false, true], [false, false]] {
            for (a, on) in host.entries.iter_mut().zip(enabled) {
                a.enabled = on;
            }
            for _ in 0..3 {
                let _ = crate::test_frame(
                    &ctx,
                    egui::RawInput {
                        screen_rect: Some(Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(1000.0, 700.0),
                        )),
                        ..Default::default()
                    },
                    |ui| {
                        host.right_panel(ui, &mut width, &mut selected);
                        egui::CentralPanel::default().show(ui, |ui| {
                            remaining.push(ui.available_width());
                        });
                    },
                );
            }
            if enabled.into_iter().any(|on| on) {
                visible_width = width;
            } else {
                assert_eq!(width, visible_width);
            }
        }
        assert!((remaining[2] - remaining[5]).abs() < 2.0);
        assert!((remaining[5] - remaining[8]).abs() < 2.0);
        assert!(remaining[11] > remaining[8] + 250.0);
        assert!(selected.is_none());
    }
    #[test]
    fn right_surface_owns_input_only_after_a_click_inside() {
        let mut host = Addons::default();
        host.entries.push(addon("any-tool"));
        host.entries[0].surface.rect = Some(Rect::from_min_max(
            egui::pos2(600.0, 0.0),
            egui::pos2(1000.0, 700.0),
        ));
        let click = |pos| Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: Default::default(),
        };
        let ctx = egui::Context::default();
        let mut input = egui::RawInput {
            events: vec![
                click(egui::pos2(700.0, 300.0)),
                Event::Text("addon draft".into()),
                Event::Copy,
            ],
            ..Default::default()
        };
        host.route_input(&ctx, &mut input);
        assert!(
            !input
                .events
                .iter()
                .any(|e| matches!(e, Event::Text(_) | Event::Copy))
        );
        assert!(
            host.entries[0]
                .surface
                .events
                .contains(&Event::Text("addon draft".into()))
        );
        host.entries[0].surface.events.clear();
        let mut input = egui::RawInput {
            events: vec![
                click(egui::pos2(100.0, 300.0)),
                Event::Text("editor text".into()),
            ],
            ..Default::default()
        };
        host.route_input(&ctx, &mut input);
        assert!(input.events.contains(&Event::Text("editor text".into())));
        assert!(
            !host.entries[0]
                .surface
                .events
                .iter()
                .any(|e| matches!(e, Event::Text(_)))
        );
    }
    #[test]
    fn workspace_changes_retain_textures_but_clear_old_content() {
        let ctx = egui::Context::default();
        let mut host = Surface::default();
        let frame = focus_addon_support::surface::render(
            &ctx,
            Input {
                generation: 1,
                sequence: 1,
                pixels_per_point: 1.0,
                input: egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(400.0, 600.0),
                    )),
                    ..Default::default()
                },
            },
            "Ready".into(),
            |ui| {
                ui.label("First workspace");
            },
        );
        let hostctx = egui::Context::default();
        host.accept(&hostctx, frame).unwrap();
        assert!(!host.textures.is_empty());
        host.workspace_changed();
        assert!(host.frame.is_none());
        assert!(!host.textures.is_empty());
    }
}
impl Surface {
    pub fn flush_input(&mut self, process: &Process, generation: u64) {
        if self.events.is_empty() {
            return;
        }
        if let Some(rect) = self.rect {
            self.sequence += 1;
            let input = egui::RawInput {
                screen_rect: Some(rect),
                events: std::mem::take(&mut self.events),
                focused: self.focused,
                ..Default::default()
            };
            let _ = process.send(Message::notification(
                "surface.input",
                Input {
                    generation,
                    sequence: self.sequence,
                    input,
                    pixels_per_point: 1.0,
                },
            ));
        }
    }
    pub fn workspace_changed(&mut self) {
        self.frame = None;
        self.events.clear();
        self.focused = false;
        self.waiting = None;
    }
    pub fn accept(&mut self, ctx: &egui::Context, mut frame: Frame) -> Result<(), String> {
        self.resources(ctx, &frame)?;
        for command in frame.output.commands.drain(..) {
            match command {
                egui::OutputCommand::CopyText(text) => ctx.copy_text(text),
                egui::OutputCommand::OpenUrl(url)
                    if reqwest::Url::parse(&url.url)
                        .is_ok_and(|u| matches!(u.scheme(), "http" | "https")) =>
                {
                    ctx.open_url(url)
                }
                _ => (),
            }
        }
        self.free = frame.free.clone();
        self.waiting = None;
        self.frame = Some(frame);
        Ok(())
    }
    pub fn resources(&mut self, ctx: &egui::Context, frame: &Frame) -> Result<(), String> {
        for id in self.free.drain(..) {
            self.textures.remove(&id);
        }
        for texture in &frame.textures {
            let image = texture.decode()?;
            if let Some(pos) = texture.pos {
                self.textures
                    .get_mut(&texture.id)
                    .ok_or("Addon sent a partial update for an unknown texture")?
                    .set_partial(pos, image, texture.options);
            } else {
                self.textures.insert(
                    texture.id,
                    ctx.load_texture("addon-surface", image, texture.options),
                );
            }
        }
        self.free = frame.free.clone();
        Ok(())
    }
    pub fn state(&self) -> &str {
        self.frame
            .as_ref()
            .map_or("Connecting", |f| f.state.as_str())
    }
    fn draw(
        &mut self,
        ui: &mut egui::Ui,
        process: &Process,
        generation: u64,
    ) -> Result<(), String> {
        let rect = ui.available_rect_before_wrap();
        self.rect = Some(rect);
        ui.allocate_rect(rect, egui::Sense::hover());
        if let Some(frame) = &self.frame {
            if ui.rect_contains_pointer(rect) {
                ui.ctx().set_cursor_icon(frame.output.cursor_icon);
            }
            if self.focused {
                ui.ctx().output_mut(|output| output.ime = frame.output.ime);
            }
            for (clip, mesh) in &frame.meshes {
                if let Some(texture) = self.textures.get(&mesh.texture_id) {
                    let mut mesh = mesh.clone();
                    mesh.texture_id = texture.id();
                    ui.painter()
                        .with_clip_rect(clip.intersect(rect))
                        .add(egui::Shape::mesh(mesh));
                }
            }
        } else {
            ui.painter().text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                "Connecting…",
                egui::FontId::proportional(14.0),
                crate::ui_theme::MUTED,
            );
        }
        if self
            .waiting
            .is_some_and(|time| time.elapsed() > Duration::from_secs(10))
        {
            return Err("Addon stopped responding. Use Restart to reconnect.".into());
        }
        if self.waiting.is_none() {
            self.sequence += 1;
            let input = ui.input(|i| egui::RawInput {
                screen_rect: Some(rect),
                time: Some(i.time),
                predicted_dt: i.predicted_dt,
                events: std::mem::take(&mut self.events),
                focused: self.focused,
                ..Default::default()
            });
            process.send(Message::notification(
                "surface.input",
                Input {
                    generation,
                    sequence: self.sequence,
                    input,
                    pixels_per_point: ui.ctx().pixels_per_point(),
                },
            ))?;
            self.waiting = Some(Instant::now());
        }
        ui.ctx().request_repaint_after(Duration::from_millis(33));
        Ok(())
    }
}

impl Addons {
    pub fn route_input(&mut self, _ctx: &egui::Context, raw: &mut egui::RawInput) {
        for addon in &mut self.entries {
            if !addon.enabled || addon.manifest.panel != focus_addon_sdk::Panel::Right {
                continue;
            }
            let surface = &mut addon.surface;
            let Some(rect) = surface.rect else { continue };
            raw.events.retain(|event| {
                match event {
                    Event::PointerButton {
                        pos, pressed: true, ..
                    } => {
                        let inside = rect.contains(*pos);
                        if !inside && surface.focused {
                            surface.events.push(Event::WindowFocused(false));
                        }
                        surface.focused = inside;
                    }
                    Event::WindowFocused(false) => surface.focused = false,
                    _ => (),
                }
                let keyboard = matches!(
                    event,
                    Event::Key { .. }
                        | Event::Text(_)
                        | Event::Paste(_)
                        | Event::Copy
                        | Event::Cut
                        | Event::Ime(_)
                        | Event::ModifiersChanged(_)
                );
                let pointer = matches!(
                    event,
                    Event::PointerMoved(_)
                        | Event::PointerButton { .. }
                        | Event::PointerGone
                        | Event::MouseWheel { .. }
                );
                if (keyboard && surface.focused)
                    || pointer
                    || matches!(event, Event::WindowFocused(_))
                {
                    surface.events.push(event.clone());
                }
                !(keyboard && surface.focused)
            });
        }
    }
    pub fn right_panel(
        &mut self,
        root: &mut egui::Ui,
        width: &mut f32,
        selected: &mut Option<String>,
    ) {
        let available: Vec<_> = self
            .entries
            .iter()
            .filter(|a| a.enabled && a.manifest.panel == focus_addon_sdk::Panel::Right)
            .map(|a| a.manifest.id.clone())
            .collect();
        for addon in &mut self.entries {
            addon.surface.rect = None;
        }
        if available.is_empty() {
            *selected = None;
            return;
        }
        if !selected.as_ref().is_some_and(|id| available.contains(id)) {
            *selected = available.first().cloned();
        }
        let response = egui::Panel::right("addon_right_dock")
            .resizable(true)
            .default_size(*width)
            .size_range(260.0..=700.0)
            .frame(
                egui::Frame::new()
                    .fill(egui::Color32::from_rgb(242, 250, 248))
                    .stroke(egui::Stroke::new(1.0, crate::ui_theme::BORDER))
                    .inner_margin(8),
            )
            .show(root, |ui| {
                if available.len() > 1 {
                    ui.horizontal_wrapped(|ui| {
                        for id in &available {
                            let addon = self.entries.iter().find(|a| &a.manifest.id == id).unwrap();
                            if ui
                                .selectable_label(
                                    selected.as_ref() == Some(id),
                                    format!(
                                        "{} · {}",
                                        addon.manifest.name,
                                        if addon.status.is_empty() {
                                            addon.surface.state()
                                        } else {
                                            &addon.status
                                        }
                                    ),
                                )
                                .clicked()
                            {
                                *selected = Some(id.clone());
                            }
                        }
                    });
                    ui.separator();
                }
                let id = selected.as_ref().unwrap().clone();
                let addon = self
                    .entries
                    .iter_mut()
                    .find(|a| a.manifest.id == id)
                    .unwrap();
                if let Some(error) = &addon.error {
                    ui.colored_label(crate::ui_theme::RED, error);
                    if ui.button("Restart").clicked() {
                        addon.error = None;
                        addon.process = None;
                    }
                    return;
                }
                if addon.manifest.ui == focus_addon_sdk::Presentation::Surface {
                    if let Some(process) = &addon.process {
                        if let Err(error) = addon.surface.draw(ui, process, self.context.generation)
                        {
                            addon.error = Some(error);
                            addon.process = None;
                        }
                    }
                } else {
                    let mut action = None;
                    egui::ScrollArea::vertical()
                        .id_salt(("addon-right", &id))
                        .show(ui, |ui| {
                            widgets::render(
                                ui,
                                &addon.view.sidebar,
                                &mut addon.values,
                                &mut self.cache,
                                &mut action,
                                addon.working,
                            )
                        });
                    if let Some(action) = action {
                        self.action(&id, action);
                    }
                }
            });
        *width = response.response.rect.width().max(260.0);
    }
}
