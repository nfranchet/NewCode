use super::{EditorApp, test_app};
use std::{fs, path::PathBuf};
mod closing;
mod lifecycle;
mod ui;

struct Project(PathBuf);
impl Project {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "focus-preview-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir(&root).unwrap();
        for name in ["a.txt", "b.txt", "c.txt", "d.txt"] {
            fs::write(root.join(name), name).unwrap();
        }
        Self(root)
    }
    fn file(&self, name: &str) -> PathBuf {
        self.0.join(name)
    }
    fn app(&self) -> EditorApp {
        let mut app = test_app();
        app.open_files.clear();
        app.active_file = None;
        app.buffer.clear();
        app.dirty = false;
        app.settings.workspace = Some(self.0.clone());
        app.settings_path = self.file("settings.toml");
        app
    }
}
impl Drop for Project {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
