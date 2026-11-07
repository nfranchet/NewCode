//! Reusable event loop for independently compiled addons contributing a UI surface.
use crate::{surface, ui_theme};
use egui;
use focus_addon_sdk::{Context, Message, PROTOCOL_VERSION, read_message, write_message};
use std::{io::BufReader, sync::mpsc, time::Duration};

pub trait Panel {
    fn context(&mut self, context: Context, ctx: &egui::Context);
    fn tick(&mut self, ctx: &egui::Context);
    fn show(&mut self, ui: &mut egui::Ui, ctx: &egui::Context);
    fn state(&self) -> String;
}
pub fn run(mut panel: impl Panel) -> Result<(), String> {
    let level = std::env::var("FOCUS_ADDON_LOG_LEVEL").unwrap_or_else(|_| "info".into());
    let _ = env_logger::Builder::new()
        .parse_filters(&format!("warn,focus_addon={level}"))
        .format(|buf, record| {
            use std::io::Write;
            writeln!(buf, "{} {}", record.level(), record.args())
        })
        .try_init();
    let ctx = egui::Context::default();
    ui_theme::apply(&ctx);
    let (tx, rx) = mpsc::sync_channel(128);
    std::thread::spawn(move || {
        let mut reader = BufReader::new(std::io::stdin());
        while let Ok(Some(message)) = read_message(&mut reader) {
            if tx.send(message).is_err() {
                return;
            }
        }
    });
    let mut output = std::io::stdout();
    let mut generation = 0;
    let mut last_state = String::new();
    loop {
        let message = match rx.recv_timeout(Duration::from_millis(50)) {
            Ok(m) => m,
            Err(mpsc::RecvTimeoutError::Timeout) => {
                panel.tick(&ctx);
                report(&panel, &mut last_state, generation, &mut output)?;
                continue;
            }
            Err(_) => break,
        };
        match message.method.as_deref() {
            Some("initialize") => {
                let params = message.params.unwrap_or_default();
                if params["protocol"].as_u64() != Some(PROTOCOL_VERSION as u64) {
                    return Err("Unsupported addon protocol".into());
                }
                write_message(&mut output,&Message::response(message.id.unwrap_or(0),serde_json::json!({"protocol":PROTOCOL_VERSION,"surface":surface::SURFACE_VERSION}))).map_err(|e|e.to_string())?;
                let context: Context =
                    serde_json::from_value(params["context"].clone()).map_err(|e| e.to_string())?;
                generation = context.generation;
                panel.context(context, &ctx);
            }
            Some("context.changed") => {
                last_state.clear();
                let context: Context = serde_json::from_value(message.params.unwrap_or_default())
                    .map_err(|e| e.to_string())?;
                generation = context.generation;
                ctx.memory_mut(|m| *m = Default::default());
                panel.context(context, &ctx);
            }
            Some("surface.input") => {
                let input: surface::Input =
                    serde_json::from_value(message.params.unwrap_or_default())
                        .map_err(|e| e.to_string())?;
                if input.generation != generation {
                    continue;
                }
                panel.tick(&ctx);
                let frame = surface::render(&ctx, input, panel.state(), |ui| panel.show(ui, &ctx));
                write_message(&mut output, &Message::notification("surface.frame", frame))
                    .map_err(|e| e.to_string())?;
            }
            Some("shutdown") => break,
            _ => panel.tick(&ctx),
        }
        report(&panel, &mut last_state, generation, &mut output)?;
    }
    Ok(())
}

fn report(
    panel: &impl Panel,
    last: &mut String,
    generation: u64,
    output: &mut impl std::io::Write,
) -> Result<(), String> {
    let state = panel.state();
    if &state != last {
        write_message(
            output,
            &Message::notification(
                "panel.status",
                serde_json::json!({"generation":generation,"state":state}),
            ),
        )
        .map_err(|e| e.to_string())?;
        *last = state;
    }
    Ok(())
}
