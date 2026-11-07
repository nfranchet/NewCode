//! Anonymous HTTPS access to the official release catalog; no stored credentials are read.
use focus_addon_sdk::{Catalog, Package};
use std::{
    io::{Read, Write},
    path::Path,
    time::Duration,
};
const REPOSITORY: &str = "https://github.com/nfranchet/NewCode";
fn client() -> Result<reqwest::blocking::Client, String> {
    reqwest::blocking::Client::builder()
        .https_only(true)
        .timeout(Duration::from_secs(120))
        .redirect(reqwest::redirect::Policy::limited(5))
        .user_agent("focus-editor-addon-manager")
        .build()
        .map_err(|e| e.to_string())
}
fn url(tag: &str, asset: &str) -> reqwest::Url {
    let mut url = reqwest::Url::parse(REPOSITORY).unwrap();
    url.path_segments_mut()
        .unwrap()
        .extend(["releases", "download", tag, asset]);
    url
}
pub(super) fn fetch(tag: &str) -> Result<Catalog, String> {
    let client = client()?;
    let tag = if tag.trim() == "latest" {
        let response = client
            .get("https://api.github.com/repos/nfranchet/NewCode/releases/latest")
            .send()
            .map_err(|e| e.to_string())?
            .error_for_status()
            .map_err(|e| e.to_string())?;
        let mut bytes = Vec::new();
        response
            .take(1024 * 1024)
            .read_to_end(&mut bytes)
            .map_err(|e| e.to_string())?;
        serde_json::from_slice::<serde_json::Value>(&bytes).map_err(|e| e.to_string())?["tag_name"]
            .as_str()
            .ok_or("Release response has no tag")?
            .to_owned()
    } else {
        tag.trim().to_owned()
    };
    if tag.is_empty() {
        return Err("Choose a release tag or latest".into());
    }
    let response = client
        .get(url(&tag, "addons.json"))
        .send()
        .map_err(|e| format!("Cannot fetch addon catalog: {e}"))?;
    if response.status() == reqwest::StatusCode::NOT_FOUND {
        return Err("This release has no available addon catalog. Select a release published with addon packages.".into());
    }
    let response = response
        .error_for_status()
        .map_err(|e| format!("Cannot fetch addon catalog: {e}"))?;
    let mut bytes = Vec::new();
    response
        .take((focus_addon_sdk::MAX_MESSAGE + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() > focus_addon_sdk::MAX_MESSAGE {
        return Err("Addon catalog exceeds size limit".into());
    }
    let catalog: Catalog = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
    catalog.validate()?;
    if catalog.release != tag {
        return Err("Catalog does not match the selected release".into());
    }
    Ok(catalog)
}
pub(super) fn download(
    tag: &str,
    package: &Package,
    path: &Path,
    mut progress: impl FnMut(String),
) -> Result<(), String> {
    let mut response = client()?
        .get(url(tag, &package.archive))
        .send()
        .map_err(|e| e.to_string())?
        .error_for_status()
        .map_err(|e| e.to_string())?;
    let mut file = std::fs::File::create(path).map_err(|e| e.to_string())?;
    let mut total = 0u64;
    let mut buffer = [0u8; 64 * 1024];
    let mut notified = std::time::Instant::now();
    loop {
        let count = response.read(&mut buffer).map_err(|e| e.to_string())?;
        if count == 0 {
            break;
        }
        total += count as u64;
        if total > package.size || total > super::packages::MAX_ARCHIVE {
            return Err("Downloaded addon exceeds its declared size".into());
        }
        file.write_all(&buffer[..count])
            .map_err(|e| e.to_string())?;
        if notified.elapsed() > Duration::from_millis(200) {
            progress(format!(
                "Downloading {}: {}%",
                package.name,
                total * 100 / package.size
            ));
            notified = std::time::Instant::now();
        }
    }
    if total != package.size {
        return Err("Addon download was interrupted or incomplete".into());
    }
    Ok(())
}
