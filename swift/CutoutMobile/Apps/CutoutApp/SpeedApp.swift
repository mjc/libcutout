#if os(macOS)
import AppKit
#endif
import SwiftUI

@main
struct CutoutApp: App {
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
        WindowGroup("CutoutApp") {
            rootView
                .task {
                    model.start()
                }
        }
    }

    @ViewBuilder
    private var rootView: some View {
        #if os(macOS)
        ContentView(model: model)
            .frame(minWidth: 360, minHeight: 280)
        #else
        ContentView(model: model)
        #endif
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
