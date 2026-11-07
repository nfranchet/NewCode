//! Window chrome, global shortcuts, and central/right pane composition.

use super::{EditorApp, SidebarMode};
use crate::ui_theme::{self as theme, Icon};
use eframe::egui::{self, Color32};

impl EditorApp {
    pub(super) fn update_window_title(&self, ctx: &egui::Context) {
        let title = crate::launch::window_title(self.settings.workspace.as_deref());
        let id = egui::Id::new("workspace_window_title");
        let changed = ctx.data_mut(|data| {
            if data.get_temp::<String>(id).as_ref() == Some(&title) {
                return false;
            }
            data.insert_temp(id, title.clone());
            true
        });
        if changed {
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(title));
        }
    }

    pub(super) fn editor_shortcuts(&mut self, ctx: &egui::Context) -> bool {
        if self.bottom_tab == super::logs::Tab::Terminal && super::terminal_input::focused(ctx) {
            return false;
        }
        let opened_find = ctx
            .input(|input| input.modifiers.command && input.key_pressed(egui::Key::F))
            && self.active_file.is_some();
        if opened_find {
            self.find_open = true;
        }
        if ctx.input(|input| input.modifiers.command && input.key_pressed(egui::Key::W)) {
            if let Some(path) = self.active_file.clone() {
                self.close_file(path);
            }
        }

        opened_find
    }
    pub(super) fn toolbar(&mut self, root: &mut egui::Ui) {
        let context = root.ctx().clone();
        let ctx = &context;
        egui::Panel::top("toolbar")
            .frame(
                egui::Frame::new()
                    .fill(Color32::WHITE)
                    .stroke(egui::Stroke::new(1.0_f32, theme::BORDER))
                    .inner_margin(egui::Margin::symmetric(12, 9)),
            )
            .show(root, |ui| {
                ui.horizontal(|ui| {
                    theme::icon(ui, Icon::Spark, theme::BLUE);
                    ui.label(
                        egui::RichText::new("Focus")
                            .strong()
                            .size(18.0)
                            .color(theme::BLUE),
                    );
                    ui.separator();
                    if theme::button(ui, Icon::Folder, "Workspace", theme::BLUE, false).clicked() {
                        self.choose_workspace();
                    }
                    if let Some(root) = self.settings.workspace.as_ref() {
                        let root = crate::launch::workspace_display_path(root);
                        ui.add_sized(
                            [180.0, 24.0],
                            egui::Label::new(
                                egui::RichText::new(
                                    root.file_name()
                                        .unwrap_or(root.as_os_str())
                                        .to_string_lossy(),
                                )
                                .color(theme::MUTED),
                            )
                            .truncate(),
                        )
                        .on_hover_text(root.display().to_string());
                    }
                    ui.separator();
                    ui.menu_button("Dev Container", |ui| self.dev_container.menu(ui, ctx));
                    if ctx
                        .data_mut(|data| {
                            data.remove_temp::<bool>(egui::Id::new("show_dev_container_logs"))
                        })
                        .unwrap_or(false)
                    {
                        self.bottom_tab = super::logs::Tab::Logs;
                        self.logs.source = Some(crate::logging::session::Source::DevContainer);
                    }
                    ui.menu_button("Addons", |ui| {
                        self.addons.menu(ui, &mut self.settings.addon_enabled)
                    });
                    self.settings.migrate_addons(
                        &self
                            .addons
                            .entries
                            .iter()
                            .map(|a| a.manifest.clone())
                            .collect::<Vec<_>>(),
                    );
                    self.addons
                        .apply_settings(&self.settings.addon_enabled, &self.settings.addon_state);
                    if self.sidebar_mode == SidebarMode::Addon && self.addons.active.is_none() {
                        self.sidebar_mode = SidebarMode::Explorer;
                    }
                    ui.menu_button("Edit", |ui| {
                        if ui
                            .add_enabled_ui(self.active_file.is_some(), |ui| {
                                theme::button(ui, Icon::Save, "Save   Ctrl+S", theme::BLUE, false)
                            })
                            .inner
                            .clicked()
                        {
                            self.save_file();
                            ui.close();
                        }
                        if ui
                            .add_enabled_ui(self.active_file.is_some(), |ui| {
                                theme::button(ui, Icon::Copy, "Copy   Ctrl+C", theme::BLUE, false)
                            })
                            .inner
                            .clicked()
                        {
                            self.copy_selection(ctx);
                            ui.close();
                        }
                        if ui
                            .add_enabled_ui(self.active_file.is_some(), |ui| {
                                theme::button(ui, Icon::Paste, "Paste   Ctrl+V", theme::BLUE, false)
                            })
                            .inner
                            .clicked()
                        {
                            self.paste_from_clipboard(ctx);
                            ui.close();
                        }
                        ui.separator();
                        if ui
                            .add_enabled_ui(self.active_file.is_some(), |ui| {
                                theme::button(
                                    ui,
                                    Icon::Search,
                                    "Find in File   Ctrl+F",
                                    theme::BLUE,
                                    false,
                                )
                            })
                            .inner
                            .clicked()
                        {
                            self.find_open = true;
                            ctx.memory_mut(|memory| {
                                memory.request_focus(egui::Id::new("find_in_file_query"))
                            });
                            ui.close();
                        }
                        if ui
                            .add_enabled(
                                self.active_file.is_some(),
                                egui::Button::new("Select All   Ctrl+A"),
                            )
                            .clicked()
                        {
                            self.select_all(ctx);
                            ui.close();
                        }
                    });
                    ui.separator();
                    if ui
                        .add_enabled_ui(self.active_file.is_some(), |ui| {
                            theme::button(ui, Icon::Save, "Save", theme::BLUE, true)
                        })
                        .inner
                        .on_hover_text("Save file · Ctrl+S")
                        .clicked()
                    {
                        self.save_file();
                    }
                });
            });
    }
    pub(super) fn status_bar(&mut self, root: &mut egui::Ui) {
        egui::Panel::bottom("status_bar")
            .exact_size(25.0)
            .frame(
                egui::Frame::new()
                    .fill(theme::BLUE_SOFT)
                    .inner_margin(egui::Margin::symmetric(12, 3)),
            )
            .show(root, |ui| {
                ui.horizontal(|ui| {
                    ui.add(
                        egui::Label::new(
                            egui::RichText::new(&self.status).small().color(theme::INK),
                        )
                        .truncate(),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.small(format!("{} open files", self.open_files.len()));
                        if self.dirty {
                            ui.label(
                                egui::RichText::new("Unsaved changes")
                                    .small()
                                    .color(theme::AMBER),
                            );
                        }
                    });
                });
            });
    }
    pub(super) fn activity_bar(&mut self, root: &mut egui::Ui) {
        egui::Panel::left("activity_bar")
            .exact_size(48.0)
            .resizable(false)
            .frame(
                egui::Frame::new()
                    .fill(theme::NAVY)
                    .inner_margin(egui::Margin::symmetric(3, 8)),
            )
            .show(root, |ui| {
                ui.vertical_centered(|ui| {
                    if theme::nav_button(
                        ui,
                        Icon::Folder,
                        self.sidebar_mode == SidebarMode::Explorer,
                        "Explorer",
                    )
                    .clicked()
                    {
                        self.sidebar_mode = SidebarMode::Explorer;
                    }
                    if theme::nav_button(
                        ui,
                        Icon::Search,
                        self.sidebar_mode == SidebarMode::Search,
                        "Search in workspace",
                    )
                    .clicked()
                    {
                        self.sidebar_mode = SidebarMode::Search;
                    }
                    if theme::nav_button(
                        ui,
                        Icon::Git,
                        self.sidebar_mode == SidebarMode::Git,
                        "Source Control",
                    )
                    .clicked()
                    {
                        self.sidebar_mode = SidebarMode::Git;
                    }
                    if self
                        .addons
                        .activity(ui, self.sidebar_mode == SidebarMode::Addon)
                    {
                        self.sidebar_mode = SidebarMode::Addon;
                    }
                });
            });
    }
    pub(super) fn document_panel(&mut self, root: &mut egui::Ui, opened_find: bool) {
        let context = root.ctx().clone();
        let ctx = &context;
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(Color32::WHITE).inner_margin(8))
            .show(root, |ui| {
                self.document_tabs(ui, ctx);
                if self.sidebar_mode == SidebarMode::Addon && self.addons.central(ui) {
                    return;
                }
                if self.sidebar_mode == SidebarMode::Git && self.git_ui.show_diff(ui) {
                    return;
                }
                if self.active_file.is_some() {
                    self.find_bar(ui, ctx, opened_find);
                    ui.add_enabled_ui(!self.addons.worktree_busy(), |ui| self.text_view(ui, ctx));
                } else {
                    ui.vertical_centered(|ui| {
                        ui.add_space((ui.available_height() * 0.23).max(12.0));
                        theme::empty_art(ui, Icon::File, theme::BLUE);
                        ui.label(
                            egui::RichText::new("Make room for your next idea")
                                .size(20.0)
                                .strong()
                                .color(theme::INK),
                        );
                        ui.label(
                            egui::RichText::new(
                                "Click a file to preview it; double-click to keep it open.",
                            )
                            .color(theme::MUTED),
                        );
                        ui.add_space(12.0);
                        ui.small("Ctrl+S  Save     Ctrl+F  Find     Ctrl+W  Close tab");
                    });
                }
            });
    }
}
