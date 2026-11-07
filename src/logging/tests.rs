#[test]
fn logger_records_current_session_to_both_sinks_and_redacts() {
    if let Some(root) = std::env::var_os("FOCUS_LOG_SMOKE_ROOT") {
        let path = std::path::PathBuf::from(root).join("launch.log");
        super::init_at(&super::Arguments::default(), &path).unwrap();
        assert!(super::session::snapshot().is_empty());
        log::info!(target:"focus_editor::git_ui", "git-session-marker");
        super::process_line("focus_editor::dev_container::compose", "Building image");
        super::process_line("focus_editor::addons::example", "WARN TOKEN=do-not-expose");
        let mut child = std::process::Command::new("sh")
            .args([
                "-c",
                "printf 'live-compose-progress\n'; read ignored; printf 'finished\n'",
            ])
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .spawn()
            .unwrap();
        let capture = super::process::capture(child.stdout.take().unwrap(), true);
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
        while !super::session::snapshot()
            .iter()
            .any(|e| e.text.contains("live-compose-progress"))
            && std::time::Instant::now() < deadline
        {
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert!(!capture.is_finished());
        assert!(
            super::session::snapshot()
                .iter()
                .any(|e| e.text.contains("live-compose-progress"))
        );
        {
            use std::io::Write;
            child
                .stdin
                .take()
                .unwrap()
                .write_all(b"continue\n")
                .unwrap();
        }
        assert!(child.wait().unwrap().success());
        assert!(capture.join().unwrap().unwrap().ends_with(b"finished\n"));
        let records = super::session::snapshot();
        assert!(
            records
                .iter()
                .any(|e| e.source == super::session::Source::Git)
        );
        assert!(
            records
                .iter()
                .any(|e| e.source == super::session::Source::DockerCompose
                    && e.level == log::Level::Info)
        );
        assert!(
            records
                .iter()
                .any(|e| e.source == super::session::Source::Addons && e.level == log::Level::Warn)
        );
        assert!(records.iter().all(|e| !e.text.contains("do-not-expose")));
        let disk = std::fs::read_to_string(path).unwrap();
        assert!(!disk.contains("old-history"));
        assert!(!disk.contains("do-not-expose"));
        assert_eq!(disk.matches("git-session-marker").count(), 1);
        return;
    }
    let root = std::env::temp_dir().join(format!("focus-logger-smoke-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    for _ in 0..2 {
        std::fs::write(root.join("launch.log"), "old-history").unwrap();
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "logging::tests::logger_records_current_session_to_both_sinks_and_redacts",
                "--nocapture",
            ])
            .env("FOCUS_LOG_SMOKE_ROOT", &root)
            .env_remove("RUST_LOG")
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    std::fs::remove_dir_all(root).unwrap();
}

use super::*;
#[test]
fn version_flags_do_not_start_a_workspace_and_respect_separator() {
    for flag in ["--version", "-V"] {
        let parsed = Arguments::parse([flag.into()]).unwrap();
        assert!(parsed.version);
        assert!(parsed.workspace.is_none());
    }
    let parsed = Arguments::parse(["--".into(), "--version".into()]).unwrap();
    assert!(!parsed.version);
    assert_eq!(parsed.workspace, Some("--version".into()));
}
#[test]
fn levels_do_not_become_workspace_arguments() {
    for args in [
        vec!["--log-level", "debug", "project"],
        vec!["project", "--log-level=debug"],
        vec![
            "--foreground",
            "--log-level",
            "debug",
            "--no-addon=example",
            "--focus-editor-detached",
            "project",
        ],
    ] {
        let parsed = Arguments::parse(args.into_iter().map(Into::into)).unwrap();
        assert_eq!(parsed.level, Some(log::LevelFilter::Debug));
        assert_eq!(parsed.workspace, Some("project".into()));
    }
    assert!(Arguments::parse(["--log-level".into()]).is_err());
    assert!(Arguments::parse(["--log-level=oops".into()]).is_err());
    assert!(Arguments::parse(["--log-level=".into()]).is_err());
    assert_eq!(
        Arguments::parse(["--".into(), "--log-level=debug".into()])
            .unwrap()
            .workspace,
        Some("--log-level=debug".into())
    );
}
#[test]
fn precedence_and_quiet_dependency_defaults() {
    assert_eq!(filter(None, None), "warn,focus_editor=info");
    assert_eq!(
        filter(Some(log::LevelFilter::Off), None),
        "off,focus_editor=off"
    );
    assert_eq!(
        filter(Some(log::LevelFilter::Error), None),
        "error,focus_editor=error"
    );
    assert_eq!(
        filter(None, Some("focus_editor=trace,reqwest=debug".into())),
        "focus_editor=trace,reqwest=debug"
    );
    assert_eq!(
        filter(Some(log::LevelFilter::Debug), Some("off".into())),
        "warn,focus_editor=debug"
    );
}
#[test]
fn filters_enable_expected_levels_without_dependency_debug_noise() {
    use log::{Level, LevelFilter, Log, Metadata};
    let build = |level| {
        let mut builder = env_logger::Builder::new();
        builder.parse_filters(&filter(level, None));
        builder.build()
    };
    let default = build(None);
    assert!(
        default.enabled(
            &Metadata::builder()
                .target("focus_editor::editor")
                .level(Level::Info)
                .build()
        )
    );
    assert!(
        !default.enabled(
            &Metadata::builder()
                .target("focus_editor::editor")
                .level(Level::Debug)
                .build()
        )
    );
    assert!(
        !default.enabled(
            &Metadata::builder()
                .target("reqwest")
                .level(Level::Info)
                .build()
        )
    );
    let debug = build(Some(LevelFilter::Debug));
    assert!(
        debug.enabled(
            &Metadata::builder()
                .target("focus_editor::git_ui")
                .level(Level::Debug)
                .build()
        )
    );
    let off = build(Some(LevelFilter::Off));
    assert!(
        !off.enabled(
            &Metadata::builder()
                .target("focus_editor")
                .level(Level::Error)
                .build()
        )
    );
}
