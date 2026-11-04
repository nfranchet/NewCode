use super::*;

pub(crate) fn key(key: Key, modifiers: Modifiers) -> Event {
    Event::Key {
        key,
        physical_key: Some(key),
        pressed: true,
        repeat: false,
        modifiers,
    }
}
fn bytes(keycode: Key, modifiers: Modifiers) -> Vec<u8> {
    encode(&key(keycode, modifiers), modifiers, InputModes::default())
}

#[test]
fn special_and_modified_keys_use_xterm_sequences() {
    for (keycode, expected) in [
        (Key::Tab, b"\t".as_slice()),
        (Key::Enter, b"\r"),
        (Key::Escape, b"\x1b"),
        (Key::Backspace, b"\x7f"),
        (Key::ArrowUp, b"\x1b[A"),
        (Key::ArrowDown, b"\x1b[B"),
        (Key::ArrowRight, b"\x1b[C"),
        (Key::ArrowLeft, b"\x1b[D"),
        (Key::Home, b"\x1b[H"),
        (Key::End, b"\x1b[F"),
        (Key::Insert, b"\x1b[2~"),
        (Key::Delete, b"\x1b[3~"),
        (Key::PageUp, b"\x1b[5~"),
        (Key::PageDown, b"\x1b[6~"),
        (Key::F1, b"\x1bOP"),
        (Key::F2, b"\x1bOQ"),
        (Key::F3, b"\x1bOR"),
        (Key::F4, b"\x1bOS"),
        (Key::F5, b"\x1b[15~"),
        (Key::F6, b"\x1b[17~"),
        (Key::F7, b"\x1b[18~"),
        (Key::F8, b"\x1b[19~"),
        (Key::F9, b"\x1b[20~"),
        (Key::F10, b"\x1b[21~"),
        (Key::F11, b"\x1b[23~"),
        (Key::F12, b"\x1b[24~"),
    ] {
        assert_eq!(bytes(keycode, Modifiers::NONE), expected, "{keycode:?}");
    }
    assert_eq!(bytes(Key::Tab, Modifiers::SHIFT), b"\x1b[Z");
    assert_eq!(bytes(Key::ArrowLeft, Modifiers::CTRL), b"\x1b[1;5D");
    assert_eq!(bytes(Key::ArrowRight, Modifiers::ALT), b"\x1b[1;3C");
    assert_eq!(
        bytes(Key::Delete, Modifiers::CTRL | Modifiers::SHIFT),
        b"\x1b[3;6~"
    );
    assert_eq!(bytes(Key::F2, Modifiers::ALT), b"\x1b[1;3Q");
    assert_eq!(bytes(Key::Backspace, Modifiers::ALT), b"\x1b\x7f");
}

#[test]
fn control_keys_clipboard_translation_and_repeats() {
    for letter in Key::ALL
        .iter()
        .copied()
        .filter(|key| (Key::A..=Key::Z).contains(key))
    {
        let expected = if letter == Key::V {
            vec![]
        } else {
            vec![letter as u8 - Key::A as u8 + 1]
        };
        assert_eq!(bytes(letter, Modifiers::CTRL), expected);
    }
    for (keycode, code) in [
        (Key::Space, 0),
        (Key::OpenBracket, 27),
        (Key::Backslash, 28),
        (Key::CloseBracket, 29),
        (Key::Num6, 30),
        (Key::Minus, 31),
        (Key::Questionmark, 127),
    ] {
        assert_eq!(bytes(keycode, Modifiers::CTRL), [code]);
    }
    let modes = InputModes::default();
    assert_eq!(encode(&Event::Copy, Modifiers::CTRL, modes), [3]);
    assert_eq!(encode(&Event::Cut, Modifiers::CTRL, modes), [24]);
    assert_eq!(
        encode(&Event::Copy, Modifiers::CTRL | Modifiers::SHIFT, modes),
        [3]
    );
    assert_eq!(
        encode(&Event::Copy, Modifiers::CTRL | Modifiers::ALT, modes),
        [27, 3]
    );
    let mut event = key(Key::Tab, Modifiers::NONE);
    if let Event::Key { repeat, .. } = &mut event {
        *repeat = true;
    }
    assert_eq!(encode(&event, Modifiers::NONE, modes), b"\t");
    if let Event::Key { pressed, .. } = &mut event {
        *pressed = false;
    }
    assert!(encode(&event, Modifiers::NONE, modes).is_empty());
}

#[test]
fn text_is_not_duplicated_and_paste_obeys_requested_modes() {
    let mut modes = InputModes::default();
    assert!(bytes(Key::A, Modifiers::NONE).is_empty());
    assert!(bytes(Key::A, Modifiers::ALT).is_empty());
    let text = Event::Text("é日本".into());
    assert_eq!(encode(&text, Modifiers::NONE, modes), "é日本".as_bytes());
    assert_eq!(
        encode(&Event::Text("b".into()), Modifiers::ALT, modes),
        b"\x1bb"
    );
    assert_eq!(
        encode(
            &Event::Text("@".into()),
            Modifiers::CTRL | Modifiers::ALT,
            modes
        ),
        b"@"
    );
    let paste = Event::Paste("a\r\nb\rc\n".into());
    assert_eq!(encode(&paste, Modifiers::CTRL, modes), b"a\rb\rc\r");
    modes.update("[?1;2004h");
    assert!(modes.application_cursor && modes.bracketed_paste);
    assert_eq!(
        encode(&key(Key::ArrowUp, Modifiers::NONE), Modifiers::NONE, modes),
        b"\x1bOA"
    );
    assert_eq!(
        encode(&paste, Modifiers::CTRL, modes),
        b"\x1b[200~a\nb\nc\n\x1b[201~"
    );
    modes.update("[?1;2004l");
    assert!(!modes.application_cursor && !modes.bracketed_paste);
    assert_eq!(
        encode(&key(Key::ArrowUp, Modifiers::NONE), Modifiers::NONE, modes),
        b"\x1b[A"
    );
}
