mod app_server;
mod codex_ui;
mod dev_container;
mod editor;
mod git_ui;
mod launch;
mod logging;
mod opencode_server;
mod opencode_ui;
mod platform;
mod settings;
mod syntax;
mod terminal;
mod terminal_input;
mod ui_theme;

use editor::EditorApp;
use eframe::egui;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    platform::prepare_environment();
    let result = run();
    if result.is_err() {
        log::error!("Application terminated with an error");
    }
    result
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let arguments = logging::Arguments::parse(std::env::args_os().skip(1))?;
    if arguments.version {
        println!("focus-editor {}", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }
    if !cfg!(any(target_os = "linux", target_os = "macos")) {
        return Err("Focus Editor supports Linux and macOS".into());
    }
    if launch::detach()? {
        return Ok(());
    }
    logging::init(&arguments)?;
    log::debug!("Launch arguments validated");
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
