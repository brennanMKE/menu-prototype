# Use the native macOS menu bar instead of the in-window menus

## Summary

On macOS, PhotoCraft draws its menus inside the window, under the title bar. The menu bar at the
top of the screen only has winit's default menu. Mac users look for menus at the top of the
screen, and macOS features that rely on the real menu bar don't work with the in-window one:

- VoiceOver can't read the menus.
- The Help menu's search field can't find commands.
- App Shortcuts in System Settings › Keyboard can't rebind them.
- There's no app menu (About, Settings, Services, Hide, Quit) and no Window menu.

The in-window bar has also been the source of several bugs. In #92 (and its duplicate #319), a tall
submenu slid up over the menu titles, and moving the pointer inside it opened a different menu.
#402 reports menus running past the window on macOS Tahoe, with a request to make them scroll. A
native menu leaves layout, scrolling and hover to AppKit.

## Proposal

On macOS, build a native menu bar (with [muda](https://github.com/tauri-apps/muda)) from the same
data the in-window bar uses (`menu_items`). Keep the in-window bar on Linux and Windows, and as an
option on macOS.

The native menu should follow the macOS layout:

- **App menu "PhotoCraft"**: About PhotoCraft (moved from Help), Settings… ⌘, (moved from
  Edit › Preferences), Services, Hide PhotoCraft, Hide Others, Show All, Quit PhotoCraft ⌘Q (moved
  from File › Exit).
- **Quit runs PhotoCraft's own `file.exit`**, not the predefined Quit item, so the unsaved-changes
  prompt still appears.
- **Window menu** registered with AppKit (Minimize, Zoom, Bring All to Front, and the window list),
  and the **Help menu** registered so it gets the search field.

## Details to get right

These come from a prototype built against PhotoCraft's real menus (797 rows):

1. **Menu rows must show the shortcut that actually runs them.** `menu_items` takes each shortcut
   from the Photoshop catalog row, but key dispatch uses `default_shortcut`, which also checks the
   engine's registry. As a result, 12 items show no shortcut even though one runs them: Undo ⌘Z,
   Redo ⇧⌘Z, Cut, Copy, Paste, Clear, Group/Ungroup Layers, Hide Layers ⌘,, Inverse, Select and
   Mask, Fit on Screen. In a native menu these wouldn't become key equivalents at all.
2. **Shortcut clashes with macOS** need a decision:

   | Key | macOS | PhotoCraft | Suggested |
   |---|---|---|---|
   | ⌘H | Hide | View › Extras | PhotoCraft keeps it; Hide has no shortcut |
   | ⌘M | Minimize | Image › Adjustments › Curves | PhotoCraft keeps it; Minimize moves to ⌃⌘M, as in Photoshop |
   | ⌘W | Close Window | File › Close | PhotoCraft's Close keeps it |
   | ⌘, | Settings | Layer › Hide Layers | Hidden today by item 1. Settings is ⌘, on every Mac app, so consider rebinding Hide Layers |

3. **Text fields keep their keys.** AppKit runs key equivalents before the app sees the key, so
   while a text field has focus the native menu must give up ⌘A, ⌘C, ⌘X, ⌘V, ⌘Z and ⇧⌘Z, and get
   them back when focus leaves. Only ⌘, ⌃ and function-key shortcuts should become native key
   equivalents. Bare letters like `B` for Brush stay in the egui handler.
4. **Item kinds must stay stable.** 22 items (Image › Mode, View › Proof Setup) are plain items with
   no document open and check items with one. A native menu can't change an item's kind in place,
   so opening the first document would rebuild the whole menu. Report them as check items in both
   states.
5. **Duplicate commands.** Keyboard Shortcuts appears in both Edit and Window. Native menu ids must
   be unique, so the second copy needs its own id that maps back to the same command.
6. **Sync on state changes, not on a timer.** Update labels, enabled and checked in place when the
   app's state changes, and rebuild only when the menu's structure changes.
7. **Install the menu once, early** (in eframe's creator closure), and wake egui when a menu event
   arrives, or a click waits for the next mouse move.

## Acceptance

- [ ] The app menu reads PhotoCraft: About, Settings, Services, Hide, Hide Others, Show All, Quit.
- [ ] File has no Exit, Help has no About, Edit has no Preferences.
- [ ] ⌘Q with unsaved changes shows the save prompt.
- [ ] Edit shows Undo ⌘Z, Copy ⌘C and Paste ⌘V.
- [ ] In a text field, ⌘C, ⌘V and ⌘Z edit the text, and typing `b` types a b.
- [ ] Help's search finds "Gaussian Blur" and highlights it in Filter › Blur.
- [ ] The Window menu lists open windows, and Minimize works.
- [ ] Items enable, disable and check as documents open and close, without reopening the app.
- [ ] A setting (and a flag or environment variable) switches back to the in-window bar.

Related: #92, #319, #402. #383 asks for the same on Linux (global menu).
