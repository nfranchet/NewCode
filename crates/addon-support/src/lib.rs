pub mod assistant_ui;
pub mod authentication;
pub mod runner;
pub mod runtime;
pub mod surface;
pub mod ui_theme;
#[cfg(test)]
pub fn test_frame(
    ctx: &egui::Context,
    input: egui::RawInput,
    render: impl FnMut(&mut egui::Ui),
) -> egui::FullOutput {
    let mut output = ctx.run_ui(input, render);
    output.textures_delta.clear();
    output
}
