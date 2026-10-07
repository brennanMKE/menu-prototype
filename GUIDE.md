# Proper macOS menus for an egui app

How to give an eframe/egui app (any of the Crafting Apps) a real macOS menu bar while keeping its
in-window bar for Linux and Windows. It comes from building this prototype against PhotoCraft's
real menus. Every rule below is implemented in `src/` and most are tested. File references point
at the code to copy.

The design and the survey of all twelve apps are in the hub:
[`projects/menu-prototype/PROPOSAL.md`](https://github.com/brennanMKE/artcraft-hub/blob/main/projects/menu-prototype/PROPOSAL.md).

## What's wrong today

Run the prototype three ways to see it (`cargo run -- --help`):

| Mode | What you see |
|---|---|
| `--menus=in-window` | PhotoCraft today: the menus are drawn inside the window, under the title bar. The macOS menu bar holds only winit's default menu. Mac users look for menus at the top of the screen, and VoiceOver, the Help search and Keyboard Shortcuts in System Settings can't find these |
| `--menus=native --layout=raw` | The menus moved to the menu bar as they are, which is roughly what several apps ship. File becomes the app menu (macOS uses the first menu for that). "Exit" sits in File, About in Help, Settings in Edit › Preferences. There's no Hide, Services or Window list. ⌘H and ⌘M run Photoshop commands, so Mac users can't hide or minimize with the keyboard |
| `--menus=native` | The same menus after the macOS layout pass, with each item showing the shortcut that runs it: everything below |

Mistakes found across the apps (proposal §1), each of which the prototype avoids:

1. **A predefined Quit** (`PredefinedMenuItem::quit`) sends `terminate:` straight to NSApp and skips the
   app's unsaved-changes prompt (GridCraft, FilmCraft, DesignCraft, CadCraft).
2. **Finding menus by label** ("Window", "Help") breaks in every other language (EffectCraft).
3. **Predefined Copy/Paste items** send `copy:` to winit's view, not to egui, so they do nothing.
4. **Native ⌘C/⌘V/⌘Z fire document commands while a text field has focus**, because AppKit runs key
   equivalents before the app sees the key (all apps except LightCraft).
5. **Bare-letter accelerators** (`B` for Brush) fire while the user types a `b` into a text field.
6. **⌃ folded into ⌘** turns ⌃⌘M into ⌘M.
7. **Positional ids** (`m.3.0.2`) force a full rebuild whenever an item is inserted.
8. **A timer-driven sync** (every 250–500 ms) wastes work while idle, and still leaves the menu stale
   in between.
9. **Duplicate command ids** (PhotoCraft lists Keyboard Shortcuts in Edit and in Window): muda ids
   must be unique, and LightCraft's code silently drops the second copy.
10. **Menu rows that don't show their real shortcut.** PhotoCraft's `menu_items` takes each
    shortcut from the Photoshop catalog row, while key dispatch uses `default_shortcut`, which also
    checks the engine's registry. In PhotoCraft 12 items show no shortcut although one runs them:
    Undo ⌘Z, Redo ⇧⌘Z, Cut, Copy, Paste, Clear, Group/Ungroup Layers, Hide Layers ⌘,, Inverse,
    Select and Mask, Fit on Screen. In a native menu those keys aren't key equivalents, the Mac
    menu bar never shows them, and the layout pass can't see their clashes (see step 1).

## The recipe

### 1. One model, built by the app

Describe the menus once, as data: a tree of menus, items, separators and submenus
([`src/model.rs`](src/model.rs)). An item carries its command id, params, translated label,
portable shortcut (`Cmd+Shift+Z`), enabled and checked state. The in-window bar and the native
menu are both drawn from it. Neither one knows about the app's commands.

Most apps already have this data:

- **A flat list with paths**, like PhotoCraft and FilmCraft's `menu_items()` →
  `from_paths(TOP_MENUS, items)`. A submenu appears where its first child is, which keeps Photoshop's
  order.
- **A tree**, like LightCraft's `menu_bar()` → convert node for node (`from_tree` loads its JSON).
- **Static tables** (GridCraft, DesignCraft, CadCraft, VectorCraft) → walk them into nodes.

Identify each item by **`id`, or `id|params`** when one command appears with different params
(`Item::key`). Never by position.

**Report the shortcut that actually runs each command**, from the same function the key
dispatcher uses. Everything downstream depends on it: what the menu shows, which keys become
native key equivalents, and which system keys clash. The prototype's exporter lists the
mismatches (`tools/export-photocraft`), and the "after" view applies PhotoCraft's real bindings
(`fixtures/photocraft-bindings.json`).

### 2. Tag roles; don't move anything yourself

The app says which of its items are About, Settings and Quit, and which menus are Window and Help.
That's all PhotoCraft's adapter does ([`src/host.rs`](src/host.rs) `photocraft_bar`):

```rust
bar.tag_item("help.about", ItemRole::About);
bar.tag_item("file.exit", ItemRole::Quit);
bar.tag_submenu(&["Edit", "Preferences"], ItemRole::Settings);
bar.tag_menu("Window", MenuRole::Window);
bar.tag_menu("Help", MenuRole::Help);
```

### 3. Run the macOS layout pass

[`mac_layout(bar, &AppInfo { name })`](src/mac_layout.rs) is a pure function, tested on every OS.
It returns the Mac menu bar and a list of clashes:

- An **app menu** titled with the app's name, containing About, Settings (⌘,), Services, Hide, Hide
  Others, Show All and Quit. About, Settings and Quit are moved out of their menus, and the
  separators they leave behind are cleaned up.
- **Quit stays the app's own command**, relabelled "Quit PhotoCraft" with ⌘Q.
- A system **Close Window ⌘W** in File, but only if the app doesn't bind ⌘W itself.
- **Minimize, Zoom and Bring All to Front** in the Window menu, which is marked as the Window menu.
- The **Help menu** marked as the Help menu, which adds the search field.
- **Clashes**: the app keeps its bindings, and the system item gives way. For PhotoCraft:

  | Key | macOS use | PhotoCraft use | Resolution |
  |---|---|---|---|
  | ⌘H | Hide | View › Extras | Hide has no shortcut |
  | ⌘M | Minimize | Image › Adjustments › Curves | Minimize moves to ⌃⌘M (as in Photoshop) |
  | ⌘W | Close Window | File › Close | No system Close Window item |
  | ⌘, | Settings | Layer › Hide Layers | Settings has no shortcut. Found only once the menu reported real bindings |

  The prototype prints these at startup. Each one is a product decision for the app; the layout's
  answer is only the default.

### 4. Install the native menu early, and only once

Install in eframe's creator closure, where NSApp and the main thread exist, so winit's default menu
never flashes ([`src/main.rs`](src/main.rs) `main`). Wrap the install in `catch_unwind` and fall
back to the in-window bar on failure. Route muda's events through a channel **and call
`ctx.request_repaint()`** in the handler, or a click waits for the next mouse move to be handled
([`src/native/macos.rs`](src/native/macos.rs) `MacMenu::new`).

### 5. Sync on state changes, not on a timer

Each frame, in this order ([`src/main.rs`](src/main.rs) `logic`):

1. Drain native menu events and run them.
2. If the app's **state hash** changed (cheap, e.g. document open, selection, undo depth), rebuild the
   model and call `sync`. Also sync after any native click, because AppKit toggles a check item
   itself.
3. `sync` compares a **structure key**: item keys, kinds, shortcuts, submenu labels and the UI
   language. If it changed, rebuild the menu. Otherwise update only labels, enabled and checked, in
   place.

Keep each item's **kind stable**. PhotoCraft reports 22 items (Image › Mode, View › Proof Setup) as
plain items with no document open and as check items with one. A native menu can't change an item's
kind in place, so opening the first document rebuilds the whole menu. Report `checked: Some(false)`
instead of `None` when there's nothing to check.

### 6. Decide who owns each key

AppKit runs a menu's key equivalents **before** winit and egui see the key. So:

- **Only ⌘, ⌃ and function keys become native accelerators** (`Chord::native_ok`). Bare letters,
  digits and ⌥/⇧-only keys stay in the app's egui shortcut handler.
- **The native menu publishes the keys it owns**, and the egui dispatcher skips them, so nothing runs
  twice (`NativeBackend::owned_shortcuts`).
- **While a text field has focus**, take away the accelerators the field needs (⌘A, ⌘C, ⌘X, ⌘V, ⌘Z,
  ⌘Y, ⇧⌘Z, navigation keys) and give them back when it loses focus (`set_text_focus`, which uses
  PhotoCraft's `Focus::allows` rules). ⌘S and the rest keep working while typing.
- **Never fold ⌃ into ⌘**: they map to `CONTROL` and `META` separately.
- **Don't use predefined Edit items** (Copy, Paste, Undo); make them app commands.

### 7. Give duplicate commands unique ids

When a command appears in two menus, give the second native item a distinct id (`key#2`) and map it
back to the same key (`MacMenu::unique_id`). Don't drop it.

### 8. Keep the in-window bar for everything else

Linux and Windows keep the in-window bar ([`src/in_window.rs`](src/in_window.rs), PhotoCraft
v0.3.0's), and so does macOS when the user prefers it. Offer the switch in three places: a flag, an
environment variable, and a menu item (View › Use In-Window Menu Bar in the prototype, which switches
live with `remove_for_nsapp` / `init_for_nsapp`).

The in-window bar must keep every menu below the bar. Otherwise a tall submenu slides up over the
titles, and moving the pointer inside it opens a different menu. That is photocraft#92, and
`--hover=legacy` brings it back for comparison. The test `legacy_hover_reproduces_photocraft_92`
reproduces it, and `fixed_hover_never_covers_the_bar` checks the fix.

## Adopting it in an app

| App | From | Work |
|---|---|---|
| PhotoCraft | `from_paths(TOP_MENUS, menu_items(app))` | New native menu. Make `menu_items` report `effective_shortcut(…, default_shortcut(id))`. Tag About/Quit/Preferences. Make the 22 mode items stable check items. Decide ⌘H, ⌘M and ⌘, |
| FilmCraft | `from_paths`, same as PhotoCraft | Replace the predefined Quit. Add enabled/checked sync and an off switch |
| LightCraft | Its tree, node for node | Already closest. Swap types, tag roles instead of filtering ids, give duplicates ids instead of dropping them |
| EffectCraft | Its engine tree, after expanding dynamic items | Replace label matching with roles |
| VectorCraft | Its `Cmd`/`Sub` tree | Replace the 250 ms timer with state-gated sync; fix `accel()` |
| GridCraft, DesignCraft, CadCraft | Static tables | Replace the predefined Quit. Add diffed sync, roles and Window/Help registration |
| SoundCraft | `from_paths` over `menus.txt` | New native menu. Its "SoundCraft" menu becomes the App role |
| PrintCraft | One-level registry | Move the hand-written View items into the registry first (printcraft#80) |
| DeckCraft | Its unused `menu_tree()` | Wire it up |
| WordCraft | Ribbon only | Needs a menu ordering first; optional |

Whether this code becomes a shared crate, extends the proposed `craft-appmenu`, or is copied into
each app is still open (proposal §4, question 1). `model.rs`, `shortcut.rs`, `mac_layout.rs` and
`native/` have no app types in them, so any of the three works.

## Checking it on a Mac

Unit tests cover the model, the layout pass, the key rules and the in-window bar
(`cargo test`). The native menu itself can only be checked by hand, with
`cargo run --release -- --menus=native`:

- [ ] The app menu reads PhotoCraft: About, Settings ▸, Services, Hide, Hide Others, Show All, Quit.
- [ ] File has no Exit; Help has no About; Edit has no Preferences.
- [ ] ⌘Q with "Document open" checked shows the quit prompt (the app's own path ran).
- [ ] Unchecking "Document open" greys Image › Mode items while the menu is closed. Reopen it to see.
- [ ] File › Open… adds a file to Open Recent without a relaunch.
- [ ] Edit shows Undo ⌘Z, Copy ⌘C and Paste ⌘V (PhotoCraft's own menu shows none of them).
- [ ] In the text field: ⌘C, ⌘V and ⌘Z edit the text, and typing `b` types a b. Outside it, ⌘C
      appears in the log as `native`. (⌘Z only after something was done: Undo is disabled at first.)
- [ ] Help's search field finds "Gaussian Blur" and highlights it in Filter › Blur.
- [ ] The Window menu lists the window; ⌃⌘M minimizes it; ⌘M runs Curves.
- [ ] View › Use In-Window Menu Bar switches to the in-window bar and back, live.
- [ ] `--layout=raw` shows the mistakes listed above, for comparison.

## Decisions the prototype made, which apps may revisit

From the proposal's open questions (§4):

| Question | Prototype's default |
|---|---|
| ⌘H, ⌘M, ⌘, | The app keeps them. Hide and Settings have no key, and Minimize moves to ⌃⌘M |
| ⌥-click variants (Photoshop's Merge → Stamp) | In-window only: the click reports ⌥. muda events carry no modifiers, so native menus would need the variants as separate items |
| Shortcuts on disabled items | Kept, and AppKit beeps. Routing them to the app to explain why needs more work |
| Menu colours and tooltips (Edit › Menus) | In-window only; native menus can't show them |
| PhotoCraft's 797 rows, 234 of them disabled (many "not yet implemented") | All shown, as the in-window bar does |
| Default on macOS | Native, with the in-window bar as an option |
