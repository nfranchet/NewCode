use super::*;
use eframe::egui::{self, Event, Key, Modifiers};
use std::time::{Duration, Instant};

struct Session(Terminal);
impl Session {
    fn new(command: &str, cwd: Option<&Path>) -> Self {
        Self(Terminal::spawn(command, cwd, egui::Context::default()).unwrap())
    }
}
fn wait_for(terminal: &mut Terminal, expected: &str) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline {
        terminal.poll();
        if terminal.view.visible_text().unwrap().contains(expected) {
            return;
        }
        thread::sleep(Duration::from_millis(10));
    }
    panic!(
        "Missing {expected:?}: {}",
        terminal.view.visible_text().unwrap()
    );
}
#[derive(Clone, Default)]
struct Capture(Arc<Mutex<Vec<u8>>>);
impl Write for Capture {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[test]
fn configured_shell_selection_and_fallback() {
    let sh = PathBuf::from("/bin/sh");
    assert_eq!(select_shell(Some(sh.clone()), Some("/missing".into())), sh);
    assert_eq!(select_shell(Some("/missing".into()), Some(sh.clone())), sh);
    assert_eq!(select_shell(Some("relative".into()), None), sh);
    assert_eq!(select_shell(None, None), sh);
    assert!(account_shell().is_some());
}

#[test]
fn streaming_unicode_charset_selection_queries_and_alternate_screen() {
    let mut session = Session::new("exec /bin/cat", None);
    let capture = Capture::default();
    let output = session.0.capture_output(Box::new(capture.clone()));
    let view = &mut session.0.view;
    // Every byte arrives independently, including UTF-8 and ESC ( B from the reported artifact.
    for byte in "before\x1b(Bé✗界e\u{301}after".as_bytes() {
        output.send(vec![*byte]).unwrap();
        view.poll();
    }
    let text = view.visible_text().unwrap();
    assert!(text.starts_with("beforeé✗界"), "{text:?}");
    assert!(text.contains("after"));
    assert!(!text.contains('B'));
    output.send(b"\x1b[6n".to_vec()).unwrap();
    view.poll();
    let reply = capture.0.lock().unwrap().clone();
    assert!(reply.starts_with(b"\x1b[1;"));
    assert!(reply.ends_with(b"R"));
    output
        .send(b"\x1b[?1049h\x1b[2J\x1b[Hfullscreen".to_vec())
        .unwrap();
    view.poll();
    assert!(view.visible_text().unwrap().starts_with("fullscreen"));
    output.send(b"\x1b[?1049l".to_vec()).unwrap();
    view.poll();
    assert_eq!(view.visible_text().unwrap(), text);
}

#[test]
fn session_drop_reaps_child_even_when_output_queue_is_full() {
    let session = Session::new("exec /bin/sh -c 'while :; do printf output; done'", None);
    let pid = session.0.process_id().unwrap();
    thread::sleep(Duration::from_millis(80));
    let start = Instant::now();
    drop(session);
    assert!(start.elapsed() < Duration::from_secs(2));
    // SAFETY: signal 0 only queries whether our reaped child still exists.
    assert_eq!(unsafe { libc::kill(pid as i32, 0) }, -1);
}

#[test]
fn output_is_retained_after_shell_exit() {
    let mut session = Session::new("printf EXIT_MARKER", None);
    wait_for(&mut session.0, "EXIT_MARKER");
    let deadline = Instant::now() + Duration::from_secs(3);
    while !session.0.view.has_exited() && Instant::now() < deadline {
        session.0.poll();
        thread::sleep(Duration::from_millis(10));
    }
    assert!(session.0.view.has_exited());
    assert!(
        session
            .0
            .view
            .visible_text()
            .unwrap()
            .contains("EXIT_MARKER")
    );
}

#[test]
fn container_shell_script_resolves_an_interactive_tty_shell() {
    let command = format!(
        "exec /bin/sh -c '{}'",
        CONTAINER_SHELL.replace('\'', "'\"'\"'")
    );
    let mut session = Session::new(
        &format!("ZDOTDIR=/tmp/focus-no-zsh-config ENV=/dev/null {command}"),
        None,
    );
    session
        .0
        .send(b"test -t 0 && test -t 1 && printf 'TTY_%s_END\\n' yes\r");
    wait_for(&mut session.0, "TTY_yes_END");
}

#[test]
fn configured_shell_loads_startup_and_preserves_workspace() {
    const CHILD: &str = "FOCUS_SHELL_TEST_CHILD";
    if std::env::var_os(CHILD).is_some() {
        let root = PathBuf::from(std::env::var_os("FOCUS_SHELL_TEST_ROOT").unwrap());
        let mut session = Session::new("", Some(&root));
        // Interactive shells can discard typeahead while initializing their line editor.
        // The fixture prompt confirms that startup files have finished loading.
        wait_for(&mut session.0, "READY>");
        session
            .0
            .send(b"test \"$FOCUS_STARTUP_MARKER\" = loaded && test \"$PWD\" = \"$FOCUS_SHELL_TEST_ROOT\" && printf 'CONFIG_%s_END\\n' verified\r");
        wait_for(&mut session.0, "CONFIG_verified_END");
        return;
    }
    let root = std::env::temp_dir().join(format!("focus-shell-startup-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    // Ubuntu's /etc/zsh/zshrc runs compinit before the user's .zshrc and can
    // block on a security prompt for runner-owned completion directories.
    // Its documented opt-out must be set in .zshenv, which loads first.
    // This fixture tests user startup and cwd, independently of system completions.
    std::fs::write(root.join(".zshenv"), "skip_global_compinit=1\n").unwrap();
    std::fs::write(
        root.join(".bashrc"),
        "export FOCUS_STARTUP_MARKER=loaded\nPS1='READY> '\n",
    )
    .unwrap();
    std::fs::write(
        root.join(".zshrc"),
        "export FOCUS_STARTUP_MARKER=loaded\nPROMPT='READY> '\n",
    )
    .unwrap();
    for shell in ["/bin/bash", "/bin/zsh"]
        .into_iter()
        .filter(|shell| Path::new(shell).is_file())
    {
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "terminal::tests::configured_shell_loads_startup_and_preserves_workspace",
                "--nocapture",
            ])
            .env(CHILD, "1")
            .env("FOCUS_SHELL_TEST_ROOT", &root)
            .env("SHELL", shell)
            .env("ZDOTDIR", &root)
            .env("HOME", &root)
            .env_remove("FOCUS_STARTUP_MARKER")
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "Shell {shell} failed\nstdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
    std::fs::remove_dir_all(&root).unwrap();
}

#[test]
fn zsh_widget_completion_history_editing_and_interrupt() {
    if !Path::new("/bin/zsh").is_file() {
        return;
    }
    let root = std::env::temp_dir().join(format!("focus-zsh-widget-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(root.join("unique_completion_file.txt"), "").unwrap();
    let ctx = egui::Context::default();
    let mut session = Session::new("exec zsh -f", Some(&root));
    session.0.view.request_focus();
    let mut frame = |events: Vec<Event>| {
        crate::test_frame(
            &ctx,
            egui::RawInput {
                events,
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(800.0, 400.0),
                )),
                ..Default::default()
            },
            |ui| {
                session.0.view.ui(ui, &egui_tty::TerminalStyle::default());
            },
        )
        .drop_without_applying_deltas();
    };
    frame(vec![]);
    let key = |key, modifiers| Event::Key {
        key,
        physical_key: Some(key),
        pressed: true,
        repeat: false,
        modifiers,
    };
    frame(vec![Event::Text(
        "PROMPT='READY> '; RPROMPT=''; bindkey -e".into(),
    )]);
    frame(vec![key(Key::Enter, Modifiers::NONE)]);
    thread::sleep(Duration::from_millis(100));
    frame(vec![Event::Text("print -r -- unique_comp".into())]);
    frame(vec![key(Key::Tab, Modifiers::NONE)]);
    frame(vec![key(Key::Enter, Modifiers::NONE)]);
    wait_for(&mut session.0, "unique_completion_file.txt");
    // Separate frames allow the foreground shell to consume each command.
    crate::test_frame(&ctx, Default::default(), |ui| {
        session.0.view.ui(ui, &egui_tty::TerminalStyle::default());
    })
    .drop_without_applying_deltas();
    session.0.send(b"print -r -- EDIT_$((6*7)) discard\x17\r");
    wait_for(&mut session.0, "EDIT_42");
    session.0.send(b"sleep 30\r");
    thread::sleep(Duration::from_millis(100));
    session.0.send(&[3]);
    session.0.send(b"print -r -- INTERRUPT_$((6*7))\r");
    wait_for(&mut session.0, "INTERRUPT_42");
    session.0.send(b"\x1b[A\r");
    wait_for(&mut session.0, "INTERRUPT_42");
    drop(session);
    std::fs::remove_dir_all(root).unwrap();
}
