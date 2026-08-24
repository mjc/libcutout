#if os(macOS)
import AppKit
#endif
import CutoutMobile
import SwiftUI

@main
struct CutoutApp: App {
    #if os(macOS)
    @NSApplicationDelegateAdaptor(CutoutAppDelegate.self) private var appDelegate
    #endif
    @State private var model = CutoutAppModel()
    @State private var rideMapPresentation = RideMapPresentationState()
    @State private var navigationPath = CutoutAppRoute.navigationPath(for: .initialRoute())
    @Environment(\.scenePhase) private var scenePhase

    init() {
        if CommandLine.arguments.contains("--smoke") {
            print("cutout_app=ok")
            Foundation.exit(EXIT_SUCCESS)
        }
    }

    var body: some Scene {
        WindowGroup("CutOut") {
            rootView
                .task {
                    model.start()
                }
                .onChange(of: scenePhase) {
                    switch scenePhase {
                    case .active:
                        model.appDidBecomeActive()
                    case .background:
                        model.appDidEnterBackground()
                    case .inactive:
                        break
                    @unknown default:
                        break
                    }
                }
        }
        .commands {
            CutoutNavigationCommands(
                navigationTabs: navigationTabs,
                currentRoute: currentRoute,
                navigationPath: $navigationPath,
                disconnect: model.disconnectTransport
            )
        }
    }

    @ViewBuilder
    private var rootView: some View {
        #if os(macOS)
            ContentView(
                model: model,
                rideMapPresentation: rideMapPresentation,
                navigationPath: $navigationPath
            )
            .frame(minWidth: 360, minHeight: 280)
        #else
        ContentView(
            model: model,
            rideMapPresentation: rideMapPresentation,
            navigationPath: $navigationPath
        )
        #endif
    }

    private var currentRoute: CutoutAppRoute {
        navigationPath.last ?? .devicePicker
    }

    private var navigationTabs: [PevScreenTab] {
        currentRoute.availableNavigationTabs(for: model.selectedConnectionRoute)
    }
}

struct CutoutNavigationCommands: Commands {
    let navigationTabs: [PevScreenTab]
    let currentRoute: CutoutAppRoute
    @Binding var navigationPath: [CutoutAppRoute]
    let disconnect: () -> Void

    nonisolated static func shortcut(for tabID: PevScreenTabID) -> Character {
        switch tabID {
        case .ride: "1"
        case .pack: "2"
        case .map: "3"
        case .tune: "4"
        case .debug: "5"
        case .logs: "6"
        }
    }

    var body: some Commands {
        CommandMenu(localizedAppText("app.command.navigate")) {
            if navigationTabs.isEmpty {
                Button(localizedAppText("app.command.no_connected_device")) {}
                    .disabled(true)
            } else {
                ForEach(navigationTabs) { tab in
                    Button(tab.title) {
                        guard let target = tab.destinationTarget else { return }
                        navigationPath = CutoutAppRoute.navigationPath(
                            for: .route(forNavigationTarget: target)
                        )
                    }
                    .keyboardShortcut(KeyEquivalent(Self.shortcut(for: tab.id)), modifiers: .command)
                    .disabled(!tab.isEnabled || tab.destinationTarget == nil)
                }
            }

            Divider()

            Button(localizedAppText("app.command.disconnect")) {
                disconnect()
                navigationPath = CutoutAppRoute.navigationPath(for: .devicePicker)
            }
            .keyboardShortcut("d", modifiers: [.command, .shift])
            .disabled(currentRoute == .devicePicker)
        }
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
