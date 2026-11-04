//! Codex status and session-selection controls.

use super::{CodexUi, catalog::session_label};
use crate::ui_theme::{self as theme, Icon};
use eframe::egui;
use serde_json::Value;
use std::path::Path;

impl CodexUi {
    pub(super) fn show_header(
        &mut self,
        ui: &mut egui::Ui,
        cwd: Option<&Path>,
        ctx: &egui::Context,
    ) {
        ui.horizontal(|ui| {
            theme::icon(ui, Icon::Spark, theme::TEAL);
            ui.label(egui::RichText::new("Codex").strong().color(theme::TEAL));
            let status = if !self.pending.is_empty() {
                "Waiting for you"
            } else if self.busy {
                "Running"
            } else if self.thread_id.is_some() {
                "Ready"
            } else {
                "Offline"
            };
            let (color, fill) = match status {
                "Running" => (theme::TEAL, theme::TEAL_SOFT),
                "Ready" => (theme::GREEN, egui::Color32::from_rgb(235, 247, 239)),
                "Waiting for you" => (theme::AMBER, egui::Color32::from_rgb(255, 246, 223)),
                _ => (theme::MUTED, theme::SURFACE),
            };
            theme::badge(ui, status, color, fill).on_hover_text(&self.status);
            if self.busy {
                ui.spinner();
            }
            egui::ScrollArea::horizontal()
                .id_salt("codex_header")
                .auto_shrink([false, true])
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        if self.server.is_some() {
                            if theme::button(ui, Icon::Stop, "", theme::MUTED, false)
                                .on_hover_text("Stop Codex")
                                .clicked()
                            {
                                self.stop();
                            }
                        } else if theme::button(ui, Icon::Spark, "Start", theme::TEAL, true)
                            .clicked()
                        {
                            self.start(cwd, ctx.clone());
                        }
                        ui.add_enabled_ui(
                            self.server.is_some()
                                && self.initializing_id.is_none()
                                && !self.busy
                                && self.pending.is_empty()
                                && !self.requests.values().any(|m| {
                                    matches!(
                                        m.as_str(),
                                        "thread/list" | "thread/start" | "thread/resume"
                                    )
                                }),
                            |ui| {
                                ui.horizontal(|ui| {
                                    if ui.small_button("+").on_hover_text("New session").clicked() {
                                        self.open_thread(cwd, None);
                                    }
                                    let selected = self.sessions.iter().find(|session| {
                                        session.get("id").and_then(Value::as_str)
                                            == self.thread_id.as_deref()
                                    });
                                    let label = selected
                                        .map(|session| session_label(session, 18, false))
                                        .unwrap_or_else(|| "Choose session…".into());
                                    let mut chosen = None;
                                    egui::ComboBox::from_id_salt("codex_session")
                                        .truncate()
                                        .selected_text(label)
                                        .width(95.0)
                                        .show_ui(ui, |ui| {
                                            ui.weak("Recent sessions in this workspace");
                                            if self.sessions.is_empty() {
                                                ui.label("No saved sessions");
                                            }
                                            for session in &self.sessions {
                                                if let Some(id) =
                                                    session.get("id").and_then(Value::as_str)
                                                {
                                                    if ui
                                                        .selectable_label(
                                                            self.thread_id.as_deref() == Some(id),
                                                            session_label(session, 100, true),
                                                        )
                                                        .on_hover_text(format!(
                                                            "{}\n{id}",
                                                            session
                                                                .get("name")
                                                                .and_then(Value::as_str)
                                                                .or_else(|| session
                                                                    .get("preview")
                                                                    .and_then(Value::as_str))
                                                                .unwrap_or("Untitled session")
                                                        ))
                                                        .clicked()
                                                        && self.thread_id.as_deref() != Some(id)
                                                    {
                                                        chosen = Some(id.to_string());
                                                    }
                                                }
                                            }
                                        });
                                    if let Some(id) = chosen {
                                        self.open_thread(cwd, Some(&id));
                                    }
                                    if ui
                                        .small_button("↻")
                                        .on_hover_text("Refresh sessions")
                                        .clicked()
                                    {
                                        self.list_sessions(cwd, None);
                                    }
                                    if let Some(cursor) = self.sessions_cursor.clone() {
                                        if ui
                                            .small_button("…")
                                            .on_hover_text("Load older sessions")
                                            .clicked()
                                        {
                                            self.list_sessions(cwd, Some(&cursor));
                                        }
                                    }
                                });
                            },
                        );
                    });
                });
        });
    }
}
