//! Docker Compose-backed Dev Container discovery, lifecycle, and command setup.

use eframe::egui;
use serde::Deserialize;
mod compose;
mod diagnostics;
mod persistence;
use compose::compose;
use std::{
    path::{Path, PathBuf},
    sync::mpsc::{self, Receiver},
    thread,
};

pub(crate) use focus_addon_support::runtime::Runtime;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct DevContainerFile {
    name: Option<String>,
    docker_compose_file: ComposeFile,
    service: String,
    workspace_folder: String,
    remote_user: Option<String>,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum ComposeFile {
    One(String),
    Many(Vec<String>),
}

#[derive(Clone)]
struct Configuration {
    name: String,
    workspace: PathBuf,
    compose_files: Vec<PathBuf>,
    service: String,
    workspace_folder: PathBuf,
    remote_user: Option<String>,
}

enum State {
    Unavailable,
    Available(Configuration),
    Starting(Configuration),
    Ready(Configuration, Runtime),
    Failed(Configuration),
}

enum Event {
    Started(Configuration, Result<Runtime, String>, &'static str),
    Stopped(Configuration, Result<(), String>),
}

pub(crate) struct DevContainer {
    workspace: Option<PathBuf>,
    state: State,
    events: Option<Receiver<Event>>,
}

impl Default for DevContainer {
    fn default() -> Self {
        Self {
            workspace: None,
            state: State::Unavailable,
            events: None,
        }
    }
}

impl DevContainer {
    pub(crate) fn sync_workspace(&mut self, workspace: Option<&Path>) {
        let workspace =
            workspace.map(|path| path.canonicalize().unwrap_or_else(|_| path.to_path_buf()));
        if self.workspace == workspace {
            return;
        }
        log::debug!("Dev Container workspace configuration changed");
        self.workspace = workspace.clone();
        self.events = None;
        self.state = workspace
            .as_deref()
            .map(load_configuration)
            .transpose()
            .map(|configuration| configuration.flatten())
            .map_or_else(
                |error| {
                    diagnostics::failure("configuration", &error);
                    State::Failed(fallback_configuration(&workspace))
                },
                |configuration| configuration.map_or(State::Unavailable, State::Available),
            );
    }

    pub(crate) fn runtime(&self) -> Option<&Runtime> {
        match &self.state {
            State::Ready(_, runtime) => Some(runtime),
            _ => None,
        }
    }

    pub(crate) fn tick(&mut self) -> bool {
        let Some(event) = self
            .events
            .as_ref()
            .and_then(|events| events.try_recv().ok())
        else {
            return false;
        };
        self.events = None;
        match event {
            Event::Started(configuration, Ok(runtime), operation) => {
                log::info!("Dev Container {operation} completed; connected");
                self.state = State::Ready(configuration, runtime);
                true
            }
            Event::Started(configuration, Err(error), operation) => {
                diagnostics::failure(operation, &error);
                self.state = State::Failed(configuration);
                true
            }
            Event::Stopped(configuration, Ok(())) => {
                log::info!("Dev Container stopped");
                self.state = State::Available(configuration);
                true
            }
            Event::Stopped(configuration, Err(error)) => {
                diagnostics::failure("stop", &error);
                self.state = State::Failed(configuration);
                true
            }
        }
    }

    pub(crate) fn menu(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        if ui.button("Show logs").clicked() {
            ctx.data_mut(|data| data.insert_temp(egui::Id::new("show_dev_container_logs"), true));
            ui.close();
        }
        let mut action = None;
        match &self.state {
            State::Unavailable => {
                ui.add_enabled(false, egui::Button::new("No Dev Container configuration"));
            }
            State::Available(configuration) => {
                if ui
                    .button(format!("Open in {}", configuration.name))
                    .clicked()
                {
                    action = Some((configuration.clone(), false, false));
                }
            }
            State::Starting(configuration) => {
                ui.add_enabled(
                    false,
                    egui::Button::new(format!("Starting {}…", configuration.name)),
                );
            }
            State::Ready(configuration, _) => {
                ui.label(format!("Connected to {}", configuration.name));
                if ui.button("Reconnect").clicked() {
                    action = Some((configuration.clone(), false, false));
                }
                if ui.button("Rebuild").clicked() {
                    action = Some((configuration.clone(), true, false));
                }
                if ui.button("Stop").clicked() {
                    action = Some((configuration.clone(), false, true));
                }
            }
            State::Failed(configuration) => {
                ui.add(
                    egui::Label::new(
                        egui::RichText::new("Dev Container failed — see application log")
                            .color(egui::Color32::from_rgb(180, 49, 64)),
                    )
                    .wrap(),
                );
                if ui.button(format!("Retry {}", configuration.name)).clicked() {
                    action = Some((configuration.clone(), false, false));
                }
            }
        }
        if let Some((configuration, rebuild, stop)) = action {
            if stop {
                self.stop(configuration, ctx.clone());
            } else {
                self.start(configuration, rebuild, ctx.clone());
            }
            ui.close();
        }
    }

    pub(crate) fn label(&self) -> Option<&str> {
        match &self.state {
            State::Ready(configuration, _) => Some(&configuration.name),
            _ => None,
        }
    }

    fn start(&mut self, configuration: Configuration, rebuild: bool, ctx: egui::Context) {
        let operation = if rebuild {
            "rebuild"
        } else if matches!(&self.state, State::Ready(..)) {
            "reconnect"
        } else {
            "start"
        };
        log::info!("Dev Container {operation} requested");
        let (tx, events) = mpsc::channel();
        self.events = Some(events);
        self.state = State::Starting(configuration.clone());
        thread::spawn(move || {
            let result = start(&configuration, rebuild);
            let _ = tx.send(Event::Started(configuration, result, operation));
            ctx.request_repaint();
        });
    }

    fn stop(&mut self, configuration: Configuration, ctx: egui::Context) {
        log::info!("Dev Container stop requested");
        let (tx, events) = mpsc::channel();
        self.events = Some(events);
        self.state = State::Starting(configuration.clone());
        thread::spawn(move || {
            let result =
                compose(&configuration, ["stop", configuration.service.as_str()]).map(|_| ());
            let _ = tx.send(Event::Stopped(configuration, result));
            ctx.request_repaint();
        });
    }
}

fn fallback_configuration(workspace: &Option<PathBuf>) -> Configuration {
    Configuration {
        name: "Dev Container".into(),
        workspace: workspace.clone().unwrap_or_default(),
        compose_files: Vec::new(),
        service: String::new(),
        workspace_folder: PathBuf::from("/workspaces"),
        remote_user: None,
    }
}

fn load_configuration(workspace: &Path) -> Result<Option<Configuration>, String> {
    let candidates = [
        workspace.join(".devcontainer/devcontainer.json"),
        workspace.join(".devcontainer.json"),
    ];
    let Some(path) = candidates.iter().find(|path| path.is_file()) else {
        return Ok(None);
    };
    let contents = std::fs::read_to_string(path)
        .map_err(|error| format!("Could not read {}: {error}", path.display()))?;
    let file: DevContainerFile = serde_json::from_str(&strip_json_comments(&contents))
        .map_err(|error| format!("Invalid {}: {error}", path.display()))?;
    let config_dir = path.parent().unwrap_or(workspace);
    let compose_names = match file.docker_compose_file {
        ComposeFile::One(file) => vec![file],
        ComposeFile::Many(files) if !files.is_empty() => files,
        ComposeFile::Many(_) => return Err("dockerComposeFile must not be empty".into()),
    };
    let compose_files: Vec<_> = compose_names
        .into_iter()
        .map(|file| config_dir.join(file))
        .collect();
    if let Some(missing) = compose_files.iter().find(|path| !path.is_file()) {
        return Err(format!("Compose file not found: {}", missing.display()));
    }
    Ok(Some(Configuration {
        name: file.name.unwrap_or_else(|| "Dev Container".into()),
        workspace: workspace.to_path_buf(),
        compose_files,
        service: file.service,
        workspace_folder: PathBuf::from(file.workspace_folder),
        remote_user: file.remote_user,
    }))
}

fn start(configuration: &Configuration, rebuild: bool) -> Result<Runtime, String> {
    let mut args = vec!["up", "-d"];
    if rebuild {
        args.extend(["--build", "--force-recreate"]);
    } else {
        args.push("--build");
    }
    args.push(&configuration.service);
    persistence::prepare(configuration)?;
    compose::start(configuration, args)?;
    let container_id = compose(configuration, ["ps", "-q", configuration.service.as_str()])?;
    let container_id = String::from_utf8_lossy(&container_id).trim().to_owned();
    if container_id.is_empty() {
        return Err(format!(
            "Compose service {} did not start",
            configuration.service
        ));
    }
    let runtime = Runtime {
        host_workspace: configuration.workspace.clone(),
        container_id,
        workspace_folder: configuration.workspace_folder.clone(),
        remote_user: configuration.remote_user.clone(),
    };
    persistence::initialize(&runtime)?;
    Ok(runtime)
}

// devcontainer.json permits comments; serde_json deliberately does not.
fn strip_json_comments(input: &str) -> String {
    let mut output = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();
    let mut quoted = false;
    let mut escaped = false;
    while let Some(ch) = chars.next() {
        if quoted {
            output.push(ch);
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == '"' {
                quoted = false;
            }
        } else if ch == '"' {
            quoted = true;
            output.push(ch);
        } else if ch == '/' && chars.peek() == Some(&'/') {
            while chars.next().is_some_and(|next| next != '\n') {}
            output.push('\n');
        } else {
            output.push(ch);
        }
    }
    output
}

#[cfg(test)]
mod parsing_tests {
    use super::strip_json_comments;

    #[test]
    fn strips_comments_without_changing_urls() {
        assert_eq!(
            strip_json_comments("{ // note\n \"url\": \"https://example.test\" }"),
            "{ \n \"url\": \"https://example.test\" }"
        );
    }
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod path_tests;
