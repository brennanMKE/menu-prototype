//! One menu model for the Crafting Apps, drawn in-window (egui) or as the native macOS menu bar.
//!
//! - [`model`]: the tree, adapters from an app's menu data, structure and state keys.
//! - [`shortcut`]: portable shortcuts, display, egui bindings, text-focus rules.
//! - [`mac_layout`]: the pass that makes a menu bar a Mac one (app menu, Window, Help, clashes).
//! - [`host`]: what an app implements; [`host::FakeHost`] plays PhotoCraft from fixtures.

pub mod host;
pub mod in_window;
pub mod mac_layout;
pub mod menu_nav;
pub mod model;
pub mod shortcut;
