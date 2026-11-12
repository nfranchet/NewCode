//! One application data root for logs and installed addons.
use std::{io, path::PathBuf};
pub(crate) fn data_root() -> io::Result<PathBuf> {
    dirs::data_local_dir()
        .map(|p| p.join("focus-editor"))
        .ok_or_else(|| io::Error::other("Cannot locate application data directory"))
}
pub(crate) fn addons() -> io::Result<PathBuf> {
    Ok(data_root()?.join("addons"))
}

/// Stable identity shared by absolute, relative, and symlinked workspace paths.
pub(crate) fn workspace_path(path: &std::path::Path) -> io::Result<PathBuf> {
    path.canonicalize()
}

pub(crate) fn workspace_dir(path: &std::path::Path) -> io::Result<PathBuf> {
    use sha2::{Digest, Sha256};
    let canonical = workspace_path(path)?;
    let id: String = Sha256::digest(canonical.as_os_str().as_encoded_bytes())
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    Ok(data_root()?.join("workspaces").join(id))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn workspace_identity_is_canonical_and_beside_shared_assets() {
        let root = tempfile::tempdir().unwrap();
        let a = root.path().join("a");
        let b = root.path().join("b");
        std::fs::create_dir_all(a.join("nested")).unwrap();
        std::fs::create_dir_all(&b).unwrap();
        let dir = workspace_dir(&a).unwrap();
        assert_eq!(
            dir.parent().unwrap(),
            data_root().unwrap().join("workspaces")
        );
        assert_ne!(dir, workspace_dir(&b).unwrap());
        assert_eq!(dir, workspace_dir(&a.join("nested/..")).unwrap());
        #[cfg(unix)]
        {
            let alias = root.path().join("alias");
            std::os::unix::fs::symlink(&a, &alias).unwrap();
            assert_eq!(dir, workspace_dir(&alias).unwrap());
        }
    }
}
