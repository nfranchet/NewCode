use super::*;

fn git_command(
    workspace: &Path,
    runtime: Option<&Runtime>,
    args: &[String],
    secret: Option<&str>,
    url: Option<&str>,
) -> Result<Command, String> {
    let mut env = vec![("GIT_TERMINAL_PROMPT".to_owned(), "0".to_owned())];
    if let (Some(secret), Some(url)) = (secret, url) {
        let parsed = reqwest::Url::parse(url).map_err(|_| "Invalid Git remote")?;
        if parsed.scheme() == "https" && parsed.username().is_empty() {
            let auth = base64::engine::general_purpose::STANDARD
                .encode(format!("x-access-token:{secret}"));
            env.extend([
                ("GIT_CONFIG_COUNT".into(), "1".into()),
                ("GIT_CONFIG_KEY_0".into(), format!("http.{url}.extraHeader")),
                (
                    "GIT_CONFIG_VALUE_0".into(),
                    format!("Authorization: Basic {auth}"),
                ),
            ]);
        }
    }
    let trace_variables = [
        "GIT_TRACE",
        "GIT_TRACE_CURL",
        "GIT_CURL_VERBOSE",
        "GIT_TRACE_PACKET",
        "GIT_TRACE2",
        "GIT_TRACE2_EVENT",
        "GIT_TRACE2_PERF",
    ];
    let mut command = if let Some(runtime) = runtime {
        let mut command = runtime.command_with_env("env", &env);
        for key in trace_variables {
            command.arg("-u").arg(key);
        }
        command
            .arg("git")
            .arg("-C")
            .arg(runtime.container_path(workspace)?);
        command
    } else {
        let mut command = Command::new("git");
        command.current_dir(workspace);
        for (key, value) in &env {
            command.env(key, value);
        }
        command
    };
    for key in trace_variables {
        command.env_remove(key);
    }
    command.args(args);
    Ok(command)
}
fn git(workspace: &Path, runtime: Option<&Runtime>, args: &[&str]) -> Result<String, String> {
    let args: Vec<_> = args.iter().map(|s| s.to_string()).collect();
    run(
        git_command(workspace, runtime, &args, None, None)?,
        None,
        None,
    )
    .result()
}
fn valid_branch(workspace: &Path, runtime: Option<&Runtime>, branch: &str) -> Result<(), String> {
    if branch.starts_with('-') || branch == "HEAD" {
        return Err("Choose a valid branch name".into());
    }
    git(
        workspace,
        runtime,
        &["check-ref-format", "--branch", branch],
    )
    .map(|_| ())
}
fn remote_url(workspace: &Path, runtime: Option<&Runtime>, remote: &str) -> Result<String, String> {
    if !git(workspace, runtime, &["remote"])?
        .lines()
        .any(|name| name == remote)
    {
        return Err("Choose a configured Git remote".into());
    }
    git(workspace, runtime, &["remote", "get-url", remote]).map(|s| s.trim().to_owned())
}
fn push_url(workspace: &Path, runtime: Option<&Runtime>, remote: &str) -> Result<String, String> {
    remote_url(workspace, runtime, remote)?;
    git(workspace, runtime, &["remote", "get-url", "--push", remote]).map(|s| s.trim().to_owned())
}
fn git_token(manifest: &Manifest, url: &str) -> Option<String> {
    let expected =
        reqwest::Url::parse(manifest.credential.as_ref()?.git_origin.as_deref()?).ok()?;
    let actual = reqwest::Url::parse(url).ok()?;
    (actual.scheme() == "https" && actual.origin() == expected.origin())
        .then(|| token(manifest))
        .flatten()
}

