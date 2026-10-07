# menu-prototype

One menu model that the Crafting Apps can draw either in-window (egui) or as a proper native macOS
menu bar (muda), worked out on PhotoCraft's real menus.

**Start with [GUIDE.md](GUIDE.md)**: what's wrong with the apps' menus on macOS today, and the
recipe for doing it properly in any egui app.

The design and the survey of all twelve apps are in the hub:
[`artcraft-hub/projects/menu-prototype/PROPOSAL.md`](https://github.com/brennanMKE/artcraft-hub/blob/main/projects/menu-prototype/PROPOSAL.md),
with the related GitHub issues in
[`ISSUES.md`](https://github.com/brennanMKE/artcraft-hub/blob/main/projects/menu-prototype/ISSUES.md).

## Run it

Needs Rust 1.95+.

```sh
cargo run --release                                  # macOS: native menu bar with the macOS layout
cargo run --release -- --menus=in-window             # PhotoCraft today: menus inside the window
cargo run --release -- --menus=native --layout=raw   # native, without the layout pass: the common mistakes
cargo run --release -- --menus=in-window --hover=legacy   # PhotoCraft v0.2.0's menus, with bug #92
cargo test                                           # model, layout pass, key rules, in-window bar
```

The window shows which mode is running, a "Document open" switch, a text field for checking key
handling, any shortcut clashes with macOS, and a log of every command with where it came from
(in-window, native, or the egui shortcut dispatcher). View › Use In-Window Menu Bar switches modes
live. `--help` lists every flag.

## Layout

```
src/model.rs          the menu tree, adapters (from_paths, from_tree), structure key
src/shortcut.rs       portable shortcuts, macOS display, egui bindings, text-focus rules
src/mac_layout.rs     the macOS layout pass: app menu, Settings, Quit, Window, Help, clashes
src/host.rs           MenuHost (what an app implements) and FakeHost (PhotoCraft from fixtures)
src/in_window.rs      PhotoCraft v0.3.0's in-window bar, drawn from the model
src/menu_nav.rs       PhotoCraft's menu keyboard navigation and scrolling, ported
src/native/           NativeBackend, and the macOS one over muda
src/main.rs           the prototype app
fixtures/             PhotoCraft's menu with and without a document, its real bindings; LightCraft's tree
tools/export-*        regenerate the fixtures headlessly from the photocraft and lightcraft checkouts
```

Clone this repo next to the storytold repos (`~/Developer/ArtCraft/`) so the exporters find them:

```sh
cargo run --release --manifest-path tools/export-photocraft/Cargo.toml -- fixtures
cargo run --release --manifest-path tools/export-lightcraft/Cargo.toml -- fixtures
```

## Status

Steps 1–8 of the proposal's build plan are done, and the write-up is [GUIDE.md](GUIDE.md). 31 tests
pass. The native menu has been built but **not yet checked by hand on a Mac**: the checklist is at
the end of the guide. Fixtures are from photocraft `1b24b52` and lightcraft `7aab3fd`.

Not done: Windows and Linux native backends (proposal §5), and per-app adoption, which happens in
the app repos.
