//! Host services. Credentials never enter the executable-addon protocol.
mod git;
use crate::dev_container::Runtime;
use base64::Engine;
use focus_addon_sdk::{GitOperation, Manifest, Service, ServiceReply};
use std::{
    io::{Read, Write},
    path::Path,
    process::{Command, Stdio},
    time::{Duration, Instant},
};

pub(super) fn entry(manifest: &Manifest) -> Result<keyring::Entry, String> {
    let credential = manifest
        .credential
        .as_ref()
        .ok_or("This addon does not declare credentials")?;
    keyring::Entry::new(
        &format!("focus-editor.addon.{}", manifest.id),
        &credential.origin,
    )
    .map_err(|_| "The OS credential store is unavailable or locked. Unlock it and retry.".into())
}
fn token(manifest: &Manifest) -> Option<String> {
    entry(manifest).ok()?.get_password().ok()
}

pub(super) fn execute(
    manifest: &Manifest,
    workspace: Option<&Path>,
    runtime: Option<&Runtime>,
    service: Service,
) -> ServiceReply {
    let started = Instant::now();
    let reply = match service {
        Service::Run {
            program,
            args,
            input,
            credential,
        } => {
            let mut command = Command::new(&program);
            command.args(&args);
            if let Some(workspace) = workspace {
                command.current_dir(workspace);
            }
            command
                .env("GH_PROMPT_DISABLED", "1")
                .env("GH_PAGER", "cat")
                .env_remove("GH_DEBUG");
            let secret = if credential {
                match &manifest.credential {
                    Some(spec) if program == spec.program => token(manifest),
                    _ => {
                        return ServiceReply::error(
                            "credential_scope",
                            "This tool cannot receive the addon's credential",
                        );
                    }
                }
            } else {
                None
            };
            if let (Some(spec), Some(secret)) = (&manifest.credential, &secret) {
                command.env(&spec.environment, secret);
            }
            run(command, input, secret.as_deref())
        }
        Service::Http {
            method,
            url,
            body,
            credential,
        } => http(manifest, &method, &url, body, credential),
        Service::Git { operation } => match workspace {
            Some(workspace) => git::execute(manifest, workspace, runtime, operation),
            None => ServiceReply::error("workspace", "Open a workspace first"),
        },
    };
    log::debug!(
        "Addon service addon={} success={} elapsed_ms={}",
        manifest.id,
        reply.ok,
        started.elapsed().as_millis()
    );
    reply
}

fn http(
    manifest: &Manifest,
    method: &str,
    url: &str,
    body: Option<serde_json::Value>,
    credential: bool,
) -> ServiceReply {
    let request = || -> Result<ServiceReply, String> {
        let url = reqwest::Url::parse(url).map_err(|_| "Invalid request URL")?;
        if url.scheme() != "https" || !url.username().is_empty() || url.password().is_some() {
            return Err("Requests require HTTPS without credentials in the URL".into());
        }
        let secret = if credential {
            let spec = manifest
                .credential
                .as_ref()
                .ok_or("No credential origin declared")?;
            let origin =
                reqwest::Url::parse(&spec.origin).map_err(|_| "Invalid credential origin")?;
            if url.origin() != origin.origin() {
                return Err("Credential destination does not match the addon manifest".into());
            }
            token(manifest)
        } else {
            None
        };
        let client = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(60))
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|_| "Cannot initialize HTTPS")?;
        let mut request = client
            .request(method.parse().map_err(|_| "Invalid HTTP method")?, url)
            .header("User-Agent", "focus-editor-addons/1")
            .header("Accept", "application/json");
        if let Some(secret) = &secret {
            request = request.bearer_auth(secret);
        }
        if let Some(body) = body {
            request = request.json(&body);
        }
        let response = request
            .send()
            .map_err(|_| "HTTPS request failed. Check the remote state before retrying a write.")?;
        let status = response.status();
        let limited = status.as_u16() == 429
            || (status.as_u16() == 403
                && response
                    .headers()
                    .get("x-ratelimit-remaining")
                    .is_some_and(|v| v == "0"));
        let retry = response
            .headers()
            .get("retry-after")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.parse::<u64>().ok())
            .or_else(|| {
                response
                    .headers()
                    .get("x-ratelimit-reset")?
                    .to_str()
                    .ok()?
                    .parse::<u64>()
                    .ok()
                    .map(|reset| {
                        reset
                            .saturating_sub(
                                std::time::SystemTime::now()
                                    .duration_since(std::time::UNIX_EPOCH)
                                    .unwrap_or_default()
                                    .as_secs(),
                            )
                            .max(1)
                    })
            })
            .unwrap_or(60);
        let mut bytes = Vec::new();
        response
            .take((focus_addon_sdk::MAX_MESSAGE + 1) as u64)
            .read_to_end(&mut bytes)
            .map_err(|_| "Cannot read HTTPS response")?;
        if bytes.len() > focus_addon_sdk::MAX_MESSAGE {
            return Err("Remote response exceeds the addon size limit".into());
        }
        let text = scrub(&String::from_utf8_lossy(&bytes), secret.as_deref());
        Ok(if limited {
            ServiceReply::error(
                "rate_limit",
                format!("Rate limit reached. Retry after {retry} seconds."),
            )
        } else if status.is_success() {
            ServiceReply::success(text)
        } else {
            ServiceReply::error("http", format!("HTTP {status}: {text}"))
        })
    };
    request().unwrap_or_else(|error| ServiceReply::error("http", error))
}

