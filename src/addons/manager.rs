//! Built-in addon management, with background catalog/download/install work.
use super::*;
use super::{
    catalog,
    packages::{self, Prepared, Receipt},
};
use crate::ui_theme as theme;
use focus_addon_sdk::{Catalog, Package};

pub(super) enum Event {
    Progress(String),
    Catalog(Catalog),
    Prepared(Prepared),
    Installed(Manifest, Receipt),
    Removed(String),
    Error(String),
}
pub(super) struct Manager {
    pub requested: bool,
    pub detail_visible: bool,
    installed: bool,
    query: String,
    selected: Option<String>,
    release: String,
    catalog: Option<Catalog>,
    receipts: BTreeMap<String, Receipt>,
    pending: Option<mpsc::Receiver<Event>>,
    progress: String,
    error: Option<String>,
    uninstall: Option<String>,
    restart: Option<String>,
}
impl Default for Manager {
    fn default() -> Self {
        Self {
            requested: false,
            detail_visible: false,
            installed: true,
            query: String::new(),
            selected: None,
            release: option_env!("FOCUS_RELEASE_TAG").unwrap_or("latest").into(),
            catalog: None,
            receipts: BTreeMap::new(),
            pending: None,
            progress: String::new(),
            error: None,
            uninstall: None,
            restart: None,
        }
    }
}
impl Addons {
    pub fn manager_busy(&self) -> bool {
        self.manager.pending.is_some()
    }
}
enum Action {
    Fetch,
    Local,
    Rescan,
    Install(Package, String),
    Enable(String, bool),
    Restart(String),
    Credentials(String),
    Remove(String),
}

