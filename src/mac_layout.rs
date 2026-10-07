//! The macOS layout pass: turn an app's menu bar, which is laid out like a Windows or Linux one,
//! into the one a Mac user expects. Pure and platform-free, so it is tested on every OS.
//!
//! What it does, in order:
//! 1. **App menu** first, titled with the app's name: About, Settings, Services, Hide, Hide Others,
//!    Show All, Quit. About, Settings and Quit are *moved* out of Help, Edit and File (found by
//!    role, never by label), and the separators they leave behind are tidied.
//! 2. **Quit** keeps the app's own command, so the unsaved-changes prompt still runs. It is
//!    relabelled "Quit <App>" with ⌘Q ("Exit" is Windows wording).
//! 3. **File** gets the system Close Window ⌘W only when the app doesn't bind ⌘W itself.
//! 4. **Window** gets Minimize and Zoom at the top and Bring All to Front at the bottom, and is
//!    marked as the Window menu so macOS lists open windows in it.
//! 5. **Help** is marked as the Help menu, which gives it the system search field.
//! 6. **Clashes**: the system's own key equivalents (⌘H, ⌥⌘H, ⌘M, ⌘W, ⌘Q, ⌘,) are checked
//!    against the app's shortcuts. The app keeps its binding; the system item moves or drops its
//!    key, and each clash is reported so the app's owners can decide for real.

use crate::model::{Item, ItemRole, Menu, MenuBar, MenuRole, Node, Standard, tidy};
use crate::shortcut::Chord;
use std::collections::HashMap;

pub struct AppInfo<'a> {
    /// Shown as the app menu's title and in About/Hide/Quit.
    pub name: &'a str,
}

/// A system key equivalent the app already uses for something else.
#[derive(Clone, Debug, PartialEq)]
pub struct Clash {
    pub shortcut: String,
    /// What macOS uses it for.
    pub system: &'static str,
    /// The app command bound to it.
    pub command: String,
    /// What the layout did about it.
    pub resolution: String,
}

pub struct Layout {
    pub bar: MenuBar,
    pub clashes: Vec<Clash>,
}

pub const HIDE: &str = "app.hide";
pub const QUIT: &str = "app.quit";
pub const MINIMIZE: &str = "window.minimize";

