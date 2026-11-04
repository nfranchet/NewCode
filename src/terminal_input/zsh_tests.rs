//! End-to-end key encoding against a real, clean zsh PTY.
use super::{encode, tests::key};
use crate::terminal::Terminal;
use eframe::egui::{self, Event, Key, Modifiers};
use std::{
    path::PathBuf,
    time::{Duration, Instant},
};

struct Shell {
    terminal: Terminal,
    root: PathBuf,
}
impl Shell {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "focus-zsh-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir(&root).unwrap();
        std::fs::write(root.join("unique_completion_file.txt"), "").unwrap();
        let terminal =
            Terminal::spawn("exec zsh -f", Some(&root), egui::Context::default()).unwrap();
        Self { terminal, root }
    }
    fn event(&mut self, event: Event) {
        let bytes = encode(&event, Modifiers::NONE, self.terminal.input_modes);
        self.terminal.send(&bytes);
    }
    fn text(&mut self, text: &str) {
        self.event(Event::Text(text.into()));
    }
    fn press(&mut self, keycode: Key) {
        self.event(key(keycode, Modifiers::NONE));
    }
    fn wait_for(&mut self, expected: &str) -> String {
        let start = Instant::now();
        let mut output = String::new();
        while start.elapsed() < Duration::from_secs(5) {
            if let Ok(bytes) = self.terminal.rx.recv_timeout(Duration::from_millis(20)) {
                output.push_str(&String::from_utf8_lossy(&bytes));
                // Track these output modes just as Terminal::poll does. Keep all
                // output for assertions rather than consuming it into screen cells.
                for sequence in output.split('\x1b').skip(1) {
                    if let Some(end) = sequence.find(['h', 'l']) {
                        self.terminal.input_modes.update(&sequence[..=end]);
                    }
                }
                if output.contains(expected) {
                    return output;
                }
            }
        }
        panic!("zsh did not produce {expected:?}; output: {output:?}");
    }
}
impl Drop for Shell {
    fn drop(&mut self) {
        let _ = self.terminal._child.kill();
        let _ = self.terminal._child.wait();
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

#[test]
fn zsh_completion_history_editing_and_interrupt() {
    if !std::process::Command::new("zsh")
        .arg("--version")
        .output()
        .is_ok_and(|o| o.status.success())
    {
        eprintln!("Skipping optional zsh PTY test: zsh is not installed");
        return;
    }
    let mut shell = Shell::new();
    shell.text(
        "PROMPT='READY> '; RPROMPT=''; unsetopt PROMPT_SP; bindkey -e; print -r -- INIT_$((40+2))",
    );
    shell.press(Key::Enter);
    shell.wait_for("\r\nINIT_42\r\n");
    shell.text("print -r -- unique_comp");
    shell.press(Key::Tab);
    shell.press(Key::Enter);
    shell.wait_for("\r\nunique_completion_file.txt\r\n");
    shell.press(Key::ArrowUp);
    shell.press(Key::Enter);
    shell.wait_for("\r\nunique_completion_file.txt\r\n");
    // Ctrl+W must remove the last word without closing an editor tab.
    shell.text("print -r -- EDIT_$((6*7)) discard");
    shell.event(key(Key::W, Modifiers::CTRL));
    shell.press(Key::Enter);
    shell.wait_for("\r\nEDIT_42\r\n");
    shell.text("sleep 30");
    shell.press(Key::Enter);
    std::thread::sleep(Duration::from_millis(100));
    shell.event(Event::Copy); // Native Ctrl+C is delivered as Copy by egui-winit.
    shell.text("print -r -- INTERRUPT_$((6*7))");
    shell.press(Key::Enter);
    shell.wait_for("\r\nINTERRUPT_42\r\n");
}
