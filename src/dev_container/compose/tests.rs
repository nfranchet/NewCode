//! Resolve Dockerfiles without rewriting user configuration or mount paths.
use super::*;
use std::{fs, path::PathBuf};

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
        fs::create_dir_all(root.join(".devcontainer")).unwrap();
        let compose = root.join(".devcontainer/compose.yml");
        fs::write(&compose, "user configuration must stay unchanged").unwrap();
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
    fn resolved(&self, dockerfile: &str) -> Value {
        json!({"services":{"dev":{"build":{"context":self.root,"dockerfile":dockerfile},
            "volumes":[{"type":"bind","source":self.root,"target":"/workspaces/test"}]}}})
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

#[test]
#[ignore = "requires Docker Compose plugin; resolves config only, never starts containers"]
fn bundled_configuration_resolves_with_runtime_override_without_file_edits() {
    let workspace = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let configuration = crate::dev_container::load_configuration(&workspace)
        .unwrap()
        .unwrap();
    let files = [
        workspace.join(".devcontainer/devcontainer.json"),
        workspace.join(".devcontainer/docker-compose.yml"),
        workspace.join(".devcontainer/Dockerfile"),
    ];
    let before: Vec<_> = files.iter().map(|file| fs::read(file).unwrap()).collect();
    let resolved: Value =
        serde_json::from_slice(&compose(&configuration, ["config", "--format", "json"]).unwrap())
            .unwrap();
    let overlay = dockerfile_override(&configuration, &resolved).unwrap();
    let merged: Value = serde_json::from_slice(
        &execute(
            &configuration,
            ["config", "--format", "json"],
            overlay.as_ref(),
        )
        .unwrap(),
    )
    .unwrap();
    let original_service = &resolved["services"][&configuration.service];
    let service = &merged["services"][&configuration.service];
    assert_eq!(
        service["build"]["context"],
        original_service["build"]["context"]
    );
    assert_eq!(service["volumes"], original_service["volumes"]);
    let context = Path::new(service["build"]["context"].as_str().unwrap());
    assert_eq!(
        context.canonicalize().unwrap(),
        workspace.canonicalize().unwrap()
    );
    assert!(
        context
            .join(service["build"]["dockerfile"].as_str().unwrap())
            .is_file()
    );
    for (file, bytes) in files.iter().zip(before) {
        assert_eq!(fs::read(file).unwrap(), bytes);
    }
}

#[test]
fn resolves_compose_directory_dockerfile_without_changing_context_or_files() {
    let fixture = Fixture::new();
    let dockerfile = fixture.root.join(".devcontainer/Dockerfile");
    fs::write(&dockerfile, "FROM scratch").unwrap();
    let resolved = fixture.resolved("Dockerfile");
    let original = resolved.clone();
    let overlay = dockerfile_override(&fixture.configuration, &resolved)
        .unwrap()
        .unwrap();
    assert_eq!(
        overlay,
        json!({"services":{"dev":{"build":{"dockerfile":dockerfile.canonicalize().unwrap()}}}})
    );
    assert_eq!(resolved, original);
    // A legacy conventional spelling uses the existing Dev Container Dockerfile.
    assert_eq!(
        dockerfile_override(&fixture.configuration, &fixture.resolved(".Dockerfile"))
            .unwrap()
            .unwrap(),
        overlay
    );
    assert_eq!(
        fs::read_to_string(&fixture.configuration.compose_files[0]).unwrap(),
        "user configuration must stay unchanged"
    );
    // Valid context-relative paths always win, including explicit .devcontainer paths.
    assert!(
        dockerfile_override(
            &fixture.configuration,
            &fixture.resolved(".devcontainer/Dockerfile")
        )
        .unwrap()
        .is_none()
    );
    fs::write(fixture.root.join("Dockerfile"), "FROM other").unwrap();
    assert!(
        dockerfile_override(&fixture.configuration, &resolved)
            .unwrap()
            .is_none()
    );
}

#[test]
fn leaves_image_inline_and_remote_builds_to_compose_and_reports_missing_paths() {
    let fixture = Fixture::new();
    for service in [
        json!({"image":"example:latest"}),
        json!({"build":{"context":fixture.root,"dockerfile_inline":"FROM scratch"}}),
        json!({"build":{"context":"https://example.invalid/repo.git","dockerfile":"Dockerfile"}}),
    ] {
        assert!(
            dockerfile_override(&fixture.configuration, &json!({"services":{"dev":service}}))
                .unwrap()
                .is_none()
        );
    }
    assert!(
        dockerfile_override(
            &fixture.configuration,
            &fixture.resolved("missing.Dockerfile")
        )
        .unwrap_err()
        .contains("was not found")
    );
    assert!(
        dockerfile_override(&fixture.configuration, &json!({"services":{}}))
            .unwrap_err()
            .contains("not defined")
    );
}
