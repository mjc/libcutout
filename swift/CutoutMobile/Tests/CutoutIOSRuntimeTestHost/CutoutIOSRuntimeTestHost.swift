import SwiftUI

// This host supplies an application sandbox without starting production services.
@main
struct CutoutIOSRuntimeTestHost: App {
    var body: some Scene {
        WindowGroup {
            Color.clear
        }
    }
}
