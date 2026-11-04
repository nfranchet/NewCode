//! Exact selected-file versions used to reject actions on stale review snapshots.

use super::repository::git_with;
use crate::dev_container::Runtime;
use std::{
    fs,
    io::ErrorKind,
    os::unix::{ffi::OsStrExt, fs::MetadataExt},
    path::Path,
    sync::Arc,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct FileVersion {
    head: Vec<u8>,
    index: Vec<u8>,
    working: Option<Arc<[u8]>>,
    mode: Option<u32>,
    kind: u8,
}

impl FileVersion {
    pub(super) fn read(root: &Path, path: &str, runtime: Option<&Runtime>) -> Result<Self, String> {
        let head =
            git_with(root, &["ls-tree", "-z", "HEAD", "--", path], runtime).unwrap_or_default();
        let index = git_with(root, &["ls-files", "--stage", "-z", "--", path], runtime)?;
        let file = root.join(path);
        let (working, mode, kind) = match fs::symlink_metadata(&file) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                let target = fs::read_link(&file).map_err(|e| e.to_string())?;
                (
                    Some(Arc::from(target.as_os_str().as_bytes())),
                    Some(metadata.mode()),
                    2,
                )
            }
            Ok(metadata) if metadata.is_file() => (
                Some(Arc::from(fs::read(&file).map_err(|e| e.to_string())?)),
                Some(metadata.mode()),
                1,
            ),
            Ok(metadata) => (None, Some(metadata.mode()), 3),
            Err(error) if error.kind() == ErrorKind::NotFound => (None, None, 0),
            Err(error) => return Err(error.to_string()),
        };
        Ok(Self {
            head,
            index,
            working,
            mode,
            kind,
        })
    }

    pub(super) fn is_symlink(&self) -> bool {
        self.kind == 2
    }

    pub(super) fn same_sources(&self, other: &Self) -> bool {
        self.head == other.head
            && self.working == other.working
            && self.mode == other.mode
            && self.kind == other.kind
    }
}
