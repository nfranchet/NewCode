//! Workspace tree rendering, entry context menus, path changes, and rename UI.

use super::EditorApp;
use super::deletion::Deletion;
use crate::ui_theme::{self as theme, Icon};
use eframe::egui;
use std::{fs, path::Path};

impl EditorApp {
    pub(super) fn tree(&mut self, ui: &mut egui::Ui, dir: &Path) {
        if self
            .creation
            .as_ref()
            .is_some_and(|entry| entry.parent == dir)
        {
            self.creation_row(ui);
        }
        let Ok(entries) = fs::read_dir(dir) else {
            return;
        };
        let mut paths: Vec<_> = entries.flatten().map(|e| e.path()).collect();
        paths.sort_by_key(|p| {
            (
                !p.is_dir(),
                p.file_name().unwrap_or_default().to_os_string(),
            )
        });
        for path in paths {
            let name = path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();
            if path.is_dir() {
                let id = ui.make_persistent_id(("folder", &path));
                let mut clicked = false;
                let mut state = egui::collapsing_header::CollapsingState::load_with_default_open(
                    ui.ctx(),
                    id,
                    false,
                );
                if self
                    .creation
                    .as_ref()
                    .is_some_and(|entry| entry.needs_focus && entry.parent.starts_with(&path))
                {
                    state.set_open(true);
                }
                let mut folder = state.show_header(ui, |ui| {
                    let (icon_rect, _) =
                        ui.allocate_exact_size(egui::vec2(20.0, 20.0), egui::Sense::hover());
                    theme::paint_icon(ui, icon_rect, Icon::Folder, theme::AMBER);
                    let icon = ui.interact(icon_rect, id.with("icon"), egui::Sense::click());
                    let label = ui.add(
                        egui::Label::new(egui::RichText::new(name).color(theme::INK)).truncate(),
                    );
                    let label = ui.interact(
                        label.rect,
                        egui::Id::new(("explorer_folder_name", &path)),
                        egui::Sense::click(),
                    );
                    let response = icon.union(label);
                    clicked = response.clicked();
                    response
                });
                if clicked {
                    log::debug!("Explorer folder expansion toggled");
                    folder.toggle();
                }
                let (toggle, header, _) = folder.body(|ui| self.tree(ui, &path));
                // The layout response is hover-only. Use the clickable children so
                // secondary clicks on the name, folder icon, or arrow open the menu.
                header
                    .inner
                    .union(toggle)
                    .context_menu(|ui| self.file_menu(ui, &path));
            } else {
                let response = ui
                    .push_id(("explorer_file", &path), |ui| {
                        ui.add(
                            egui::Button::selectable(
                                self.active_file.as_ref() == Some(&path),
                                theme::file_text(&path, &name),
                            )
                            .truncate(),
                        )
                        .on_hover_text(path.display().to_string())
                    })
                    .inner;
                ui.interact(
                    response.rect,
                    egui::Id::new(("explorer_file_row", &path)),
                    egui::Sense::hover(),
                );
                if response.double_clicked() {
                    self.open_file(path.clone());
                } else if response.clicked() {
                    self.preview_file(path.clone());
                }
                response.context_menu(|ui| self.file_menu(ui, &path));
            }
        }
    }

    pub(super) fn file_menu(&mut self, ui: &mut egui::Ui, path: &Path) {
        if ui.button("Rename…").clicked() {
            self.rename_target = Some(path.to_path_buf());
            self.rename_text = path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();
            ui.close();
        }
        if ui.button("Move to…").clicked() {
            if let Some(dest) = rfd::FileDialog::new()
                .set_directory(path.parent().unwrap_or(Path::new(".")))
                .save_file()
            {
                if let Err(e) = fs::rename(path, &dest) {
                    self.status = format!("Move failed: {e}");
                } else {
                    self.update_open_path(path, &dest);
                }
            }
            ui.close();
        }
        let delete = ui.button(egui::RichText::new("Delete…").color(theme::RED));
        ui.interact(
            delete.rect,
            egui::Id::new(("explorer_delete_action", path)),
            egui::Sense::hover(),
        );
        if delete.clicked() {
            self.deletion = Some(Deletion {
                path: path.to_path_buf(),
                error: String::new(),
                needs_focus: true,
            });
            ui.close();
        }
    }

    pub(super) fn explorer_panel(&mut self, ui: &mut egui::Ui) {
        let workspace = self
            .settings
            .workspace
            .as_ref()
            .map(|root| fs::canonicalize(root).unwrap_or(root.clone()));
        let outside = self
            .creation
            .as_ref()
            .filter(|entry| {
                workspace
                    .as_ref()
                    .is_none_or(|root| !entry.parent.starts_with(root))
            })
            .map(|entry| entry.parent.clone());
        if let Some(parent) = outside {
            ui.add(
                egui::Label::new(
                    egui::RichText::new(format!("Create in {}", parent.display()))
                        .small()
                        .color(theme::BLUE),
                )
                .wrap(),
            );
            self.creation_row(ui);
            ui.separator();
        }
        if let Some(root) = workspace {
            ui.label(
                egui::RichText::new(root.file_name().unwrap_or_default().to_string_lossy())
                    .strong(),
            );
            egui::ScrollArea::vertical().show(ui, |ui| self.tree(ui, &root));
        } else {
            ui.vertical_centered(|ui| {
                theme::empty_art(ui, Icon::Folder, theme::BLUE);
                ui.strong("Your project starts here");
                ui.label("Choose a workspace to browse its files.");
                if theme::button(ui, Icon::Folder, "Open workspace", theme::BLUE, true).clicked() {
                    self.choose_workspace();
                }
            });
        }
    }
    pub(super) fn rename_dialog(&mut self, ctx: &egui::Context) {
        if let Some(path) = self.rename_target.clone() {
            egui::Window::new("Rename")
                .collapsible(false)
                .show(ctx, |ui| {
                    ui.text_edit_singleline(&mut self.rename_text);
                    ui.horizontal(|ui| {
                        if ui.button("Rename").clicked() {
                            let dest = path.with_file_name(&self.rename_text);
                            self.status = match fs::rename(&path, &dest) {
                                Ok(()) => {
                                    self.update_open_path(&path, &dest);
                                    "Renamed".into()
                                }
                                Err(e) => format!("Rename failed: {e}"),
                            };
                            self.rename_target = None;
                        }
                        if ui.button("Cancel").clicked() {
                            self.rename_target = None;
                        }
                    });
                });
        }
    }
}
