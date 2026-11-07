use crate::dev_container::Runtime;
pub(crate) const ENV: &[&str] = &[
    "OPENAI_API_KEY",
    "OPENAI_BASE_URL",
    "OPENAI_ORG_ID",
    "OPENAI_PROJECT_ID",
];
pub(crate) fn prepare(runtime: &Runtime) -> Result<(), String> {
    let home = std::env::var_os("HOME")
        .map(std::path::PathBuf::from)
        .ok_or("Cannot locate host home")?;
    focus_addon_support::authentication::copy_json(
        runtime,
        &(std::env::var_os("CODEX_HOME")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|| home.join(".codex"))
            .join("auth.json")),
        "${CODEX_HOME:-$home/.codex}",
    )
}
