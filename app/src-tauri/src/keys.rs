//! Keyboard chords for physical buttons, sent with `SendInput` as scan codes (layout-independent).

use x3d_core::config::{KeyBinding, PHYSICAL_BUTTONS};

/// W3C `KeyboardEvent.code` -> Windows scan code; `0xE0xx` = extended key.
const KEYS: &[(&str, u16)] = &[
    ("Escape", 0x01),
    ("Digit1", 0x02),
    ("Digit2", 0x03),
    ("Digit3", 0x04),
    ("Digit4", 0x05),
    ("Digit5", 0x06),
    ("Digit6", 0x07),
    ("Digit7", 0x08),
    ("Digit8", 0x09),
    ("Digit9", 0x0A),
    ("Digit0", 0x0B),
    ("Minus", 0x0C),
    ("Equal", 0x0D),
    ("Backspace", 0x0E),
    ("Tab", 0x0F),
    ("KeyQ", 0x10),
    ("KeyW", 0x11),
    ("KeyE", 0x12),
    ("KeyR", 0x13),
    ("KeyT", 0x14),
    ("KeyY", 0x15),
    ("KeyU", 0x16),
    ("KeyI", 0x17),
    ("KeyO", 0x18),
    ("KeyP", 0x19),
    ("BracketLeft", 0x1A),
    ("BracketRight", 0x1B),
    ("Enter", 0x1C),
    ("ControlLeft", 0x1D),
    ("KeyA", 0x1E),
    ("KeyS", 0x1F),
    ("KeyD", 0x20),
    ("KeyF", 0x21),
    ("KeyG", 0x22),
    ("KeyH", 0x23),
    ("KeyJ", 0x24),
    ("KeyK", 0x25),
    ("KeyL", 0x26),
    ("Semicolon", 0x27),
    ("Quote", 0x28),
    ("Backquote", 0x29),
    ("ShiftLeft", 0x2A),
    ("Backslash", 0x2B),
    ("KeyZ", 0x2C),
    ("KeyX", 0x2D),
    ("KeyC", 0x2E),
    ("KeyV", 0x2F),
    ("KeyB", 0x30),
    ("KeyN", 0x31),
    ("KeyM", 0x32),
    ("Comma", 0x33),
    ("Period", 0x34),
    ("Slash", 0x35),
    ("ShiftRight", 0x36),
    ("NumpadMultiply", 0x37),
    ("AltLeft", 0x38),
    ("Space", 0x39),
    ("CapsLock", 0x3A),
    ("F1", 0x3B),
    ("F2", 0x3C),
    ("F3", 0x3D),
    ("F4", 0x3E),
    ("F5", 0x3F),
    ("F6", 0x40),
    ("F7", 0x41),
    ("F8", 0x42),
    ("F9", 0x43),
    ("F10", 0x44),
    ("Pause", 0x45),
    ("ScrollLock", 0x46),
    ("Numpad7", 0x47),
    ("Numpad8", 0x48),
    ("Numpad9", 0x49),
    ("NumpadSubtract", 0x4A),
    ("Numpad4", 0x4B),
    ("Numpad5", 0x4C),
    ("Numpad6", 0x4D),
    ("NumpadAdd", 0x4E),
    ("Numpad1", 0x4F),
    ("Numpad2", 0x50),
    ("Numpad3", 0x51),
    ("Numpad0", 0x52),
    ("NumpadDecimal", 0x53),
    ("IntlBackslash", 0x56),
    ("F11", 0x57),
    ("F12", 0x58),
    ("F13", 0x64),
    ("F14", 0x65),
    ("F15", 0x66),
    ("F16", 0x67),
    ("F17", 0x68),
    ("F18", 0x69),
    ("F19", 0x6A),
    ("F20", 0x6B),
    ("F21", 0x6C),
    ("F22", 0x6D),
    ("F23", 0x6E),
    ("F24", 0x76),
    ("NumpadEnter", 0xE01C),
    ("ControlRight", 0xE01D),
    ("NumpadDivide", 0xE035),
    ("PrintScreen", 0xE037),
    ("AltRight", 0xE038),
    ("NumLock", 0xE045),
    ("Home", 0xE047),
    ("ArrowUp", 0xE048),
    ("PageUp", 0xE049),
    ("ArrowLeft", 0xE04B),
    ("ArrowRight", 0xE04D),
    ("End", 0xE04F),
    ("ArrowDown", 0xE050),
    ("PageDown", 0xE051),
    ("Insert", 0xE052),
    ("Delete", 0xE053),
    ("MetaLeft", 0xE05B),
    ("MetaRight", 0xE05C),
    ("ContextMenu", 0xE05D),
];

