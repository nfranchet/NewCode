//! Durable workspace identities, independent of running editors and server lifetimes.
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    io,
    path::{Path, PathBuf},
};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub(crate) struct Workspace {
    pub id: String,
    pub path: PathBuf,
}
fn identity(path: &Path) -> String {
    Sha256::digest(path.as_os_str().as_encoded_bytes())
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}
fn persist(workspace: &Workspace) -> io::Result<()> {
    let directory = crate::app_paths::data_root()?
        .join("workspaces")
        .join(&workspace.id);
    std::fs::create_dir_all(&directory)?;
    let path = directory.join("workspace.json");
    if std::fs::read(&path)
        .ok()
        .and_then(|bytes| serde_json::from_slice::<Workspace>(&bytes).ok())
        .as_ref()
        == Some(workspace)
    {
        return Ok(());
    }
    super::browser::registry::write_private(&path, workspace)
}
pub(crate) fn remember(path: &Path) -> io::Result<Workspace> {
    let path = path.canonicalize()?;
    if !path.is_dir() {
        return Err(io::Error::other("Workspace must be an existing directory"));
    }
    let workspace = Workspace {
        id: identity(&path),
        path,
    };
    persist(&workspace)?;
    Ok(workspace)
}
pub(crate) fn list() -> io::Result<Vec<Workspace>> {
    let directory = crate::app_paths::data_root()?.join("workspaces");
    let entries = match std::fs::read_dir(directory) {
        Ok(entries) => entries,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(vec![]),
        Err(error) => return Err(error),
    };
    let mut workspaces = Vec::new();
    for entry in entries.flatten() {
        let id = entry.file_name().to_string_lossy().into_owned();
        if id.len() != 64
            || !id.bytes().all(|byte| byte.is_ascii_hexdigit())
            || !entry.file_type().is_ok_and(|kind| kind.is_dir())
        {
            continue;
        }
        let path = entry.path().join("workspace.json");
        let workspace = if path.exists() {
            std::fs::read(path)
                .ok()
                .and_then(|bytes| serde_json::from_slice::<Workspace>(&bytes).ok())
        } else {
            #[derive(Deserialize)]
            struct Saved {
                workspace: Option<PathBuf>,
            }
            std::fs::read_to_string(entry.path().join("settings.toml"))
                .ok()
                .and_then(|text| toml::from_str::<Saved>(&text).ok())
                .and_then(|saved| saved.workspace)
                .map(|path| Workspace {
                    id: id.clone(),
                    path,
                })
        };
        if let Some(workspace) = workspace.filter(|workspace| {
            workspace.id == id && workspace.path.is_absolute() && identity(&workspace.path) == id
        }) {
            if let Err(error) = persist(&workspace) {
                log::warn!("Could not remember workspace: {error}");
            }
            workspaces.push(workspace);
        }
    }
    workspaces.sort_by(|a, b| {
        a.path
            .file_name()
            .cmp(&b.path.file_name())
            .then(a.path.cmp(&b.path))
    });
    Ok(workspaces)
}
