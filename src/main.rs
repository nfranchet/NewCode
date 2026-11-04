#![cfg_attr(not(target_os = "linux"), allow(unused))]

mod app_server;
mod codex_ui;
mod dev_container;
mod editor;
mod git_ui;
mod launch;
mod logging;
mod opencode_server;
mod opencode_ui;
mod settings;
mod syntax;
mod terminal;
mod terminal_input;
mod ui_theme;

use editor::EditorApp;
use eframe::egui;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let result = run();
    if result.is_err() {
        log::error!("Application terminated with an error");
    }
    result
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let arguments = logging::Arguments::parse(std::env::args_os().skip(1))?;
    logging::init(&arguments)?;
    if !cfg!(target_os = "linux") {
        log::error!("Focus Editor currently supports Linux only");
        return Ok(());
    }
    log::debug!("Launch arguments validated");
    if launch::detach()? {
        log::debug!("Background editor spawned; returning shell prompt");
        return Ok(());
    }
    log::info!("Application starting version={}", env!("CARGO_PKG_VERSION"));
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
    )?;
    log::info!("Application stopped");
    Ok(())
}
