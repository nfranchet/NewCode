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
