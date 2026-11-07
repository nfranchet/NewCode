use crate::dev_container::Runtime;
pub(crate) const ENV: &[&str] = &[
    "ANTHROPIC_API_KEY",
    "ANTHROPIC_AUTH_TOKEN",
    "ANTHROPIC_BASE_URL",
    "OPENAI_API_KEY",
    "OPENAI_BASE_URL",
    "GEMINI_API_KEY",
    "GOOGLE_API_KEY",
    "OPENCODE_API_KEY",
    "OPENCODE_SERVER_PASSWORD",
    "OPENCODE_SERVER_USERNAME",
];
pub(crate) fn prepare(runtime: &Runtime) -> Result<(), String> {
    let home = std::env::var_os("HOME")
        .map(std::path::PathBuf::from)
        .ok_or("Cannot locate host home")?;
    focus_addon_support::authentication::copy_json(
        runtime,
        &(std::env::var_os("XDG_DATA_HOME")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|| home.join(".local/share"))
            .join("opencode/auth.json")),
        "${XDG_DATA_HOME:-$home/.local/share}/opencode",
    )
}
