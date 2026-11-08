//! Sidebar shell and routing between Explorer, Search, and Source Control.

use super::{EditorApp, SidebarMode};
use crate::ui_theme::{self as theme, Icon};
use eframe::egui;

impl EditorApp {
    pub(super) fn sidebar(&mut self, root: &mut egui::Ui) {
        let left_width = self.settings.explorer_width;
        let left_response = egui::Panel::left("explorer")
            .frame(
                egui::Frame::new()
                    .fill(theme::colors(root).panel)
                    .inner_margin(0),
            )
            .resizable(true)
            .default_size(left_width)
            .size_range(150.0..=500.0)
            .show(root, |ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
                if matches!(
                    self.sidebar_mode,
                    SidebarMode::Explorer | SidebarMode::Search
                ) {
                    let colors = theme::colors(ui);
                    let height = theme::metrics(ui).panel_header_height;
                    let (rect, _) = ui.allocate_exact_size(
                        egui::vec2(ui.available_width(), height),
                        egui::Sense::hover(),
                    );
                    ui.painter().rect_filled(rect, 0.0, colors.chrome);
                    ui.painter().text(
                        rect.left_center() + egui::vec2(13.0, 0.0),
                        egui::Align2::LEFT_CENTER,
                        if self.sidebar_mode == SidebarMode::Explorer {
                            "EXPLORER"
                        } else {
                            "SEARCH"
                        },
                        egui::FontId::proportional(10.0),
                        colors.muted,
                    );
                    if self.sidebar_mode == SidebarMode::Explorer {
                        let actions = egui::Rect::from_min_max(
                            egui::pos2(rect.right() - 58.0, rect.top()),
                            rect.max - egui::vec2(8.0, 0.0),
                        );
                        ui.scope_builder(
                            egui::UiBuilder::new()
                                .max_rect(actions)
                                .layout(egui::Layout::right_to_left(egui::Align::Center)),
                            |ui| {
                                if theme::button(ui, Icon::NewFolder, "", colors.accent, false)
                                    .on_hover_text("New folder")
                                    .clicked()
                                {
                                    self.create_entry(true);
                                }
                                if theme::button(ui, Icon::NewFile, "", colors.accent, false)
                                    .on_hover_text("New file")
                                    .clicked()
                                {
                                    self.create_entry(false);
                                }
                            },
                        );
                    }
                }
                egui::Frame::new()
                    .inner_margin(if self.sidebar_mode == SidebarMode::Explorer {
                        egui::Margin::ZERO
                    } else {
                        egui::Margin::symmetric(8, 4)
                    })
                    .show(ui, |ui| {
                        if self.sidebar_mode != SidebarMode::Explorer {
                            ui.spacing_mut().item_spacing.y = 4.0;
                        }
                        match self.sidebar_mode {
                            SidebarMode::AddonManager => self
                                .addons
                                .manager_sidebar(ui, &mut self.settings.addon_enabled),
                            SidebarMode::Addon => self.addons.sidebar(ui),
                            SidebarMode::Git => {
                                if let Some(path) = self.git_ui.ui(
                                    ui,
                                    self.settings.workspace.as_deref(),
                                    self.dev_container.runtime(),
                                ) {
                                    self.open_file(path);
                                }
                            }
                            SidebarMode::Explorer => {
                                self.explorer_panel(ui);
                            }
                            SidebarMode::Search => {
                                self.search_panel(ui);
                            }
                        }
                    });
            });
        self.settings.explorer_width = left_response.response.rect.width().max(150.0);
    }
}
