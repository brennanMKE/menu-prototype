//! Native menu bars. The app talks to a [`NativeBackend`]; each platform that has a native menu
//! bar implements one. Only macOS does so far: muda can't attach a menu to an eframe window on
//! Linux (it needs a GTK window), and on Windows an HMENU needs `unsafe` message-loop hooks (see
//! the proposal, §5).

use crate::model::MenuBar;
use std::collections::HashSet;

#[cfg(target_os = "macos")]
pub mod macos;

pub trait NativeBackend {
    /// Build `bar` and install it as the app's menu bar. Call on the main thread once the app is
    /// running (eframe's creator closure).
    fn install(&mut self, bar: &MenuBar) -> Result<(), String>;
    /// Bring the installed menu in step with `bar`: update labels, enabled and checked in place,
    /// or rebuild when its structure changed. Cheap to call when nothing changed.
    fn sync(&mut self, bar: &MenuBar);
    /// A text field gained or lost keyboard focus: take away (or give back) the accelerators the
    /// field needs for itself, like ⌘C and bare letters.
    fn set_text_focus(&mut self, focused: bool);
    /// Keys of the items chosen since the last call.
    fn drain(&mut self) -> Vec<String>;
    /// Shortcuts (portable form) the native menu owns right now. The app's own shortcut dispatcher
    /// must skip these so nothing runs twice.
    fn owned_shortcuts(&self) -> &HashSet<String>;
    /// Take the menu bar down (switching back to the in-window bar).
    fn remove(&mut self);
    /// A platform action for a role item (Hide). Returns false when `key` isn't one.
    fn platform_action(&self, key: &str) -> bool;
}

/// The platform's backend, if it has one.
#[allow(unused_variables)]
pub fn backend(ctx: &egui::Context) -> Option<Box<dyn NativeBackend>> {
    #[cfg(target_os = "macos")]
    return Some(Box::new(macos::MacMenu::new(ctx)));
    #[cfg(not(target_os = "macos"))]
    None
}
