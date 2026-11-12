//! Window chrome, global shortcuts, and central/right pane composition.

use super::{EditorApp, SidebarMode};
use crate::ui_theme::{self as theme, Icon};
use eframe::egui;

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
        let colors = theme::colors(root);
        let height = theme::metrics(root).toolbar_height;
        egui::Panel::top("toolbar")
            .exact_size(height)
            .frame(egui::Frame::new().fill(colors.chrome).inner_margin(0))
            .show(root, |ui| {
                let rect = ui.available_rect_before_wrap();
                let side_width = 300.0_f32.min(rect.width() * 0.36);
                let left = egui::Rect::from_min_max(
                    rect.min + egui::vec2(10.0, 0.0),
                    egui::pos2(rect.left() + side_width, rect.bottom()),
                );
                let right = egui::Rect::from_min_max(
                    egui::pos2(rect.right() - side_width, rect.top()),
                    rect.max - egui::vec2(10.0, 0.0),
                );
                let center = egui::Rect::from_min_max(
                    egui::pos2(rect.left() + side_width, rect.top()),
                    egui::pos2(rect.right() - side_width, rect.bottom()),
                );
                ui.scope_builder(
                    egui::UiBuilder::new()
                        .id_salt("toolbar_workspace")
                        .max_rect(left)
                        .layout(egui::Layout::left_to_right(egui::Align::Center)),
                    |ui| {
                        if theme::button(ui, Icon::Folder, "Workspace", colors.accent, false)
                            .clicked()
                        {
                            self.choose_workspace();
                        }
                    },
                );
                let title = self
                    .settings
                    .workspace
                    .as_deref()
                    .map(|path| {
                        let path = crate::launch::workspace_display_path(path);
                        format!(
                            "Focus Editor · {}",
                            path.file_name()
                                .unwrap_or(path.as_os_str())
                                .to_string_lossy()
                        )
                    })
                    .unwrap_or_else(|| "Focus Editor".into());
                ui.scope_builder(
                    egui::UiBuilder::new()
                        .id_salt("toolbar_title")
                        .max_rect(center)
                        .layout(egui::Layout::left_to_right(egui::Align::Center)),
                    |ui| {
                        ui.add_sized(
                            center.size(),
                            egui::Label::new(egui::RichText::new(title).strong().size(12.0))
                                .truncate(),
                        );
                    },
                );
                ui.scope_builder(
                    egui::UiBuilder::new()
                        .id_salt("toolbar_actions")
                        .max_rect(right)
                        .layout(egui::Layout::right_to_left(egui::Align::Center)),
                    |ui| {
                        self.toolbar_actions(ui);
                    },
                );
                ui.painter().hline(
                    rect.x_range(),
                    rect.bottom() - 1.0,
                    egui::Stroke::new(1.0, colors.border),
                );
            });
    }

    fn toolbar_actions(&mut self, ui: &mut egui::Ui) {
        let context = ui.ctx().clone();
        let ctx = &context;
        if ui
            .add_enabled_ui(self.active_file.is_some(), |ui| {
                theme::button(ui, Icon::Save, "Save", theme::colors(ui).accent, true)
            })
            .inner
            .on_hover_text("Save file · Ctrl+S")
            .clicked()
        {
            self.save_file();
        }
        ui.separator();
        ui.menu_button("Edit", |ui| {
            if ui
                .add_enabled_ui(self.active_file.is_some(), |ui| {
                    theme::button(
                        ui,
                        Icon::Save,
                        "Save   Ctrl+S",
                        theme::colors(ui).accent,
                        false,
                    )
                })
                .inner
                .clicked()
            {
                self.save_file();
                ui.close();
            }
            if ui
                .add_enabled_ui(self.active_file.is_some(), |ui| {
                    theme::button(
                        ui,
                        Icon::Copy,
                        "Copy   Ctrl+C",
                        theme::colors(ui).accent,
                        false,
                    )
                })
                .inner
                .clicked()
            {
                self.copy_selection(ctx);
                ui.close();
            }
            if ui
                .add_enabled_ui(self.active_file.is_some(), |ui| {
                    theme::button(
                        ui,
                        Icon::Paste,
                        "Paste   Ctrl+V",
                        theme::colors(ui).accent,
                        false,
                    )
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
                        theme::colors(ui).accent,
                        false,
                    )
                })
                .inner
                .clicked()
            {
                self.find_open = true;
                ctx.memory_mut(|memory| memory.request_focus(egui::Id::new("find_in_file_query")));
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
        ui.menu_button("Addons", |ui| {
            self.addons.menu(ui, &mut self.settings.addon_enabled)
        });
        self.addons
            .apply_settings(&self.settings.addon_enabled, &self.settings.addon_state);
        if self.addons.take_manager_request() {
            self.sidebar_mode = SidebarMode::AddonManager;
        }
        if self.sidebar_mode == SidebarMode::Addon && self.addons.active.is_none() {
            self.sidebar_mode = SidebarMode::Explorer;
        }
        ui.menu_button("Dev Container", |ui| self.dev_container.menu(ui, ctx));
        if ctx
            .data_mut(|data| data.remove_temp::<bool>(egui::Id::new("show_dev_container_logs")))
            .unwrap_or(false)
        {
            self.bottom_tab = super::logs::Tab::Logs;
            self.logs.source = Some(crate::logging::session::Source::DevContainer);
        }
    }
    pub(super) fn status_bar(&mut self, root: &mut egui::Ui) {
        let colors = theme::colors(root);
        egui::Panel::bottom("status_bar")
            .exact_size(theme::metrics(root).status_height)
            .frame(
                egui::Frame::new()
                    .fill(colors.accent)
                    .inner_margin(egui::Margin::symmetric(12, 3)),
            )
            .show(root, |ui| {
                ui.horizontal(|ui| {
                    ui.add(
                        egui::Label::new(
                            egui::RichText::new(&self.status)
                                .small()
                                .color(colors.on_accent),
                        )
                        .truncate(),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(
                            egui::RichText::new(format!("{} open files", self.open_files.len()))
                                .small()
                                .color(colors.on_accent),
                        );
                        if self.dirty {
                            ui.label(
                                egui::RichText::new("Unsaved changes")
                                    .small()
                                    .color(colors.on_accent),
                            );
                        }
                    });
                });
            });
    }
    pub(super) fn activity_bar(&mut self, root: &mut egui::Ui) {
        let colors = theme::colors(root);
        egui::Panel::left("activity_bar")
            .exact_size(theme::metrics(root).activity_width)
            .resizable(false)
            .frame(egui::Frame::new().fill(colors.activity).inner_margin(0))
            .show(root, |ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
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
                    if theme::nav_button(
                        ui,
                        Icon::Addons,
                        self.sidebar_mode == SidebarMode::AddonManager,
                        "Addons",
                    )
                    .clicked()
                    {
                        self.sidebar_mode = SidebarMode::AddonManager;
                        self.addons.open_manager();
                        self.addons.take_manager_request();
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
        let colors = theme::colors(root);
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(colors.editor).inner_margin(0))
            .show(root, |ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
                self.document_tabs(ui, ctx);
                if self.sidebar_mode == SidebarMode::AddonManager
                    && self
                        .addons
                        .manager_central(ui, &mut self.settings.addon_enabled)
                {
                    return;
                }
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
                        theme::empty_art(ui, Icon::File, theme::colors(ui).accent);
                        ui.label(
                            egui::RichText::new("Make room for your next idea")
                                .size(20.0)
                                .strong()
                                .color(theme::colors(ui).text),
                        );
                        ui.label(
                            egui::RichText::new(
                                "Click a file to preview it; double-click to keep it open.",
                            )
                            .color(theme::colors(ui).muted),
                        );
                        ui.add_space(12.0);
                        ui.small("Ctrl+S  Save     Ctrl+F  Find     Ctrl+W  Close tab");
                    });
                }
            });
    }
}
