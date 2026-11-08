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
                    .inner_margin(egui::Margin::symmetric(8, 0)),
            )
            .resizable(true)
            .default_size(left_width)
            .size_range(150.0..=500.0)
            .show(root, |ui| {
                if self.sidebar_mode == SidebarMode::Explorer {
                    ui.horizontal_wrapped(|ui| {
                        theme::icon(ui, Icon::Folder, theme::colors(ui).accent);
                        ui.label(
                            egui::RichText::new("EXPLORER")
                                .size(11.0)
                                .strong()
                                .color(theme::colors(ui).muted),
                        );
                        if theme::button(ui, Icon::NewFile, "", theme::colors(ui).accent, false)
                            .on_hover_text("New file")
                            .clicked()
                        {
                            self.create_entry(false);
                        }
                        if theme::button(ui, Icon::NewFolder, "", theme::colors(ui).accent, false)
                            .on_hover_text("New folder")
                            .clicked()
                        {
                            self.create_entry(true);
                        }
                    });
                } else if self.sidebar_mode == SidebarMode::Search {
                    theme::heading(ui, Icon::Search, "Search", theme::colors(ui).accent);
                }
                if self.sidebar_mode != SidebarMode::Git {
                    ui.separator();
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
        self.settings.explorer_width = left_response.response.rect.width().max(150.0);
    }
}
