//! The macOS menu bar, through muda (pure Rust over AppKit, no GTK).
//!
//! Built from LightCraft's `native_menu.rs`, the most complete of the apps' seven versions, with
//! the fixes the proposal lists:
//! - item ids are the model's keys, made unique when one command appears in two menus (LightCraft
//!   drops the second copy);
//! - syncing happens only when the app's state changed (EffectCraft's gate), and diffs per item;
//!   a structure change (new items, a plain item becoming a check item) rebuilds;
//! - platform items come from the layout pass as roles, never by matching labels;
//! - Quit is the app's own command, so its unsaved-changes prompt runs;
//! - Hide is a custom item calling `NSApplication hide:`, so it can give ⌘H to the app.

use super::NativeBackend;
use crate::model::{Item, MenuBar, MenuRole, Node, Standard, structure_key};
use crate::shortcut::{Chord, Focus};
use muda::accelerator::{Accelerator, Code, Modifiers};
use muda::{CheckMenuItem, Menu, MenuEvent, MenuItem, PredefinedMenuItem, Submenu};
use std::collections::{HashMap, HashSet};
use std::sync::mpsc::{Receiver, channel};

enum Handle {
    Plain(MenuItem),
    Check(CheckMenuItem),
}

impl Handle {
    fn set_text(&self, s: &str) {
        match self {
            Handle::Plain(h) => h.set_text(s),
            Handle::Check(h) => h.set_text(s),
        }
    }
    fn set_enabled(&self, on: bool) {
        match self {
            Handle::Plain(h) => h.set_enabled(on),
            Handle::Check(h) => h.set_enabled(on),
        }
    }
    fn set_accelerator(&self, a: Option<Accelerator>) {
        let _ = match self {
            Handle::Plain(h) => h.set_accelerator(a),
            Handle::Check(h) => h.set_accelerator(a),
        };
    }
}

/// One native item. Several can share a key (the same command in two menus).
struct Entry {
    handle: Handle,
    key: String,
    chord: Option<Chord>,
    accel: Option<Accelerator>,
    label: String,
    enabled: bool,
}

pub struct MacMenu {
    menu: Option<Menu>,
    /// By muda id.
    entries: HashMap<String, Entry>,
    rx: Receiver<String>,
    structure: u64,
    text_focus: bool,
    owned: HashSet<String>,
}

impl MacMenu {
    pub fn new(ctx: &egui::Context) -> MacMenu {
        // AppKit calls the handler; wake egui so the click is handled this frame, not the next.
        let (tx, rx) = channel::<String>();
        let repaint = ctx.clone();
        MenuEvent::set_event_handler(Some(move |e: MenuEvent| {
            let _ = tx.send(e.id.0);
            repaint.request_repaint();
        }));
        MacMenu { menu: None, entries: HashMap::new(), rx, structure: 0, text_focus: false, owned: HashSet::new() }
    }

    fn build(&mut self, bar: &MenuBar) {
        if let Some(old) = self.menu.take() {
            old.remove_for_nsapp();
        }
        self.entries.clear();
        let menu = Menu::new();
        for m in &bar.menus {
            let sub = Submenu::new(escape(&m.title), true);
            self.append(&sub, &m.children);
            if m.role == MenuRole::Window {
                sub.set_as_windows_menu_for_nsapp();
            }
            if m.role == MenuRole::Help {
                sub.set_as_help_menu_for_nsapp();
            }
            let _ = menu.append(&sub);
        }
        menu.init_for_nsapp();
        self.menu = Some(menu);
        self.structure = structure_key(bar, "en");
        // A rebuild starts with every accelerator; re-apply the text-focus state.
        let focused = std::mem::replace(&mut self.text_focus, false);
        self.set_text_focus(focused);
        self.publish();
    }