#[cfg(test)]
mod tests {
    use super::*;
    fn package() -> Package {
        Package {
            id: "fixture".into(),
            name: "Fixture".into(),
            description: String::new(),
            publisher: String::new(),
            version: "1.0.0".into(),
            protocol: 1,
            target: packages::target().unwrap().into(),
            archive: "fixture.tar.gz".into(),
            size: 1,
            sha256: "0".repeat(64),
        }
    }
    #[test]
    fn incompatible_packages_cannot_be_selected_for_installation() {
        let mut p = package();
        let mut catalog = Catalog {
            schema: 1,
            release: "v1".into(),
            editor_version: env!("CARGO_PKG_VERSION").into(),
            packages: vec![p.clone()],
        };
        assert!(compatibility(&catalog, &p).is_none());
        p.protocol = 99;
        assert!(compatibility(&catalog, &p).is_some());
        p.protocol = 1;
        p.target = "other-platform".into();
        assert!(compatibility(&catalog, &p).is_some());
        p = package();
        catalog.editor_version = "99.0.0".into();
        assert!(compatibility(&catalog, &p).is_some());
    }
    #[test]
    fn disconnected_worker_releases_manager_controls() {
        let mut host = Addons::default();
        let (sender, receiver) = mpsc::channel();
        host.manager.pending = Some(receiver);
        drop(sender);
        host.manager_tick(&egui::Context::default(), &BTreeMap::new());
        assert!(host.manager.pending.is_none());
        assert!(host.manager.error.as_deref().unwrap().contains("stopped"));
    }
    #[test]
    fn manager_empty_and_filtered_lists_fit_narrow_panels() {
        fn text(shape: &egui::Shape, expected: &str) -> bool {
            match shape {
                egui::Shape::Text(t) => t.galley.job.text == expected,
                egui::Shape::Vec(shapes) => shapes.iter().any(|s| text(s, expected)),
                _ => false,
            }
        }
        let mut host = Addons::default();
        host.manager.catalog = Some(Catalog {
            schema: 1,
            release: "v1".into(),
            editor_version: env!("CARGO_PKG_VERSION").into(),
            packages: vec![package()],
        });
        host.manager.installed = false;
        host.manager.detail_visible = true;
        host.manager.selected = Some("fixture".into());
        let ctx = egui::Context::default();
        let mut disabled = BTreeMap::new();
        for (width, installed, query, expected) in [
            (150.0, false, "fixture", "Fixture\n1.0.0"),
            (
                260.0,
                false,
                "absent",
                "No available addons match this search.",
            ),
            (500.0, true, "", "No installed addons match this search."),
        ] {
            host.manager.installed = installed;
            host.manager.query = query.into();
            let output = crate::test_frame(
                &ctx,
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(900.0, 700.0),
                    )),
                    ..Default::default()
                },
                |ui| {
                    egui::Panel::left("manager-test")
                        .exact_size(width)
                        .show(ui, |ui| {
                            let available = ui.available_width();
                            host.manager_sidebar(ui, &mut disabled);
                            assert!(ui.min_rect().width() <= available + 1.0);
                        });
                    egui::CentralPanel::default().show(ui, |ui| {
                        assert!(host.manager_central(ui, &mut disabled));
                    });
                },
            );
            assert!(output.shapes.iter().any(|s| text(&s.shape, expected)));
        }
    }
}
fn compatibility(catalog: &Catalog, package: &Package) -> Option<String> {
    if catalog.editor_version != env!("CARGO_PKG_VERSION") {
        Some(format!(
            "Requires editor {}; this editor is {}",
            catalog.editor_version,
            env!("CARGO_PKG_VERSION")
        ))
    } else if package.protocol != focus_addon_sdk::PROTOCOL_VERSION {
        Some(format!("Requires addon protocol {}", package.protocol))
    } else if Some(package.target.as_str()) != packages::target() {
        Some(format!("Not built for this platform ({})", package.target))
    } else {
        None
    }
}
fn available(catalog: &Catalog) -> Vec<Package> {
    let mut map = BTreeMap::new();
    for p in &catalog.packages {
        if !map.contains_key(&p.id) || Some(p.target.as_str()) == packages::target() {
            map.insert(p.id.clone(), p.clone());
        }
    }
    map.into_values().collect()
}
fn state(addon: &Addon) -> &'static str {
    if addon.error.is_some() {
        "Error"
    } else if !addon.enabled {
        "Disabled"
    } else if addon.working {
        "Working"
    } else if addon.process.as_ref().is_some_and(|p| p.ready) {
        "Running"
    } else if addon.process.is_some() {
        "Starting"
    } else {
        "Stopped"
    }
}

impl Addons {
    pub fn dismiss_manager_detail(&mut self) {
        self.manager.detail_visible = false;
    }

