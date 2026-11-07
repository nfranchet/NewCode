//! Finder does not inherit the user's interactive shell PATH.
#[cfg(any(target_os = "macos", test))]
fn macos_path(
    original: Option<&std::ffi::OsStr>,
    home: Option<&std::path::Path>,
) -> Result<std::ffi::OsString, std::env::JoinPathsError> {
    let mut paths: Vec<_> = std::env::split_paths(
        original.unwrap_or_else(|| std::ffi::OsStr::new("/usr/bin:/bin:/usr/sbin:/sbin")),
    )
    .collect();
    let mut extra = Vec::new();
    if let Some(home) = home {
        extra.push(home.join(".local/bin"));
    }
    extra.extend(["/opt/homebrew/bin".into(), "/usr/local/bin".into()]);
    for path in extra {
        if !paths.contains(&path) {
            paths.push(path);
        }
    }
    std::env::join_paths(paths)
}

pub(crate) fn prepare_environment() {
    #[cfg(target_os = "macos")]
    if let Ok(path) = macos_path(
        std::env::var_os("PATH").as_deref(),
        dirs::home_dir().as_deref(),
    ) {
        // SAFETY: called at main entry, before logging, GUI initialization, or workers.
        unsafe { std::env::set_var("PATH", path) };
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{ffi::OsStr, path::Path};
    #[test]
    fn finder_paths_preserve_existing_priority_and_non_utf8_home() {
        use std::os::unix::ffi::OsStrExt;
        let home = Path::new(OsStr::from_bytes(b"/Users/test\xff"));
        let result = macos_path(Some(OsStr::new("/custom:/usr/local/bin")), Some(home)).unwrap();
        let paths: Vec<_> = std::env::split_paths(&result).collect();
        assert_eq!(paths[0], Path::new("/custom"));
        assert_eq!(paths[1], Path::new("/usr/local/bin"));
        assert!(paths.contains(&home.join(".local/bin")));
        assert!(paths.contains(&"/opt/homebrew/bin".into()));
        assert_eq!(
            paths
                .iter()
                .filter(|p| **p == Path::new("/usr/local/bin"))
                .count(),
            1
        );
        let defaults = macos_path(None, None).unwrap();
        assert!(std::env::split_paths(&defaults).any(|p| p == Path::new("/usr/bin")));
    }
}