pub fn scan_code(code: &str) -> Option<u16> {
    KEYS.iter()
        .find(|(name, _)| *name == code)
        .map(|&(_, sc)| sc)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KeyEvent {
    pub scan: u16,
    pub down: bool,
}

/// Turns physical button edges into key presses/releases.
#[derive(Debug, Default)]
pub struct Chords {
    prev: u16,
    /// Scan codes pressed per button, so a profile switch mid-press still releases them.
    held: [Vec<u16>; PHYSICAL_BUTTONS],
}

impl Chords {
    pub fn update(&mut self, buttons: u16, bindings: &[KeyBinding]) -> Vec<KeyEvent> {
        let changed = buttons ^ self.prev;
        self.prev = buttons;
        let mut out = Vec::new();
        for (i, held) in self.held.iter_mut().enumerate() {
            if changed & (1 << i) == 0 {
                continue;
            }
            if buttons & (1 << i) != 0 {
                *held = bindings
                    .iter()
                    .filter(|b| usize::from(b.button) == i + 1)
                    .flat_map(|b| &b.keys)
                    .filter_map(|k| scan_code(k))
                    .collect();
                out.extend(held.iter().map(|&scan| KeyEvent { scan, down: true }));
            } else {
                out.extend(
                    held.drain(..)
                        .rev()
                        .map(|scan| KeyEvent { scan, down: false }),
                );
            }
        }
        out
    }

    /// Releases everything (device lost).
    pub fn release_all(&mut self) -> Vec<KeyEvent> {
        self.update(0, &[])
    }
}

#[cfg(windows)]
pub fn send(events: &[KeyEvent]) {
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
        SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_EXTENDEDKEY,
        KEYEVENTF_KEYUP, KEYEVENTF_SCANCODE,
    };
    let inputs: Vec<INPUT> = events
        .iter()
        .map(|e| {
            let mut flags = KEYEVENTF_SCANCODE;
            if e.scan & 0xFF00 == 0xE000 {
                flags |= KEYEVENTF_EXTENDEDKEY;
            }
            if !e.down {
                flags |= KEYEVENTF_KEYUP;
            }
            INPUT {
                r#type: INPUT_KEYBOARD,
                Anonymous: INPUT_0 {
                    ki: KEYBDINPUT {
                        wVk: 0,
                        wScan: e.scan & 0xFF,
                        dwFlags: flags,
                        time: 0,
                        dwExtraInfo: 0,
                    },
                },
            }
        })
        .collect();
    if inputs.is_empty() {
        return;
    }
    // SAFETY: `inputs` is a valid slice of initialised INPUT structs for the duration of the call.
    let sent = unsafe {
        SendInput(
            inputs.len() as u32,
            inputs.as_ptr(),
            std::mem::size_of::<INPUT>() as i32,
        )
    };
    if sent as usize != inputs.len() {
        log::warn!("SendInput injected {sent} of {} key events", inputs.len());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_covers_frontend_codes() {
        let mut codes: Vec<String> = Vec::new();
        codes.extend(('A'..='Z').map(|c| format!("Key{c}")));
        codes.extend((0..=9).map(|d| format!("Digit{d}")));
        codes.extend((0..=9).map(|d| format!("Numpad{d}")));
        codes.extend((1..=24).map(|n| format!("F{n}")));
        for side in ["Left", "Right"] {
            for m in ["Control", "Shift", "Alt", "Meta"] {
                codes.push(format!("{m}{side}"));
            }
            codes.push(format!("Arrow{side}"));
        }
        codes.extend(
            [
                "ArrowUp",
                "ArrowDown",
                "Space",
                "Enter",
                "Escape",
                "Tab",
                "Backspace",
                "CapsLock",
                "Insert",
                "Delete",
                "Home",
                "End",
                "PageUp",
                "PageDown",
                "NumpadAdd",
                "NumpadSubtract",
                "NumpadMultiply",
                "NumpadDivide",
                "NumpadDecimal",
                "NumpadEnter",
                "NumLock",
                "Minus",
                "Equal",
                "BracketLeft",
                "BracketRight",
                "Backslash",
                "Semicolon",
                "Quote",
                "Backquote",
                "Comma",
                "Period",
                "Slash",
                "PrintScreen",
                "ScrollLock",
                "Pause",
                "ContextMenu",
            ]
            .map(String::from),
        );
        for c in &codes {
            assert!(scan_code(c).is_some(), "missing {c}");
        }
        assert!(KEYS.len() >= 80);
        let mut scans: Vec<u16> = KEYS.iter().map(|&(_, s)| s).collect();
        scans.sort_unstable();
        scans.dedup();
        assert_eq!(scans.len(), KEYS.len(), "duplicate scan codes");
        assert_eq!(scan_code("ArrowUp"), Some(0xE048));
        assert_eq!(scan_code("LControl"), None);
    }

    #[test]
    fn chords_press_in_order_release_in_reverse() {
        let bindings = vec![KeyBinding {
            button: 2,
            keys: vec!["ControlLeft".into(), "KeyF".into()],
        }];
        let mut c = Chords::default();
        let down = |scan| KeyEvent { scan, down: true };
        let up = |scan| KeyEvent { scan, down: false };
        assert_eq!(c.update(0b01, &bindings), vec![]);
        assert_eq!(c.update(0b11, &bindings), vec![down(0x1D), down(0x21)]);
        assert_eq!(c.update(0b11, &bindings), vec![]);
        // Bindings changed while held: release what was pressed.
        assert_eq!(c.update(0b01, &[]), vec![up(0x21), up(0x1D)]);
        c.update(0b10, &bindings);
        assert_eq!(c.release_all(), vec![up(0x21), up(0x1D)]);
    }
}
