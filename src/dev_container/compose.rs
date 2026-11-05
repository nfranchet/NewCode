//! Resolve build paths using Compose-file semantics while retaining workspace mounts.
use super::{Configuration, diagnostics};
use serde_json::{Value, json};
use std::process::{Command, Stdio};

pub(super) fn compose<I, S>(configuration: &Configuration, args: I) -> Result<Vec<u8>, String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    execute(configuration, args, None)
}

#[cfg(test)]
mod tests;

pub(super) fn start<I, S>(configuration: &Configuration, args: I) -> Result<Vec<u8>, String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    // Compose normally resolves build contexts from the first Compose file.
    // The runtime uses the host workspace as its project directory for stable
    // project identity and workspace bind mounts. Resolve builds separately so
    // that adjusting the Dockerfile alone cannot break COPY/ADD or .dockerignore.
    let directory = configuration
        .compose_files
        .first()
        .and_then(|file| file.parent())
        .ok_or("No Compose file directory is configured")?
        .canonicalize()
        .map_err(|error| format!("Cannot resolve Compose directory: {error}"))?;
    let mut build_configuration = configuration.clone();
    build_configuration.workspace = directory;
    // Keep COMPOSE_PROJECT_NAME interpolation stable even during path resolution.
    let runtime_bytes = compose(configuration, ["config", "--format", "json"])?;
    let runtime: Value = serde_json::from_slice(&runtime_bytes)
        .map_err(|_| "Could not parse runtime Docker Compose configuration")?;
    let project_name = runtime["name"]
        .as_str()
        .ok_or("Docker Compose did not resolve the project name")?;
    let bytes = compose(
        &build_configuration,
        ["--project-name", project_name, "config", "--format", "json"],
    )?;
    let resolved: Value = serde_json::from_slice(&bytes)
        .map_err(|_| "Could not parse resolved Docker Compose configuration")?;
    let override_config = build_override(configuration, &resolved)?;
    execute(configuration, args, override_config.as_ref())
}

fn build_override(
    configuration: &Configuration,
    resolved: &Value,
) -> Result<Option<Value>, String> {
    let services = resolved["services"]
        .as_object()
        .ok_or("Docker Compose did not resolve services")?;
    if !services.contains_key(&configuration.service) {
        return Err(format!(
            "Compose service {} is not defined",
            configuration.service
        ));
    }
    let mut overrides = serde_json::Map::new();
    for (name, service) in services {
        if let Some(build) = service.get("build").filter(|build| !build.is_null()) {
            overrides.insert(name.clone(), json!({"build": build}));
        }
    }
    if overrides.is_empty() {
        Ok(None)
    } else {
        log::debug!(
            "Using Compose-directory build settings for {} Dev Container services",
            overrides.len()
        );
        Ok(Some(json!({"services": overrides})))
    }
}

fn execute<I, S>(
    configuration: &Configuration,
    args: I,
    override_config: Option<&Value>,
) -> Result<Vec<u8>, String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let mut command = Command::new("docker");
    command
        .arg("compose")
        .arg("--project-directory")
        .arg(&configuration.workspace);
    for file in &configuration.compose_files {
        command.arg("-f").arg(file);
    }
    if override_config.is_some() {
        command.args(["-f", "-"]);
    }
    command.args(args.into_iter().map(|arg| arg.as_ref().to_owned()));
    command
        .stdin(if override_config.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = command
        .spawn()
        .map_err(|error| format!("Could not run Docker Compose: {error}"))?;
    if let Some(value) = override_config {
        let result = child
            .stdin
            .take()
            .ok_or("Compose override input unavailable")
            .and_then(|mut input| {
                serde_json::to_writer(&mut input, value)
                    .map_err(|_| "Could not send Compose build override")
            });
        if let Err(error) = result {
            let _ = child.kill();
            let _ = child.wait();
            return Err(error.into());
        }
    }
    let output = child
        .wait_with_output()
        .map_err(|error| format!("Could not wait for Docker Compose: {error}"))?;
    if output.status.success() {
        Ok(output.stdout)
    } else {
        Err(diagnostics::compose_failure(&output))
    }
}