pub fn mac_layout(bar: &MenuBar, app: &AppInfo) -> Layout {
    let mut bar = bar.clone();
    let mut clashes = Vec::new();
    // Shortcut → command, for every binding the app has.
    let bound: HashMap<String, String> = bar
        .items()
        .iter()
        .filter_map(|it| Some((Chord::parse(it.shortcut.as_deref()?)?.portable(), it.id.clone())))
        .collect();
    let taken = |sc: &str, own: Option<&str>| bound.get(sc).filter(|cmd| Some(cmd.as_str()) != own).cloned();

    let about = take(&mut bar, |n| matches!(n, Node::Item(it) if it.role == Some(ItemRole::About)));
    let settings = take(&mut bar, |n| match n {
        Node::Item(it) => it.role == Some(ItemRole::Settings),
        Node::Submenu { role, .. } => *role == Some(ItemRole::Settings),
        _ => false,
    });
    let quit = take(&mut bar, |n| matches!(n, Node::Item(it) if it.role == Some(ItemRole::Quit)));

    // 1. The app menu.
    let mut menu = Vec::new();
    if let Some(Node::Item(mut it)) = about {
        it.label = format!("About {}", app.name);
        menu.extend([Node::Item(it), Node::Separator]);
    }
    if let Some(s) = settings {
        // ⌘, on the settings item itself (LightCraft) is not a clash.
        let own = node_ids(&s);
        let comma = bound.get("Cmd+,").filter(|cmd| !own.contains(*cmd)).cloned();
        menu.push(settings_node(s, comma, &mut clashes));
        menu.push(Node::Separator);
    }
    menu.extend([Node::Standard { item: Standard::Services }, Node::Separator]);
    let mut hide = Item::new(HIDE, &format!("Hide {}", app.name));
    hide.role = Some(ItemRole::Hide);
    match taken("Cmd+H", None) {
        Some(cmd) => clashes.push(Clash {
            shortcut: "Cmd+H".into(),
            system: "Hide",
            command: cmd,
            resolution: "Hide has no shortcut; the app keeps ⌘H".into(),
        }),
        None => hide.shortcut = Some("Cmd+H".into()),
    }
    menu.push(Node::Item(hide));
    if let Some(cmd) = taken("Cmd+Alt+H", None) {
        clashes.push(Clash {
            shortcut: "Cmd+Alt+H".into(),
            system: "Hide Others",
            command: cmd,
            resolution: "the system Hide Others item always takes ⌥⌘H; rebind the app command".into(),
        });
    }
    menu.extend([Node::Standard { item: Standard::HideOthers }, Node::Standard { item: Standard::ShowAll }, Node::Separator]);
    // 2. Quit: the app's own command if it has one.
    let mut quit = match quit {
        Some(Node::Item(it)) => it,
        _ => Item::new(QUIT, ""),
    };
    quit.label = format!("Quit {}", app.name);
    quit.role = Some(ItemRole::Quit);
    match taken("Cmd+Q", Some(&quit.id)) {
        Some(cmd) => {
            clashes.push(Clash {
                shortcut: "Cmd+Q".into(),
                system: "Quit",
                command: cmd,
                resolution: "Quit has no shortcut; rebind the app command, ⌘Q is universal".into(),
            });
            quit.shortcut = None;
        }
        None => quit.shortcut = Some("Cmd+Q".into()),
    }
    menu.push(Node::Item(quit));
    bar.menus.insert(0, Menu { title: app.name.to_string(), role: MenuRole::App, children: menu });

    // 3. File › Close Window.
    match taken("Cmd+W", None) {
        Some(cmd) => clashes.push(Clash {
            shortcut: "Cmd+W".into(),
            system: "Close Window",
            command: cmd,
            resolution: "no system Close Window item; the app's ⌘W command closes".into(),
        }),
        None => {
            if let Some(file) = bar.menus.iter_mut().find(|m| m.title == "File") {
                file.children.extend([Node::Separator, Node::Standard { item: Standard::CloseWindow }]);
            }
        }
    }

    // 4. Window.
    if bar.menu(MenuRole::Window).is_none() {
        let at = bar.menus.iter().position(|m| m.role == MenuRole::Help).unwrap_or(bar.menus.len());
        bar.menus.insert(at, Menu { title: "Window".into(), role: MenuRole::Window, children: Vec::new() });
    }
    let minimize = match taken("Cmd+M", None) {
        Some(cmd) => {
            let mut it = Item::new(MINIMIZE, "Minimize");
            it.role = Some(ItemRole::Minimize);
            let alt = "Ctrl+Cmd+M";
            let free = taken(alt, None).is_none();
            if free {
                it.shortcut = Some(alt.into());
            }
            clashes.push(Clash {
                shortcut: "Cmd+M".into(),
                system: "Minimize",
                command: cmd,
                resolution: if free { "Minimize moves to ⌃⌘M, as in Photoshop".into() } else { "Minimize has no shortcut".into() },
            });
            Node::Item(it)
        }
        None => Node::Standard { item: Standard::Minimize },
    };
    let window = bar.menus.iter_mut().find(|m| m.role == MenuRole::Window).expect("inserted above");
    let mut children = vec![minimize, Node::Standard { item: Standard::Zoom }, Node::Separator];
    children.append(&mut window.children);
    children.extend([Node::Separator, Node::Standard { item: Standard::BringAllToFront }]);
    window.children = children;

    for m in &mut bar.menus {
        m.children = tidy(std::mem::take(&mut m.children));
    }
    // A menu emptied by the moves (a Help menu that only held About) goes away.
    bar.menus.retain(|m| !m.children.is_empty() || m.role == MenuRole::Help);
    Layout { bar, clashes }
}

/// Settings in the app menu: a single item becomes "Settings…" ⌘,; a submenu of settings pages
/// (PhotoCraft's Edit › Preferences) becomes a "Settings" submenu whose first page gets ⌘,.
fn settings_node(node: Node, comma_taken: Option<String>, clashes: &mut Vec<Clash>) -> Node {
    if let Some(cmd) = &comma_taken {
        clashes.push(Clash {
            shortcut: "Cmd+,".into(),
            system: "Settings",
            command: cmd.clone(),
            resolution: "Settings has no shortcut; ⌘, is universal, rebind the app command".into(),
        });
    }
    let give = |it: &mut Item| {
        if comma_taken.is_none() && it.shortcut.is_none() {
            it.shortcut = Some("Cmd+,".into());
        }
    };
    match node {
        Node::Item(mut it) => {
            it.label = "Settings…".into();
            give(&mut it);
            Node::Item(it)
        }
        Node::Submenu { mut children, role, .. } => {
            if let Some(Node::Item(first)) = children.iter_mut().find(|n| matches!(n, Node::Item(i) if i.enabled)) {
                give(first);
            }
            Node::Submenu { label: "Settings".into(), children, role }
        }
        other => other,
    }
}