fn scrub(text: &str, secret: Option<&str>) -> String {
    let mut text = text.to_owned();
    if let Some(secret) = secret.filter(|s| !s.is_empty()) {
        text = text.replace(secret, "[redacted]");
        let basic =
            base64::engine::general_purpose::STANDARD.encode(format!("x-access-token:{secret}"));
        text = text.replace(&basic, "[redacted]");
    }
    for secret in crate::logging::redaction::environment_secrets() {
        text = text.replace(&secret, "[redacted]");
    }
    text
}

fn run(mut command: Command, input: Option<String>, secret: Option<&str>) -> ServiceReply {
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        unsafe {
            command.pre_exec(|| {
                if libc::setpgid(0, 0) == -1 {
                    Err(std::io::Error::last_os_error())
                } else {
                    Ok(())
                }
            });
        }
    }
    let mut child = match command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
    {
        Ok(child) => child,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return ServiceReply::error("missing_tool", "Required executable is not installed");
        }
        Err(_) => return ServiceReply::error("spawn", "Cannot start the requested executable"),
    };
    let process_group = child.id();
    let stdin = child.stdin.take();
    let input_thread = std::thread::spawn(move || {
        if let (Some(mut stdin), Some(input)) = (stdin, input) {
            let _ = stdin.write_all(input.as_bytes());
        }
    });
    fn capture(stream: impl Read + Send + 'static) -> std::thread::JoinHandle<Vec<u8>> {
        std::thread::spawn(move || {
            let mut bytes = Vec::new();
            let _ = stream
                .take((focus_addon_sdk::MAX_MESSAGE + 1) as u64)
                .read_to_end(&mut bytes);
            bytes
        })
    }
    let stdout = capture(child.stdout.take().unwrap());
    let stderr = capture(child.stderr.take().unwrap());
    let start = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Some(status),
            Ok(None) if start.elapsed() < Duration::from_secs(120) => {
                std::thread::sleep(Duration::from_millis(25))
            }
            _ => {
                #[cfg(unix)]
                unsafe {
                    libc::kill(-(process_group as i32), libc::SIGKILL);
                }
                let _ = child.kill();
                let _ = child.wait();
                break None;
            }
        }
    };
    // Helpers retaining inherited pipes must not keep a finished job's readers alive.
    #[cfg(unix)]
    unsafe {
        libc::kill(-(process_group as i32), libc::SIGKILL);
    }
    let _ = input_thread.join();
    let out = stdout.join().unwrap_or_default();
    let err = stderr.join().unwrap_or_default();
    if out.len() > focus_addon_sdk::MAX_MESSAGE || err.len() > focus_addon_sdk::MAX_MESSAGE {
        return ServiceReply::error("size", "Tool output exceeded the addon size limit");
    }
    match status {
        Some(status) if status.success() => {
            ServiceReply::success(scrub(&String::from_utf8_lossy(&out), secret))
        }
        Some(_) => ServiceReply::error("command", scrub(&String::from_utf8_lossy(&err), secret)),
        None => ServiceReply::error(
            "timeout",
            "Operation timed out. Check the remote state before retrying a write.",
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn redacts_raw_and_encoded_tokens() {
        let secret = "test-private-value";
        let encoded =
            base64::engine::general_purpose::STANDARD.encode(format!("x-access-token:{secret}"));
        let result = scrub(&format!("{secret} {encoded}"), Some(secret));
        assert_eq!(result, "[redacted] [redacted]");
    }
    #[test]
    fn refuses_credential_forwarding_to_a_different_origin() {
        let manifest = Manifest {
            id: "fixture".into(),
            name: "Fixture".into(),
            version: "1".into(),
            protocol: 1,
            executable: "program".into(),
            icon: "icon.png".into(),
            credential: Some(focus_addon_sdk::Credential {
                origin: "https://api.github.com".into(),
                program: "gh".into(),
                environment: "GH_TOKEN".into(),
                git_origin: Some("https://github.com".into()),
                instructions: String::new(),
            }),
            ..Default::default()
        };
        let reply = execute(
            &manifest,
            None,
            None,
            Service::Http {
                method: "GET".into(),
                url: "https://other.example.invalid/".into(),
                body: None,
                credential: true,
            },
        );
        assert!(!reply.ok);
        assert!(reply.output.contains("does not match"));
        let reply = execute(
            &manifest,
            None,
            None,
            Service::Run {
                program: "other-tool".into(),
                args: vec![],
                input: None,
                credential: true,
            },
        );
        assert_eq!(reply.code, "credential_scope");
    }
}
