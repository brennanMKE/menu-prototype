//! Write LightCraft's menu tree, in the shape `ui.menu.tree` returns, with the demo library loaded.
//!
//! ```sh
//! cargo run --release --manifest-path tools/export-lightcraft/Cargo.toml -- fixtures
//! ```

use lightcraft_ui_egui::{LightcraftApp, Services, menubar};
use serde_json::{Value, json};
use std::path::PathBuf;

fn main() {
    let dir = PathBuf::from(std::env::args().nth(1).unwrap_or_else(|| "fixtures".into()));
    let app = LightcraftApp::new(lightcraft_engine::Session::with_demo(), Services::default());
    let tree: Vec<Value> =
        menubar::menu_bar(&app).into_iter().map(|(title, items)| json!({"label": title, "children": items})).collect();
    let path = dir.join("lightcraft-demo.json");
    std::fs::write(&path, serde_json::to_string_pretty(&tree).expect("serialize") + "\n")
        .unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    println!("{}: {} menus", path.display(), tree.len());
}
