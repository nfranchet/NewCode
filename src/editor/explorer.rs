//! Workspace tree rendering, entry context menus, path changes, and rename UI.

use super::EditorApp;
use super::deletion::Deletion;
use crate::ui_theme::{self as theme, Icon};
use eframe::egui;
use std::{
    ffi::OsString,
    fs,
    path::{Path, PathBuf},
};

struct TreeEntry {
    path: PathBuf,
    name: OsString,
    is_dir: bool,
}

impl TreeEntry {
    fn snapshot(path: PathBuf) -> Self {
        Self {
            name: path.file_name().unwrap_or_default().to_os_string(),
            is_dir: path.is_dir(),
            path,
        }
    }
}

fn sort_entries(entries: &mut [TreeEntry]) {
    // Filesystem metadata can change mid-sort; compare only captured values.
    entries.sort_by(|a, b| (!a.is_dir, &a.name).cmp(&(!b.is_dir, &b.name)));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sorting_uses_snapshot_after_filesystem_changes() {
        let root = tempfile::tempdir().unwrap();
        let mut entries = Vec::new();
        for (name, is_dir) in [
            ("z-folder", true),
            ("z-file", false),
            ("a-folder", true),
            ("a-file", false),
        ] {
            let path = root.path().join(name);
            if is_dir {
                fs::create_dir(&path).unwrap();
            } else {
                fs::write(&path, "").unwrap();
            }
            entries.push(TreeEntry::snapshot(path));
        }
        // Replace one directory with a file and remove the other entirely.
        fs::remove_dir(root.path().join("z-folder")).unwrap();
        fs::write(root.path().join("z-folder"), "").unwrap();
        fs::remove_dir(root.path().join("a-folder")).unwrap();
        sort_entries(&mut entries);
        let names: Vec<_> = entries
            .iter()
            .map(|entry| entry.name.to_str().unwrap())
            .collect();
        assert_eq!(names, ["a-folder", "z-folder", "a-file", "z-file"]);
        assert!(entries[0].is_dir && entries[1].is_dir);
        assert!(!entries[2].is_dir && !entries[3].is_dir);
    }
}

impl EditorApp {
    pub(super) fn tree(&mut self, ui: &mut egui::Ui, dir: &Path) {
        self.tree_at_depth(ui, dir, 0);
    }

    fn tree_at_depth(&mut self, ui: &mut egui::Ui, dir: &Path, depth: usize) {
        let metrics = theme::metrics(ui);
        if self
            .creation
            .as_ref()
            .is_some_and(|entry| entry.parent == dir)
        {
            ui.horizontal(|ui| {
                ui.add_space(8.0 + depth as f32 * metrics.tree_indent);
                ui.vertical(|ui| self.creation_row(ui));
            });
        }
        let Ok(entries) = fs::read_dir(dir) else {
            return;
        };
        let mut entries: Vec<_> = entries
            .flatten()
            .map(|entry| TreeEntry::snapshot(entry.path()))
            .collect();
        sort_entries(&mut entries);
        for entry in entries {
            let path = entry.path;
            let name = entry.name.to_string_lossy();
            if entry.is_dir {
                let id = ui.make_persistent_id(("folder", &path));
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
                let response = self.explorer_row(ui, &path, &name, depth, Some(state.is_open()));
                if response.clicked() {
                    state.set_open(!state.is_open());
                }
                let open = state.is_open();
                state.store(ui.ctx());
                response.context_menu(|ui| self.file_menu(ui, &path));
                if open {
                    self.tree_at_depth(ui, &path, depth + 1);
                }
            } else {
                let response = self.explorer_row(ui, &path, &name, depth, None);
                if response.double_clicked() {
                    self.open_file(path.clone());
                } else if response.clicked() {
                    self.preview_file(path.clone());
                }
                response.context_menu(|ui| self.file_menu(ui, &path));
            }
        }
    }

