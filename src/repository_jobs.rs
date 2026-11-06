//! Shared mutation lease for Source Control and executable addons.
use std::{
    collections::HashSet,
    path::{Path, PathBuf},
    sync::{LazyLock, Mutex},
};
static ACTIVE: LazyLock<Mutex<HashSet<PathBuf>>> = LazyLock::new(|| Mutex::new(HashSet::new()));
pub(crate) struct Lease(PathBuf);
pub(crate) fn acquire(workspace: &Path) -> Result<Lease, String> {
    let key = workspace.canonicalize().map_err(|e| e.to_string())?;
    let mut active = ACTIVE.lock().unwrap_or_else(|e| e.into_inner());
    if !active.insert(key.clone()) {
        return Err("Another Git operation is running. Retry when it finishes.".into());
    }
    Ok(Lease(key))
}
pub(crate) fn busy(workspace: &Path) -> bool {
    let key = workspace
        .canonicalize()
        .unwrap_or_else(|_| workspace.to_path_buf());
    ACTIVE
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .contains(&key)
}
impl Drop for Lease {
    fn drop(&mut self) {
        ACTIVE
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(&self.0);
    }
}
