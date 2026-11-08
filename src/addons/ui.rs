use super::*;
use crate::ui_theme as theme;
use std::fs;

impl Addons {
    pub fn menu(&mut self, ui: &mut egui::Ui, _enabled: &mut BTreeMap<String, bool>) {
        if ui.button("Manage addons…").clicked() {
            self.open_manager();
            ui.close();
        }
    }
    pub fn sidebar(&mut self, ui: &mut egui::Ui) {
        let Some(id) = self.active.clone() else {
            ui.label("Select an addon");
            return;
        };
        if self
            .entries
            .iter()
            .any(|a| a.manifest.id == id && a.enabled && a.process.is_none() && a.error.is_none())
        {
            self.activate(id.clone(), ui.ctx());
        }
        let Some(addon) = self.entries.iter_mut().find(|a| a.manifest.id == id) else {
            return;
        };
        ui.heading(&addon.manifest.name);
        if let Some(error) = &addon.error {
            ui.colored_label(theme::colors(ui).error, error);
            if ui.button("Restart").clicked() {
                addon.error = None;
            }
            return;
        }
        if addon.working || addon.process.as_ref().is_none_or(|p| !p.ready) {
            ui.spinner();
        }
        let mut action = None;
        egui::ScrollArea::vertical()
            .id_salt(("addon-sidebar", &id))
            .show(ui, |ui| {
                widgets::render(
                    ui,
                    &addon.view.sidebar,
                    &mut addon.values,
                    &mut self.cache,
                    &mut action,
                    addon.working,
                );
            });
        if let Some(action) = action {
            self.action(&id, action);
        }
    }
    pub fn central(&mut self, ui: &mut egui::Ui) -> bool {
        if !self.detail {
            return false;
        }
        let Some(id) = self.active.clone() else {
            return false;
        };
        let Some(addon) = self.entries.iter_mut().find(|a| a.manifest.id == id) else {
            return false;
        };
        ui.horizontal(|ui| {
            ui.heading(&addon.view.title);
            if ui.button("Back to editor").clicked() {
                self.detail = false;
            }
        });
        let mut action = None;
        egui::ScrollArea::vertical()
            .id_salt(("addon-detail", &id))
            .show(ui, |ui| {
                widgets::render(
                    ui,
                    &addon.view.detail,
                    &mut addon.values,
                    &mut self.cache,
                    &mut action,
                    addon.working,
                );
            });
        if let Some(action) = action {
            self.action(&id, action);
        }
        true
    }
    pub(super) fn action(&mut self, id: &str, action: String) {
        if action == "host.credentials" {
            self.credentials = Some(Credentials {
                addon: id.to_owned(),
                secret: String::new(),
                result: String::new(),
                pending: None,
            });
            return;
        }
        if let Some(addon) = self.entries.iter_mut().find(|a| a.manifest.id == id) {
            if let Some(process) = &addon.process {
                let message = Action {
                    generation: self.context.generation,
                    id: action,
                    values: addon.values.clone(),
                };
                if let Err(error) = process.send(Message::notification("action", message)) {
                    addon.error = Some(error);
                }
                addon.working = true;
            }
        }
    }
    pub fn activity(&mut self, ui: &mut egui::Ui, selected: bool) -> bool {
        let mut chosen = None;
        for addon in self
            .entries
            .iter_mut()
            .filter(|a| a.enabled && a.manifest.panel == focus_addon_sdk::Panel::Left)
        {
            if addon.icon.is_none() {
                if let Ok(bytes) = fs::read(addon.directory.join(&addon.manifest.icon)) {
                    if let Ok(image) =
                        image::load_from_memory_with_format(&bytes, image::ImageFormat::Png)
                    {
                        let image = image.to_rgba8();
                        let image = egui::ColorImage::from_rgba_unmultiplied(
                            [image.width() as usize, image.height() as usize],
                            image.as_raw(),
                        );
                        addon.icon = Some(ui.ctx().load_texture(
                            &addon.manifest.id,
                            image,
                            Default::default(),
                        ));
                    }
                }
            }
            let active = selected && self.active.as_ref() == Some(&addon.manifest.id);
            let response = if let Some(icon) = &addon.icon {
                ui.add(
                    egui::Button::image(
                        egui::Image::new(icon).fit_to_exact_size(egui::vec2(24.0, 24.0)),
                    )
                    .selected(active)
                    .min_size(egui::vec2(38.0, 38.0)),
                )
            } else {
                ui.add(
                    egui::Button::new("◇")
                        .selected(active)
                        .min_size(egui::vec2(38.0, 38.0)),
                )
            };
            if response.on_hover_text(&addon.manifest.name).clicked() {
                chosen = Some(addon.manifest.id.clone());
            }
        }
        if let Some(id) = chosen {
            self.activate(id, ui.ctx());
            true
        } else {
            false
        }
    }
}
