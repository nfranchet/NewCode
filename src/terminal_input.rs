//! Encode GUI input as traditional xterm input without duplicating text events.
use eframe::egui::{Event, Key, Modifiers};

#[derive(Clone, Copy, Default)]
pub(crate) struct InputModes {
    pub(crate) application_cursor: bool,
    pub(crate) bracketed_paste: bool,
}

impl InputModes {
    pub(crate) fn update(&mut self, sequence: &str) {
        let Some(body) = sequence.strip_prefix("[?") else {
            return;
        };
        let enabled = match body.as_bytes().last() {
            Some(b'h') => true,
            Some(b'l') => false,
            _ => return,
        };
        for mode in body[..body.len() - 1].split(';') {
            match mode {
                "1" => self.application_cursor = enabled,
                "2004" => self.bracketed_paste = enabled,
                _ => {}
            }
        }
    }
}

pub(crate) fn owns(event: &Event) -> bool {
    matches!(
        event,
        Event::Text(_)
            | Event::Paste(_)
            | Event::Copy
            | Event::Cut
            | Event::Key { .. }
            | Event::Ime(_)
    )
}

pub(crate) fn encode(event: &Event, modifiers: Modifiers, modes: InputModes) -> Vec<u8> {
    match event {
        Event::Text(text) => {
            // AltGr produces text on international layouts, rather than a Meta chord.
            if modifiers.alt && !modifiers.ctrl {
                prefixed(text.as_bytes())
            } else {
                text.as_bytes().to_vec()
            }
        }
        Event::Ime(eframe::egui::ImeEvent::Commit(text)) => text.as_bytes().to_vec(),
        Event::Paste(text) => {
            let text = text.replace("\r\n", "\n").replace('\r', "\n");
            if modes.bracketed_paste {
                // A pasted closing delimiter must not escape bracketed-paste mode.
                let text = text.replace('\x1b', "");
                format!("\x1b[200~{text}\x1b[201~").into_bytes()
            } else {
                text.replace('\n', "\r").into_bytes()
            }
        }
        // egui-winit translates Ctrl+C/X into these events, with no key press.
        Event::Copy => key_bytes(
            Key::C,
            Modifiers {
                ctrl: true,
                ..modifiers
            },
            modes,
        ),
        Event::Cut => key_bytes(
            Key::X,
            Modifiers {
                ctrl: true,
                ..modifiers
            },
            modes,
        ),
        Event::Key {
            key,
            pressed: true,
            modifiers,
            ..
        } => key_bytes(*key, *modifiers, modes),
        _ => Vec::new(),
    }
}

fn prefixed(bytes: &[u8]) -> Vec<u8> {
    let mut result = vec![0x1b];
    result.extend_from_slice(bytes);
    result
}

fn key_bytes(key: Key, modifiers: Modifiers, modes: InputModes) -> Vec<u8> {
    let ctrl = modifiers.ctrl;
    let parameter =
        1 + u8::from(modifiers.shift) + 2 * u8::from(modifiers.alt) + 4 * u8::from(ctrl);
    let final_char = match key {
        Key::ArrowUp => Some('A'),
        Key::ArrowDown => Some('B'),
        Key::ArrowRight => Some('C'),
        Key::ArrowLeft => Some('D'),
        Key::Home => Some('H'),
        Key::End => Some('F'),
        Key::F1 => Some('P'),
        Key::F2 => Some('Q'),
        Key::F3 => Some('R'),
        Key::F4 => Some('S'),
        _ => None,
    };
    if let Some(final_char) = final_char {
        return if parameter != 1 {
            format!("\x1b[1;{parameter}{final_char}").into_bytes()
        } else if modes.application_cursor || matches!(key, Key::F1 | Key::F2 | Key::F3 | Key::F4) {
            format!("\x1bO{final_char}").into_bytes()
        } else {
            format!("\x1b[{final_char}").into_bytes()
        };
    }
    let tilde = match key {
        Key::Insert => Some(2),
        Key::Delete => Some(3),
        Key::PageUp => Some(5),
        Key::PageDown => Some(6),
        Key::F5 => Some(15),
        Key::F6 => Some(17),
        Key::F7 => Some(18),
        Key::F8 => Some(19),
        Key::F9 => Some(20),
        Key::F10 => Some(21),
        Key::F11 => Some(23),
        Key::F12 => Some(24),
        _ => None,
    };
    if let Some(number) = tilde {
        return if parameter == 1 {
            format!("\x1b[{number}~").into_bytes()
        } else {
            format!("\x1b[{number};{parameter}~").into_bytes()
        };
    }
    let bytes = match key {
        Key::Enter => vec![b'\r'],
        Key::Tab if modifiers.shift => b"\x1b[Z".to_vec(),
        Key::Tab => vec![b'\t'],
        Key::Escape => vec![0x1b],
        Key::Backspace => vec![if ctrl { 8 } else { 127 }],
        // Ctrl+V remains clipboard paste (the backend supplies Event::Paste).
        Key::V if ctrl => Vec::new(),
        _ if ctrl => control(key).map(|code| vec![code]).unwrap_or_default(),
        // Printable keys are sent exclusively through Text, respecting the layout.
        _ => Vec::new(),
    };
    if modifiers.alt && !bytes.is_empty() {
        prefixed(&bytes)
    } else {
        bytes
    }
}

fn control(key: Key) -> Option<u8> {
    if (Key::A..=Key::Z).contains(&key) {
        return Some(key as u8 - Key::A as u8 + 1);
    }
    match key {
        Key::Space | Key::Num2 | Key::Backtick => Some(0),
        Key::OpenBracket | Key::OpenCurlyBracket | Key::Num3 => Some(27),
        Key::Backslash | Key::Pipe | Key::Num4 => Some(28),
        Key::CloseBracket | Key::CloseCurlyBracket | Key::Num5 => Some(29),
        Key::Num6 => Some(30),
        Key::Minus | Key::Num7 | Key::Slash => Some(31),
        Key::Num8 | Key::Questionmark => Some(127),
        _ => None,
    }
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod zsh_tests;
