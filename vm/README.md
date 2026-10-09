# Native menu screenshots in a Tart VM

An XCUITest (`menushots/`) that opens PhotoCraft's native macOS menus and screenshots each one,
and the script that runs it in a disposable clone of `craft-uitest-golden`
(see artcraft-hub `docs/ui-test-vm.md`). It never runs on the host.

```sh
vm/run-menu-shots-vm.sh <PhotoCraft.app> vm/menushots <fixtures dir with sample.png> <results dir>
xcrun xcresulttool export attachments --path <results dir>/MenuShots.xcresult --output-path <dir>
```

The test runner is sandboxed, so screenshots are kept as test attachments and exported from the
result bundle. The results of the first run, for storytold/photocraft#894, are in
`../screenshots/photocraft-894/`.

## Quit paths (storytold/photocraft#1575, #1458)

`QuitPathsUITests` quits PhotoCraft one way per test: the app menu's Quit (with and without a
document), ⌘Q, the window's close button, and ⌘Q with `--in-window-menus`. Run each test on its
own with `ONLY_TESTS`; after each, the script records whether PhotoCraft is still running, new
crash reports and `ui.ron`'s modification time, and copies PhotoCraft's crash reports to
`<results dir>/crashes`.

```sh
C=QuitPathsUITests
ONLY_TESTS="$C/testQuitFromAppMenu $C/testQuitFromAppMenuWithDocument $C/testCommandQ $C/testCloseButton $C/testCommandQInWindowMenus" \
  vm/run-menu-shots-vm.sh <PhotoCraft.app> vm/menushots <fixtures dir> <results dir>
```

Without `ONLY_TESTS` the script runs only the menu screenshots.
