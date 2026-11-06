use super::*;

impl Addons {
    pub fn credential_dialog(&mut self, ctx: &egui::Context) {
        let Some(dialog) = &mut self.credentials else {
            return;
        };
        if let Some(receiver) = &dialog.pending {
            if let Ok(result) = receiver.try_recv() {
                dialog.result = result
                    .map(|_| {
                        "Credential updated in the OS keyring. Refresh the addon to reconnect."
                            .into()
                    })
                    .unwrap_or_else(|e| e);
                dialog.pending = None;
            }
            ctx.request_repaint_after(Duration::from_millis(100));
        }
        let mut close = false;
        let Some(addon) = self.entries.iter().find(|a| a.manifest.id == dialog.addon) else {
            return;
        };
        egui::Window::new(format!("{} credentials", addon.manifest.name)).collapsible(false).show(ctx, |ui| {
            ui.label("Reuse an existing CLI login, or store a personal access token in the OS keyring.");
            ui.label(&addon.manifest.credential.as_ref().unwrap().instructions);
            ui.label(&addon.manifest.credential.as_ref().unwrap().origin);
            ui.add(egui::TextEdit::singleline(&mut dialog.secret).password(true).hint_text("Personal access token"));
            ui.label(&dialog.result);
            ui.add_enabled_ui(dialog.pending.is_none(), |ui| {
                ui.horizontal(|ui| {
                    let save = ui.add_enabled(!dialog.secret.trim().is_empty(), egui::Button::new("Save in OS keyring")).clicked();
                    let delete = ui.button("Forget saved token").clicked();
                    if save || delete {
                        let secret = std::mem::take(&mut dialog.secret); let manifest = addon.manifest.clone(); let ctx = ctx.clone();
                        let (tx, rx) = mpsc::channel(); dialog.pending = Some(rx);
                        std::thread::spawn(move || {
                            let result = services::entry(&manifest).and_then(|entry| {
                                (if save { entry.set_password(secret.trim()) } else { entry.delete_credential() })
                                    .map_err(|_| "The OS keyring could not update the credential. Unlock it and retry; no plaintext file was written.".into())
                            });
                            let _ = tx.send(result); ctx.request_repaint();
                        });
                    }
                    close = ui.button("Close").clicked();
                });
            });
        });
        if close {
            self.credentials = None;
        }
    }
}
