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
    @State private var model: CutoutAppModel?
    @State private var rideMapPresentation = RideMapPresentationState()
    @State private var lighting: LightingRouteModel?
    @State private var startupError: String?
    @State private var startupAttempt = 0
    @State private var pendingMusicURL: URL?
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
                .task(id: startupAttempt) {
                    await openApplication()
                }
                .onOpenURL { url in
                    if let model { _ = model.music.handleProviderURL(url) }
                    else { pendingMusicURL = url }
                }
                .onChange(of: scenePhase) {
                    switch scenePhase {
                    case .active:
                        model?.appDidBecomeActive()
                    case .background:
                        model?.appDidEnterBackground()
                    case .inactive:
                        break
                    @unknown default:
                        break
                    }
                }
                .alert(
                    pevLocalizedText("music.command.title"),
                    isPresented: Binding(
                        get: { model?.music.commandStatusText != nil },
                        set: { isPresented in
                            guard !isPresented else { return }
                            model?.music.dismissCommandFeedback()
                        }
                    ),
                    presenting: model?.music.commandFeedback
                ) { feedback in
                    Button(pevLocalizedText("music.command.dismiss")) {
                        model?.music.dismissCommandFeedback(requestID: feedback.requestID)
                    }
                } message: { feedback in
                    Text(feedback.messageKey.map { pevLocalizedText($0) } ?? "")
                }
        }
        .commands {
            if let model {
                CutoutNavigationCommands(
                    navigationTabs: navigationTabs,
                    currentRoute: currentRoute,
                    connectionRoute: model.selectedConnectionRoute,
                    navigationPath: $navigationPath,
                    canDisconnect: model.selectedConnectionRoute != nil,
                    disconnect: model.disconnectTransport
                )
            }
        }
    }

    @ViewBuilder
    private var rootView: some View {
        if let model, let lighting {
            ContentView(
                model: model,
                rideMapPresentation: rideMapPresentation,
                lighting: lighting,
                navigationPath: $navigationPath
            )
            #if os(macOS)
            .frame(minWidth: 360, minHeight: 280)
            #endif
        } else if let startupError {
            ContentUnavailableView {
                Label(localizedAppText("app.startup.unavailable"), systemImage: "externaldrive.badge.exclamationmark")
            } description: {
                Text(startupError)
            } actions: {
                Button(localizedAppText("app.startup.retry")) { startupAttempt += 1 }
            }
        } else {
            ProgressView(localizedAppText("app.startup.loading"))
                .frame(maxWidth: .infinity, maxHeight: .infinity)
        }
    }

    @MainActor
    private func openApplication() async {
        guard model == nil else { return }
        startupError = nil
        do {
            let opened = try await CutoutAppModel.open()
            try Task.checkCancellation()
            let lighting = LightingRouteModel()
            self.lighting = lighting
            model = opened
            opened.start(sceneIsActive: scenePhase == .active)
            lighting.startIfRemembered()
            if let pendingMusicURL {
                _ = opened.music.handleProviderURL(pendingMusicURL)
                self.pendingMusicURL = nil
            }
        } catch is CancellationError {
            // A later scene task may retry the same durable bootstrap.
        } catch {
            guard !Task.isCancelled else { return }
            startupError = String(describing: error)
        }
    }

    private var currentRoute: CutoutAppRoute {
        navigationPath.last ?? .devicePicker
    }

    private var navigationTabs: [PevScreenTab] {
        currentRoute.availableNavigationTabs(for: model?.selectedConnectionRoute)
    }
}

struct CutoutNavigationCommands: Commands {
    let navigationTabs: [PevScreenTab]
    let currentRoute: CutoutAppRoute
    let connectionRoute: DevicePickerConnectionRoute?
    @Binding var navigationPath: [CutoutAppRoute]
    let canDisconnect: Bool
    let disconnect: () -> Void

    nonisolated static func shortcut(for tabID: PevScreenTabID) -> Character {
        switch tabID {
        case .ride: "1"
        case .lighting: "7"
        case .pack: "2"
        case .map: "3"
        case .tune: "4"
        case .debug: "5"
        case .logs: "6"
        }
    }

    nonisolated static func canDisconnect(
        currentRoute: CutoutAppRoute,
        hasConnection: Bool
    ) -> Bool {
        currentRoute != .devicePicker && hasConnection
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
                            for: currentRoute.destination(
                                forNavigationTarget: target,
                                connectionRoute: connectionRoute
                            )
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
            .disabled(!Self.canDisconnect(currentRoute: currentRoute, hasConnection: canDisconnect))
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
