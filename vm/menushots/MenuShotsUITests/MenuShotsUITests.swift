// Opens PhotoCraft's native macOS menus and screenshots each one (storytold/photocraft#894).
// Runs only inside a disposable Tart guest.
import XCTest

final class MenuShotsUITests: XCTestCase {
    let out = URL(fileURLWithPath: "/Users/admin/results/shots")
    var n = 0

    override func setUp() {
        continueAfterFailure = true
        try? FileManager.default.createDirectory(at: out, withIntermediateDirectories: true)
    }

    func shot(_ name: String) {
        n += 1
        let s = XCUIScreen.main.screenshot()
        try? s.pngRepresentation.write(to: out.appendingPathComponent(String(format: "%02d-%@.png", n, name)))
        let a = XCTAttachment(screenshot: s)
        a.name = name
        a.lifetime = .keepAlways
        add(a)
    }

    func note(_ name: String, _ text: String) {
        try? text.write(to: out.appendingPathComponent("\(name).txt"), atomically: true, encoding: .utf8)
    }

    func testNativeMenus() throws {
        let app = XCUIApplication(bundleIdentifier: "ai.storyteller.photocraft")
        app.launchArguments = ["/Users/admin/fixtures/sample.png"]
        app.launch()
        XCTAssertTrue(app.windows.firstMatch.waitForExistence(timeout: 90), "PhotoCraft opened no window")
        sleep(8)
        shot("window")

        let bar = app.menuBars.firstMatch
        note("menubar-titles", bar.menuBarItems.allElementsBoundByIndex.map(\.title).joined(separator: "\n"))
        XCTAssertTrue(bar.menuBarItems["PhotoCraft"].exists, "no PhotoCraft app menu")

        for title in ["PhotoCraft", "File", "Edit", "Image", "Layer", "View", "Window", "Help"] {
            let item = bar.menuBarItems[title]
            guard item.exists else { XCTFail("no \(title) menu"); continue }
            item.click()
            sleep(1)
            shot(title)
            note("menu-\(title)", item.menus.firstMatch.menuItems.allElementsBoundByIndex.map(\.title).joined(separator: "\n"))
            app.typeKey(.escape, modifierFlags: [])
            sleep(1)
        }

        // The app menu holds About, Settings and Quit; File no longer has Exit.
        bar.menuBarItems["PhotoCraft"].click()
        let appMenu = bar.menuBarItems["PhotoCraft"].menus.firstMatch
        XCTAssertTrue(appMenu.menuItems["Quit PhotoCraft"].exists, "no Quit PhotoCraft")
        XCTAssertTrue(appMenu.menuItems["About PhotoCraft"].exists, "no About PhotoCraft")
        app.typeKey(.escape, modifierFlags: [])
        sleep(1)

        // Layer › New › Layer… is enabled with a document open.
        let layer = bar.menuBarItems["Layer"]
        layer.click()
        let new = layer.menus.firstMatch.menuItems["New"]
        new.hover()
        sleep(1)
        shot("Layer-New")
        let newLayer = new.menus.firstMatch.menuItems["Layer…"]
        XCTAssertTrue(newLayer.exists && newLayer.isEnabled, "Layer › New › Layer… is not enabled")
        app.typeKey(.escape, modifierFlags: [])
        app.typeKey(.escape, modifierFlags: [])
        sleep(1)

        // Help's search field finds menu commands.
        bar.menuBarItems["Help"].click()
        sleep(1)
        app.typeText("Gaussian")
        sleep(3)
        shot("Help-search")
        app.typeKey(.escape, modifierFlags: [])
    }
}
