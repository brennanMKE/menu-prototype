// The UI test bundle needs a host app target; the tests drive PhotoCraft by bundle id instead.
import SwiftUI

@main
struct HostApp: App {
    var body: some Scene { WindowGroup { Text("MenuShots host") } }
}
