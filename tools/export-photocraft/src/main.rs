//! Write PhotoCraft's menu, as `ui.menu.list` returns it, with no document open and with one open.
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
}

fn write(path: &std::path::Path, app: &PhotocraftApp) {
    let items = menus::menu_items(app);
    let text = serde_json::to_string_pretty(&items).expect("serialize") + "\n";
    std::fs::write(path, text).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    println!("{}: {} rows", path.display(), items.len());
}
