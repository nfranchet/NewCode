//! Host-filesystem dialogs rendered inside the shared UI for remote controllers.
use eframe::egui;
use std::path::{Path, PathBuf};

#[derive(Clone)]
pub(crate) enum Purpose {
    Workspace,
    Move(PathBuf),
    Addon,
}
#[derive(Clone)]
struct Picker {
    purpose: Purpose,
    directory: PathBuf,
    location: String,
    name: String,
    error: String,
    overwrite: bool,
    focus_location: bool,
}
const STATE: &str = "host_file_picker";
const RESULT: &str = "host_file_picker_result";

pub(crate) fn request(ctx: &egui::Context, purpose: Purpose, directory: Option<&Path>) {
    let remote = ctx.data(|data| {
        data.get_temp::<bool>(egui::Id::new("remote_controller"))
            .unwrap_or(false)
    });
    if !remote {
        let dialog = rfd::FileDialog::new().set_directory(directory.unwrap_or(Path::new(".")));
        let path = match &purpose {
            Purpose::Workspace => dialog.pick_folder(),
            Purpose::Move(source) => dialog
                .set_file_name(source.file_name().unwrap_or_default().to_string_lossy())
                .save_file(),
            Purpose::Addon => dialog
                .add_filter("Addon manifest or package", &["toml", "gz"])
                .pick_file(),
        };
        if let Some(path) = path {
            ctx.data_mut(|data| data.insert_temp(egui::Id::new(RESULT), (purpose, path)));
        }
        return;
    }
    let directory = directory
        .map(Path::to_path_buf)
        .or_else(dirs::home_dir)
        .unwrap_or_else(|| PathBuf::from("/"));
    let name = match &purpose {
        Purpose::Move(source) => source
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned(),
        _ => String::new(),
    };
    let picker = Picker {
        purpose,
        location: directory.display().to_string(),
        directory,
        name,
        error: String::new(),
        overwrite: false,
        focus_location: true,
    };
    ctx.data_mut(|data| data.insert_temp(egui::Id::new(STATE), picker));
}

pub(crate) fn take(ctx: &egui::Context) -> Option<(Purpose, PathBuf)> {
    ctx.data_mut(|data| {
        let result = data.get_temp(egui::Id::new(RESULT));
        data.remove::<(Purpose, PathBuf)>(egui::Id::new(RESULT));
        result
    })
}

fn destination(directory: &Path, name: &str) -> Result<PathBuf, &'static str> {
    let mut components = Path::new(name).components();
    if !matches!(components.next(), Some(std::path::Component::Normal(_)))
        || components.next().is_some()
    {
        return Err("Enter a file name without directory separators");
    }
    Ok(directory.join(name))
}

