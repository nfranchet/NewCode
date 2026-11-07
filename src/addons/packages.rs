//! Validated packages and transactional installation shared by every manager entry point.
use super::installation::load_manifest;
use focus_addon_sdk::{Manifest, Package};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::HashSet,
    fs::{self, File, OpenOptions},
    io::Write,
    path::{Component, Path, PathBuf},
};

pub(super) const MAX_ARCHIVE: u64 = 256 * 1024 * 1024;
const MAX_EXPANDED: u64 = 512 * 1024 * 1024;
#[derive(Clone, Debug, Serialize, Deserialize)]
pub(super) struct Receipt {
    pub source: String,
    pub release: Option<String>,
    pub package: Option<Package>,
}
pub(super) struct Prepared {
    pub manifest: Manifest,
    pub receipt: Receipt,
    stage: tempfile::TempDir,
}
pub(super) struct Lease(File);
#[derive(Serialize, Deserialize)]
struct Transaction {
    id: String,
    stage: String,
    had_previous: bool,
    had_receipt: bool,
}
fn transaction_path(root: &Path, id: &str) -> PathBuf {
    root.join("addons/.transactions").join(format!("{id}.json"))
}
fn write_transaction(root: &Path, transaction: &Transaction) -> Result<(), String> {
    let directory = root.join("addons/.transactions");
    directory_checked(root, &directory)?;
    let mut file = tempfile::NamedTempFile::new_in(directory).map_err(|e| e.to_string())?;
    file.write_all(&serde_json::to_vec(transaction).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    file.as_file().sync_all().map_err(|e| e.to_string())?;
    file.persist(transaction_path(root, &transaction.id))
        .map_err(|e| e.to_string())?;
    Ok(())
}
fn restore(root: &Path, transaction: &Transaction) -> Result<(), String> {
    if !focus_addon_sdk::valid_id(&transaction.id)
        || !transaction.stage.starts_with(".install-")
        || Path::new(&transaction.stage).components().count() != 1
    {
        return Err("Invalid interrupted addon transaction".into());
    }
    let addons = root.join("addons");
    let stage = addons.join(&transaction.stage);
    let target = addons.join(&transaction.id);
    let backup = stage.join("previous");
    if stage
        .symlink_metadata()
        .is_ok_and(|m| m.file_type().is_symlink())
        || target
            .symlink_metadata()
            .is_ok_and(|m| m.file_type().is_symlink())
    {
        return Err("Interrupted addon transaction contains a symlink".into());
    }
    if backup.exists() {
        if target.exists() {
            fs::remove_dir_all(&target).map_err(|e| e.to_string())?;
        }
        fs::rename(backup, &target).map_err(|e| e.to_string())?;
    } else if !transaction.had_previous && target.exists() {
        fs::remove_dir_all(&target).map_err(|e| e.to_string())?;
    }
    let receipt = root
        .join("addon-receipts")
        .join(format!("{}.json", transaction.id));
    if transaction.had_receipt {
        fs::copy(stage.join("receipt.previous.json"), receipt).map_err(|e| e.to_string())?;
    } else if receipt.exists() {
        fs::remove_file(receipt).map_err(|e| e.to_string())?;
    }
    fs::remove_file(transaction_path(root, &transaction.id)).map_err(|e| e.to_string())?;
    Ok(())
}
pub(super) fn recover(root: &Path) -> Result<(), String> {
    let directory = root.join("addons/.transactions");
    if !directory.exists() {
        return Ok(());
    }
    for entry in fs::read_dir(directory).map_err(|e| e.to_string())? {
        let path = entry.map_err(|e| e.to_string())?.path();
        if path.extension().is_none_or(|s| s != "json") {
            continue;
        }
        let transaction: Transaction =
            serde_json::from_slice(&fs::read(path).map_err(|e| e.to_string())?)
                .map_err(|e| e.to_string())?;
        let _lease = Lease::acquire(&root.join("addons"), &transaction.id, false)?;
        restore(root, &transaction)?;
        let stage = root.join("addons").join(transaction.stage);
        if stage.is_dir() {
            fs::remove_dir_all(stage).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}
impl Lease {
    pub fn acquire(addons: &Path, id: &str, shared: bool) -> Result<Self, String> {
        if !focus_addon_sdk::valid_id(id) {
            return Err("Invalid addon ID".into());
        }
        fs::create_dir_all(addons).map_err(|e| e.to_string())?;
        let directory = addons.join(".locks");
        directory_checked(addons, &directory)?;
        let path = directory.join(format!("{id}.lock"));
        if path
            .symlink_metadata()
            .is_ok_and(|m| m.file_type().is_symlink())
        {
            return Err("Invalid addon lock path".into());
        }
        let file = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(path)
            .map_err(|e| e.to_string())?;
        use std::os::fd::AsRawFd;
        // Closing the owned file releases this advisory cross-process lease.
        if unsafe {
            libc::flock(
                file.as_raw_fd(),
                (if shared { libc::LOCK_SH } else { libc::LOCK_EX }) | libc::LOCK_NB,
            )
        } != 0
        {
            return Err(
                "This addon is in use by another operation or editor. Stop it there and retry."
                    .into(),
            );
        }
        Ok(Self(file))
    }
}
impl Drop for Lease {
    fn drop(&mut self) {
        use std::os::fd::AsRawFd;
        let _ = unsafe { libc::flock(self.0.as_raw_fd(), libc::LOCK_UN) };
    }
}
fn directory_checked(root: &Path, path: &Path) -> Result<(), String> {
    if path
        .symlink_metadata()
        .is_ok_and(|m| m.file_type().is_symlink())
    {
        return Err("Addon directory must not be a symlink".into());
    }
    fs::create_dir_all(path).map_err(|e| e.to_string())?;
    if !path
        .canonicalize()
        .map_err(|e| e.to_string())?
        .starts_with(root.canonicalize().map_err(|e| e.to_string())?)
    {
        return Err("Addon directory escapes application data".into());
    }
    Ok(())
}
pub(super) fn target() -> Option<&'static str> {
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("linux", "x86_64") => Some("x86_64-unknown-linux-gnu"),
        ("linux", "aarch64") => Some("aarch64-unknown-linux-gnu"),
        ("macos", "aarch64") => Some("aarch64-apple-darwin"),
        _ => None,
    }
}
pub(super) fn data_path(root: &Path, id: &str) -> PathBuf {
    root.join("addon-data").join(id)
}
fn migrate_data(root: &Path, id: &str) -> Result<(), String> {
    let installation = root.join("addons").join(id);
    if installation
        .symlink_metadata()
        .is_ok_and(|m| m.file_type().is_symlink())
    {
        return Err("Addon installation must not be a symlink".into());
    }
    let old = installation.join("data");
    let new = data_path(root, id);
    directory_checked(root, &root.join("addon-data"))?;
    if old.exists() {
        if old
            .symlink_metadata()
            .is_ok_and(|m| m.file_type().is_symlink())
        {
            return Err("Legacy addon data must not be a symlink".into());
        }
        if new.exists() {
            return Err("Both legacy and new addon data exist. Resolve them before updating; nothing was overwritten.".into());
        }
        fs::rename(old, new).map_err(|e| e.to_string())?;
    }
    Ok(())
}
pub(super) fn ensure_data(root: &Path, id: &str) -> Result<PathBuf, String> {
    let old = root.join("addons").join(id).join("data");
    if old.exists() {
        let _lease = Lease::acquire(&root.join("addons"), id, false)?;
        migrate_data(root, id)?;
    }
    let path = data_path(root, id);
    directory_checked(root, &path)?;
    Ok(path)
}
pub(super) fn receipt(root: &Path, id: &str) -> Option<Receipt> {
    serde_json::from_slice(&fs::read(root.join("addon-receipts").join(format!("{id}.json"))).ok()?)
        .ok()
}
fn check_manifest(manifest: &Manifest) -> Result<(), String> {
    if manifest
        .target
        .as_deref()
        .is_some_and(|value| Some(value) != target())
    {
        return Err("This addon package targets a different platform".into());
    }
    Ok(())
}
fn staging(root: &Path) -> Result<tempfile::TempDir, String> {
    fs::create_dir_all(root).map_err(|e| e.to_string())?;
    let addons = root.join("addons");
    directory_checked(root, &addons)?;
    let stage = tempfile::Builder::new()
        .prefix(".install-")
        .tempdir_in(addons)
        .map_err(|e| e.to_string())?;
    fs::create_dir(stage.path().join("payload")).map_err(|e| e.to_string())?;
    Ok(stage)
}
pub(super) fn prepare_local(root: &Path, source: &Path) -> Result<Prepared, String> {
    let manifest = load_manifest(source)?;
    check_manifest(&manifest)?;
    let stage = staging(root)?;
    let from = source.parent().ok_or("Missing addon directory")?;
    let mut assets = vec![
        PathBuf::from("addon.toml"),
        manifest.executable.clone(),
        manifest.icon.clone(),
    ];
    for optional in ["LICENSE", "THIRD_PARTY_NOTICES.txt"] {
        if from.join(optional).is_file() {
            assets.push(optional.into());
        }
    }
    for relative in assets {
        let input = from.join(&relative);
        if input
            .symlink_metadata()
            .map_err(|e| e.to_string())?
            .file_type()
            .is_symlink()
        {
            return Err("Addon assets must not be symlinks".into());
        }
        let output = stage.path().join("payload").join(relative);
        fs::create_dir_all(output.parent().unwrap()).map_err(|e| e.to_string())?;
        fs::copy(input, output).map_err(|e| e.to_string())?;
    }
    Ok(Prepared {
        manifest,
        receipt: Receipt {
            source: "local".into(),
            release: None,
            package: None,
        },
        stage,
    })
}
pub(super) fn prepare_archive(
    root: &Path,
    path: &Path,
    expected: Option<(&Package, &str)>,
) -> Result<Prepared, String> {
    if fs::metadata(path).map_err(|e| e.to_string())?.len() > MAX_ARCHIVE {
        return Err("Addon archive exceeds size limit".into());
    }
    if let Some((package, _)) = expected {
        let bytes = fs::read(path).map_err(|e| e.to_string())?;
        let hash = Sha256::digest(&bytes)
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>();
        if bytes.len() as u64 != package.size || !hash.eq_ignore_ascii_case(&package.sha256) {
            return Err("Addon archive checksum or size does not match the catalog".into());
        }
    }
    let stage = staging(root)?;
    let payload = stage.path().join("payload");
    let file = File::open(path).map_err(|e| e.to_string())?;
    let mut archive = tar::Archive::new(flate2::read::GzDecoder::new(file));
    let mut total = 0u64;
    let mut names = HashSet::new();
    for entry in archive.entries().map_err(|e| e.to_string())? {
        let mut entry = entry.map_err(|e| e.to_string())?;
        let name = entry.path().map_err(|e| e.to_string())?.into_owned();
        if name.as_os_str().is_empty()
            || !name.components().all(|c| matches!(c, Component::Normal(_)))
            || !entry.header().entry_type().is_file()
            || !names.insert(name.clone())
            || names.len() > 64
        {
            return Err(
                "Addon archive contains unsafe paths, links, duplicate, or excessive entries"
                    .into(),
            );
        }
        let size = entry.size();
        total = total
            .checked_add(size)
            .filter(|v| *v <= MAX_EXPANDED)
            .ok_or("Expanded addon exceeds size limit")?;
        let output = payload.join(&name);
        fs::create_dir_all(output.parent().unwrap()).map_err(|e| e.to_string())?;
        let mut file = File::create(output).map_err(|e| e.to_string())?;
        let copied = std::io::copy(&mut entry, &mut file).map_err(|e| e.to_string())?;
        if copied != size {
            return Err("Incomplete addon archive".into());
        }
    }
    let manifest = load_manifest(&payload.join("addon.toml"))?;
    check_manifest(&manifest)?;
    let allowed: HashSet<_> = [
        PathBuf::from("addon.toml"),
        manifest.executable.clone(),
        manifest.icon.clone(),
        "LICENSE".into(),
        "THIRD_PARTY_NOTICES.txt".into(),
    ]
    .into_iter()
    .collect();
    if names.iter().any(|name| !allowed.contains(name)) {
        return Err("Addon archive includes unexpected files".into());
    }
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(
        payload.join(&manifest.executable),
        fs::Permissions::from_mode(0o755),
    )
    .map_err(|e| e.to_string())?;
    let receipt = if let Some((package, release)) = expected {
        if manifest.id != package.id
            || manifest.version != package.version
            || manifest.protocol != package.protocol
            || manifest.target.as_deref() != Some(&package.target)
        {
            return Err("Addon manifest does not match the release catalog".into());
        }
        Receipt {
            source: "official".into(),
            release: Some(release.into()),
            package: Some(package.clone()),
        }
    } else {
        Receipt {
            source: "local".into(),
            release: None,
            package: None,
        }
    };
    Ok(Prepared {
        manifest,
        receipt,
        stage,
    })
}
pub(super) fn commit(root: &Path, prepared: Prepared) -> Result<(Manifest, Receipt), String> {
    let id = &prepared.manifest.id;
    let addons = root.join("addons");
    let _lease = Lease::acquire(&addons, id, false)?;
    if transaction_path(root, id).exists() {
        return Err(
            "An interrupted installation needs recovery. Rescan installed addons first.".into(),
        );
    }
    migrate_data(root, id)?;
    directory_checked(root, &data_path(root, id))?;
    let target = addons.join(id);
    if target
        .symlink_metadata()
        .is_ok_and(|m| m.file_type().is_symlink())
    {
        return Err("Addon installation must not be a symlink".into());
    }
    let journal = Transaction {
        id: id.clone(),
        stage: prepared
            .stage
            .path()
            .file_name()
            .unwrap()
            .to_string_lossy()
            .into_owned(),
        had_previous: target.exists(),
        had_receipt: root
            .join("addon-receipts")
            .join(format!("{id}.json"))
            .is_file(),
    };
    if journal.had_receipt {
        fs::copy(
            root.join("addon-receipts").join(format!("{id}.json")),
            prepared.stage.path().join("receipt.previous.json"),
        )
        .map_err(|e| e.to_string())?;
    }
    write_transaction(root, &journal)?;
    let result = (|| -> Result<(), String> {
        if journal.had_previous {
            fs::rename(&target, prepared.stage.path().join("previous"))
                .map_err(|e| e.to_string())?;
        }
        fs::rename(prepared.stage.path().join("payload"), &target).map_err(|e| e.to_string())?;
        let receipts = root.join("addon-receipts");
        directory_checked(root, &receipts)?;
        let mut file = tempfile::NamedTempFile::new_in(&receipts).map_err(|e| e.to_string())?;
        file.write_all(&serde_json::to_vec(&prepared.receipt).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
        file.as_file().sync_all().map_err(|e| e.to_string())?;
        file.persist(receipts.join(format!("{id}.json")))
            .map_err(|e| e.to_string())?;
        fs::remove_file(transaction_path(root, id)).map_err(|e| e.to_string())?;
        Ok(())
    })();
    if let Err(error) = result {
        if let Err(rollback) = restore(root, &journal) {
            let retained = prepared.stage.keep();
            return Err(format!(
                "{error}; rollback pending at {}: {rollback}",
                retained.display()
            ));
        }
        return Err(error);
    }
    Ok((prepared.manifest, prepared.receipt))
}
pub(super) fn uninstall(root: &Path, id: &str, delete_data: bool) -> Result<(), String> {
    let addons = root.join("addons");
    let _lease = Lease::acquire(&addons, id, false)?;
    if transaction_path(root, id).exists() {
        return Err(
            "An interrupted installation needs recovery. Rescan installed addons first.".into(),
        );
    }
    migrate_data(root, id)?;
    let target = addons.join(id);
    let stage = staging(root)?;
    let saved = stage.path().join("previous");
    if target
        .symlink_metadata()
        .is_ok_and(|m| m.file_type().is_symlink())
    {
        return Err("Addon installation must not be a symlink".into());
    }
    let existed = target.exists();
    if existed {
        fs::rename(&target, &saved).map_err(|e| e.to_string())?;
    }
    let result = (|| -> Result<(), String> {
        if delete_data {
            let data = data_path(root, id);
            if data.exists() {
                if data
                    .symlink_metadata()
                    .map_err(|e| e.to_string())?
                    .file_type()
                    .is_symlink()
                {
                    return Err("Addon data must not be a symlink".into());
                }
                fs::rename(data, stage.path().join("data")).map_err(|e| e.to_string())?;
            }
        }
        let receipt = root.join("addon-receipts").join(format!("{id}.json"));
        if receipt.exists() {
            fs::rename(receipt, stage.path().join("receipt.json")).map_err(|e| e.to_string())?;
        }
        Ok(())
    })();
    if let Err(error) = result {
        let rollback = (|| -> Result<(), std::io::Error> {
            if existed {
                fs::rename(saved, target)?;
            }
            let data = stage.path().join("data");
            if data.exists() {
                fs::rename(data, data_path(root, id))?;
            }
            Ok(())
        })();
        if let Err(rollback) = rollback {
            let retained = stage.keep();
            return Err(format!(
                "{error}; retained files at {}: {rollback}",
                retained.display()
            ));
        }
        return Err(error);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn source(version: &str) -> tempfile::TempDir {
        let directory = tempfile::tempdir().unwrap();
        fs::write(directory.path().join("addon.toml"),format!("id='fixture'\nname='Fixture'\nversion='{version}'\nprotocol=1\nexecutable='program'\nicon='icon.png'\ntarget='{}'\n",target().unwrap())).unwrap();
        fs::write(directory.path().join("program"), version).unwrap();
        fs::write(directory.path().join("icon.png"), "icon").unwrap();
        directory
    }
    fn install(root: &Path, source: &Path) {
        commit(
            root,
            prepare_local(root, &source.join("addon.toml")).unwrap(),
        )
        .unwrap();
    }
    fn archive(source: &Path, extra: Option<(&str, tar::EntryType)>) -> tempfile::NamedTempFile {
        let file = tempfile::NamedTempFile::new().unwrap();
        let encoder =
            flate2::write::GzEncoder::new(file.reopen().unwrap(), flate2::Compression::default());
        let mut archive = tar::Builder::new(encoder);
        for name in ["addon.toml", "program", "icon.png"] {
            archive
                .append_path_with_name(source.join(name), name)
                .unwrap();
        }
        if let Some((name, kind)) = extra {
            let mut header = tar::Header::new_gnu();
            header.set_entry_type(kind);
            header.set_size(0);
            header.set_mode(0o755);
            if kind.is_symlink() {
                header.set_link_name("/outside").unwrap();
            }
            let bytes = header.as_mut_bytes();
            bytes[..100].fill(0);
            bytes[..name.len()].copy_from_slice(name.as_bytes());
            header.set_cksum();
            archive.append(&header, &[][..]).unwrap();
        }
        archive.into_inner().unwrap().finish().unwrap();
        file
    }
    #[test]
    fn install_update_and_uninstall_preserve_data() {
        let root = tempfile::tempdir().unwrap();
        let one = source("1");
        install(root.path(), one.path());
        // Simulate a package installed by the previous editor's storage layout.
        fs::remove_dir(data_path(root.path(), "fixture")).unwrap();
        let legacy = root.path().join("addons/fixture/data");
        fs::create_dir_all(&legacy).unwrap();
        fs::write(legacy.join("state.json"), "keep").unwrap();
        let two = source("2");
        install(root.path(), two.path());
        assert_eq!(
            fs::read_to_string(root.path().join("addons/fixture/program")).unwrap(),
            "2"
        );
        assert_eq!(
            fs::read_to_string(data_path(root.path(), "fixture").join("state.json")).unwrap(),
            "keep"
        );
        uninstall(root.path(), "fixture", false).unwrap();
        assert!(!root.path().join("addons/fixture").exists());
        assert!(data_path(root.path(), "fixture").exists());
        install(root.path(), two.path());
        uninstall(root.path(), "fixture", true).unwrap();
        assert!(!data_path(root.path(), "fixture").exists());
    }
    #[test]
    fn failed_update_restores_previous_package() {
        let root = tempfile::tempdir().unwrap();
        let one = source("1");
        install(root.path(), one.path());
        fs::remove_dir_all(root.path().join("addon-receipts")).unwrap();
        fs::write(root.path().join("addon-receipts"), "blocked").unwrap();
        let two = source("2");
        assert!(
            commit(
                root.path(),
                prepare_local(root.path(), &two.path().join("addon.toml")).unwrap()
            )
            .is_err()
        );
        assert_eq!(
            fs::read_to_string(root.path().join("addons/fixture/program")).unwrap(),
            "1"
        );
    }
    #[test]
    fn interrupted_swap_is_recovered() {
        let root = tempfile::tempdir().unwrap();
        let one = source("1");
        install(root.path(), one.path());
        let stage = staging(root.path()).unwrap();
        let transaction = Transaction {
            id: "fixture".into(),
            stage: stage.path().file_name().unwrap().to_string_lossy().into(),
            had_previous: true,
            had_receipt: true,
        };
        fs::copy(
            root.path().join("addon-receipts/fixture.json"),
            stage.path().join("receipt.previous.json"),
        )
        .unwrap();
        write_transaction(root.path(), &transaction).unwrap();
        fs::rename(
            root.path().join("addons/fixture"),
            stage.path().join("previous"),
        )
        .unwrap();
        let _ = stage.keep();
        recover(root.path()).unwrap();
        assert_eq!(
            fs::read_to_string(root.path().join("addons/fixture/program")).unwrap(),
            "1"
        );
        assert!(!transaction_path(root.path(), "fixture").exists());
    }
    #[test]
    fn active_process_lease_blocks_replacement() {
        let root = tempfile::tempdir().unwrap();
        let one = source("1");
        install(root.path(), one.path());
        let _running = Lease::acquire(&root.path().join("addons"), "fixture", true).unwrap();
        assert!(
            commit(
                root.path(),
                prepare_local(root.path(), &one.path().join("addon.toml")).unwrap()
            )
            .unwrap_err()
            .contains("in use")
        );
    }
    #[test]
    fn python_release_packages_install_through_the_manager() {
        let release = tempfile::tempdir().unwrap();
        let source = source("1.0.0");
        let definition = release.path().join("addons/fixture");
        let stage = release.path().join("build/addons/fixture");
        fs::create_dir_all(&definition).unwrap();
        fs::create_dir_all(&stage).unwrap();
        fs::copy(
            source.path().join("addon.toml"),
            definition.join("addon.toml"),
        )
        .unwrap();
        for name in ["addon.toml", "program", "icon.png"] {
            fs::copy(source.path().join(name), stage.join(name)).unwrap();
        }
        for name in ["LICENSE", "THIRD_PARTY_NOTICES.txt"] {
            fs::write(stage.join(name), "release notice").unwrap();
        }
        fs::write(
            release.path().join("Cargo.toml"),
            "[package]\nversion='1.0.0'\n",
        )
        .unwrap();
        let result = std::process::Command::new("python3")
            .args(["-c", "import sys; from pathlib import Path; sys.path.insert(0, sys.argv[1]); from addon_release import package_addons; root=Path(sys.argv[2]); package_addons(root/'build', root/'dist', 'release/v1', sys.argv[3], root)"])
            .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("scripts/release"))
            .arg(release.path())
            .arg(target().unwrap())
            .output().unwrap();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        let index = fs::read_dir(release.path().join("dist"))
            .unwrap()
            .map(|e| e.unwrap().path())
            .find(|p| p.extension().is_some_and(|v| v == "json"))
            .unwrap();
        let catalog: focus_addon_sdk::Catalog =
            serde_json::from_slice(&fs::read(index).unwrap()).unwrap();
        catalog.validate().unwrap();
        let package = &catalog.packages[0];
        let root = tempfile::tempdir().unwrap();
        let prepared = prepare_archive(
            root.path(),
            &release.path().join("dist").join(&package.archive),
            Some((package, &catalog.release)),
        )
        .unwrap();
        commit(root.path(), prepared).unwrap();
        assert_eq!(
            fs::read_to_string(root.path().join("addons/fixture/program")).unwrap(),
            "1.0.0"
        );
        assert_eq!(
            receipt(root.path(), "fixture").unwrap().release.as_deref(),
            Some("release/v1")
        );
    }
    #[test]
    fn unsafe_archives_and_checksum_mismatches_are_rejected() {
        let root = tempfile::tempdir().unwrap();
        let source = source("1");
        for extra in [
            ("../escape", tar::EntryType::Regular),
            ("link", tar::EntryType::Symlink),
            ("state.json", tar::EntryType::Regular),
        ] {
            let archive = archive(source.path(), Some(extra));
            assert!(prepare_archive(root.path(), archive.path(), None).is_err());
        }
        let file = archive(source.path(), None);
        let package = Package {
            id: "fixture".into(),
            name: "Fixture".into(),
            description: String::new(),
            publisher: String::new(),
            version: "1".into(),
            protocol: 1,
            target: target().unwrap().into(),
            archive: "fixture.tar.gz".into(),
            size: file.as_file().metadata().unwrap().len(),
            sha256: "0".repeat(64),
        };
        assert!(
            prepare_archive(root.path(), file.path(), Some((&package, "v1")))
                .err()
                .unwrap()
                .contains("checksum")
        );
        let mut package = package;
        package.sha256 = Sha256::digest(fs::read(file.path()).unwrap())
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        let prepared = prepare_archive(root.path(), file.path(), Some((&package, "v1"))).unwrap();
        assert_eq!(prepared.receipt.release.as_deref(), Some("v1"));
    }
}
