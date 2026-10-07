//! The app side of the menus. A real app implements [`MenuHost`] over its command registry; the
//! prototype's [`FakeHost`] implements it over PhotoCraft's exported menu (the fixtures), so the
//! menus behave like PhotoCraft's without building PhotoCraft:
//!
//! - New/Open open a document and Close/Close All close it, switching between the two fixtures'
//!   enabled and checked state (264 items change);
//! - Open adds a file to File › Open Recent and Clear Recent empties it (a structural change);
//! - clicking a check item toggles it;
//! - File › Exit asks first while a document is open (the app's own quit path).

use crate::model::{FlatItem, ItemRole, MenuBar, MenuRole, from_paths};
use std::collections::BTreeMap;
use std::hash::{DefaultHasher, Hash, Hasher};

/// Where a command came from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Source {
    /// Clicked in the in-window bar; `alt` = ⌥ was held (Photoshop's ⌥-click variants).
    InWindow { alt: bool },
    /// Chosen from the native menu bar, or its key equivalent.
    Native,
    /// The egui shortcut dispatcher.
    Shortcut,
}

/// What an app gives the menus.
pub trait MenuHost {
    /// The full menu, translated, with current enabled/checked state. Called only when
    /// [`MenuHost::state_hash`] changes, so it may be expensive.
    fn menu_bar(&self) -> MenuBar;
    /// Cheap: changes whenever anything `menu_bar` returns would change.
    fn state_hash(&self) -> u64;
    /// Run the item with this key ([`crate::model::Item::key`]).
    fn invoke(&mut self, key: &str, source: Source) -> Result<(), String>;
}

pub const TOPS: [&str; 10] = ["File", "Edit", "Image", "Layer", "Type", "Select", "Filter", "View", "Window", "Help"];

/// PhotoCraft's adapter: its flat list becomes the tree, and the items macOS moves are tagged by
/// command id. This is the whole of what PhotoCraft would need to tell the layout pass.
pub fn photocraft_bar(rows: &[FlatItem]) -> MenuBar {
    let mut bar = from_paths(&TOPS, rows);
    bar.tag_item("help.about", ItemRole::About);
    bar.tag_item("file.exit", ItemRole::Quit);
    bar.tag_submenu(&["Edit", "Preferences"], ItemRole::Settings);
    bar.tag_menu("Window", MenuRole::Window);
    bar.tag_menu("Help", MenuRole::Help);
    bar
}

#[derive(Clone, Debug, PartialEq)]
pub struct LogEntry {
    pub key: String,
    pub label: String,
    pub source: Source,
}

pub struct FakeHost {
    nodoc: Vec<FlatItem>,
    doc: Vec<FlatItem>,
    pub doc_open: bool,
    /// Check items the user toggled, over the fixture's value.
    checked: BTreeMap<String, bool>,
    pub recent: Vec<String>,
    opened: usize,
    pub log: Vec<LogEntry>,
    /// File › Exit was chosen; the window decides how to quit.
    pub quit_requested: bool,
}

impl FakeHost {
    pub fn new(nodoc: Vec<FlatItem>, doc: Vec<FlatItem>) -> FakeHost {
        FakeHost {
            nodoc,
            doc,
            doc_open: true,
            checked: BTreeMap::new(),
            recent: Vec::new(),
            opened: 0,
            log: Vec::new(),
            quit_requested: false,
        }
    }

    /// The fixtures compiled into the binary.
    pub fn photocraft() -> FakeHost {
        let nodoc = serde_json::from_str(include_str!("../fixtures/photocraft-nodoc.json")).expect("nodoc fixture");
        let doc = serde_json::from_str(include_str!("../fixtures/photocraft-doc.json")).expect("doc fixture");
        FakeHost::new(nodoc, doc)
    }

    /// Load another app's flat export (same shape as `ui.menu.list`) for both states.
    pub fn from_file(path: &str) -> Result<FakeHost, String> {
        let text = std::fs::read_to_string(path).map_err(|e| format!("{path}: {e}"))?;
        let rows: Vec<FlatItem> = serde_json::from_str(&text).map_err(|e| format!("{path}: {e}"))?;
        Ok(FakeHost::new(rows.clone(), rows))
    }

