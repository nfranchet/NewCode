#![cfg_attr(not(target_os = "linux"), allow(unused))]

mod app_server;
mod codex_ui;
mod editor;
mod git_ui;
mod settings;
mod syntax;
mod terminal;

use editor::EditorApp;
use eframe::egui;

fn main() -> eframe::Result<()> {
    if !cfg!(target_os = "linux") {
        eprintln!("Focus Editor currently supports Linux only");
        return Ok(());
    }
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1440.0, 900.0])
            .with_min_inner_size([900.0, 600.0]),
        ..Default::default()
    };
    eframe::run_native(
        "Focus Editor",
        options,
        Box::new(|cc| Ok(Box::new(EditorApp::new(cc)))),
    )
}