    fn append(&mut self, sub: &Submenu, nodes: &[Node]) {
        for n in nodes {
            match n {
                Node::Separator => {
                    let _ = sub.append(&PredefinedMenuItem::separator());
                }
                Node::Submenu { label, children, .. } => {
                    let s = Submenu::new(escape(label), !children.is_empty());
                    self.append(&s, children);
                    let _ = sub.append(&s);
                }
                Node::Standard { item } => {
                    let p = match item {
                        Standard::Services => PredefinedMenuItem::services(None),
                        Standard::HideOthers => PredefinedMenuItem::hide_others(None),
                        Standard::ShowAll => PredefinedMenuItem::show_all(None),
                        Standard::Minimize => PredefinedMenuItem::minimize(None),
                        Standard::Zoom => PredefinedMenuItem::maximize(Some("Zoom")),
                        Standard::BringAllToFront => PredefinedMenuItem::bring_all_to_front(None),
                        Standard::CloseWindow => PredefinedMenuItem::close_window(None),
                    };
                    let _ = sub.append(&p);
                }
                Node::Item(it) => {
                    let id = self.unique_id(&it.key());
                    let chord = it.shortcut.as_deref().and_then(Chord::parse);
                    let accel = chord.as_ref().filter(|c| c.native_ok()).and_then(accelerator);
                    let handle = match it.checked {
                        Some(c) => {
                            let h = CheckMenuItem::with_id(id.clone(), escape(&it.label), it.enabled, c, accel);
                            let _ = sub.append(&h);
                            Handle::Check(h)
                        }
                        None => {
                            let h = MenuItem::with_id(id.clone(), escape(&it.label), it.enabled, accel);
                            let _ = sub.append(&h);
                            Handle::Plain(h)
                        }
                    };
                    self.entries
                        .insert(id, Entry { handle, key: it.key(), chord, accel, label: it.label.clone(), enabled: it.enabled });
                }
            }
        }
    }

    /// muda ids must be unique: the second `edit.keyboardShortcuts` becomes `…#2`.
    fn unique_id(&self, key: &str) -> String {
        if !self.entries.contains_key(key) {
            return key.to_string();
        }
        (2..).map(|n| format!("{key}#{n}")).find(|id| !self.entries.contains_key(id)).expect("a free id")
    }

    fn publish(&mut self) {
        self.owned = self
            .entries
            .values()
            .filter(|e| e.accel.is_some() && !(self.text_focus && yields_to_text(e)))
            .filter_map(|e| Some(e.chord.as_ref()?.portable()))
            .collect();
    }
}

/// While a text field has focus, these keys belong to it (⌘C, ⌘V, ⌘Z, …).
fn yields_to_text(e: &Entry) -> bool {
    e.chord.as_ref().is_some_and(|c| !Focus::Text.allows(c))
}

impl NativeBackend for MacMenu {
    fn install(&mut self, bar: &MenuBar) -> Result<(), String> {
        // muda panics if AppKit isn't ready; the app falls back to the in-window bar.
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| self.build(bar))).map_err(|_| "could not install the native menu".to_string())
    }

    fn sync(&mut self, bar: &MenuBar) {
        if self.menu.is_none() {
            return;
        }
        if structure_key(bar, "en") != self.structure {
            self.build(bar);
            return;
        }
        let by_key: HashMap<String, &Item> = bar.items().into_iter().map(|it| (it.key(), it)).collect();
        for e in self.entries.values_mut() {
            let Some(it) = by_key.get(&e.key) else { continue };
            if e.label != it.label {
                e.handle.set_text(&escape(&it.label));
                e.label = it.label.clone();
            }
            if e.enabled != it.enabled {
                e.handle.set_enabled(it.enabled);
                e.enabled = it.enabled;
            }
            // Always compare with AppKit's own state: it toggles a check item when clicked.
            if let (Handle::Check(h), Some(c)) = (&e.handle, it.checked)
                && h.is_checked() != c
            {
                h.set_checked(c);
            }
        }
    }

    fn set_text_focus(&mut self, focused: bool) {
        if focused == self.text_focus {
            return;
        }
        self.text_focus = focused;
        for e in self.entries.values() {
            if e.accel.is_some() && yields_to_text(e) {
                e.handle.set_accelerator(if focused { None } else { e.accel });
            }
        }
        self.publish();
    }

    fn drain(&mut self) -> Vec<String> {
        let mut out = Vec::new();
        while let Ok(id) = self.rx.try_recv() {
            // Ids that aren't ours are platform items AppKit already handled.
            if let Some(e) = self.entries.get(&id) {
                out.push(e.key.clone());
            }
        }
        out
    }

    fn owned_shortcuts(&self) -> &HashSet<String> {
        &self.owned
    }

    fn remove(&mut self) {
        if let Some(m) = self.menu.take() {
            m.remove_for_nsapp();
        }
        self.entries.clear();
        self.owned.clear();
        self.structure = 0;
    }

    fn platform_action(&self, key: &str) -> bool {
        use objc2::MainThreadMarker;
        use objc2_app_kit::NSApplication;
        let Some(mtm) = MainThreadMarker::new() else { return false };
        match key {
            crate::mac_layout::HIDE => NSApplication::sharedApplication(mtm).hide(None),
            _ => return false,
        }
        true
    }
}

