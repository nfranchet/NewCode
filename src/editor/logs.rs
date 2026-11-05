//! Bottom-panel log viewer; records remain process-local and independent of workspaces.
use crate::logging::session::{self, Source};
use eframe::egui;
#[derive(Default, Clone, Copy, PartialEq, Eq)]
pub(super) enum Tab {
    #[default]
    Terminal,
    Logs,
}
pub(super) struct Logs {
    pub source: Option<Source>,
    query: String,
    level: log::Level,
    follow: bool,
}
impl Default for Logs {
    fn default() -> Self {
        Self {
            source: None,
            query: String::new(),
            level: log::Level::Trace,
            follow: true,
        }
    }
}
impl Logs {
    fn matches(&self, entry: &session::Entry, query: &str) -> bool {
        self.source.is_none_or(|s| s == entry.source)
            && entry.level <= self.level
            && entry.text.to_lowercase().contains(query)
    }
    pub(super) fn ui(&mut self, ui: &mut egui::Ui) {
        ui.horizontal_wrapped(|ui| {
            egui::ComboBox::from_id_salt("log_source").selected_text(self.source.map_or("All",Source::label)).show_ui(ui,|ui| {
                ui.selectable_value(&mut self.source,None,"All");
                for source in Source::ALL {ui.selectable_value(&mut self.source,Some(source),source.label());}
            });
            egui::ComboBox::from_id_salt("log_level").selected_text(format!("{} and above",self.level)).show_ui(ui,|ui|{
                for level in [log::Level::Trace,log::Level::Debug,log::Level::Info,log::Level::Warn,log::Level::Error] {ui.selectable_value(&mut self.level,level,level.as_str());}
            }).response.on_hover_text("Filters captured records. Use --log-level debug at launch to capture Debug diagnostics.");
            ui.add(egui::TextEdit::singleline(&mut self.query).hint_text("Search logs").desired_width(140.0));
            ui.checkbox(&mut self.follow,"Follow latest");
            if ui.button("Clear").clicked(){ session::clear(self.source); }
        });
        let query = self.query.to_lowercase();
        let entries: Vec<_> = session::snapshot()
            .into_iter()
            .filter(|entry| self.matches(entry, &query))
            .collect();
        ui.horizontal_wrapped(|ui| {
            if ui.button("Copy visible logs").clicked() {
                ui.ctx().copy_text(
                    entries
                        .iter()
                        .map(|e| e.text.as_str())
                        .collect::<Vec<_>>()
                        .join("\n"),
                );
            }
            ui.small(format!("{} records · current session", entries.len()));
        });
        if entries.is_empty() {
            ui.label("No logs match the current filters.");
        }
        let row_height = ui.text_style_height(&egui::TextStyle::Monospace) + 4.0;
        let out = egui::ScrollArea::both()
            .id_salt(("logs", self.source))
            .stick_to_bottom(self.follow)
            .auto_shrink([false, false])
            .show_rows(ui, row_height, entries.len(), |ui, rows| {
                for row in rows {
                    let entry = &entries[row];
                    let color = match entry.level {
                        log::Level::Error => egui::Color32::from_rgb(190, 45, 55),
                        log::Level::Warn => egui::Color32::from_rgb(155, 100, 0),
                        _ => ui.visuals().text_color(),
                    };
                    ui.add(
                        egui::Label::new(
                            egui::RichText::new(format!(
                                "[{}] {}",
                                entry.source.label(),
                                entry.text
                            ))
                            .monospace()
                            .color(color),
                        )
                        .selectable(true)
                        .wrap_mode(egui::TextWrapMode::Extend),
                    )
                    .on_hover_text(&entry.timestamp);
                }
            });
        if out.inner_rect.contains(
            ui.input(|i| i.pointer.hover_pos())
                .unwrap_or(egui::Pos2::ZERO),
        ) && ui.input(|i| i.raw_scroll_delta.y > 0.0)
        {
            self.follow = false;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn source_severity_and_search_filters_combine() {
        let mut view = Logs::default();
        let entry = session::Entry {
            timestamp: "now".into(),
            level: log::Level::Warn,
            source: Source::DockerCompose,
            text: "Build failed".into(),
        };
        assert!(view.matches(&entry, "failed"));
        view.source = Some(Source::Git);
        assert!(!view.matches(&entry, ""));
        view.source = Some(Source::DockerCompose);
        view.level = log::Level::Error;
        assert!(!view.matches(&entry, ""));
        view.level = log::Level::Warn;
        assert!(view.matches(&entry, "build"));
        assert!(!view.matches(&entry, "absent"));
    }
}

#[cfg(test)]
mod layout_tests {
    use super::*;
    #[test]
    fn log_controls_fit_narrow_and_wide_panels() {
        for width in [320.0, 1000.0] {
            let ctx = egui::Context::default();
            let mut logs = Logs::default();
            let _ = ctx.run(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(width, 300.0),
                    )),
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| logs.ui(ui));
                    assert!(
                        ctx.used_rect().right() <= width + 1.0,
                        "controls overflow at {width}: {:?}",
                        ctx.used_rect()
                    );
                },
            );
        }
    }
}
