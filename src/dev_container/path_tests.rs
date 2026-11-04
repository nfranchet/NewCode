//! Host spelling must not affect container mapping or weaken mount boundaries.
use super::Runtime;
use std::{fs, os::unix::fs::symlink, path::Path};

#[test]
fn relative_and_absolute_workspace_paths_map_identically() {
    // Do not change process cwd: tests and application workers share it.
    let root = std::env::current_dir().unwrap().canonicalize().unwrap();
    let runtime = Runtime {
        host_workspace: root.clone(),
        workspace_folder: "/workspaces/project".into(),
        container_id: "unused".into(),
        remote_user: None,
    };
    for path in [Path::new("."), root.as_path(), Path::new("src/..")] {
        assert_eq!(
            runtime.container_path(path).unwrap(),
            Path::new("/workspaces/project")
        );
    }
    assert_eq!(
        runtime.container_path(Path::new("src")).unwrap(),
        Path::new("/workspaces/project/src")
    );
    assert!(runtime.container_path(Path::new("..")).is_err());
    assert!(
        runtime
            .container_path(&root.with_file_name("NewCode-other"))
            .is_err()
    );
    assert_eq!(
        runtime
            .host_path(Path::new("/workspaces/project/src"))
            .unwrap(),
        root.join("src")
    );
}

#[test]
fn symlinked_workspaces_and_nested_paths_respect_real_boundary() {
    let root = std::env::temp_dir().join(format!("focus-container-paths-{}", std::process::id()));
    fs::create_dir_all(root.join("workspace/nested")).unwrap();
    fs::create_dir_all(root.join("outside")).unwrap();
    symlink(root.join("workspace"), root.join("alias")).unwrap();
    symlink(root.join("outside"), root.join("workspace/escape")).unwrap();
    let runtime = Runtime {
        host_workspace: root.join("alias"),
        workspace_folder: "/workspaces/project".into(),
        container_id: "unused".into(),
        remote_user: None,
    };
    for path in [root.join("alias/nested"), root.join("workspace/nested")] {
        assert_eq!(
            runtime.container_path(&path).unwrap(),
            Path::new("/workspaces/project/nested")
        );
    }
    for path in [
        root.join("outside"),
        root.join("alias/../outside"),
        root.join("workspace/escape"),
    ] {
        assert!(
            runtime
                .container_path(&path)
                .unwrap_err()
                .contains("outside the Dev Container workspace")
        );
    }
    assert!(
        runtime
            .container_path(&root.join("workspace/missing"))
            .unwrap_err()
            .contains("Cannot resolve host path")
    );
    fs::remove_dir_all(root).unwrap();
}
