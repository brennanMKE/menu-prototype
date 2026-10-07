//! Write PhotoCraft's menu, as `ui.menu.list` returns it, with no document open and with one open,
//! plus the shortcut each menu command is actually bound to (`photocraft-bindings.json`): the menu
//! rows don't always show it.
//!
//! ```sh
//! cargo run --release --manifest-path tools/export-photocraft/Cargo.toml -- fixtures
//! ```

use photocraft_ui_egui::{PhotocraftApp, Services, menus};
use serde_json::json;
use std::path::PathBuf;

fn main() {
    let dir = PathBuf::from(std::env::args().nth(1).unwrap_or_else(|| "fixtures".into()));
    let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), Services::default());
    write(&dir.join("photocraft-nodoc.json"), &app);
    app.run("file.new", json!({"width": 800, "height": 600})).expect("file.new");
    write(&dir.join("photocraft-doc.json"), &app);
    report_hidden_shortcuts(&app, &dir);
}

/// Items whose menu row shows no shortcut (or a different one) although the command is bound:
/// `menu_items` takes the shortcut from the menu catalog row, while dispatch uses
/// `default_shortcut`, which also looks in the engine's command registry.
fn report_hidden_shortcuts(app: &PhotocraftApp, dir: &std::path::Path) {
    let mut n = 0;
    let mut bindings = serde_json::Map::new();
    for it in menus::menu_items(app) {
        let bound = photocraft_ui_egui::shortcuts::effective_shortcut(app, &it.id, photocraft_ui_egui::shortcuts::default_shortcut(&it.id).as_deref());
        if let Some(b) = &bound {
            bindings.insert(it.id.clone(), json!(b));
        }
        if it.label != "---" && bound.is_some() && bound != it.shortcut {
            println!("  menu shows {:<16} bound to {:<16} {} ({})", it.shortcut.as_deref().unwrap_or("nothing"), bound.unwrap_or_default(), it.label, it.id);
            n += 1;
        }
    }
    println!("{n} menu items don't show the shortcut that runs them");
    let path = dir.join("photocraft-bindings.json");
    std::fs::write(&path, serde_json::to_string_pretty(&bindings).expect("serialize") + "\n").expect("write bindings");
    println!("{}: {} bound commands", path.display(), bindings.len());
}

fn write(path: &std::path::Path, app: &PhotocraftApp) {
    let items = menus::menu_items(app);
    let text = serde_json::to_string_pretty(&items).expect("serialize") + "\n";
    std::fs::write(path, text).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    println!("{}: {} rows", path.display(), items.len());
}
