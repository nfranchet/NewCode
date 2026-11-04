//! Logging configuration and launch argument parsing. Never accept content payloads.
use std::{ffi::OsString, io::Write, path::PathBuf, str::FromStr};

#[derive(Default, Debug)]
pub(crate) struct Arguments {
    pub(crate) level: Option<log::LevelFilter>,
    pub(crate) workspace: Option<PathBuf>,
}
impl Arguments {
    pub(crate) fn parse(args: impl IntoIterator<Item = OsString>) -> Result<Self, String> {
        let mut result = Self::default();
        let mut args = args.into_iter();
        let mut positional = false;
        while let Some(arg) = args.next() {
            if !positional && arg == "--" {
                positional = true;
                continue;
            }
            let text = arg.to_string_lossy();
            let value = if !positional && arg == "--log-level" {
                Some(
                    args.next()
                        .ok_or("--log-level requires trace, debug, info, warn, error, or off")?,
                )
            } else if !positional && text.starts_with("--log-level=") {
                Some(OsString::from(&text[12..]))
            } else {
                None
            };
            if let Some(value) = value {
                let value = value.to_str().ok_or("Invalid --log-level value")?;
                result.level = Some(log::LevelFilter::from_str(value).map_err(
                    |_| "Invalid --log-level; use trace, debug, info, warn, error, or off",
                )?);
            } else if (positional || !text.starts_with('-')) && result.workspace.is_none() {
                result.workspace = Some(arg.into());
            }
        }
        Ok(result)
    }
}
fn filter(level: Option<log::LevelFilter>, environment: Option<String>) -> String {
    if let Some(level) = level {
        format!(
            "{},focus_editor={}",
            level.min(log::LevelFilter::Warn).as_str().to_lowercase(),
            level.as_str().to_lowercase()
        )
    } else {
        environment
            .filter(|v| !v.trim().is_empty())
            .unwrap_or_else(|| "warn,focus_editor=info".into())
    }
}
pub(crate) fn init(arguments: &Arguments) -> Result<(), Box<dyn std::error::Error>> {
    let filter = filter(arguments.level, std::env::var("RUST_LOG").ok());
    let mut logger = env_logger::Builder::new();
    logger
        .parse_filters(&filter)
        .target(env_logger::Target::Stderr)
        .format(|buffer, record| {
            writeln!(
                buffer,
                "{} {:5} pid={} [{}] {}",
                buffer.timestamp_millis(),
                record.level(),
                std::process::id(),
                record.target(),
                record.args()
            )
        });
    logger.try_init()?;
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn levels_do_not_become_workspace_arguments() {
        for args in [
            vec!["--log-level", "debug", "project"],
            vec!["project", "--log-level=debug"],
            vec![
                "--foreground",
                "--log-level",
                "debug",
                "--no-codex",
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
}