pub(crate) fn show(ctx: &egui::Context) {
    let Some(mut picker) = ctx.data_mut(|data| {
        let picker = data.get_temp::<Picker>(egui::Id::new(STATE));
        data.remove::<Picker>(egui::Id::new(STATE));
        picker
    }) else {
        return;
    };
    let mut closed = false;
    let mut selected = None;
    let submit = ctx.input(|input| input.modifiers.command && input.key_pressed(egui::Key::Enter));
    egui::Modal::new(egui::Id::new(STATE)).show(ctx, |ui| {
        ui.set_width(620.0_f32.min(ctx.viewport_rect().width() - 40.0).max(240.0));
        ui.heading(match picker.purpose {
            Purpose::Workspace => "Open workspace",
            Purpose::Move(_) => "Move to",
            Purpose::Addon => "Install local addon",
        });
        ui.label("Files on the editor host");
        ui.horizontal(|ui| {
            if ui.button("Up").clicked() {
                if let Some(parent) = picker.directory.parent() {
                    picker.location = parent.display().to_string();
                    picker.directory = parent.to_path_buf();
                    picker.overwrite = false;
                }
            }
            let response = ui.text_edit_singleline(&mut picker.location);
            if picker.focus_location {
                response.request_focus();
                picker.focus_location = false;
            }
            if ui.button("Go").clicked()
                || response.lost_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter))
            {
                let path = PathBuf::from(&picker.location);
                if path.is_dir() {
                    picker.directory = path;
                    picker.error.clear();
                    picker.overwrite = false;
                } else {
                    picker.error = "Cannot open that directory".into();
                }
            }
        });
        let mut entries = match std::fs::read_dir(&picker.directory) {
            Ok(entries) => entries
                .flatten()
                .take(2000)
                .map(|entry| entry.path())
                .filter(|path| path.is_dir() || !matches!(picker.purpose, Purpose::Workspace))
                .collect::<Vec<_>>(),
            Err(error) => {
                picker.error = error.to_string();
                vec![]
            }
        };
        entries.sort_by_key(|path| (!path.is_dir(), path.file_name().map(|name| name.to_owned())));
        egui::ScrollArea::vertical()
            .max_height(320.0)
            .show(ui, |ui| {
                for path in entries {
                    let directory = path.is_dir();
                    let name = path.file_name().unwrap_or_default().to_string_lossy();
                    let response = ui.selectable_label(
                        !directory && picker.name == name,
                        format!("{} {}", if directory { "▸" } else { " " }, name),
                    );
                    if response.clicked() {
                        picker.error.clear();
                        picker.overwrite = false;
                        if directory {
                            picker.location = path.display().to_string();
                            picker.directory = path;
                        } else {
                            picker.name = name.into_owned();
                        }
                    }
                }
            });
        if !matches!(picker.purpose, Purpose::Workspace) {
            ui.horizontal(|ui| {
                ui.label("File name");
                if ui.text_edit_singleline(&mut picker.name).changed() {
                    picker.overwrite = false;
                }
            });
        }
        if !picker.error.is_empty() {
            ui.colored_label(egui::Color32::from_rgb(180, 50, 50), &picker.error);
        }
        if picker.overwrite {
            ui.label("The destination already exists. Replace it?");
        }
        ui.horizontal(|ui| {
            let label = if picker.overwrite {
                "Replace"
            } else if matches!(picker.purpose, Purpose::Move(_)) {
                "Move"
            } else {
                "Open"
            };
            if ui.button(label).clicked() || submit {
                if picker.location != picker.directory.display().to_string() {
                    let directory = PathBuf::from(&picker.location);
                    if directory.is_dir() {
                        picker.directory = directory;
                    } else {
                        picker.error = "Cannot open that directory".into();
                        return;
                    }
                }
                let path = if matches!(picker.purpose, Purpose::Workspace) {
                    Ok(picker.directory.clone())
                } else {
                    destination(&picker.directory, &picker.name)
                };
                match path {
                    Err(error) => picker.error = error.into(),
                    Ok(path) if matches!(picker.purpose, Purpose::Addon) && !path.is_file() => {
                        picker.error = "Select an existing file".into()
                    }
                    Ok(path)
                        if matches!(picker.purpose, Purpose::Move(_))
                            && path.exists()
                            && !picker.overwrite =>
                    {
                        picker.overwrite = true
                    }
                    Ok(path) => selected = Some(path),
                }
            }
            if ui.button("Cancel").clicked() {
                closed = true;
            }
        });
    });
    ctx.data_mut(|data| {
        if let Some(path) = selected {
            data.insert_temp(egui::Id::new(RESULT), (picker.purpose, path));
        } else if !closed {
            data.insert_temp(egui::Id::new(STATE), picker);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn destination_is_a_single_file_name() {
        for name in ["", "..", ".", "/tmp/x", "../x", "a/b"] {
            assert!(destination(Path::new("/tmp"), name).is_err());
        }
        assert_eq!(
            destination(Path::new("/tmp"), "hello.txt").unwrap(),
            Path::new("/tmp/hello.txt")
        );
    }
}
