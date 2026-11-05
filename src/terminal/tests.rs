use super::*;
use std::time::{Duration, Instant};

struct Session(Terminal);
impl Session {
    fn new(command: &str, cwd: Option<&Path>) -> Self {
        Self(Terminal::spawn(command, cwd, eframe::egui::Context::default()).unwrap())
    }
}
impl Drop for Session {
    fn drop(&mut self) {
        let _ = self.0._child.kill();
        let _ = self.0._child.wait();
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
fn output_preserves_split_unicode_and_handles_invalid_bytes() {
    let mut session = Session::new("exec /bin/cat", None);
    let t = &mut session.0;
    t.receive(&[0xc3]);
    assert_eq!(t.cursor_x, 0);
    t.receive(&[0xa9, 0xff, b'A']);
    assert_eq!(t.cells[0][0].ch, 'é');
    assert_eq!(t.cells[0][1].ch, '\u{fffd}');
    assert_eq!(t.cells[0][2].ch, 'A');
    assert!(t.pending_utf8.is_empty());
}

#[test]
fn wide_combining_overwrite_wrap_and_erase() {
    let mut session = Session::new("exec /bin/cat", None);
    let t = &mut session.0;
    t.receive("界e\u{301}".as_bytes());
    assert_eq!(t.cursor_x, 3);
    assert_eq!(t.cells[0][0].width, 2);
    assert_eq!(t.cells[0][1].width, 0);
    assert_eq!(t.cells[0][2].combining, "\u{301}");
    t.receive(b"\rX");
    assert_eq!(t.cells[0][1].width, 1);
    t.receive(b"\x1b[80G");
    t.receive("界".as_bytes());
    assert_eq!((t.cursor_x, t.cursor_y), (2, 1));
    t.receive(b"\x1b[80Gx\x1b[K\x1b[1K");
    assert_eq!(t.cells[1][79].ch, ' ');
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
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut output = Vec::new();
    while Instant::now() < deadline {
        if let Ok(bytes) = session.0.rx.recv_timeout(Duration::from_millis(20)) {
            output.extend(bytes);
            if String::from_utf8_lossy(&output).contains("TTY_yes_END") {
                return;
            }
        }
    }
    panic!("No TTY confirmation: {}", String::from_utf8_lossy(&output));
}

#[test]
fn configured_shell_loads_startup_and_preserves_workspace() {
    const CHILD: &str = "FOCUS_SHELL_TEST_CHILD";
    if std::env::var_os(CHILD).is_some() {
        let root = PathBuf::from(std::env::var_os("FOCUS_SHELL_TEST_ROOT").unwrap());
        let mut session = Session::new("", Some(&root));
        session
            .0
            .send(b"printf 'CONFIG_%s_DIR_%s_END\\n' \"$FOCUS_STARTUP_MARKER\" \"$PWD\"\r");
        let expected = format!("CONFIG_loaded_DIR_{}_END", root.display());
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut output = Vec::new();
        while Instant::now() < deadline {
            if let Ok(bytes) = session.0.rx.recv_timeout(Duration::from_millis(20)) {
                output.extend(bytes);
                if String::from_utf8_lossy(&output).contains(&expected) {
                    return;
                }
            }
        }
        panic!(
            "configured startup/cwd not observed: {}",
            String::from_utf8_lossy(&output)
        );
    }
    let root = std::env::temp_dir().join(format!("focus-shell-startup-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
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
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stdout)
        );
    }
    std::fs::remove_dir_all(&root).unwrap();
}

#[test]
fn prompt_insert_delete_and_wide_resize_keep_cell_positions() {
    let mut session = Session::new("exec /bin/cat", None);
    let t = &mut session.0;
    t.receive(b"abc\r\x1b[C\x1b[@X");
    assert_eq!(
        t.cells[0][..4].iter().map(|c| c.ch).collect::<String>(),
        "aXbc"
    );
    t.receive(b"\r\x1b[C\x1b[P");
    assert_eq!(
        t.cells[0][..3].iter().map(|c| c.ch).collect::<String>(),
        "abc"
    );
    t.receive("\r界".as_bytes());
    t.receive(b"\x1b[2G\x1b[K");
    assert_eq!(t.cells[0][0].ch, ' ');
    t.receive("\rA界".as_bytes());
    t.resize(2, 24);
    assert_eq!(t.cells[0][1].width, 1);
    assert_eq!(t.cells[0][1].ch, ' ');
}

#[test]
fn escape_intermediates_never_leak_into_terminal_text() {
    let mut session = Session::new("exec /bin/cat", None);
    let t = &mut session.0;
    for sequence in [
        b"\x1b(B".as_slice(),
        b"\x1b)B",
        b"\x1b*B",
        b"\x1b+B",
        b"\x1b-A",
        b"\x1b.A",
        b"\x1b/A",
        b"\x1b(0",
        b"\x1b#8",
        b"\x1b%G",
        b"\x1b F",
        b"\x1b !0",
    ] {
        for split in 0..=sequence.len() {
            t.receive(b"\x1b[2J\x1b[Hbefore");
            t.receive(&sequence[..split]);
            t.receive(&sequence[split..]);
            assert_eq!((t.cursor_x, t.cursor_y), (6, 0), "{sequence:?} at {split}");
            t.receive(b"\x1b[31mafter\x1b[0m");
            assert_eq!(
                t.cells[0][..11]
                    .iter()
                    .map(|cell| cell.ch)
                    .collect::<String>(),
                "beforeafter",
                "{sequence:?} at {split}"
            );
            assert_eq!(t.cells[0][6].fg, ansi_color(1, false));
            assert_eq!((t.cursor_x, t.cursor_y), (11, 0));
            assert!(!t.in_escape);
        }
    }
}

#[test]
fn incomplete_escape_sequences_can_be_replaced_or_cancelled() {
    let mut session = Session::new("exec /bin/cat", None);
    let t = &mut session.0;
    for bytes in [b"\x1b(\x1b[31mX".as_slice(), b"\x1b(\x18X", b"\x1b(\x1aX"] {
        t.receive(b"\x1b[2J\x1b[H");
        for byte in bytes {
            t.receive(&[*byte]);
        }
        assert_eq!(t.cells[0][0].ch, 'X');
        assert_eq!((t.cursor_x, t.cursor_y), (1, 0));
        assert!(!t.in_escape);
    }
}
