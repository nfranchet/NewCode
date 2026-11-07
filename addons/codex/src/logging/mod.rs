pub(crate) mod protocol;
pub(crate) mod redaction;
pub(crate) fn process_line(target: &str, line: &str) {
    for line in redaction::sanitize(line) {
        log::info!("{target}: {line}");
    }
}
