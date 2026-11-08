//! OpenCode lifecycle/session adapter for the common assistant header.
use super::{OpenCodeUi, short, string};
use crate::{
    assistant_ui::header::{self, Action, Header, Session},
    ui_theme as theme,
};
use egui;
impl OpenCodeUi {
    pub(super) fn header(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let sessions = self
            .sessions
            .iter()
            .map(|s| Session {
                id: string(s, "id").into(),
                label: short(string(s, "title"), 18),
                detail: format!("{}\n{}", string(s, "title"), string(s, "id")),
            })
            .collect::<Vec<_>>();
        let action = header::show(
            ui,
            Header {
                name: "OpenCode",
                state: self.state(),
                status: &self.status,
                connected: self.server.is_some(),
                enabled: self.online && !self.locked(),
                selected: self.session.as_deref(),
                sessions: &sessions,
                more: self.online,
            },
        );
        match action {
            Some(Action::Start) => {
                let cwd = self.workspace.clone();
                self.start(Some(&cwd), ctx.clone());
            }
            Some(Action::Stop) => self.stop(),
            Some(Action::New) => self.select_session(None),
            Some(Action::Select(id)) => self.select_session(Some(id)),
            Some(Action::Refresh) => {
                self.list_sessions(false);
                self.refresh();
            }
            Some(Action::More) => {
                self.session_limit += 50;
                self.list_sessions(false);
            }
            None => {}
        }
        if let Some(error) = &self.error {
            ui.colored_label(theme::colors(ui).error, error);
        }
        if self.online && self.models.is_empty() {
            ui.colored_label(
                theme::colors(ui).warning,
                "No connected models. Configure a provider in OpenCode, then restart.",
            );
        }
        if self.online && self.agents.is_empty() {
            ui.colored_label(
                theme::colors(ui).warning,
                "No primary agents available in OpenCode.",
            );
        }
    }
}
