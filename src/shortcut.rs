//! Shortcuts in the portable form the apps store (`Cmd+Shift+Z`): parsing, display, the egui
//! binding, which keys a native menu may own, and which keys a focused text field keeps.
//!
//! `Cmd` is the platform's command key (⌘ on macOS, Ctrl elsewhere). `Ctrl` is the real Control
//! key and is never folded into `Cmd`: ⌃⌘M and ⌘M are different shortcuts.

use egui::{Key, KeyboardShortcut, Modifiers};

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Chord {
    pub cmd: bool,
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
    /// The key as written: `Z`, `1`, `'`, `F5`, `Delete`, `Enter`.
    pub key: String,
}

impl Chord {
    pub fn parse(s: &str) -> Option<Chord> {
        let mut c = Chord { cmd: false, ctrl: false, alt: false, shift: false, key: String::new() };
        for part in s.split('+') {
            match part {
                "Cmd" => c.cmd = true,
                "Ctrl" => c.ctrl = true,
                "Alt" => c.alt = true,
                "Shift" => c.shift = true,
                // `Cmd++` splits into an empty last part.
                "" => c.key = "+".into(),
                k if c.key.is_empty() => c.key = k.into(),
                _ => return None,
            }
        }
        (!c.key.is_empty()).then_some(c)
    }

    /// The portable form, modifiers in a fixed order.
    pub fn portable(&self) -> String {
        let mut parts = Vec::new();
        for (on, name) in [(self.ctrl, "Ctrl"), (self.cmd, "Cmd"), (self.alt, "Alt"), (self.shift, "Shift")] {
            if on {
                parts.push(name);
            }
        }
        parts.push(&self.key);
        parts.join("+")
    }

    /// How menus show it. macOS: `⌃⌥⇧⌘Z` (Apple's modifier order). Elsewhere: `Ctrl+Shift+Z`.
    pub fn display(&self, mac: bool) -> String {
        if mac {
            let key = match self.key.as_str() {
                "Delete" => "⌫",
                "Enter" => "↩",
                "Escape" => "⎋",
                "Tab" => "⇥",
                "Space" => "Space",
                k => k,
            };
            let mut s = String::new();
            for (on, sym) in [(self.ctrl, '⌃'), (self.alt, '⌥'), (self.shift, '⇧'), (self.cmd, '⌘')] {
                if on {
                    s.push(sym);
                }
            }
            s + key
        } else {
            let mut parts = Vec::new();
            for (on, name) in [(self.ctrl || self.cmd, "Ctrl"), (self.alt, "Alt"), (self.shift, "Shift")] {
                if on {
                    parts.push(name);
                }
            }
            parts.push(&self.key);
            parts.join("+")
        }
    }

    pub fn egui_key(&self) -> Option<Key> {
        match self.key.as_str() {
            "'" => Some(Key::Quote),
            ";" => Some(Key::Semicolon),
            "=" => Some(Key::Equals),
            "-" => Some(Key::Minus),
            "[" => Some(Key::OpenBracket),
            "]" => Some(Key::CloseBracket),
            "\\" => Some(Key::Backslash),
            "/" => Some(Key::Slash),
            "," => Some(Key::Comma),
            "." => Some(Key::Period),
            "Delete" => Some(Key::Backspace),
            k => Key::from_name(k),
        }
    }

    /// The egui binding. `Cmd` is egui's `command` (⌘ on macOS, Ctrl elsewhere).
    pub fn egui(&self) -> Option<KeyboardShortcut> {
        let mut m = Modifiers::NONE;
        m.alt = self.alt;
        m.shift = self.shift;
        m.ctrl = self.ctrl;
        if self.cmd {
            m = m | Modifiers::COMMAND;
        }
        Some(KeyboardShortcut::new(m, self.egui_key()?))
    }

    pub fn is_function_key(&self) -> bool {
        self.key.len() >= 2 && self.key.starts_with('F') && self.key[1..].parse::<u8>().is_ok_and(|n| (1..=24).contains(&n))
    }