    pub fn open_manager(&mut self) {
        self.manager.requested = true;
        self.manager.detail_visible = true;
        if let Ok(root) = crate::app_paths::data_root() {
            self.manager.receipts = self
                .entries
                .iter()
                .filter_map(|a| {
                    packages::receipt(&root, &a.manifest.id).map(|r| (a.manifest.id.clone(), r))
                })
                .collect();
        }
    }
    pub fn take_manager_request(&mut self) -> bool {
        std::mem::take(&mut self.manager.requested)
    }
    fn worker(
        &mut self,
        ctx: &egui::Context,
        label: &str,
        job: impl FnOnce(mpsc::Sender<Event>) -> Result<Event, String> + Send + 'static,
    ) {
        let (tx, rx) = mpsc::channel();
        self.manager.pending = Some(rx);
        self.manager.progress = label.into();
        self.manager.error = None;
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let result = job(tx.clone()).unwrap_or_else(Event::Error);
            let _ = tx.send(result);
            ctx.request_repaint();
        });
    }
    fn manager_action(
        &mut self,
        action: Action,
        ctx: &egui::Context,
        enabled_settings: &mut BTreeMap<String, bool>,
    ) {
        if self.manager.pending.is_some() {
            return;
        }
        match action {
            Action::Rescan => {
                self.discover(enabled_settings);
                self.open_manager();
                self.manager.requested = false;
            }
            Action::Fetch => {
                let tag = self.manager.release.clone();
                self.worker(ctx, "Loading release catalog…", move |_| {
                    catalog::fetch(&tag).map(Event::Catalog)
                });
            }
            Action::Local => {
                if let Some(source) = rfd::FileDialog::new()
                    .add_filter("Addon manifest or package", &["toml", "gz"])
                    .pick_file()
                {
                    let root = match crate::app_paths::data_root() {
                        Ok(p) => p,
                        Err(e) => {
                            self.manager.error = Some(e.to_string());
                            return;
                        }
                    };
                    self.worker(ctx, "Validating local addon…", move |_| {
                        if source.extension().is_some_and(|e| e == "toml") {
                            packages::prepare_local(&root, &source)
                        } else {
                            packages::prepare_archive(&root, &source, None)
                        }
                        .map(Event::Prepared)
                    });
                }
            }
            Action::Install(package, release) => {
                let root = match crate::app_paths::data_root() {
                    Ok(p) => p,
                    Err(e) => {
                        self.manager.error = Some(e.to_string());
                        return;
                    }
                };
                self.worker(ctx, "Downloading addon…", move |tx| {
                    let download = tempfile::NamedTempFile::new().map_err(|e| e.to_string())?;
                    catalog::download(&release, &package, download.path(), |text| {
                        let _ = tx.send(Event::Progress(text));
                    })?;
                    let _ = tx.send(Event::Progress("Checking archive and checksum…".into()));
                    packages::prepare_archive(&root, download.path(), Some((&package, &release)))
                        .map(Event::Prepared)
                });
            }
            Action::Enable(id, enabled) => {
                enabled_settings.insert(id.clone(), enabled);
                if let Some(addon) = self.entries.iter_mut().find(|a| a.manifest.id == id) {
                    addon.enabled = enabled;
                    if !enabled {
                        addon.process = None;
                        addon.working = false;
                    }
                }
                if !enabled && self.active.as_ref() == Some(&id) {
                    self.active = None;
                    self.detail = false;
                }
            }
            Action::Restart(id) => {
                if let Some(addon) = self.entries.iter_mut().find(|a| a.manifest.id == id) {
                    if let Some(process) = addon.process.take() {
                        process.stop_now();
                    }
                    addon.error = None;
                    addon.working = false;
                }
                self.activate(id, ctx);
            }
            Action::Credentials(id) => {
                self.credentials = Some(Credentials {
                    addon: id,
                    secret: String::new(),
                    result: String::new(),
                    pending: None,
                })
            }
            Action::Remove(id) => self.manager.uninstall = Some(id),
        }
    }
    fn stop_managed(&mut self, id: &str) -> Result<(), String> {
        if let Some(addon) = self.entries.iter_mut().find(|a| a.manifest.id == id) {
            if addon.working {
                return Err("Wait for this addon's current operation to finish before changing its installation".into());
            }
            if let Some(process) = addon.process.take() {
                if addon.enabled {
                    self.manager.restart = Some(id.into());
                }
                process.stop_now();
            }
        }
        Ok(())
    }
    pub fn manager_tick(&mut self, ctx: &egui::Context, enabled_settings: &BTreeMap<String, bool>) {
        let mut events = Vec::new();
        if let Some(receiver) = &self.manager.pending {
            loop {
                match receiver.try_recv() {
                    Ok(event) => events.push(event),
                    Err(mpsc::TryRecvError::Empty) => break,
                    Err(mpsc::TryRecvError::Disconnected) => {
                        if events
                            .iter()
                            .all(|event| matches!(event, Event::Progress(_)))
                        {
                            events.push(Event::Error(
                                "The addon task stopped before completing. Retry the operation."
                                    .into(),
                            ));
                        }
                        break;
                    }
                }
            }
        }
        for event in events {
            match event {
                Event::Progress(text) => self.manager.progress = text,
                Event::Catalog(catalog) => {
                    self.manager.catalog = Some(catalog);
                    self.manager.pending = None;
                    self.manager.progress = "Catalog ready".into();
                }
                Event::Prepared(prepared) => {
                    let id = prepared.manifest.id.clone();
                    if let Err(error) = self.stop_managed(&id) {
                        self.manager.pending = None;
                        self.manager.error = Some(error);
                        continue;
                    }
                    let root = match crate::app_paths::data_root() {
                        Ok(root) => root,
                        Err(e) => {
                            self.manager.pending = None;
                            self.manager.error = Some(e.to_string());
                            continue;
                        }
                    };
                    self.worker(ctx, "Installing addon…", move |_| {
                        packages::commit(&root, prepared).map(|(m, r)| Event::Installed(m, r))
                    });
                }
                Event::Installed(manifest, receipt) => {
                    self.manager.pending = None;
                    let id = manifest.id.clone();
                    if let Ok(root) = crate::app_paths::data_root() {
                        let storage = match self.addon_storage(&id) {
                            Ok(storage) => storage,
                            Err(error) => {
                                self.manager.error = Some(error);
                                continue;
                            }
                        };
                        let seed = self
                            .entries
                            .iter()
                            .find(|a| a.manifest.id == id)
                            .map(|a| a.seed.clone())
                            .unwrap_or_default();
                        self.entries.retain(|a| a.manifest.id != id);
                        self.entries.push(Addon {
                            directory: root.join("addons").join(&id),
                            storage,
                            enabled: !self.suppressed.contains(&id)
                                && enabled_settings
                                    .get(&id)
                                    .copied()
                                    .unwrap_or(manifest.enabled_by_default),
                            manifest,
                            error: None,
                            process: None,
                            view: View::default(),
                            values: BTreeMap::new(),
                            icon: None,
                            working: false,
                            incarnation: 0,
                            surface: Default::default(),
                            seed,
                            status: String::new(),
                        });
                        self.entries
                            .sort_by(|a, b| a.manifest.id.cmp(&b.manifest.id));
                    }
                    self.manager.receipts.insert(id.clone(), receipt);
                    self.manager.selected = Some(id);
                    self.manager.detail_visible = true;
                    self.manager.installed = true;
                    self.manager.progress = "Addon installed".into();
                    if let Some(id) = self.manager.restart.take() {
                        self.activate(id, ctx);
                    }
                }
                Event::Removed(id) => {
                    self.manager.pending = None;
                    self.manager.restart = None;
                    self.entries.retain(|a| a.manifest.id != id);
                    self.manager.receipts.remove(&id);
                    if self.active.as_ref() == Some(&id) {
                        self.active = None;
                        self.detail = false;
                    }
                    self.manager.selected = None;
                    self.manager.progress = "Addon uninstalled".into();
                }
                Event::Error(error) => {
                    self.manager.pending = None;
                    self.manager.error = Some(error);
                    if let Some(id) = self.manager.restart.take() {
                        self.activate(id, ctx);
                    }
                }
            }
        }
        if self.manager.pending.is_some() {
            ctx.request_repaint_after(Duration::from_millis(100));
        }
    }
    pub fn manager_sidebar(
        &mut self,
        ui: &mut egui::Ui,
        enabled_settings: &mut BTreeMap<String, bool>,
    ) {
        ui.heading("Addons");
        let mut action = None;
        ui.horizontal_wrapped(|ui| {
            ui.selectable_value(&mut self.manager.installed, true, "Installed");
            ui.selectable_value(&mut self.manager.installed, false, "Available");
        });
        ui.add(
            egui::TextEdit::singleline(&mut self.manager.query)
                .hint_text("Search addons")
                .desired_width(f32::INFINITY),
        );
        ui.add_enabled_ui(self.manager.pending.is_none(), |ui| {
            if ui.button("Rescan installed").clicked() {
                action = Some(Action::Rescan);
            }
            if ui.button("Install local addon…").clicked() {
                action = Some(Action::Local);
            }
            ui.label("Release tag");
            ui.add(
                egui::TextEdit::singleline(&mut self.manager.release).desired_width(f32::INFINITY),
            );
            if ui.button("Refresh / check updates").clicked() {
                action = Some(Action::Fetch);
            }
        });
        if self.manager.pending.is_some() {
            ui.spinner();
        }
        if !self.manager.progress.is_empty() {
            ui.label(&self.manager.progress);
        }
        if let Some(error) = &self.manager.error {
            ui.colored_label(theme::colors(ui).error, error);
        }
        if let Some(notice) = &self.notice {
            ui.colored_label(theme::colors(ui).error, notice);
        }
        ui.separator();
        let query = self.manager.query.to_lowercase();
        let rows: Vec<(String, String, String)> = if self.manager.installed {
            self.entries
                .iter()
                .filter(|a| {
                    format!(
                        "{} {} {}",
                        a.manifest.id, a.manifest.name, a.manifest.description
                    )
                    .to_lowercase()
                    .contains(&query)
                })
                .map(|a| {
                    (
                        a.manifest.id.clone(),
                        a.manifest.name.clone(),
                        format!("{} · {}", a.manifest.version, state(a)),
                    )
                })
                .collect()
        } else {
            self.manager
                .catalog
                .as_ref()
                .map(|c| {
                    available(c)
                        .into_iter()
                        .filter(|p| {
                            format!("{} {} {}", p.id, p.name, p.description)
                                .to_lowercase()
                                .contains(&query)
                        })
                        .map(|p| (p.id, p.name, p.version))
                        .collect()
                })
                .unwrap_or_default()
        };
        egui::ScrollArea::vertical()
            .id_salt("addon_manager_list")
            .show(ui, |ui| {
                if rows.is_empty() {
                    ui.label(if self.manager.installed {
                        "No installed addons match this search."
                    } else {
                        if self.manager.catalog.is_some() {
                            "No available addons match this search."
                        } else {
                            "Refresh a release catalog to browse available addons."
                        }
                    });
                }
                for (id, name, detail) in rows {
                    if ui
                        .selectable_label(
                            self.manager.selected.as_ref() == Some(&id),
                            format!("{name}\n{detail}"),
                        )
                        .clicked()
                    {
                        self.manager.selected = Some(id);
                        self.manager.detail_visible = true;
                    }
                }
            });
        if let Some(action) = action {
            self.manager_action(action, ui.ctx(), enabled_settings);
        }
    }
    pub fn manager_central(
        &mut self,
        ui: &mut egui::Ui,
        enabled_settings: &mut BTreeMap<String, bool>,
    ) -> bool {
        if !self.manager.detail_visible {
            return false;
        }
        let mut action = None;
        egui::ScrollArea::vertical()
            .id_salt("addon_manager_details")
            .show(ui, |ui| {
                let Some(id) = self.manager.selected.clone() else {
                    ui.heading("Manage addons");
                    ui.label("Select an installed addon, or browse the packages published with an editor release.");
                    ui.label("Install updates when you choose. Updating restarts only the affected addon.");
                    return;
                };
                let installed = self.entries.iter().find(|a| a.manifest.id == id);
                let remote = self.manager.catalog.as_ref()
                    .and_then(|c| available(c).into_iter().find(|p| p.id == id));
                ui.heading(installed.map(|a| a.manifest.name.as_str())
                    .or_else(|| remote.as_ref().map(|p| p.name.as_str())).unwrap_or(&id));
                ui.label(format!("ID: {id}"));
                if let Some(addon) = installed {
                    ui.label(&addon.manifest.description);
                    ui.label(format!("Installed version: {} · {}", addon.manifest.version, state(addon)));
                    if !addon.manifest.publisher.is_empty() {
                        ui.label(format!("Publisher: {}", addon.manifest.publisher));
                    }
                    if let Some(receipt) = self.manager.receipts.get(&id) {
                        ui.label(format!("Source: {} · release {}", receipt.source,
                            receipt.release.as_deref().unwrap_or("local")));
                    } else {
                        ui.label("Source: local installation");
                    }
                    if let Some(error) = &addon.error {
                        ui.colored_label(theme::colors(ui).error, error);
                    }
                    ui.add_enabled_ui(self.manager.pending.is_none(), |ui| {
                        ui.horizontal_wrapped(|ui| {
                            let mut enabled = addon.enabled;
                            if ui.checkbox(&mut enabled, "Enabled").changed() {
                                action = Some(Action::Enable(id.clone(), enabled));
                            }
                            if ui.add_enabled(enabled, egui::Button::new("Restart")).clicked() {
                                action = Some(Action::Restart(id.clone()));
                            }
                            if addon.manifest.credential.is_some() && ui.button("Credentials…").clicked() {
                                action = Some(Action::Credentials(id.clone()));
                            }
                            if ui.add_enabled(!addon.working, egui::Button::new("Uninstall…")).clicked() {
                                action = Some(Action::Remove(id.clone()));
                            }
                        });
                    });
                }
                if let (Some(catalog), Some(package)) = (&self.manager.catalog, remote) {
                    ui.separator();
                    ui.label(format!("Available: {} · release {} · {}", package.version,
                        catalog.release, package.target));
                    ui.label(&package.description);
                    if !package.publisher.is_empty() {
                        ui.label(format!("Publisher: {}", package.publisher));
                    }
                    let reason = compatibility(catalog, &package);
                    if let Some(reason) = &reason {
                        ui.colored_label(theme::colors(ui).warning, reason);
                    }
                    let can_install = reason.is_none() && self.manager.pending.is_none()
                        && !installed.is_some_and(|a| a.working);
                    let label = if installed.is_some() { "Update / install selected release" } else { "Install" };
                    if ui.add_enabled(can_install, egui::Button::new(label)).clicked() {
                        action = Some(Action::Install(package, catalog.release.clone()));
                    }
                }
                if let Ok(root) = crate::app_paths::data_root() {
                    ui.separator();
                    ui.label(format!("Installation: {}", root.join("addons").join(&id).display()));
                    if let Ok(storage) = self.addon_storage(&id) {
                        ui.label(format!("Data: {}", storage.display()));
                    }
                }
            });
        if let Some(action) = action {
            self.manager_action(action, ui.ctx(), enabled_settings);
        }
        true
    }
    pub fn manager_dialog(&mut self, ctx: &egui::Context) {
        let Some(id) = &self.manager.uninstall else {
            return;
        };
        let mut confirm = false;
        let mut cancel = false;
        egui::Window::new("Uninstall addon")
            .collapsible(false)
            .resizable(false)
            .show(ctx, |ui| {
                ui.label(format!("Uninstall {id}? Its process will stop."));
                ui.label("Workspace addon data is preserved.");
                ui.label("Credentials remain in the OS keyring. Use Credentials → Forget before uninstalling to remove a stored token.");
                ui.horizontal(|ui| {
                    confirm = ui.button("Uninstall").clicked();
                    cancel = ui.button("Cancel").clicked();
                });
            });
        if cancel {
            self.manager.uninstall = None;
        }
        if confirm {
            let id = self.manager.uninstall.take().unwrap();
            if let Err(error) = self.stop_managed(&id) {
                self.manager.error = Some(error);
                return;
            }
            match crate::app_paths::data_root() {
                Ok(root) => self.worker(ctx, "Uninstalling addon…", move |_| {
                    packages::uninstall(&root, &id)?;
                    Ok(Event::Removed(id))
                }),
                Err(e) => self.manager.error = Some(e.to_string()),
            }
        }
    }
}
