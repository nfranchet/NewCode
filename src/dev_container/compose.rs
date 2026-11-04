//! Compose execution and runtime-only Dockerfile path resolution.
use super::{Configuration, diagnostics};
use serde_json::{Value, json};
use std::{
    path::Path,
    process::{Command, Stdio},
};

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
    let bytes = compose(configuration, ["config", "--format", "json"])?;
    let resolved: Value = serde_json::from_slice(&bytes)
        .map_err(|_| "Could not parse resolved Docker Compose configuration")?;
    let override_config = dockerfile_override(configuration, &resolved)?;
    execute(configuration, args, override_config.as_ref())
}

fn dockerfile_override(
    configuration: &Configuration,
    resolved: &Value,
) -> Result<Option<Value>, String> {
    let service = &resolved["services"][&configuration.service];
    if service.is_null() {
        return Err(format!(
            "Compose service {} is not defined",
            configuration.service
        ));
    }
    let build = &service["build"];
    if build.is_null() || build.get("dockerfile_inline").is_some() {
        return Ok(None);
    }
    let context = build["context"]
        .as_str()
        .map(Path::new)
        .ok_or("Docker Compose did not resolve the build context")?;
    // Remote Git/URL contexts are resolved by Docker, never against the host.
    if !context.is_dir() {
        return Ok(None);
    }
    let dockerfile = build["dockerfile"].as_str().unwrap_or("Dockerfile");
    if context.join(dockerfile).is_file() {
        return Ok(None);
    }
    let compose_directory = configuration
        .compose_files
        .first()
        .and_then(|file| file.parent());
    let configured = compose_directory.map(|directory| directory.join(dockerfile));
    let mut path = configured.filter(|path| path.is_file());
    if path.is_none() && matches!(dockerfile, "Dockerfile" | ".Dockerfile" | "./Dockerfile") {
        path = compose_directory
            .map(|directory| directory.join("Dockerfile"))
            .filter(|path| path.is_file())
            .or_else(|| {
                let path = configuration.workspace.join(".devcontainer/Dockerfile");
                path.is_file().then_some(path)
            });
        if path.is_some() {
            log::warn!(
                "Configured Dockerfile was not found; using the existing Dev Container Dockerfile for service {}",
                configuration.service
            );
        }
    }
    let path = path.ok_or_else(|| {
        format!("Dockerfile {dockerfile} was not found in the build context or Compose directory")
    })?;
    let path = path
        .canonicalize()
        .map_err(|error| format!("Cannot resolve Dockerfile: {error}"))?;
    log::debug!(
        "Using Compose-directory Dockerfile for Dev Container service {}",
        configuration.service
    );
    Ok(Some(
        json!({"services": {&configuration.service: {"build": {"dockerfile": path}}}}),
    ))
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
                    .map_err(|_| "Could not send Compose Dockerfile override")
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
