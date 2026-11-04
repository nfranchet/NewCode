//! Keep open tabs, preview identity, and unsaved text attached during moves.
use super::EditorApp;
use std::path::Path;

impl EditorApp {
    pub(super) fn update_open_path(&mut self, from: &Path, to: &Path) {
        let remap = |path: &Path| match path.strip_prefix(from) {
            Ok(suffix) => to.join(suffix),
            Err(_) => path.to_path_buf(),
        };
        for path in &mut self.open_files {
            *path = remap(path);
        }
        for path in [
            &mut self.active_file,
            &mut self.preview_file,
            &mut self.close_confirmation,
        ]
        .into_iter()
        .flatten()
        {
            *path = remap(path);
        }
        self.document_ids = std::mem::take(&mut self.document_ids)
            .into_iter()
            .map(|(path, id)| (remap(&path), id))
            .collect();
        self.unsaved_files = std::mem::take(&mut self.unsaved_files)
            .into_iter()
            .map(|(path, text)| (remap(&path), text))
            .collect();
    }
}
