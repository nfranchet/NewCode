//! Document lifecycle, one replaceable preview, and inactive unsaved buffers.
use super::EditorApp;
use std::{
    fs,
    path::{Path, PathBuf},
};

#[derive(Clone, Copy, PartialEq, Eq)]
enum OpenMode {
    Preview,
    Permanent,
    Activate,
}

impl EditorApp {
    pub(super) fn editor_id(&self) -> eframe::egui::Id {
        self.active_file
            .as_ref()
            .and_then(|path| self.document_ids.get(path))
            .copied()
            .unwrap_or_else(|| eframe::egui::Id::new("main_editor_text"))
    }

    pub(super) fn open_file(&mut self, path: PathBuf) -> bool {
        self.open_document(path, OpenMode::Permanent)
    }
    pub(super) fn preview_file(&mut self, path: PathBuf) -> bool {
        self.open_document(path, OpenMode::Preview)
    }
    pub(super) fn activate_file(&mut self, path: PathBuf) -> bool {
        self.open_document(path, OpenMode::Activate)
    }
    pub(super) fn keep_preview(&mut self, path: &Path) {
        if self.preview_file.as_deref() == Some(path) {
            self.preview_file = None;
        }
    }
    pub(super) fn mark_modified(&mut self) {
        self.dirty = true;
        if let Some(path) = self.active_file.clone() {
            self.keep_preview(&path);
        }
    }
    pub(super) fn file_is_dirty(&self, path: &Path) -> bool {
        (self.active_file.as_deref() == Some(path) && self.dirty)
            || self.unsaved_files.contains_key(path)
    }

    fn open_document(&mut self, path: PathBuf, mode: OpenMode) -> bool {
        if self.active_file.as_ref() == Some(&path) {
            if mode == OpenMode::Permanent || self.dirty {
                self.keep_preview(&path);
            }
            self.git_ui.dismiss_diff();
            self.reveal_active_tab = true;
            return true;
        }
        // Read before changing tabs or taking the outgoing buffer: failed opens
        // must leave the current file, preview slot, and unsaved edits intact.
        let dirty = self.unsaved_files.contains_key(&path);
        let content = match self
            .unsaved_files
            .get(&path)
            .cloned()
            .map(Ok)
            .unwrap_or_else(|| fs::read_to_string(&path))
        {
            Ok(content) => content,
            Err(error) => {
                self.status = format!("Could not open file: {error}");
                return false;
            }
        };
        if self.dirty {
            if let Some(outgoing) = self.active_file.clone() {
                self.keep_preview(&outgoing);
                self.unsaved_files
                    .insert(outgoing, std::mem::take(&mut self.buffer));
            }
        }
        let existing = self.open_files.contains(&path);
        if mode == OpenMode::Preview && !existing {
            let replacement = self.preview_file.take().and_then(|preview| {
                if self.file_is_dirty(&preview) {
                    return None;
                }
                self.document_ids.remove(&preview);
                self.open_files.iter().position(|open| open == &preview)
            });
            if let Some(index) = replacement {
                self.open_files[index] = path.clone();
            } else {
                self.open_files.push(path.clone());
            }
            self.preview_file = Some(path.clone());
        } else if !existing {
            self.open_files.push(path.clone());
        }
        if mode == OpenMode::Permanent || dirty {
            self.keep_preview(&path);
        }
        if !self.document_ids.contains_key(&path) {
            self.next_document_id += 1;
            self.document_ids.insert(
                path.clone(),
                eframe::egui::Id::new(("document_editor", self.next_document_id)),
            );
        }
        self.unsaved_files.remove(&path);
        self.active_file = Some(path);
        self.buffer = content;
        self.dirty = dirty;
        self.pending_editor_cursor = None;
        self.find_active = None;
        self.find_query_snapshot.clear();
        self.reveal_active_tab = true;
        self.git_ui.dismiss_diff();
        self.status = "File opened".into();
        true
    }

    pub(super) fn save_file(&mut self) {
        if let Some(path) = self.active_file.clone() {
            self.save_document(&path);
        }
    }
    pub(super) fn save_document(&mut self, path: &Path) -> bool {
        let active = self.active_file.as_deref() == Some(path);
        let content = if active {
            &self.buffer
        } else if let Some(content) = self.unsaved_files.get(path) {
            content
        } else {
            return true;
        };
        match fs::write(path, content) {
            Ok(()) => {
                if active {
                    self.dirty = false;
                }
                self.unsaved_files.remove(path);
                self.status = format!("Saved {}", path.display());
                true
            }
            Err(error) => {
                self.status = format!("Save failed: {error}");
                false
            }
        }
    }

    pub(super) fn close_file(&mut self, path: PathBuf) {
        if self.file_is_dirty(&path) {
            self.close_confirmation = Some(path);
        } else {
            self.finish_close_file(path);
        }
    }
    pub(super) fn finish_close_file(&mut self, path: PathBuf) {
        let Some(index) = self.open_files.iter().position(|open| *open == path) else {
            return;
        };
        let was_active = self.active_file.as_ref() == Some(&path);
        self.open_files.remove(index);
        self.document_ids.remove(&path);
        self.unsaved_files.remove(&path);
        self.keep_preview(&path);
        if self.close_confirmation.as_ref() == Some(&path) {
            self.close_confirmation = None;
        }
        if was_active {
            self.active_file = None;
            self.buffer.clear();
            self.dirty = false;
            self.pending_editor_cursor = None;
            self.find_open = false;
            self.find_active = None;
            if !self.open_files.is_empty() {
                let next = self.open_files[index.min(self.open_files.len() - 1)].clone();
                self.activate_file(next);
            } else {
                self.status = "File closed".into();
            }
        } else {
            self.status = "File closed".into();
        }
    }
}
