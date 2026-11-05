//! Build contexts must preserve COPY sources without rewriting user files/mounts.
use super::*;
use std::{
    fs,
    path::{Path, PathBuf},
};

struct Fixture {
    root: PathBuf,
    configuration: Configuration,
}
impl Fixture {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "focus-compose-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(root.join(".devcontainer/vscode/files")).unwrap();
        let compose = root.join(".devcontainer/compose.yml");
        fs::write(&compose, "services:\n  dev:\n    build:\n      context: .\n      dockerfile: ./vscode/Dockerfile\n      args:\n        PROJECT_NAME: ${COMPOSE_PROJECT_NAME}\n    volumes:\n      - .:/workspaces/test\n").unwrap();
        fs::write(
            root.join(".devcontainer/vscode/Dockerfile"),
            "FROM scratch\nCOPY ./vscode/files/toto.txt /tmp/toto.txt\n",
        )
        .unwrap();
        fs::write(
            root.join(".devcontainer/vscode/files/toto.txt"),
            "copy source stays in place\n",
        )
        .unwrap();
        let configuration = Configuration {
            name: "Test".into(),
            workspace: root.clone(),
            compose_files: vec![compose],
            service: "dev".into(),
            workspace_folder: "/workspaces/test".into(),
            remote_user: None,
        };
        Self {
            root,
            configuration,
        }
    }
    fn files(&self) -> Vec<PathBuf> {
        vec![
            self.root.join(".devcontainer/compose.yml"),
            self.root.join(".devcontainer/vscode/Dockerfile"),
            self.root.join(".devcontainer/vscode/files/toto.txt"),
        ]
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

#[test]
fn overrides_only_builds_including_dependency_inline_and_remote_builds() {
    let fixture = Fixture::new();
    let build = json!({"context": fixture.root.join(".devcontainer"), "dockerfile":"vscode/Dockerfile",
        "args":{"EXAMPLE":"value"}, "additional_contexts":{"files":fixture.root.join(".devcontainer/vscode/files")}});
    let inline =
        json!({"context":fixture.root.join(".devcontainer"),"dockerfile_inline":"FROM scratch"});
    let remote = json!({"context":"https://example.invalid/repo.git","dockerfile":"Dockerfile"});
    let resolved = json!({"services":{
        "dev":{"build":build,"volumes":[{"source":"must-not-override","target":"/workspaces/test"}]},
        "dependency":{"build":inline}, "remote":{"build":remote}, "database":{"image":"example:latest"}}});
    let before = resolved.clone();
    let overlay = build_override(&fixture.configuration, &resolved)
        .unwrap()
        .unwrap();
    assert_eq!(
        overlay,
        json!({"services":{"dev":{"build":build},"dependency":{"build":inline},"remote":{"build":remote}}})
    );
    assert_eq!(resolved, before);
    let context = Path::new(
        overlay["services"]["dev"]["build"]["context"]
            .as_str()
            .unwrap(),
    );
    assert!(context.join("vscode/Dockerfile").is_file());
    assert!(context.join("vscode/files/toto.txt").is_file());
    assert!(!fixture.root.join("vscode/files/toto.txt").exists());
}

#[test]
fn image_only_services_need_no_override_and_missing_services_are_errors() {
    let fixture = Fixture::new();
    assert!(
        build_override(
            &fixture.configuration,
            &json!({"services":{"dev":{"image":"example:latest"}}})
        )
        .unwrap()
        .is_none()
    );
    assert!(
        build_override(&fixture.configuration, &json!({"services":{}}))
            .unwrap_err()
            .contains("not defined")
    );
}

fn verify_resolved(configuration: &Configuration) -> Value {
    let original: Value =
        serde_json::from_slice(&compose(configuration, ["config", "--format", "json"]).unwrap())
            .unwrap();
    let merged: Value =
        serde_json::from_slice(&start(configuration, ["config", "--format", "json"]).unwrap())
            .unwrap();
    for (name, service) in merged["services"].as_object().unwrap() {
        assert_eq!(service["volumes"], original["services"][name]["volumes"]);
    }
    assert_eq!(merged["name"], original["name"]);
    let service = &merged["services"][&configuration.service];
    if service["build"]["args"].get("PROJECT_NAME").is_some() {
        assert_eq!(service["build"]["args"]["PROJECT_NAME"], merged["name"]);
    }
    merged
}

#[test]
#[ignore = "requires Docker Compose plugin; resolves config only, never starts containers"]
fn nested_copy_context_and_workspace_mount_resolve_independently() {
    let fixture = Fixture::new();
    let before: Vec<_> = fixture
        .files()
        .iter()
        .map(|file| fs::read(file).unwrap())
        .collect();
    let merged = verify_resolved(&fixture.configuration);
    let service = &merged["services"]["dev"];
    let context = Path::new(service["build"]["context"].as_str().unwrap());
    assert_eq!(
        context.canonicalize().unwrap(),
        fixture.root.join(".devcontainer").canonicalize().unwrap()
    );
    assert!(
        context
            .join(service["build"]["dockerfile"].as_str().unwrap())
            .is_file()
    );
    assert!(context.join("vscode/files/toto.txt").is_file());
    assert_eq!(
        service["volumes"][0]["source"],
        fixture.root.to_str().unwrap()
    );
    for (file, bytes) in fixture.files().iter().zip(before) {
        assert_eq!(fs::read(file).unwrap(), bytes);
    }
}

#[test]
#[ignore = "requires Docker Compose plugin; resolves config only, never starts containers"]
fn bundled_configuration_resolves_with_runtime_override_without_file_edits() {
    let workspace = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let configuration = crate::dev_container::load_configuration(&workspace)
        .unwrap()
        .unwrap();
    let merged = verify_resolved(&configuration);
    let build = &merged["services"][&configuration.service]["build"];
    let context = Path::new(build["context"].as_str().unwrap());
    let dockerfile = context.join(build["dockerfile"].as_str().unwrap_or("Dockerfile"));
    let files = [
        workspace.join(".devcontainer/devcontainer.json"),
        configuration.compose_files[0].clone(),
        dockerfile,
    ];
    let before: Vec<_> = files.iter().map(|file| fs::read(file).unwrap()).collect();
    verify_resolved(&configuration);
    for (file, bytes) in files.iter().zip(before) {
        assert_eq!(fs::read(file).unwrap(), bytes);
    }
}

#[test]
#[ignore = "requires Docker daemon; builds the configured image without recreating containers"]
fn bundled_image_build_succeeds_through_launcher() {
    let workspace = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let configuration = crate::dev_container::load_configuration(&workspace)
        .unwrap()
        .unwrap();
    start(&configuration, ["build", configuration.service.as_str()]).unwrap();
}