    /// May a native menu own this key? Only with ⌘ or ⌃, or a function key. Bare letters, digits
    /// and ⌥/⇧-only keys are tool keys in these apps: AppKit runs key equivalents before the app
    /// sees the key, so a native `B` would fire while the user types a `b` into a text field.
    pub fn native_ok(&self) -> bool {
        self.cmd || self.ctrl || self.is_function_key()
    }

    /// Text-editing keys a focused text field must keep: select all, clipboard, undo/redo.
    pub fn is_text_editing(&self) -> bool {
        self.cmd && !self.alt && !self.ctrl && matches!(self.key.as_str(), "A" | "C" | "X" | "V" | "Z" | "Y")
    }
}

/// Where keyboard focus is (PhotoCraft's `Focus`): nowhere, on a widget (slider, button reached
/// with Tab), or in a text field.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Focus {
    None,
    Widget,
    Text,
}

impl Focus {
    pub fn of(ctx: &egui::Context) -> Focus {
        if ctx.text_edit_focused() {
            Focus::Text
        } else if ctx.egui_wants_keyboard_input() {
            Focus::Widget
        } else {
            Focus::None
        }
    }

    /// May `c` fire as a shortcut with this focus? A focused widget keeps its navigation keys; a
    /// text field keeps everything except ⌘ shortcuts that aren't text editing, and function keys.
    pub fn allows(self, c: &Chord) -> bool {
        let navigation = matches!(
            c.key.as_str(),
            "Left" | "Right" | "Up" | "Down" | "Enter" | "Space" | "Tab" | "Escape" | "Delete" | "Home" | "End" | "PageUp" | "PageDown"
        );
        match self {
            Focus::None => true,
            Focus::Widget => c.cmd || !navigation,
            Focus::Text => c.is_function_key() || (c.cmd && !c.is_text_editing() && !navigation),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_and_prints() {
        let c = Chord::parse("Cmd+Alt+Shift+O").unwrap();
        assert!(c.cmd && c.alt && c.shift && !c.ctrl);
        assert_eq!(c.key, "O");
        assert_eq!(c.portable(), "Cmd+Alt+Shift+O");
        assert_eq!(c.display(true), "⌥⇧⌘O");
        assert_eq!(c.display(false), "Ctrl+Alt+Shift+O");
        assert_eq!(Chord::parse("Ctrl+Cmd+M").unwrap().display(true), "⌃⌘M");
        assert_eq!(Chord::parse("Cmd++").unwrap().key, "+");
        assert_eq!(Chord::parse("Cmd+A+B"), None);
    }

    #[test]
    fn ctrl_is_never_folded_into_cmd() {
        let a = Chord::parse("Cmd+M").unwrap().egui().unwrap();
        let b = Chord::parse("Ctrl+Cmd+M").unwrap().egui().unwrap();
        assert_ne!(a, b);
    }

    #[test]
    fn native_menus_own_only_command_and_function_keys() {
        assert!(Chord::parse("Cmd+B").unwrap().native_ok());
        assert!(Chord::parse("F5").unwrap().native_ok());
        assert!(!Chord::parse("B").unwrap().native_ok());
        assert!(!Chord::parse("Alt+F").unwrap().native_ok());
        assert!(Chord::parse("Alt+F9").unwrap().native_ok());
        assert!(!Chord::parse("Shift+X").unwrap().native_ok());
    }

    #[test]
    fn text_fields_keep_their_keys() {
        let ok = |s: &str| Focus::Text.allows(&Chord::parse(s).unwrap());
        assert!(!ok("Cmd+C") && !ok("Cmd+V") && !ok("Cmd+Z") && !ok("Cmd+A") && !ok("B"));
        assert!(ok("Cmd+S") && ok("Cmd+Shift+E") && ok("F5"));
        // ⌥⌘C is not text editing.
        assert!(ok("Cmd+Alt+C"));
    }

    #[test]
    fn every_photocraft_shortcut_parses_and_maps_to_egui() {
        for row in crate::model::tests_support::photocraft("doc") {
            let Some(sc) = row.shortcut else { continue };
            let c = Chord::parse(&sc).unwrap_or_else(|| panic!("{sc} does not parse"));
            assert!(c.egui().is_some(), "{sc} has no egui key");
            assert_eq!(c.portable(), sc, "portable form round-trips");
        }
    }
}
