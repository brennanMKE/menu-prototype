//! The menu model: one tree that both the in-window bar and the native menu bar are built from.
//!
//! It is LightCraft's `MenuNode` tree (serializable, carries `params`, keyed by `id|params`) with
//! roles added, so the macOS layout pass ([`crate::mac_layout`]) can find About, Settings, Quit and
//! the Window and Help menus without matching labels, which breaks in other languages.
//!
//! Nothing here depends on egui, muda or any app: an app reaches a [`MenuBar`] through an adapter
//! ([`from_paths`] for apps with a flat `path` list like PhotoCraft and FilmCraft, [`from_tree`]
//! for apps that already build a tree like LightCraft).

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::hash::{DefaultHasher, Hash, Hasher};

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct MenuBar {
    pub menus: Vec<Menu>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Menu {
    pub title: String,
    #[serde(default)]
    pub role: MenuRole,
    pub children: Vec<Node>,
}

/// What a top-level menu is for. macOS treats the App, Window and Help menus specially.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum MenuRole {
    #[default]
    Normal,
    /// The menu named after the app, first on macOS (About, Settings, Services, Hide, Quit).
    App,
    /// Gets Minimize/Zoom and the list of open windows.
    Window,
    /// Gets the system's search field, which finds menu items.
    Help,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Node {
    Item(Item),
    Separator,
    Submenu {
        label: String,
        children: Vec<Node>,
        /// `Settings` marks a submenu of settings pages that moves to the app menu on macOS.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        role: Option<ItemRole>,
    },
    /// An item the platform provides and runs itself (Services, Hide Others, Zoom, …). Only the
    /// macOS layout adds these; the in-window bar skips them.
    Standard { item: Standard },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Item {
    /// The command id, e.g. `file.open`.
    pub id: String,
    /// Parameters for the command; `Null` for most items.
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub params: Value,
    /// Already translated.
    pub label: String,
    /// Portable form: `Cmd+Shift+Z` (⌘ on macOS, Ctrl elsewhere), `Ctrl+…` for the real Control key.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shortcut: Option<String>,
    pub enabled: bool,
    /// `Some` for check items.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub checked: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<ItemRole>,
}

/// Items macOS expects in fixed places.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ItemRole {
    About,
    Settings,
    /// Must run the app's own quit path (unsaved-changes prompt), never `NSApp terminate:`.
    Quit,
    /// Hide the app (macOS). A custom item so it can drop ⌘H when the app uses ⌘H.
    Hide,
    /// Minimize the window (macOS). A custom item so it can move off ⌘M when the app uses ⌘M.
    Minimize,
}

/// Items AppKit implements: they act on the app or window directly and need no command.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Standard {
    Services,
    HideOthers,
    ShowAll,
    Minimize,
    Zoom,
    BringAllToFront,
    CloseWindow,
}

impl Item {
    pub fn new(id: &str, label: &str) -> Item {
        Item { id: id.into(), params: Value::Null, label: label.into(), shortcut: None, enabled: true, checked: None, role: None }
    }

    /// The item's identity: the command id, plus its params when it has any (one command can
    /// appear several times with different params, e.g. LightCraft's rating items).
    pub fn key(&self) -> String {
        if self.params.is_null() { self.id.clone() } else { format!("{}|{}", self.id, self.params) }
    }
}

