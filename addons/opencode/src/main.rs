use egui;
use focus_addon_sdk::Context;
use focus_addon_support::{assistant_ui, runner::Panel, ui_theme};
use std::{
    collections::{BTreeMap, HashMap},
    path::PathBuf,
};
mod authentication;
mod dev_container;
mod logging;
mod opencode_server;
mod opencode_ui;
#[cfg(test)]
fn test_frame(
    ctx: &egui::Context,
    input: egui::RawInput,
    render: impl FnMut(&mut egui::Ui),
) -> egui::FullOutput {
    let mut output = ctx.run_ui(input, render);
    output.textures_delta.clear();
    output
}
#[derive(Default)]
struct App {
    current: opencode_ui::OpenCodeUi,
    cache: HashMap<PathBuf, opencode_ui::OpenCodeUi>,
    context: Context,
    sessions: BTreeMap<PathBuf, String>,
}
impl Panel for App {
    fn context(&mut self, context: Context, ctx: &egui::Context) {
        self.tick(ctx);
        self.current.stop();
        if let Some(old) = self.context.workspace.take() {
            self.cache.insert(old, std::mem::take(&mut self.current));
        }
        self.context = context;
        if let Some(root) = &self.context.workspace {
            self.current = self.cache.remove(root).unwrap_or_default();
        }
        self.sessions = std::fs::read(self.context.storage.join("sessions.json"))
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .or_else(|| serde_json::from_value(self.context.state["sessions"].clone()).ok())
            .unwrap_or_default();
        if let Some(root) = &self.context.workspace {
            self.current.set_directory(root);
        }
        let runtime = self.context.runtime.clone().map(Into::into);
        self.current.set_runtime(runtime.as_ref());
        if let Some(id) = self
            .context
            .workspace
            .as_ref()
            .and_then(|p| self.sessions.get(p))
            .cloned()
        {
            self.current.remember_session(id);
        }
        if self.context.workspace.is_some()
            && self.context.state["autostart"].as_bool() != Some(false)
        {
            self.current
                .start(self.context.workspace.as_deref(), ctx.clone());
        }
    }
    fn tick(&mut self, ctx: &egui::Context) {
        self.current.tick(ctx);
        if let (Some(root), Some(id)) = (
            &self.context.workspace,
            self.current.session_id().map(str::to_owned),
        ) {
            if self.sessions.get(root) != Some(&id) {
                self.sessions.insert(root.clone(), id);
                if std::fs::create_dir_all(&self.context.storage).is_ok() {
                    if let Ok(bytes) = serde_json::to_vec(&self.sessions) {
                        let temporary = self.context.storage.join("sessions.tmp");
                        if std::fs::write(&temporary, bytes).is_ok() {
                            let _ = std::fs::rename(
                                temporary,
                                self.context.storage.join("sessions.json"),
                            );
                        }
                    }
                }
            }
        }
    }
    fn show(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        if self.context.workspace.is_none() {
            ui.heading("Open a workspace");
            ui.label("Choose a workspace before starting this addon.");
            return;
        }
        self.current.show(ui, ctx);
    }
    fn state(&self) -> String {
        self.current.state().into()
    }
}
fn main() {
    if let Some(home) = std::env::var_os("HOME") {
        let mut paths: Vec<_> =
            std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default()).collect();
        paths.push(PathBuf::from(home).join(".opencode/bin"));
        if let Ok(path) = std::env::join_paths(paths) {
            /* Before any addon workers exist. */
            unsafe {
                std::env::set_var("PATH", path);
            }
        }
    }
    if std::env::args().any(|arg| arg == "--version") {
        println!("focus-addon-opencode {}", env!("CARGO_PKG_VERSION"));
        return;
    }
    if let Err(error) = focus_addon_support::runner::run(App::default()) {
        eprintln!("ERROR Addon stopped: {error}");
        std::process::exit(1)
    }
}

#[cfg(test)]
mod workspace_tests {
    use super::*;
    #[test]
    fn workspace_changes_preserve_drafts_and_hidden_events_update_status() {
        let root = std::env::temp_dir().join(format!(
            "focus-opencode-addon-workspace-{}",
            std::process::id()
        ));
        let ctx = egui::Context::default();
        let mut app = App::default();
        let context = |name: &str| Context {
            workspace: Some(root.join(name)),
            storage: root.join("data"),
            state: serde_json::json!({"autostart":false}),
            ..Default::default()
        };
        app.context(context("first"), &ctx);
        let (ui, _requests, events) = opencode_ui::OpenCodeUi::test_connection();
        app.current = ui;
        events.send(opencode_server::Event::Data(serde_json::json!({"type":"permission.asked","properties":{"id":"permission","sessionID":"session1","permission":"edit"}}))).unwrap();
        app.tick(&ctx);
        assert_eq!(app.state(), "Waiting for you");
        app.context(context("second"), &ctx);
        assert_eq!(app.current.test_draft(), "");
        app.context(context("first"), &ctx);
        assert_eq!(app.current.test_draft(), "OpenCode draft");
        let _ = std::fs::remove_dir_all(root);
    }
}