pub(super) fn execute(
    manifest: &Manifest,
    workspace: &Path,
    runtime: Option<&Runtime>,
    operation: GitOperation,
) -> ServiceReply {
    let perform = || -> Result<String, String> {
        let _lease = if operation.mutation() {
            Some(crate::repository_jobs::acquire(workspace)?)
        } else {
            None
        };
        let pushing = matches!(&operation, GitOperation::Push { .. });
        match operation {
            GitOperation::Read { args } => {
                let allowed = matches!(
                    args.first().map(String::as_str),
                    Some(
                        "rev-parse"
                            | "remote"
                            | "log"
                            | "status"
                            | "show-ref"
                            | "symbolic-ref"
                            | "worktree"
                            | "check-ref-format"
                    )
                );
                if !allowed {
                    return Err("Unsupported Git read operation".into());
                }
                // Avoid read commands' mutation flags (e.g. `remote set-url`, `worktree remove`).
                if args.first().is_some_and(|s| s == "remote")
                    && args.len() > 1
                    && args.get(1).map(String::as_str) != Some("get-url")
                {
                    return Err("Only remote listing/get-url is supported".into());
                }
                if args.first().is_some_and(|s| s == "worktree")
                    && args.get(1).map(String::as_str) != Some("list")
                {
                    return Err("Only worktree listing is supported".into());
                }
                if args.first().is_some_and(|s| s == "symbolic-ref")
                    && args != ["symbolic-ref", "--short", "HEAD"]
                {
                    return Err("Only current-branch queries are supported".into());
                }
                run(
                    git_command(workspace, runtime, &args, None, None)?,
                    None,
                    None,
                )
                .result()
            }
            GitOperation::Fetch { remote, branch } | GitOperation::Push { remote, branch } => {
                valid_branch(workspace, runtime, &branch)?;
                let url = if pushing {
                    push_url(workspace, runtime, &remote)?
                } else {
                    remote_url(workspace, runtime, &remote)?
                };
                let args = if pushing {
                    vec![
                        "push".into(),
                        "--set-upstream".into(),
                        "--".into(),
                        remote,
                        branch,
                    ]
                } else {
                    vec![
                        "fetch".into(),
                        "--".into(),
                        remote.clone(),
                        format!("refs/heads/{branch}:refs/remotes/{remote}/{branch}"),
                    ]
                };
                let secret = git_token(manifest, &url);
                run(
                    git_command(workspace, runtime, &args, secret.as_deref(), Some(&url))?,
                    None,
                    secret.as_deref(),
                )
                .result()
            }
            GitOperation::Checkout { branch, remote } => {
                valid_branch(workspace, runtime, &branch)?;
                remote_url(workspace, runtime, &remote)?;
                if !git(workspace, runtime, &["status", "--porcelain"])?
                    .trim()
                    .is_empty()
                {
                    return Err(
                        "Save and commit or stash worktree changes before switching branches"
                            .into(),
                    );
                }
                if git(
                    workspace,
                    runtime,
                    &["show-ref", "--verify", &format!("refs/heads/{branch}")],
                )
                .is_ok()
                {
                    git(workspace, runtime, &["switch", &branch])
                } else {
                    git(
                        workspace,
                        runtime,
                        &[
                            "switch",
                            "--track",
                            "-c",
                            &branch,
                            &format!("{remote}/{branch}"),
                        ],
                    )
                }
            }
            GitOperation::DeleteLocal {
                branch,
                expected,
                base,
            } => {
                valid_branch(workspace, runtime, &branch)?;
                valid_branch(workspace, runtime, &base)?;
                if branch == base {
                    return Err("The base branch cannot be deleted".into());
                }
                let reference = format!("refs/heads/{branch}");
                if git(workspace, runtime, &["rev-parse", "--verify", &reference])?.trim()
                    != expected
                {
                    return Err("Local branch contains different commits; kept it".into());
                }
                let current = git(workspace, runtime, &["symbolic-ref", "--short", "HEAD"])
                    .unwrap_or_default();
                if current.trim() == branch {
                    if !git(workspace, runtime, &["status", "--porcelain"])?
                        .trim()
                        .is_empty()
                    {
                        return Err("Worktree has changes; branch cleanup is pending".into());
                    }
                    git(workspace, runtime, &["switch", &base])?;
                }
                let worktrees = git(workspace, runtime, &["worktree", "list", "--porcelain"])?;
                if worktrees
                    .lines()
                    .any(|line| line == format!("branch {reference}"))
                {
                    return Err("Branch is checked out in another worktree; kept it".into());
                }
                git(
                    workspace,
                    runtime,
                    &["update-ref", "-d", &reference, &expected],
                )
            }
            GitOperation::DeleteRemote {
                remote,
                branch,
                expected,
            } => {
                valid_branch(workspace, runtime, &branch)?;
                let url = push_url(workspace, runtime, &remote)?;
                let args = vec![
                    "push".into(),
                    format!("--force-with-lease=refs/heads/{branch}:{expected}"),
                    "--".into(),
                    remote,
                    format!(":refs/heads/{branch}"),
                ];
                let secret = git_token(manifest, &url);
                run(
                    git_command(workspace, runtime, &args, secret.as_deref(), Some(&url))?,
                    None,
                    secret.as_deref(),
                )
                .result()
            }
        }
    };
    perform()
        .map(ServiceReply::success)
        .unwrap_or_else(|e| ServiceReply::error("git", e))
}
