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
