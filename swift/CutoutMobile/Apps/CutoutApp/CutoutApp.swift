#if os(macOS)
import AppKit
#endif
import SwiftUI

@main
struct CutoutApp: App {
    #if os(macOS)
    @NSApplicationDelegateAdaptor(CutoutAppDelegate.self) private var appDelegate
    #endif
    @StateObject private var model = CutoutAppModel()

    init() {
        if CommandLine.arguments.contains("--smoke") {
            print("cutout_app=ok")
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
final class CutoutAppDelegate: NSObject, NSApplicationDelegate {
    func applicationDidFinishLaunching(_: Notification) {
        NSApp.setActivationPolicy(.regular)
        NSApp.activate(ignoringOtherApps: true)
        if CommandLine.arguments.contains("--launch-smoke") {
            print("cutout_app_launch=ok")
            Foundation.exit(EXIT_SUCCESS)
        }
    }
}
#endif