impl MenuBar {
    /// Every item, depth first.
    pub fn items(&self) -> Vec<&Item> {
        fn walk<'a>(nodes: &'a [Node], out: &mut Vec<&'a Item>) {
            for n in nodes {
                match n {
                    Node::Item(it) => out.push(it),
                    Node::Submenu { children, .. } => walk(children, out),
                    Node::Separator | Node::Standard { .. } => {}
                }
            }
        }
        let mut out = Vec::new();
        for m in &self.menus {
            walk(&m.children, &mut out);
        }
        out
    }

    pub fn find(&self, key: &str) -> Option<&Item> {
        self.items().into_iter().find(|it| it.key() == key)
    }

    pub fn menu(&self, role: MenuRole) -> Option<&Menu> {
        self.menus.iter().find(|m| m.role == role)
    }

    /// Set a role on every item matching `id` (adapters tag their app's About, Quit, …).
    pub fn tag_item(&mut self, id: &str, role: ItemRole) {
        fn walk(nodes: &mut [Node], id: &str, role: ItemRole) {
            for n in nodes {
                match n {
                    Node::Item(it) if it.id == id => it.role = Some(role),
                    Node::Submenu { children, .. } => walk(children, id, role),
                    _ => {}
                }
            }
        }
        for m in &mut self.menus {
            walk(&mut m.children, id, role);
        }
    }

    /// Set a role on the submenu reached by `path` (`["Edit", "Preferences"]`).
    pub fn tag_submenu(&mut self, path: &[&str], role: ItemRole) {
        let Some((top, rest)) = path.split_first() else { return };
        let Some(menu) = self.menus.iter_mut().find(|m| m.title == *top) else { return };
        let mut nodes = &mut menu.children;
        for (i, name) in rest.iter().enumerate() {
            let Some(Node::Submenu { children, role: r, .. }) =
                nodes.iter_mut().find(|n| matches!(n, Node::Submenu { label, .. } if label == name))
            else {
                return;
            };
            if i + 1 == rest.len() {
                *r = Some(role);
                return;
            }
            nodes = children;
        }
    }

    pub fn tag_menu(&mut self, title: &str, role: MenuRole) {
        if let Some(m) = self.menus.iter_mut().find(|m| m.title == title) {
            m.role = role;
        }
    }
}

/// One row of a flat menu list, as PhotoCraft's `menu_items` and `ui.menu.list` return it:
/// `path` is the menu and submenus the row sits in (`["Image", "Adjustments"]`), and a `"---"`
/// label is a separator.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FlatItem {
    pub id: String,
    pub label: String,
    pub path: Vec<String>,
    #[serde(default)]
    pub shortcut: Option<String>,
    pub enabled: bool,
    #[serde(default)]
    pub checked: Option<bool>,
}

pub const SEPARATOR: &str = "---";

/// Build the tree from a flat list in menu order (PhotoCraft's `render_level_rows` without the
/// drawing): at each depth, leaves and separators appear in order, and a submenu appears where its
/// first child is. `tops` gives the menus' order; rows with an empty path belong to no menu.
pub fn from_paths(tops: &[&str], items: &[FlatItem]) -> MenuBar {
    fn level(rows: &[&FlatItem], depth: usize) -> Vec<Node> {
        let mut out = Vec::new();
        let mut shown: Vec<&str> = Vec::new();
        for it in rows {
            if it.path.len() == depth {
                if it.label == SEPARATOR {
                    out.push(Node::Separator);
                } else {
                    out.push(Node::Item(Item {
                        id: it.id.clone(),
                        params: Value::Null,
                        label: it.label.clone(),
                        shortcut: it.shortcut.clone(),
                        enabled: it.enabled,
                        checked: it.checked,
                        role: None,
                    }));
                }
            } else if it.path.len() > depth {
                let name = it.path[depth].as_str();
                if shown.contains(&name) {
                    continue;
                }
                shown.push(name);
                let child: Vec<&FlatItem> =
                    rows.iter().copied().filter(|c| c.path.len() > depth && c.path[depth] == name).collect();
                out.push(Node::Submenu { label: name.to_string(), children: level(&child, depth + 1), role: None });
            }
        }
        tidy(out)
    }
    let menus = tops
        .iter()
        .map(|top| {
            let rows: Vec<&FlatItem> = items.iter().filter(|i| i.path.first().map(String::as_str) == Some(*top)).collect();
            Menu { title: top.to_string(), role: MenuRole::Normal, children: level(&rows, 1) }
        })
        .collect();
    MenuBar { menus }
}

/// Load a tree in the shape LightCraft's `ui.menu.tree` returns: `[{label, children}]`.
pub fn from_tree(json: &Value) -> Result<MenuBar, String> {
    let menus = json.as_array().ok_or("expected an array of menus")?;
    let menus = menus
        .iter()
        .map(|m| {
            let title = m["label"].as_str().ok_or("menu without a label")?.to_string();
            let children: Vec<Node> = serde_json::from_value(m["children"].clone()).map_err(|e| format!("{title}: {e}"))?;
            Ok(Menu { title, role: MenuRole::Normal, children: tidy(children) })
        })
        .collect::<Result<_, String>>()?;
    Ok(MenuBar { menus })
}

