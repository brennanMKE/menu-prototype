//! `menu-proto`: PhotoCraft's real menus, three ways.
//!
//! ```text
//! menu-proto [--menus=auto|in-window|native] [--layout=mac|raw] [--hover=fixed|legacy]
//!            [--state=doc|nodoc] [--menu-json=<flat export>] [--app-name=<name>]
//! ```
//!
//! - `--menus=in-window`: what PhotoCraft does on every platform today, macOS included.
//! - `--menus=native --layout=raw`: the menus moved to the macOS menu bar as they are, with no
//!   layout pass. This is roughly what several Crafting Apps ship, and shows what goes wrong.
//! - `--menus=native` (the default on macOS): the same menus after the macOS layout pass.
//!
//! `MENU_PROTO_MENUS` sets `--menus`; the flag wins. View › Use In-Window Menu Bar switches live.

use menu_proto::host::{FakeHost, MenuHost, Source};
use menu_proto::in_window::{self, Hover};
use menu_proto::mac_layout::{self, AppInfo, Clash, mac_layout};
use menu_proto::model::{Item, MenuBar, Node};
use menu_proto::native::{self, NativeBackend};
use menu_proto::shortcut::{Chord, Focus};
use std::hash::{DefaultHasher, Hash, Hasher};

const IN_WINDOW_TOGGLE: &str = "proto.inWindowMenuBar";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum Mode {
    InWindow,
    Native,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum LayoutKind {
    Mac,
    Raw,
}

struct Args {
    menus: Option<Mode>,
    layout: LayoutKind,
    hover: Hover,
    doc: bool,
    json: Option<String>,
    name: String,
}

fn parse_args() -> Result<Args, String> {
    let mut a = Args { menus: None, layout: LayoutKind::Mac, hover: Hover::Fixed, doc: true, json: None, name: "PhotoCraft".into() };
    let mode = |v: &str| match v {
        "auto" => Ok(None),
        "in-window" => Ok(Some(Mode::InWindow)),
        "native" => Ok(Some(Mode::Native)),
        _ => Err(format!("--menus={v}: expected auto, in-window or native")),
    };
    if let Ok(v) = std::env::var("MENU_PROTO_MENUS") {
        a.menus = mode(&v)?;
    }
    for arg in std::env::args().skip(1) {
        let (k, v) = arg.split_once('=').unwrap_or((arg.as_str(), ""));
        match k {
            "--menus" => a.menus = mode(v)?,
            "--layout" => a.layout = if v == "raw" { LayoutKind::Raw } else { LayoutKind::Mac },
            "--hover" => a.hover = if v == "legacy" { Hover::Legacy } else { Hover::Fixed },
            "--state" => a.doc = v != "nodoc",
            "--menu-json" => a.json = Some(v.to_string()),
            "--app-name" => a.name = v.to_string(),
            "-h" | "--help" => {
                println!("{}", include_str!("main.rs").lines().take_while(|l| l.starts_with("//!")).map(|l| l.trim_start_matches("//!").trim_start_matches(' ')).collect::<Vec<_>>().join("\n"));
                std::process::exit(0);
            }
            _ => return Err(format!("unknown argument {arg}")),
        }
    }
    Ok(a)
}

struct ProtoApp {
    host: FakeHost,
    name: String,
    mode: Mode,
    layout: LayoutKind,
    hover: Hover,
    backend: Option<Box<dyn NativeBackend>>,
    /// The host's menu plus the prototype's own View item.
    bar: MenuBar,
    /// What the native menu shows (after the layout pass, unless `--layout=raw`).
    native_bar: MenuBar,
    clashes: Vec<Clash>,
    gate: Option<u64>,
    note: String,
    text: String,
    confirm_quit: bool,
    syncs: usize,
}

impl ProtoApp {
    fn new(ctx: &egui::Context, args: &Args, host: FakeHost) -> ProtoApp {
        let wanted = args.menus.unwrap_or(if cfg!(target_os = "macos") { Mode::Native } else { Mode::InWindow });
        let mut app = ProtoApp {
            host,
            name: args.name.clone(),
            mode: Mode::InWindow,
            layout: args.layout,
            hover: args.hover,
            backend: None,
            bar: MenuBar::default(),
            native_bar: MenuBar::default(),
            clashes: Vec::new(),
            gate: None,
            note: String::new(),
            text: String::new(),
            confirm_quit: false,
            syncs: 0,
        };
        app.host.doc_open = args.doc;
        app.refresh();
        if wanted == Mode::Native {
            app.set_mode(ctx, Mode::Native);
        }
        for c in &app.clashes {
            eprintln!("clash: {} is {} in macOS and {} in {}; {}", c.shortcut, c.system, c.command, app.name, c.resolution);
        }
        app
    }

    /// Rebuild the menu model from the host, and the native layout from that.
    fn refresh(&mut self) {
        let mut bar = self.host.menu_bar();
        // The prototype's own switch, in View like EffectCraft's.
        if let Some(view) = bar.menus.iter_mut().find(|m| m.title == "View") {
            let mut it = Item::new(IN_WINDOW_TOGGLE, "Use In-Window Menu Bar");
            it.checked = Some(self.mode == Mode::InWindow);
            it.enabled = cfg!(target_os = "macos");
            view.children.extend([Node::Separator, Node::Item(it)]);
        }
        let layout = mac_layout(&bar, &AppInfo { name: &self.name });
        self.clashes = layout.clashes;
        self.native_bar = if self.layout == LayoutKind::Mac { layout.bar } else { bar.clone() };
        self.bar = bar;
    }

    fn set_mode(&mut self, ctx: &egui::Context, mode: Mode) {
        if mode == self.mode {
            return;
        }
        match mode {
            Mode::Native => {
                let mut b = match native::backend(ctx) {
                    Some(b) => b,
                    None => {
                        self.note = "No native menu bar on this platform; using the in-window bar.".into();
                        return;
                    }
                };
                self.mode = Mode::Native;
                self.refresh();
                match b.install(&self.native_bar) {
                    Ok(()) => self.backend = Some(b),
                    Err(e) => {
                        self.note = format!("{e}; using the in-window bar.");
                        self.mode = Mode::InWindow;
                    }
                }
            }
            Mode::InWindow => {
                if let Some(mut b) = self.backend.take() {
                    b.remove();
                }
                self.mode = Mode::InWindow;
            }
        }
        self.gate = None;
    }

    fn run(&mut self, ctx: &egui::Context, key: &str, source: Source) {
        match key {
            IN_WINDOW_TOGGLE => {
                let to = if self.mode == Mode::InWindow { Mode::Native } else { Mode::InWindow };
                self.set_mode(ctx, to);
            }
            mac_layout::MINIMIZE => ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(true)),
            mac_layout::HIDE | mac_layout::QUIT if self.backend.as_ref().is_some_and(|b| b.platform_action(key)) => {}
            mac_layout::QUIT => self.host.quit_requested = true,
            _ => {
                if let Err(e) = self.host.invoke(key, source) {
                    self.note = e;
                }
            }
        }
        if std::mem::take(&mut self.host.quit_requested) {
            // The app's own quit path: ask first while a document is open.
            if self.host.doc_open { self.confirm_quit = true } else { ctx.send_viewport_cmd(egui::ViewportCommand::Close) }
        }
    }

    /// The egui side of shortcuts: every binding in the menu, except those the native menu owns
    /// (AppKit already ran them) and those the focused widget keeps.
    fn dispatch_shortcuts(&mut self, ctx: &egui::Context) {
        if menu_proto::menu_nav::is_open(ctx) || !ctx.input(|i| i.events.iter().any(|e| matches!(e, egui::Event::Key { pressed: true, .. }))) {
            return;
        }
        let focus = Focus::of(ctx);
        let owned = self.backend.as_ref().map(|b| b.owned_shortcuts().clone()).unwrap_or_default();
        let mut table: Vec<(String, Chord)> = self
            .bar
            .items()
            .iter()
            .filter(|it| it.enabled)
            .filter_map(|it| Some((it.key(), Chord::parse(it.shortcut.as_deref()?)?)))
            .filter(|(_, c)| !owned.contains(&c.portable()) && focus.allows(c))
            .collect();
        // ⇧⌘Z before ⌘Z: egui matches a shortcut with extra modifiers held otherwise.
        table.sort_by_key(|(_, c)| std::cmp::Reverse(c.shift as u8 + c.alt as u8 + c.cmd as u8 + c.ctrl as u8));
        for (key, c) in table {
            if let Some(sc) = c.egui()
                && ctx.input_mut(|i| i.consume_shortcut(&sc))
            {
                self.run(ctx, &key, Source::Shortcut);
            }
        }
    }

    fn side_panel(&mut self, ui: &mut egui::Ui) {
        ui.heading("Menu prototype");
        let mode = match (self.mode, self.layout) {
            (Mode::InWindow, _) => "In-window menu bar (PhotoCraft today)",
            (Mode::Native, LayoutKind::Raw) => "Native menu bar, no macOS layout (the common mistakes)",
            (Mode::Native, LayoutKind::Mac) => "Native menu bar with the macOS layout",
        };
        ui.label(egui::RichText::new(mode).strong());
        ui.add_space(6.0);
        let mut doc = self.host.doc_open;
        if ui.checkbox(&mut doc, "Document open").changed() {
            self.host.doc_open = doc;
        }
        ui.label("Type here to check that ⌘C/⌘V/⌘Z and bare letters reach the field, not the menu:");
        ui.text_edit_singleline(&mut self.text);
        ui.add_space(6.0);
        ui.label(format!("Native syncs: {}   Open Recent: {} files", self.syncs, self.host.recent.len()));
        if let Some(b) = &self.backend {
            ui.label(format!("Shortcuts owned by the native menu: {}", b.owned_shortcuts().len()));
        }
        if !self.note.is_empty() {
            ui.colored_label(ui.visuals().warn_fg_color, &self.note);
        }
        if !self.clashes.is_empty() {
            ui.separator();
            ui.label(egui::RichText::new("Clashes with macOS key equivalents").strong());
            for c in &self.clashes {
                let sc = Chord::parse(&c.shortcut).map(|c| c.display(true)).unwrap_or_default();
                ui.label(format!("{sc}  {} vs {}: {}", c.system, c.command, c.resolution));
            }
        }
        ui.separator();
        ui.label(egui::RichText::new("Commands run").strong());
        egui::ScrollArea::vertical().show(ui, |ui| {
            for e in self.host.log.iter().rev().take(200) {
                let src = match e.source {
                    Source::InWindow { alt: true } => "in-window ⌥",
                    Source::InWindow { alt: false } => "in-window",
                    Source::Native => "native",
                    Source::Shortcut => "egui shortcut",
                };
                ui.monospace(format!("{src:>13}  {}  ({})", e.label, e.key));
            }
        });
    }
}

