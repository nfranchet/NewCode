//! Codex lifecycle/session adapter for the common assistant header.
use super::{CodexUi, catalog::session_label};
use crate::assistant_ui::header::{self, Action, Header, Session};
use egui;
use std::path::Path;
impl CodexUi {
    pub(super) fn show_header(
        &mut self,
        ui: &mut egui::Ui,
        cwd: Option<&Path>,
        ctx: &egui::Context,
    ) {
        let sessions = self
            .sessions
            .iter()
            .filter_map(|s| {
                Some(Session {
                    id: s["id"].as_str()?.into(),
                    label: session_label(s, 18, false),
                    detail: session_label(s, 100, true),
                })
            })
            .collect::<Vec<_>>();
        let enabled = self.server.is_some()
            && self.initializing_id.is_none()
            && !self.busy
            && self.pending.is_empty()
            && !self
                .requests
                .values()
                .any(|m| matches!(m.as_str(), "thread/list" | "thread/start" | "thread/resume"));
        let action = header::show(
            ui,
            Header {
                name: "Codex",
                state: self.state(),
                status: &self.status,
                connected: self.server.is_some(),
                enabled,
                selected: self.thread_id.as_deref(),
                sessions: &sessions,
                more: self.sessions_cursor.is_some(),
            },
        );
        match action {
            Some(Action::Start) => self.start(cwd, ctx.clone()),
            Some(Action::Stop) => self.stop(),
            Some(Action::New) => self.open_thread(cwd, None),
            Some(Action::Select(id)) => self.open_thread(cwd, Some(&id)),
            Some(Action::Refresh) => self.list_sessions(cwd, None),
            Some(Action::More) => self.list_sessions(cwd, self.sessions_cursor.clone().as_deref()),
            None => {}
        }
    }
}