/// Drop separators at the start or end of a level and collapse runs of them, at every depth.
/// Moving items (e.g. Quit to the app menu) leaves such separators behind.
pub fn tidy(nodes: Vec<Node>) -> Vec<Node> {
    let mut out: Vec<Node> = Vec::with_capacity(nodes.len());
    for n in nodes {
        let n = match n {
            Node::Submenu { label, children, role } => Node::Submenu { label, children: tidy(children), role },
            other => other,
        };
        if matches!(n, Node::Separator) && matches!(out.last(), None | Some(Node::Separator)) {
            continue;
        }
        out.push(n);
    }
    while matches!(out.last(), Some(Node::Separator)) {
        out.pop();
    }
    out
}

/// A hash of everything that can only change by rebuilding a native menu: menus, roles, item keys,
/// item kinds (plain or check), shortcuts, submenu labels, and the UI language (titles are
/// translated, so a language switch must rebuild). Labels, enabled and checked are left out:
/// those are updated in place.
pub fn structure_key(bar: &MenuBar, lang: &str) -> u64 {
    fn walk(nodes: &[Node], h: &mut DefaultHasher) {
        for n in nodes {
            match n {
                Node::Item(it) => {
                    ('i', it.key(), it.checked.is_some(), &it.shortcut, it.role).hash(h);
                }
                Node::Separator => '-'.hash(h),
                Node::Submenu { label, children, role } => {
                    ('[', label, role).hash(h);
                    walk(children, h);
                    ']'.hash(h);
                }
                Node::Standard { item } => ('s', item).hash(h),
            }
        }
    }
    let mut h = DefaultHasher::new();
    lang.hash(&mut h);
    for m in &bar.menus {
        (&m.title, m.role).hash(&mut h);
        walk(&m.children, &mut h);
    }
    h.finish()
}

/// A hash of everything a native menu shows, structure plus labels, enabled and checked. A host
/// that can't compute a cheaper state hash can use this to gate syncing.
pub fn content_hash(bar: &MenuBar) -> u64 {
    let mut h = DefaultHasher::new();
    structure_key(bar, "").hash(&mut h);
    for it in bar.items() {
        (&it.label, it.enabled, it.checked).hash(&mut h);
    }
    h.finish()
}

/// Fixture loading shared by the tests.
#[cfg(test)]
pub mod tests_support {
    use super::*;

    pub fn photocraft(name: &str) -> Vec<FlatItem> {
        let path = format!("{}/fixtures/photocraft-{name}.json", env!("CARGO_MANIFEST_DIR"));
        serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
    }
}

#[cfg(test)]
mod tests {
    use super::tests_support::photocraft;
    use super::*;

    const TOPS: [&str; 10] = ["File", "Edit", "Image", "Layer", "Type", "Select", "Filter", "View", "Window", "Help"];

