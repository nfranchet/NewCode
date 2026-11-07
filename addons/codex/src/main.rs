use egui;
use focus_addon_sdk::Context;
use focus_addon_support::{assistant_ui, runner::Panel, ui_theme};
use std::{
    collections::{BTreeMap, HashMap},
    path::PathBuf,
};
mod app_server;
mod authentication;
mod codex_ui;
mod dev_container;
mod logging;
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
    current: codex_ui::CodexUi,
    cache: HashMap<PathBuf, codex_ui::CodexUi>,
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
        let runtime = self.context.runtime.clone().map(Into::into);
        self.current.set_runtime(runtime.as_ref());
        if let Some(id) = self
            .context
            .workspace
            .as_ref()
            .and_then(|p| self.sessions.get(p))
            .cloned()
        {
            self.current.restore_session(id);
        }
        if self.context.workspace.is_some()
            && self.context.state["autostart"].as_bool() != Some(false)
        {
            self.current
                .start(self.context.workspace.as_deref(), ctx.clone());
        }
    }
    fn tick(&mut self, ctx: &egui::Context) {
        self.current.tick(self.context.workspace.as_deref(), ctx);
        if let (Some(root), Some(id)) = (&self.context.workspace, self.current.saved_session()) {
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
        self.current
            .show(ui, self.context.workspace.as_deref(), ctx);
    }
    fn state(&self) -> String {
        self.current.state().into()
    }
}
fn main() {
    if std::env::args().any(|arg| arg == "--version") {
        println!("focus-addon-codex {}", env!("CARGO_PKG_VERSION"));
        return;
    }
    if let Err(error) = focus_addon_support::runner::run(App::default()) {
        eprintln!("ERROR Addon stopped: {error}");
        std::process::exit(1)
    }
}