impl eframe::App for ProtoApp {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // 1. Run what was chosen in the native menu.
        let chosen = self.backend.as_mut().map(|b| b.drain()).unwrap_or_default();
        let clicked = !chosen.is_empty();
        for key in chosen {
            self.run(ctx, &key, Source::Native);
        }
        // 2. Rebuild the model only when the app's state changed, then sync the native menu.
        let mut h = DefaultHasher::new();
        (self.host.state_hash(), self.mode, self.layout).hash(&mut h);
        let gate = h.finish();
        if self.gate != Some(gate) || clicked {
            self.gate = Some(gate);
            self.refresh();
            if let Some(b) = self.backend.as_mut() {
                b.sync(&self.native_bar);
                self.syncs += 1;
            }
        }
        // 3. Hand text-editing keys to a focused text field.
        let text = ctx.text_edit_focused();
        if let Some(b) = self.backend.as_mut() {
            b.set_text_focus(text);
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        if self.mode == Mode::InWindow {
            egui::Panel::top("menu_bar").show(ui, |ui| {
                if let Some((key, alt)) = in_window::menu_bar(ui, &self.bar, self.hover) {
                    self.run(&ctx, &key, Source::InWindow { alt });
                }
            });
        }
        egui::CentralPanel::default().show(ui, |ui| self.side_panel(ui));
        self.dispatch_shortcuts(&ctx);
        if self.confirm_quit {
            egui::Window::new("Quit with a document open?").collapsible(false).resizable(false).show(&ctx, |ui| {
                ui.label("The app's own quit path runs: this is the prompt a system Quit item would skip.");
                ui.horizontal(|ui| {
                    if ui.button("Quit").clicked() {
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                    if ui.button("Cancel").clicked() {
                        self.confirm_quit = false;
                    }
                });
            });
        }
    }
}

fn main() -> eframe::Result {
    let args = match parse_args() {
        Ok(a) => a,
        Err(e) => {
            eprintln!("menu-proto: {e} (see --help)");
            std::process::exit(2);
        }
    };
    let host = match &args.json {
        Some(path) => FakeHost::from_file(path).unwrap_or_else(|e| {
            eprintln!("menu-proto: {e}");
            std::process::exit(2);
        }),
        None => FakeHost::photocraft(),
    };
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([1280.0, 720.0]).with_title(format!("{} menus", args.name)),
        ..Default::default()
    };
    eframe::run_native(
        "menu-proto",
        options,
        Box::new(move |cc| {
            // Install the native menu here, where NSApp and the main thread exist, so the default
            // winit menu never shows for a frame.
            Ok(Box::new(ProtoApp::new(&cc.egui_ctx, &args, host)))
        }),
    )
}