    fn submenu<'a>(nodes: &'a [Node], name: &str) -> &'a [Node] {
        nodes
            .iter()
            .find_map(|n| match n {
                Node::Submenu { label, children, .. } if label == name => Some(children.as_slice()),
                _ => None,
            })
            .unwrap_or_else(|| panic!("no submenu {name}"))
    }

    #[test]
    fn photocraft_fixture_gives_ten_menus_and_keeps_every_item() {
        let rows = photocraft("doc");
        let bar = from_paths(&TOPS, &rows);
        assert_eq!(bar.menus.iter().map(|m| m.title.as_str()).collect::<Vec<_>>(), TOPS);
        // Every non-separator row that sits in a menu becomes exactly one item.
        let placed = rows.iter().filter(|r| !r.path.is_empty() && r.label != SEPARATOR).count();
        assert_eq!(bar.items().len(), placed);
    }

    #[test]
    fn a_submenu_sits_where_its_first_child_is() {
        let bar = from_paths(&TOPS, &photocraft("doc"));
        let file = &bar.menus[0].children;
        let pos = |pred: &dyn Fn(&Node) -> bool| file.iter().position(pred).unwrap();
        let export = pos(&|n| matches!(n, Node::Submenu { label, .. } if label == "Export"));
        // File › Export comes after Save As… and before Print…, as in PhotoCraft's File menu.
        let save_as = pos(&|n| matches!(n, Node::Item(it) if it.id == "file.saveAs"));
        assert!(save_as < export, "Export should follow Save As");
        assert!(!submenu(file, "Export").is_empty());
    }

    #[test]
    fn separators_never_lead_trail_or_double() {
        fn check(nodes: &[Node], at: &str) {
            assert!(!matches!(nodes.first(), Some(Node::Separator)), "{at} starts with a separator");
            assert!(!matches!(nodes.last(), Some(Node::Separator)), "{at} ends with a separator");
            for w in nodes.windows(2) {
                assert!(!(matches!(w[0], Node::Separator) && matches!(w[1], Node::Separator)), "{at} has two separators in a row");
            }
            for n in nodes {
                if let Node::Submenu { label, children, .. } = n {
                    check(children, &format!("{at} › {label}"));
                }
            }
        }
        for m in from_paths(&TOPS, &photocraft("nodoc")).menus {
            check(&m.children, &m.title);
        }
    }

    /// PhotoCraft reports 22 items (Image › Mode, View › Proof Setup, …) as plain items with no
    /// document and as check items with one. A native menu can't turn a plain item into a check
    /// item in place, so opening the first document rebuilds the native menu. The key must see
    /// that; an app that keeps each item's kind stable (`checked: Some(false)` when there's
    /// nothing to check) changes only state.
    #[test]
    fn opening_a_document_rebuilds_only_because_items_change_kind() {
        let (nodoc, doc) = (photocraft("nodoc"), photocraft("doc"));
        let flips = nodoc.iter().zip(&doc).filter(|(a, b)| a.checked.is_some() != b.checked.is_some()).count();
        assert_eq!(flips, 22);
        let a = from_paths(&TOPS, &nodoc);
        let b = from_paths(&TOPS, &doc);
        assert_ne!(structure_key(&a, "en"), structure_key(&b, "en"));
        // With stable kinds, the same change is state only.
        let stable: Vec<FlatItem> = nodoc
            .iter()
            .zip(&doc)
            .map(|(a, b)| FlatItem { checked: a.checked.or(b.checked.map(|_| false)), ..a.clone() })
            .collect();
        let a = from_paths(&TOPS, &stable);
        assert_eq!(structure_key(&a, "en"), structure_key(&b, "en"));
        assert_ne!(content_hash(&a), content_hash(&b));
        assert_ne!(structure_key(&a, "en"), structure_key(&a, "ja"), "a language switch must rebuild");
    }

    #[test]
    fn structure_key_sees_a_plain_item_becoming_a_check_item() {
        let mut bar = from_paths(&TOPS, &photocraft("doc"));
        let before = structure_key(&bar, "en");
        if let Some(Node::Item(it)) = bar.menus[0].children.iter_mut().find(|n| matches!(n, Node::Item(_))) {
            it.checked = Some(false);
        }
        assert_ne!(before, structure_key(&bar, "en"));
    }

    #[test]
    fn lightcraft_tree_loads_and_round_trips() {
        let path = format!("{}/fixtures/lightcraft-demo.json", env!("CARGO_MANIFEST_DIR"));
        let json: Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
        let bar = from_tree(&json).unwrap();
        assert_eq!(bar.menus.len(), 6);
        assert!(bar.items().len() > 50);
        // Items with params keep them, and their key includes them.
        assert!(bar.items().iter().any(|it| !it.params.is_null() && it.key().contains('|')));
        let back: MenuBar = serde_json::from_value(serde_json::to_value(&bar).unwrap()).unwrap();
        assert_eq!(back, bar);
    }

    #[test]
    fn tags_find_items_and_submenus() {
        let mut bar = from_paths(&TOPS, &photocraft("doc"));
        bar.tag_item("file.exit", ItemRole::Quit);
        bar.tag_submenu(&["Edit", "Preferences"], ItemRole::Settings);
        assert_eq!(bar.find("file.exit").unwrap().role, Some(ItemRole::Quit));
        assert!(
            bar.menus[1].children.iter().any(|n| matches!(n, Node::Submenu { role: Some(ItemRole::Settings), .. })),
            "Edit › Preferences tagged"
        );
    }
}
