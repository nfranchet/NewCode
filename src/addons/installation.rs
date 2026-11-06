use super::*;
use std::fs;
use std::path::Component;

pub(super) fn load_manifest(path: &Path) -> Result<Manifest, String> {
    let metadata = fs::metadata(path).map_err(|e| e.to_string())?;
    if metadata.len() > 64 * 1024 {
        return Err("Addon manifest is too large".into());
    }
    let manifest: Manifest = toml::from_str(&fs::read_to_string(path).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    if manifest.id.is_empty()
        || !manifest
            .id
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_')
    {
        return Err("Addon ID must contain letters, digits, - or _".into());
    }
    if manifest.protocol != focus_addon_sdk::PROTOCOL_VERSION {
        return Err(format!("Unsupported addon protocol {}", manifest.protocol));
    }
    let root = path
        .parent()
        .ok_or("Missing addon directory")?
        .canonicalize()
        .map_err(|e| e.to_string())?;
    for relative in [&manifest.executable, &manifest.icon] {
        if relative.as_os_str().is_empty()
            || !relative
                .components()
                .all(|p| matches!(p, Component::Normal(_)))
        {
            return Err("Addon assets must use relative paths within the addon directory".into());
        }
        let resolved = root
            .join(relative)
            .canonicalize()
            .map_err(|e| e.to_string())?;
        if !resolved.starts_with(&root) || !resolved.is_file() {
            return Err("Addon asset escapes its installation directory".into());
        }
    }
    if let Some(spec) = &manifest.credential {
        let url = reqwest::Url::parse(&spec.origin).map_err(|_| "Invalid credential origin")?;
        if url.scheme() != "https"
            || url.path() != "/"
            || url.query().is_some()
            || !url.username().is_empty()
            || url.password().is_some()
        {
            return Err("Credential origin must be an HTTPS origin".into());
        }
        if spec.program.is_empty()
            || spec.program.contains('/')
            || !spec
                .environment
                .bytes()
                .all(|c| c.is_ascii_uppercase() || c == b'_')
        {
            return Err("Invalid credential tool/environment".into());
        }
    }
    Ok(manifest)
}

impl Addons {
    pub fn discover(&mut self, disabled: &[String]) {
        let mut result = || -> Result<Vec<Addon>, String> {
            let root = crate::app_paths::addons().map_err(|e| e.to_string())?;
            fs::create_dir_all(&root).map_err(|e| e.to_string())?;
            let mut entries = Vec::new();
            for directory in fs::read_dir(&root).map_err(|e| e.to_string())?.flatten() {
                let path = directory.path();
                if !directory.file_type().is_ok_and(|kind| kind.is_dir())
                    || path
                        .file_name()
                        .is_some_and(|s| s.to_string_lossy().starts_with('.'))
                {
                    continue;
                }
                match load_manifest(&path.join("addon.toml")) {
                    Ok(manifest) if path.file_name().is_some_and(|n| n == manifest.id.as_str()) => {
                        let enabled = !disabled.contains(&manifest.id);
                        entries.push(Addon {
                            manifest,
                            directory: path,
                            enabled,
                            process: None,
                            error: None,
                            view: View::default(),
                            values: BTreeMap::new(),
                            icon: None,
                            working: false,
                            incarnation: 0,
                        });
                    }
                    Ok(_) => {
                        self.notice =
                            Some("An addon directory does not match its manifest ID".into())
                    }
                    Err(error) => self.notice = Some(format!("Cannot load addon: {error}")),
                }
            }
            entries.sort_by(|a, b| a.manifest.id.cmp(&b.manifest.id));
            Ok(entries)
        };
        match result() {
            Ok(entries) => self.entries = entries,
            Err(error) => self.notice = Some(error),
        }
    }
    pub fn install(&mut self, source: &Path, disabled: &[String]) -> Result<(), String> {
        let manifest = load_manifest(source)?;
        let from = source.parent().ok_or("Missing addon directory")?;
        let root = crate::app_paths::addons().map_err(|e| e.to_string())?;
        let target = root.join(&manifest.id);
        fs::create_dir_all(&root).map_err(|e| e.to_string())?;
        let canonical_root = root.canonicalize().map_err(|e| e.to_string())?;
        if target.exists()
            && !target
                .canonicalize()
                .map_err(|e| e.to_string())?
                .starts_with(&canonical_root)
        {
            return Err("Addon installation destination escapes the addons directory".into());
        }
        if from.canonicalize().ok() == target.canonicalize().ok() && target.exists() {
            return Ok(());
        }
        // Stop the old executable before replacing it; keep its non-secret state.
        if let Some(addon) = self
            .entries
            .iter_mut()
            .find(|a| a.manifest.id == manifest.id)
        {
            addon.process = None;
        }
        fs::create_dir_all(&target).map_err(|e| e.to_string())?;
        for relative in [
            &manifest.executable,
            &manifest.icon,
            Path::new("addon.toml"),
        ] {
            let destination = target.join(relative);
            let mut parent = target.clone();
            for component in relative.parent().unwrap_or(Path::new("")).components() {
                parent.push(component);
                if fs::symlink_metadata(&parent).is_ok_and(|m| m.file_type().is_symlink()) {
                    return Err("Addon installation directory contains a symlink".into());
                }
            }
            fs::create_dir_all(destination.parent().unwrap()).map_err(|e| e.to_string())?;
            let temporary =
                destination.with_extension(format!("installing-{}", std::process::id()));
            let mut output = fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&temporary)
                .map_err(|e| e.to_string())?;
            let copied = (|| -> std::io::Result<()> {
                let mut input = fs::File::open(from.join(relative))?;
                std::io::copy(&mut input, &mut output)?;
                output.set_permissions(input.metadata()?.permissions())?;
                drop(output);
                fs::rename(&temporary, destination)
            })();
            if copied.is_err() {
                let _ = fs::remove_file(&temporary);
            }
            copied.map_err(|e| e.to_string())?;
        }
        self.discover(disabled);
        Ok(())
    }
}
