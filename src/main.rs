use focus_addon_support::assistant_ui;
mod addons;
mod app_paths;
mod browser;
mod dev_container;
mod editor;
mod file_picker;
mod git_ui;
mod launch;
mod logging;
mod platform;
mod repository_jobs;
mod server;
mod settings;
mod syntax;
mod terminal;
mod terminal_fonts;
mod ui_theme;

#[cfg(test)]
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
    if arguments.server {
        return server::run(&arguments);
    }
    if !arguments.ui {
        browser::validate_runtime()?;
    }
    if launch::detach()? {
        return Ok(());
    }
    logging::init(&arguments)?;
    log::debug!("Launch arguments validated");
    log::info!("Application starting version={}", env!("CARGO_PKG_VERSION"));
    if !arguments.ui {
        browser::run(&arguments)?;
        log::info!("Application stopped");
        return Ok(());
    }
    browser::run_native(&arguments)?;
    log::info!("Application stopped");
    Ok(())
}

// Headless frames have no renderer to apply texture uploads to.
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
