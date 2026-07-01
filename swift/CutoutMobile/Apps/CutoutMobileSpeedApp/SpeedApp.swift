#if os(macOS)
import AppKit
#endif
import SwiftUI

@main
struct CutoutMobileSpeedApp: App {
    #if os(macOS)
    @NSApplicationDelegateAdaptor(SpeedAppDelegate.self) private var appDelegate
    #endif
    @StateObject private var model = LiveSpeedModel()

    init() {
        if CommandLine.arguments.contains("--smoke") {
            print("speed_app=ok")
            Foundation.exit(EXIT_SUCCESS)
        }
    }

    var body: some Scene {
        WindowGroup("Cutout Speed") {
            ContentView(model: model)
                .frame(minWidth: 360, minHeight: 280)
                .task {
                    model.start()
                }
        }
    }
}

#if os(macOS)
final class SpeedAppDelegate: NSObject, NSApplicationDelegate {
    func applicationDidFinishLaunching(_: Notification) {
        NSApp.setActivationPolicy(.regular)
        NSApp.activate(ignoringOtherApps: true)
    }
}
#endif