fn node_ids(node: &Node) -> Vec<String> {
    match node {
        Node::Item(it) => vec![it.id.clone()],
        Node::Submenu { children, .. } => children.iter().flat_map(node_ids).collect(),
        _ => Vec::new(),
    }
}

/// Remove and return the first node matching `pred`, at any depth.
fn take(bar: &mut MenuBar, pred: impl Fn(&Node) -> bool + Copy) -> Option<Node> {
    fn walk(nodes: &mut Vec<Node>, pred: impl Fn(&Node) -> bool + Copy) -> Option<Node> {
        if let Some(i) = nodes.iter().position(pred) {
            return Some(nodes.remove(i));
        }
        nodes.iter_mut().find_map(|n| match n {
            Node::Submenu { children, .. } => walk(children, pred),
            _ => None,
        })
    }
    bar.menus.iter_mut().find_map(|m| walk(&mut m.children, pred))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{from_paths, from_tree, tests_support::photocraft};

    fn photocraft_layout() -> Layout {
        mac_layout(&crate::host::photocraft_bar(&photocraft("doc")), &AppInfo { name: "PhotoCraft" })
    }

    fn ids(nodes: &[Node]) -> Vec<String> {
        nodes
            .iter()
            .map(|n| match n {
                Node::Item(it) => it.id.clone(),
                Node::Separator => "---".into(),
                Node::Submenu { label, .. } => format!("[{label}]"),
                Node::Standard { item } => format!("<{item:?}>"),
            })
            .collect()
    }

    #[test]
    fn photocraft_gets_a_mac_app_menu() {
        let l = photocraft_layout();
        let app = &l.bar.menus[0];
        assert_eq!((app.title.as_str(), app.role), ("PhotoCraft", MenuRole::App));
        assert_eq!(
            ids(&app.children),
            [
                "help.about",
                "---",
                "[Settings]",
                "---",
                "<Services>",
                "---",
                HIDE,
                "<HideOthers>",
                "<ShowAll>",
                "---",
                "file.exit"
            ]
        );
        let quit = l.bar.find("file.exit").unwrap();
        assert_eq!((quit.label.as_str(), quit.shortcut.as_deref()), ("Quit PhotoCraft", Some("Cmd+Q")));
    }

    #[test]
    fn moved_items_leave_their_old_menus() {
        let l = photocraft_layout();
        let by = |t: &str| l.bar.menus.iter().find(|m| m.title == t).unwrap();
        assert!(!ids(&by("File").children).contains(&"file.exit".to_string()));
        assert!(!ids(&by("Help").children).contains(&"help.about".to_string()));
        assert!(!ids(&by("Edit").children).contains(&"[Preferences]".to_string()));
        // Settings… ⌘, lands on the first settings page.
        assert_eq!(l.bar.find("edit.preferences.general").unwrap().shortcut.as_deref(), Some("Cmd+,"));
    }

    #[test]
    fn photocraft_clashes_are_resolved_in_the_apps_favour() {
        let l = photocraft_layout();
        let clash = |sc: &str| l.clashes.iter().find(|c| c.shortcut == sc).map(|c| c.command.as_str());
        assert_eq!(clash("Cmd+H"), Some("view.extras"));
        assert_eq!(clash("Cmd+M"), Some("image.adjustments.curves"));
        assert_eq!(clash("Cmd+W"), Some("file.close"));
        assert_eq!(l.bar.find(HIDE).unwrap().shortcut, None, "Hide gives ⌘H to View › Extras");
        assert_eq!(l.bar.find(MINIMIZE).unwrap().shortcut.as_deref(), Some("Ctrl+Cmd+M"));
        let file = l.bar.menus.iter().find(|m| m.title == "File").unwrap();
        assert!(!file.children.contains(&Node::Standard { item: Standard::CloseWindow }), "⌘W stays File › Close");
    }

    /// PhotoCraft binds ⌘, to Hide Layers (as Photoshop does) but its menu row doesn't show it.
    /// The layout pass can only see a clash the app reports, so feed it real bindings.
    #[test]
    fn with_real_bindings_settings_gives_cmd_comma_to_hide_layers() {
        use crate::host::{FakeHost, MenuHost};
        let mut h = FakeHost::photocraft();
        h.real_shortcuts = true;
        let l = mac_layout(&h.menu_bar(), &AppInfo { name: "PhotoCraft" });
        let comma = l.clashes.iter().find(|c| c.shortcut == "Cmd+,").expect("⌘, clash");
        assert_eq!(comma.command, "layer.hideLayers");
        assert_eq!(l.bar.find("edit.preferences.general").unwrap().shortcut, None);
        // And ⌘Z now reaches the native menu.
        assert_eq!(l.bar.find("edit.undo").unwrap().shortcut.as_deref(), Some("Cmd+Z"));
        let mut seen = HashMap::new();
        for it in l.bar.items() {
            if let Some(sc) = &it.shortcut
                && let Some(other) = seen.insert(sc.clone(), it.id.clone())
            {
                assert_eq!(other, it.id, "{sc} on {other} and {}", it.id);
            }
        }
    }

    #[test]
    fn window_and_help_menus_are_marked() {
        let l = photocraft_layout();
        let window = l.bar.menu(MenuRole::Window).unwrap();
        assert_eq!(ids(&window.children)[..3], [MINIMIZE.to_string(), "<Zoom>".into(), "---".into()]);
        assert_eq!(ids(&window.children).last().unwrap(), "<BringAllToFront>");
        assert!(l.bar.menu(MenuRole::Help).is_some());
    }

    #[test]
    fn no_shortcut_is_used_twice() {
        let l = photocraft_layout();
        let mut seen = HashMap::new();
        for it in l.bar.items() {
            // The same command may appear in two menus (PhotoCraft lists Keyboard Shortcuts in
            // Edit and Window); two *different* commands may not share a key.
            if let Some(sc) = &it.shortcut
                && let Some(other) = seen.insert(sc.clone(), it.id.clone())
            {
                assert_eq!(other, it.id, "{sc} on {other} and {}", it.id);
            }
        }
    }

    #[test]
    fn an_app_without_cmd_w_or_cmd_m_gets_the_system_items() {
        let path = format!("{}/fixtures/lightcraft-demo.json", env!("CARGO_MANIFEST_DIR"));
        let json = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
        let mut bar = from_tree(&json).unwrap();
        bar.tag_menu("Window", MenuRole::Window);
        bar.tag_menu("Help", MenuRole::Help);
        bar.tag_item("app.about", ItemRole::About);
        bar.tag_item("app.settings", ItemRole::Settings);
        bar.tag_item("app.quit", ItemRole::Quit);
        let l = mac_layout(&bar, &AppInfo { name: "LightCraft" });
        assert!(l.clashes.is_empty(), "{:?}", l.clashes);
        assert_eq!(l.bar.find("app.settings").unwrap().shortcut.as_deref(), Some("Cmd+,"));
        let file = l.bar.menus.iter().find(|m| m.title == "File").unwrap();
        assert_eq!(file.children.last(), Some(&Node::Standard { item: Standard::CloseWindow }));
        let window = l.bar.menu(MenuRole::Window).unwrap();
        assert_eq!(window.children[0], Node::Standard { item: Standard::Minimize });
        assert_eq!(l.bar.find(HIDE).unwrap().shortcut.as_deref(), Some("Cmd+H"));
        // Quit moved out of File, so it appears once.
        assert_eq!(l.bar.items().iter().filter(|it| it.id == QUIT).count(), 1);
    }

    #[test]
    fn a_missing_window_menu_is_added_before_help() {
        let bar = from_paths(&["File", "Help"], &photocraft("doc"));
        let mut bar = bar;
        bar.tag_menu("Help", MenuRole::Help);
        let l = mac_layout(&bar, &AppInfo { name: "X" });
        let titles: Vec<&str> = l.bar.menus.iter().map(|m| m.title.as_str()).collect();
        assert_eq!(titles, ["X", "File", "Window", "Help"]);
    }
}
