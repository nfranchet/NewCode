use crate::ui_theme::{self as theme, Icon};
use egui;

pub struct Choice {
    pub label: String,
    pub detail: String,
}
pub struct Selector {
    pub id: &'static str,
    pub title: &'static str,
    pub label: String,
    pub selected: Option<usize>,
    pub options: Vec<Choice>,
    pub enabled: bool,
}
#[derive(Default)]
pub struct Action {
    pub send: bool,
    pub interrupt: bool,
    pub cycle: Option<bool>,
    pub selected: [Option<usize>; 3],
}

pub fn show(
    ui: &mut egui::Ui,
    id: egui::Id,
    input: &mut String,
    input_height: f32,
    can_send: bool,
    busy: bool,
    can_interrupt: bool,
    selectors: [Selector; 3],
) -> Action {
    let mut action = Action::default();
    if ui.memory(|m| m.has_focus(id)) {
        action.cycle = ui.input_mut(|i| {
            if i.consume_key(egui::Modifiers::SHIFT, egui::Key::Tab) {
                Some(true)
            } else if i.consume_key(egui::Modifiers::NONE, egui::Key::Tab) {
                Some(false)
            } else {
                None
            }
        });
    }
    egui::Frame::new()
        .fill(theme::colors(ui).chrome)
        .stroke(egui::Stroke::new(1.0, theme::colors(ui).border_strong))
        .inner_margin(8)
        .show(ui, |ui| {
            let viewport = egui::ScrollArea::vertical()
                .id_salt(id.with("scroll"))
                .max_height(input_height)
                .min_scrolled_height(input_height)
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    ui.add(
                        egui::TextEdit::multiline(input)
                            .id(id)
                            .lock_focus(true)
                            .hint_text("Message assistant…")
                            .desired_width(f32::INFINITY)
                            .desired_rows(3),
                    )
                });
            ui.ctx()
                .data_mut(|data| data.insert_temp(id.with("viewport"), viewport.inner_rect));
            #[cfg(test)]
            ui.ctx()
                .data_mut(|data| data.insert_temp(id.with("scroll_id"), viewport.id));
            let can_send = can_send && !input.trim().is_empty();
            action.send = can_send
                && viewport.inner.has_focus()
                && ui.input_mut(|i| i.consume_key(egui::Modifiers::CTRL, egui::Key::Enter));
            ui.horizontal(|ui| {
                action.send |= ui
                    .add_enabled_ui(can_send, |ui| {
                        theme::button(ui, Icon::Send, "Send", theme::colors(ui).accent, true)
                    })
                    .inner
                    .on_hover_text("Ctrl+Enter to send")
                    .clicked();
                if busy {
                    action.interrupt = ui
                        .add_enabled_ui(can_interrupt, |ui| {
                            theme::button(
                                ui,
                                Icon::Stop,
                                "Interrupt",
                                theme::colors(ui).warning,
                                false,
                            )
                        })
                        .inner
                        .clicked();
                }
            });
            ui.horizontal(|ui| {
                let width =
                    ((ui.available_width() - 2.0 * ui.spacing().item_spacing.x) / 3.0).max(1.0);
                for (index, selector) in selectors.into_iter().enumerate() {
                    ui.allocate_ui_with_layout(
                        egui::vec2(width, 28.0),
                        egui::Layout::top_down(egui::Align::Min),
                        |ui| {
                            ui.add_enabled_ui(
                                selector.enabled && !selector.options.is_empty(),
                                |ui| {
                                    egui::ComboBox::from_id_salt(selector.id)
                                        .width(width)
                                        .truncate()
                                        .selected_text(&selector.label)
                                        .show_ui(ui, |ui| {
                                            for (choice, option) in
                                                selector.options.iter().enumerate()
                                            {
                                                if ui
                                                    .selectable_label(
                                                        selector.selected == Some(choice),
                                                        &option.label,
                                                    )
                                                    .on_hover_text(&option.detail)
                                                    .clicked()
                                                {
                                                    action.selected[index] = Some(choice);
                                                }
                                            }
                                        })
                                        .response
                                        .on_hover_text(format!(
                                            "{}: {}",
                                            selector.title, selector.label
                                        ));
                                },
                            );
                        },
                    );
                }
            });
        });
    if action.selected.iter().any(Option::is_some) || action.cycle.is_some() {
        log::debug!("Assistant model/thinking/mode selection changed");
    }
    action
}
