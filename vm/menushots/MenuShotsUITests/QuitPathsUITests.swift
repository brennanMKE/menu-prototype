// Each way of quitting PhotoCraft on macOS must end the process (storytold/photocraft#1575,
// #1458). One quit per test, so the script can check crash reports and the saved window layout
// after each. Runs only inside a disposable Tart guest.
import XCTest

final class QuitPathsUITests: XCTestCase {
    func launch(_ args: [String] = []) -> XCUIApplication {
        let app = XCUIApplication(bundleIdentifier: "ai.storyteller.photocraft")
        app.launchArguments = args
        app.launch()
        XCTAssertTrue(app.windows.firstMatch.waitForExistence(timeout: 90), "PhotoCraft opened no window")
        // Let it sit, as in #1575.
        sleep(10)
        return app
    }

    func shot(_ name: String) {
        let a = XCTAttachment(screenshot: XCUIScreen.main.screenshot())
        a.name = name
        a.lifetime = .keepAlways
        add(a)
    }

    func expectExit(_ app: XCUIApplication, _ how: String) {
        let exited = app.wait(for: .notRunning, timeout: 20)
        if !exited { shot("\(how)-still-running") }
        XCTAssertTrue(exited, "PhotoCraft still running 20 s after \(how)")
    }

    func quitFromAppMenu(_ app: XCUIApplication) {
        let bar = app.menuBars.firstMatch
        bar.menuBarItems["PhotoCraft"].click()
        bar.menuBarItems["PhotoCraft"].menus.firstMatch.menuItems["Quit PhotoCraft"].click()
    }

    func testQuitFromAppMenu() {
        let app = launch()
        quitFromAppMenu(app)
        expectExit(app, "PhotoCraft › Quit PhotoCraft")
    }

    func testQuitFromAppMenuWithDocument() {
        let app = launch(["/Users/admin/fixtures/sample.png"])
        quitFromAppMenu(app)
        expectExit(app, "PhotoCraft › Quit PhotoCraft with a document open")
    }

    func testCommandQ() {
        let app = launch()
        app.typeKey("q", modifierFlags: .command)
        expectExit(app, "⌘Q")
    }

    func testCloseButton() {
        let app = launch()
        let close = app.windows.firstMatch.buttons[XCUIIdentifierCloseWindow]
        XCTAssertTrue(close.exists, "no close button")
        close.click()
        expectExit(app, "the window's close button")
    }

    func testCommandQInWindowMenus() {
        let app = launch(["--in-window-menus"])
        app.typeKey("q", modifierFlags: .command)
        expectExit(app, "⌘Q with --in-window-menus")
    }
}