/// `&` marks a mnemonic in muda labels.
fn escape(s: &str) -> String {
    s.replace('&', "&&")
}

/// A key equivalent for AppKit. `Cmd` is ⌘ (`META`), `Ctrl` is ⌃: never folded together.
pub fn accelerator(c: &Chord) -> Option<Accelerator> {
    let mut mods = Modifiers::empty();
    for (on, m) in [(c.cmd, Modifiers::META), (c.ctrl, Modifiers::CONTROL), (c.alt, Modifiers::ALT), (c.shift, Modifiers::SHIFT)] {
        if on {
            mods |= m;
        }
    }
    let k = c.key.as_str();
    let b = k.as_bytes();
    let code = if k.len() == 1 && b[0].is_ascii_alphabetic() {
        const LETTERS: [Code; 26] = [
            Code::KeyA,
            Code::KeyB,
            Code::KeyC,
            Code::KeyD,
            Code::KeyE,
            Code::KeyF,
            Code::KeyG,
            Code::KeyH,
            Code::KeyI,
            Code::KeyJ,
            Code::KeyK,
            Code::KeyL,
            Code::KeyM,
            Code::KeyN,
            Code::KeyO,
            Code::KeyP,
            Code::KeyQ,
            Code::KeyR,
            Code::KeyS,
            Code::KeyT,
            Code::KeyU,
            Code::KeyV,
            Code::KeyW,
            Code::KeyX,
            Code::KeyY,
            Code::KeyZ,
        ];
        LETTERS[(b[0].to_ascii_uppercase() - b'A') as usize]
    } else if k.len() == 1 && b[0].is_ascii_digit() {
        const DIGITS: [Code; 10] = [
            Code::Digit0,
            Code::Digit1,
            Code::Digit2,
            Code::Digit3,
            Code::Digit4,
            Code::Digit5,
            Code::Digit6,
            Code::Digit7,
            Code::Digit8,
            Code::Digit9,
        ];
        DIGITS[(b[0] - b'0') as usize]
    } else if c.is_function_key() {
        const F: [Code; 24] = [
            Code::F1,
            Code::F2,
            Code::F3,
            Code::F4,
            Code::F5,
            Code::F6,
            Code::F7,
            Code::F8,
            Code::F9,
            Code::F10,
            Code::F11,
            Code::F12,
            Code::F13,
            Code::F14,
            Code::F15,
            Code::F16,
            Code::F17,
            Code::F18,
            Code::F19,
            Code::F20,
            Code::F21,
            Code::F22,
            Code::F23,
            Code::F24,
        ];
        F[k[1..].parse::<usize>().ok()? - 1]
    } else {
        match k {
            "[" => Code::BracketLeft,
            "]" => Code::BracketRight,
            "\\" => Code::Backslash,
            "/" => Code::Slash,
            "=" => Code::Equal,
            "-" => Code::Minus,
            "'" => Code::Quote,
            ";" => Code::Semicolon,
            "," => Code::Comma,
            "." => Code::Period,
            "Delete" => Code::Backspace,
            "Enter" if !mods.is_empty() => Code::Enter,
            _ => return None,
        }
    };
    Some(Accelerator::new(mods, code))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::host::{FakeHost, MenuHost};
    use crate::mac_layout::{AppInfo, mac_layout};

    #[test]
    fn every_menu_shortcut_maps_to_a_key_equivalent_or_stays_in_egui() {
        let bar = mac_layout(&FakeHost::photocraft().menu_bar(), &AppInfo { name: "PhotoCraft" }).bar;
        let mut native = 0;
        for it in bar.items() {
            let Some(c) = it.shortcut.as_deref().and_then(Chord::parse) else { continue };
            if c.native_ok() {
                assert!(accelerator(&c).is_some(), "{} ({}) has no key equivalent", c.portable(), it.id);
                native += 1;
            }
        }
        assert!(native > 70, "{native} native shortcuts");
    }

    #[test]
    fn ctrl_and_cmd_stay_distinct() {
        let a = accelerator(&Chord::parse("Cmd+M").unwrap()).unwrap();
        let b = accelerator(&Chord::parse("Ctrl+Cmd+M").unwrap()).unwrap();
        assert_ne!(a, b);
    }
}
