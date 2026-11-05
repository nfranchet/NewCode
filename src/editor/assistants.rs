//! Independent assistant lifecycles, workspace states, and a shared optional pane.
use super::EditorApp;
use crate::{settings::Assistant, ui_theme as theme};
use eframe::egui;
use std::path::Path;
impl EditorApp {
    pub(super) fn set_assistant_enabled(
        &mut self,
        assistant: Assistant,
        enabled: bool,
        ctx: &egui::Context,
    ) {
        log::info!("set_assistant_enabled requested");
        let cwd = self.settings.workspace.clone();
        match assistant {
            Assistant::Codex => {
                self.settings.codex_enabled = enabled;
                if enabled {
                    self.skip_codex_start = false;
                    self.codex_ui.start(cwd.as_deref(), ctx.clone());
                } else {
                    self.codex_ui.stop();
                }
            }
            Assistant::OpenCode => {
                self.settings.opencode_enabled = enabled;
                if enabled {
                    self.skip_opencode_start = false;
                    self.opencode_ui.start(cwd.as_deref(), ctx.clone());
                } else {
                    self.opencode_ui.stop();
                }
            }
        }
        if enabled {
            self.settings.selected_assistant = assistant;
        }
        self.normalize_assistant();
        self.save_settings();
    }
    fn normalize_assistant(&mut self) {
        if self.settings.selected_assistant == Assistant::Codex && !self.settings.codex_enabled {
            self.settings.selected_assistant = Assistant::OpenCode;
        }
        if self.settings.selected_assistant == Assistant::OpenCode
            && !self.settings.opencode_enabled
        {
            self.settings.selected_assistant = Assistant::Codex;
        }
    }
    pub(super) fn assistants_menu(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        ui.menu_button("Assistants", |ui| {
            let mut codex = self.settings.codex_enabled;
            let mut opencode = self.settings.opencode_enabled;
            if ui.checkbox(&mut codex, "Codex").changed() {
                self.set_assistant_enabled(Assistant::Codex, codex, ctx);
            }
            if ui.checkbox(&mut opencode, "OpenCode").changed() {
                self.set_assistant_enabled(Assistant::OpenCode, opencode, ctx);
            }
        });
    }
    pub(super) fn update_assistants(&mut self, ctx: &egui::Context) {
        let root = self
            .settings
            .workspace
            .as_deref()
            .unwrap_or(Path::new("."))
            .canonicalize()
            .unwrap_or_else(|_| {
                self.settings
                    .workspace
                    .clone()
                    .unwrap_or_else(|| ".".into())
            });
        if self
            .assistant_workspace
            .as_ref()
            .is_some_and(|old| old != &root)
        {
            self.codex_ui.stop();
            self.opencode_ui.stop();
            if let Some(old) = self.assistant_workspace.take() {
                self.assistant_workspaces.insert(
                    old,
                    (
                        std::mem::take(&mut self.codex_ui),
                        std::mem::take(&mut self.opencode_ui),
                    ),
                );
            }
            if let Some((codex, opencode)) = self.assistant_workspaces.remove(&root) {
                self.codex_ui = codex;
                self.opencode_ui = opencode;
            }
            if self.settings.codex_enabled
                && self.settings.codex_app_server
                && !self.skip_codex_start
            {
                self.codex_ui.start(Some(&root), ctx.clone());
            }
            if let Some(id) = self.settings.opencode_sessions.get(&root) {
                self.opencode_ui.remember_session(id.clone());
            }
            if self.settings.opencode_enabled && !self.skip_opencode_start {
                self.opencode_ui.start(Some(&root), ctx.clone());
            }
        }
        self.assistant_workspace = Some(root.clone());
        let runtime_changed = self.codex_ui.set_runtime(self.dev_container.runtime());
        if runtime_changed
            && self.settings.codex_enabled
            && self.settings.codex_app_server
            && !self.skip_codex_start
        {
            self.codex_ui.start(Some(&root), ctx.clone());
        }
        let opencode_runtime_changed = self.opencode_ui.set_runtime(self.dev_container.runtime());
        if opencode_runtime_changed && self.settings.opencode_enabled && !self.skip_opencode_start {
            self.opencode_ui.start(Some(&root), ctx.clone());
        }
        self.normalize_assistant();
        self.opencode_ui.set_directory(&root);
        if self.settings.codex_enabled {
            self.codex_ui.tick(Some(&root), ctx);
        }
        if self.settings.opencode_enabled {
            self.opencode_ui.tick(ctx);
            if let Some(id) = self.opencode_ui.session_id().map(str::to_owned) {
                if self.settings.opencode_sessions.get(&root) != Some(&id) {
                    self.settings.opencode_sessions.insert(root, id);
                    self.save_settings();
                }
            }
        }
    }
    pub(super) fn assistant_panel(&mut self, root: &mut egui::Ui, cwd: Option<&Path>) {
        let context = root.ctx().clone();
        let ctx = &context;
        if !self.settings.codex_enabled && !self.settings.opencode_enabled {
            return;
        }
        let response = egui::Panel::right("codex_terminal")
            .resizable(true)
            .default_size(self.settings.right_width)
            .size_range(260.0..=700.0)
            .frame(
                egui::Frame::new()
                    .fill(egui::Color32::from_rgb(242, 250, 248))
                    .stroke(egui::Stroke::new(1.0_f32, theme::BORDER))
                    .inner_margin(8),
            )
            .show(root, |ui| {
                if self.settings.codex_enabled && self.settings.opencode_enabled {
                    ui.horizontal_wrapped(|ui| {
                        for (assistant, name, state) in [
                            (Assistant::Codex, "Codex", self.codex_ui.state()),
                            (Assistant::OpenCode, "OpenCode", self.opencode_ui.state()),
                        ] {
                            let label = format!(
                                "{name} · {}",
                                if state == "Waiting for you" {
                                    "Needs you"
                                } else {
                                    state
                                }
                            );
                            if ui
                                .selectable_label(
                                    self.settings.selected_assistant == assistant,
                                    label,
                                )
                                .clicked()
                            {
                                self.settings.selected_assistant = assistant;
                                self.save_settings();
                            }
                        }
                    });
                    ui.separator();
                }
                match self.settings.selected_assistant {
                    Assistant::Codex => {
                        ui.push_id(
                            ("assistant_codex", self.assistant_workspace.as_ref()),
                            |ui| self.codex_ui.show(ui, cwd, ctx),
                        );
                    }
                    Assistant::OpenCode => {
                        ui.push_id(
                            ("assistant_opencode", self.assistant_workspace.as_ref()),
                            |ui| self.opencode_ui.show(ui, ctx),
                        );
                    }
                }
            });
        self.settings.right_width = response.response.rect.width().max(260.0);
    }
}