    fn explorer_row(
        &self,
        ui: &mut egui::Ui,
        path: &Path,
        name: &str,
        depth: usize,
        expanded: Option<bool>,
    ) -> egui::Response {
        let metrics = theme::metrics(ui);
        let colors = theme::colors(ui);
        let (rect, _) = ui.allocate_exact_size(
            egui::vec2(ui.available_width(), metrics.tree_row_height),
            egui::Sense::hover(),
        );
        let id = egui::Id::new((
            if expanded.is_some() {
                "explorer_folder_name"
            } else {
                "explorer_file_row"
            },
            path,
        ));
        let response = ui.interact(rect, id, egui::Sense::click());
        let selected = expanded.is_none() && self.active_file.as_deref() == Some(path);
        let painter = ui.painter().with_clip_rect(ui.clip_rect().intersect(rect));
        if selected || response.hovered() {
            painter.rect_filled(
                rect,
                0.0,
                if selected {
                    colors.selection
                } else {
                    colors.hover
                },
            );
        }
        if response.has_focus() {
            painter.rect_stroke(
                rect.shrink(1.0),
                0.0,
                egui::Stroke::new(1.0, colors.accent),
                egui::StrokeKind::Inside,
            );
        }
        let icon_x = rect.left() + 19.0 + depth as f32 * metrics.tree_indent;
        if let Some(open) = expanded {
            let center = egui::pos2(icon_x + 6.0, rect.center().y);
            let points = if open {
                vec![
                    center + egui::vec2(-3.5, -2.0),
                    center + egui::vec2(3.5, -2.0),
                    center + egui::vec2(0.0, 3.0),
                ]
            } else {
                vec![
                    center + egui::vec2(-2.0, -3.5),
                    center + egui::vec2(-2.0, 3.5),
                    center + egui::vec2(3.0, 0.0),
                ]
            };
            painter.add(egui::Shape::convex_polygon(
                points,
                colors.muted,
                egui::Stroke::NONE,
            ));
        } else {
            let (kind, color) = theme::file_kind(ui, path);
            painter.text(
                egui::pos2(icon_x + 6.0, rect.center().y),
                egui::Align2::CENTER_CENTER,
                kind,
                egui::FontId::monospace(10.0),
                color,
            );
        }
        let text_x = icon_x + 20.0;
        let mut job = egui::text::LayoutJob::single_section(
            name.into(),
            egui::TextFormat::simple(
                egui::TextStyle::Body.resolve(ui.style()),
                if selected {
                    colors.accent_strong
                } else {
                    colors.text
                },
            ),
        );
        job.wrap.max_width = (rect.right() - text_x - 8.0).max(0.0);
        job.wrap.max_rows = 1;
        job.wrap.break_anywhere = true;
        let galley = ui.fonts_mut(|fonts| fonts.layout_job(job));
        painter.galley(
            egui::pos2(text_x, rect.center().y - galley.size().y * 0.5),
            galley,
            colors.text,
        );
        response.widget_info(|| {
            egui::WidgetInfo::labeled(egui::WidgetType::Button, ui.is_enabled(), name)
        });
        response.on_hover_text(path.display().to_string())
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
        let delete = ui.button(egui::RichText::new("Delete…").color(theme::colors(ui).error));
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
                        .color(theme::colors(ui).accent),
                )
                .wrap(),
            );
            self.creation_row(ui);
            ui.separator();
        }
        if let Some(root) = workspace {
            let colors = theme::colors(ui);
            let (rect, _) = ui.allocate_exact_size(
                egui::vec2(ui.available_width(), theme::metrics(ui).project_row_height),
                egui::Sense::hover(),
            );
            ui.painter().hline(
                rect.x_range(),
                rect.top(),
                egui::Stroke::new(1.0, colors.border),
            );
            ui.painter().hline(
                rect.x_range(),
                rect.bottom(),
                egui::Stroke::new(1.0, colors.border),
            );
            let label = root
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_uppercase();
            let chevron = rect.left_center() + egui::vec2(12.0, 0.0);
            ui.painter().add(egui::Shape::convex_polygon(
                vec![
                    chevron + egui::vec2(-3.5, -2.0),
                    chevron + egui::vec2(3.5, -2.0),
                    chevron + egui::vec2(0.0, 3.0),
                ],
                colors.muted,
                egui::Stroke::NONE,
            ));
            let mut job = egui::text::LayoutJob::simple_singleline(
                label,
                egui::FontId::proportional(11.0),
                colors.text,
            );
            job.wrap.max_width = (rect.width() - 30.0).max(0.0);
            job.wrap.max_rows = 1;
            let galley = ui.fonts_mut(|fonts| fonts.layout_job(job));
            ui.painter().galley(
                egui::pos2(rect.left() + 22.0, rect.center().y - galley.size().y * 0.5),
                galley,
                colors.text,
            );
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    ui.spacing_mut().item_spacing.y = 0.0;
                    ui.add_space(5.0);
                    self.tree(ui, &root);
                });
        } else {
            ui.vertical_centered(|ui| {
                theme::empty_art(ui, Icon::Folder, theme::colors(ui).accent);
                ui.strong("Your project starts here");
                ui.label("Choose a workspace to browse its files.");
                if theme::button(
                    ui,
                    Icon::Folder,
                    "Open workspace",
                    theme::colors(ui).accent,
                    true,
                )
                .clicked()
                {
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