    fn rows(&self) -> Vec<FlatItem> {
        let mut rows = if self.doc_open { self.doc.clone() } else { self.nodoc.clone() };
        for r in &mut rows {
            if let (Some(c), Some(v)) = (r.checked.as_mut(), self.checked.get(&r.id)) {
                *c = *v;
            }
        }
        // Open Recent: the files, then Clear Recent (which the fixture already has).
        let at = rows.iter().position(|r| r.id == "file.clearRecent").unwrap_or(rows.len());
        let mut files: Vec<FlatItem> = self
            .recent
            .iter()
            .enumerate()
            .map(|(i, name)| FlatItem {
                id: format!("file.openRecent.{i}"),
                label: name.clone(),
                path: vec!["File".into(), "Open Recent".into()],
                shortcut: None,
                enabled: true,
                checked: None,
            })
            .collect();
        if !files.is_empty() {
            files.push(FlatItem {
                id: "---".into(),
                label: crate::model::SEPARATOR.into(),
                path: vec!["File".into(), "Open Recent".into()],
                shortcut: None,
                enabled: false,
                checked: None,
            });
        }
        if let Some(clear) = rows.get_mut(at) {
            clear.enabled = !self.recent.is_empty();
        }
        rows.splice(at..at, files);
        rows
    }
}

impl MenuHost for FakeHost {
    fn menu_bar(&self) -> MenuBar {
        photocraft_bar(&self.rows())
    }

    fn state_hash(&self) -> u64 {
        let mut h = DefaultHasher::new();
        (self.doc_open, &self.checked, &self.recent).hash(&mut h);
        h.finish()
    }

    fn invoke(&mut self, key: &str, source: Source) -> Result<(), String> {
        let bar = self.menu_bar();
        let it = bar.find(key).ok_or_else(|| format!("no menu item {key}"))?;
        if !it.enabled {
            return Err(format!("{} is not available", it.label));
        }
        self.log.push(LogEntry { key: key.into(), label: it.label.clone(), source });
        if let Some(c) = it.checked {
            self.checked.insert(it.id.clone(), !c);
        }
        match it.id.as_str() {
            "file.new" => self.doc_open = true,
            "file.open" | "file.openAs" => {
                self.doc_open = true;
                self.opened += 1;
                self.recent.insert(0, format!("photo-{}.psd", self.opened));
            }
            id if id.starts_with("file.openRecent.") => self.doc_open = true,
            "file.clearRecent" => self.recent.clear(),
            "file.close" | "file.closeAll" => self.doc_open = false,
            "file.exit" => self.quit_requested = true,
            _ => {}
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{content_hash, structure_key};

    #[test]
    fn closing_the_document_greys_items() {
        let mut h = FakeHost::photocraft();
        let s0 = h.state_hash();
        assert!(h.menu_bar().find("image.mode.rgb").is_some_and(|i| i.enabled));
        h.invoke("file.close", Source::Native).unwrap();
        assert_ne!(h.state_hash(), s0);
        assert!(h.menu_bar().find("image.mode.rgb").is_some_and(|i| !i.enabled));
        assert_eq!(h.invoke("image.mode.rgb", Source::Native), Err("RGB Color is not available".into()));
    }

    #[test]
    fn open_recent_grows_and_that_is_a_structural_change() {
        let mut h = FakeHost::photocraft();
        let k0 = structure_key(&h.menu_bar(), "en");
        h.invoke("file.open", Source::InWindow { alt: false }).unwrap();
        h.invoke("file.open", Source::Shortcut).unwrap();
        let bar = h.menu_bar();
        assert_eq!(bar.find("file.openRecent.0").unwrap().label, "photo-2.psd");
        assert_ne!(structure_key(&bar, "en"), k0);
        h.invoke("file.clearRecent", Source::Native).unwrap();
        assert_eq!(structure_key(&h.menu_bar(), "en"), k0);
    }

    #[test]
    fn check_items_toggle_and_the_log_records_the_source() {
        let mut h = FakeHost::photocraft();
        let id = h.menu_bar().items().iter().find(|i| i.checked.is_some() && i.enabled).unwrap().id.clone();
        let before = h.menu_bar().find(&id).unwrap().checked;
        let c0 = content_hash(&h.menu_bar());
        h.invoke(&id, Source::Native).unwrap();
        assert_eq!(h.menu_bar().find(&id).unwrap().checked, before.map(|c| !c));
        assert_ne!(content_hash(&h.menu_bar()), c0);
        assert_eq!(h.log.last().unwrap().source, Source::Native);
    }
}
