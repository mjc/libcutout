import Accessibility
import CutoutMobile
import Foundation
import SwiftUI

#if os(iOS)
    import UIKit
#endif

struct ContentView: View {
    let model: CutoutAppModel
    let rideMapPresentation: RideMapPresentationState
    let lighting: LightingRouteModel
    @Binding private var navigationPath: [CutoutAppRoute]
    @AccessibilityFocusState private var focusedRoute: CutoutAppRoute?
    @State private var connectionAnnouncements = ConnectionAccessibilityAnnouncements()
    @State private var presentedSheet: Sheet?
    @Environment(\.openURL) private var openURL

    private enum Sheet: Identifiable {
        case setup, music, musicSettings
        var id: Self { self }
    }

    init(
        model: CutoutAppModel,
        rideMapPresentation: RideMapPresentationState,
        lighting: LightingRouteModel,
        navigationPath: Binding<[CutoutAppRoute]>
    ) {
        self.model = model
        self.rideMapPresentation = rideMapPresentation
        self.lighting = lighting
        _navigationPath = navigationPath
    }

    private var route: CutoutAppRoute {
        navigationPath.last ?? .devicePicker
    }

    private var rootRoute: CutoutAppRoute {
        CutoutAppRoute.navigationRoot(for: navigationPath)
    }

    private func stackNavigationPath(for tabRoute: CutoutAppRoute) -> Binding<[CutoutAppRoute]> {
        Binding(
            get: { CutoutAppRoute.stackNavigationPath(for: navigationPath, root: tabRoute) },
            set: {
                navigationPath = CutoutAppRoute.replacingStackNavigationPath($0, in: navigationPath, root: tabRoute)
            }
        )
    }

    var body: some View {
        primaryTabs
            .frame(maxWidth: .infinity, maxHeight: .infinity)
            .background(PevColors.pageBackground.ignoresSafeArea())
            .sheet(item: $presentedSheet) { destination in
                switch destination {
                case .setup:
                    AppSetupView(model: model)
                case .music:
                    AppMusicPlayerView(model: model, onOpenSettings: { presentedSheet = .musicSettings })
                case .musicSettings:
                    AppSetupView(model: model, opensMusic: true)
                }
            }
            .safeAreaInset(edge: .top) {
                if let presentation = model.liveActivityError?.failurePresentation {
                    liveActivityFailureBanner(presentation)
                }
            }
            .onChange(of: route, initial: true) { _, route in
                focusedRoute = route
            }
            .onChange(of: model.capture.status) { _, status in
                if let announcement = status?.accessibilityAnnouncement {
                    AccessibilityNotification.Announcement(announcement).post()
                }
            }
            .onChange(of: model.device.phase) { _, phase in
                if let announcement = connectionAnnouncements.next(for: phase) {
                    AccessibilityNotification.Announcement(announcement).post()
                }
            }
            .onChange(of: model.device.connectionState, initial: true) { _, state in
                if let announcement = connectionAnnouncements.next(for: state) {
                    AccessibilityNotification.Announcement(announcement).post()
                }
                switch state.navigationIntent(isRecordOnlyCapture: model.isRecordOnlyCapture) {
                case .returnToPicker where !route.preservesNavigationOnConnectionLoss:
                    navigate(to: .devicePicker)
                case .returnToPicker:
                    break
                case .openRide(let connectionRoute) where route == .devicePicker:
                    navigate(to: CutoutAppRoute.route(for: connectionRoute))
                case .stay, .openCapture, .openRide:
                    break
                }
            }
            .onChange(of: model.device.scanState?.status) { _, _ in
                guard let scanState = model.device.scanState,
                    let announcement = connectionAnnouncements.next(for: scanState)
                else {
                    return
                }
                AccessibilityNotification.Announcement(announcement).post()
            }
            .onChange(of: model.device.bmsSnapshot?.accessibilityAlertLevel) { _, level in
                if let announcement = level?.accessibilityAnnouncement {
                    AccessibilityNotification.Announcement(announcement).post()
                }
            }
            .onChange(of: model.liveActivityError) { _, error in
                if let error {
                    AccessibilityNotification.Announcement(error.accessibilityAnnouncement).post()
                }
            }
    }

    private func liveActivityFailureBanner(_ presentation: LiveActivityFailurePresentation) -> some View {
        HStack(spacing: 12) {
            Image(systemName: "exclamationmark.triangle.fill")
                .foregroundStyle(PevColors.warningText)
            Text(presentation.message)
                .font(.subheadline)
                .frame(maxWidth: .infinity, alignment: .leading)
            Button(presentation.actionTitle) {
                performLiveActivityRecovery(presentation.action)
            }
            .buttonStyle(.bordered)
        }
        .padding(.horizontal, 16)
        .padding(.vertical, 10)
        .background(PevColors.warningFill)
        .overlay(alignment: .bottom) {
            Rectangle()
                .fill(PevColors.warningStroke)
                .frame(height: 1)
        }
        .accessibilityIdentifier("live-activity.failure")
    }

    private func performLiveActivityRecovery(_ action: LiveActivityFailureAction) {
        switch action {
        case .retry:
            model.retryLiveActivity()
        case .openSettings:
            #if os(iOS)
                if let settingsURL = URL(string: UIApplication.openSettingsURLString) {
                    openURL(settingsURL)
                }
            #else
                model.retryLiveActivity()
            #endif
        }
    }

    private func pair(_ row: DevicePickerRow) {
        guard row.isSupported || row.isProbeRecommended else { return }

        connectionAnnouncements.beginUserInitiatedAttempt()
        guard model.pair(platformIdentifier: row.id) else { return }
    }

    private func selectTarget(_ target: PevNavigationTarget) {
        navigate(
            to: route.destination(forNavigationTarget: target, connectionRoute: model.device.selectedConnectionRoute))
    }

    private func openMusicPlayer() {
        model.music.restorePlayer()
        presentedSheet = .music
    }

    private func navigate(to route: CutoutAppRoute) {
        navigationPath = CutoutAppRoute.navigationPath(for: route)
    }

    private func closeRideMapDetail() {
        guard case .rideMapDetail = navigationPath.last else {
            navigate(to: .rideMap)
            return
        }
        navigationPath.removeLast()
    }

    private func closeNestedDestination() {
        guard !CutoutAppRoute.stackNavigationPath(for: navigationPath).isEmpty else { return }
        navigationPath.removeLast()
    }

    private func disconnectAndReturnToPicker() {
        Task { @MainActor in
            guard await model.disconnectTransport() else { return }
            navigate(to: .devicePicker)
        }
    }

    @ViewBuilder
    private var primaryTabs: some View {
        let tabs = TabView(selection: tabSelection) {
            ForEach(availableTabs) { tab in
                if let tabRoute = rootRoute.destination(for: tab, connectionRoute: model.device.selectedConnectionRoute)
                {
                    Tab(value: tab.id) {
                        NavigationStack(path: stackNavigationPath(for: tabRoute)) {
                            destinationSurface(for: tabRoute)
                                .navigationDestination(for: CutoutAppRoute.self) { destination in
                                    destinationContent(for: destination)
                                        .navigationBarBackButtonHidden(destination != .capture)
                                        #if os(iOS)
                                            .toolbarVisibility(
                                                destination == .capture ? .visible : .hidden, for: .navigationBar)
                                        #endif
                                }
                                #if os(iOS)
                                    .toolbarVisibility(.hidden, for: .navigationBar)
                                #endif
                        }
                    } label: {
                        Label(tab.title, systemImage: tab.id.systemImage)
                    }
                    .accessibilityIdentifier(tab.accessibilityIdentifier)
                }
            }
        }
        .tint(tabAccent)
        .appMusicCompactPlayer(
            model: model,
            onOpenDetails: openMusicPlayer,
            onOpenSettings: { presentedSheet = .musicSettings }
        )
        #if os(iOS)
            tabs
                .toolbarBackground(PevColors.pageBackground, for: .tabBar)
                .toolbarBackgroundVisibility(.visible, for: .tabBar)
        #else
            tabs
        #endif
    }

    @ViewBuilder
    private func destinationContent(for destination: CutoutAppRoute) -> some View {
        if destination == .capture {
            routedContent(for: destination)
                .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .top)
                .background(PevColors.pageBackground.ignoresSafeArea())
                .accessibilityFocused($focusedRoute, equals: destination)
        } else {
            destinationSurface(for: destination)
        }
    }

    private func isRideMapDetail(_ destination: CutoutAppRoute) -> Bool {
        if case .rideMapDetail = destination { return true }
        return false
    }

    private func disconnectAction(for destination: CutoutAppRoute) -> (() -> Void)? {
        guard
            CutoutNavigationCommands.canDisconnect(
                currentRoute: destination,
                hasConnection: model.device.selectedConnectionRoute != nil
            )
        else { return nil }
        return { disconnectAndReturnToPicker() }
    }

    @ViewBuilder
    private func destinationSurface(for destination: CutoutAppRoute) -> some View {
        let back: (() -> Void)? = destination.isMoreDestination ? { closeNestedDestination() } : nil
        Group {
            if destination == .devicePicker || isRideMapDetail(destination) {
                routedContent(for: destination)
            } else {
                PevAppShell(
                    sectionTitle: appSectionTitle(for: destination),
                    isRideScreen: destination == .eucRide || destination == .vescRide,
                    disconnect: disconnectAction(for: destination),
                    back: back
                ) {
                    routedContent(for: destination)
                }
            }
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .top)
        .background(PevColors.pageBackground.ignoresSafeArea())
        .accessibilityElement(children: .contain)
        .accessibilityLabel(appSectionTitle(for: destination))
        .accessibilityFocused($focusedRoute, equals: destination)
    }

    @ViewBuilder
    private func routedContent(for destination: CutoutAppRoute) -> some View {
        switch destination {
        case .more:
            List {
                ForEach(destination.moreNavigationTabs(for: model.device.selectedConnectionRoute)) { tab in
                    if let target = tab.destinationTarget {
                        NavigationLink(
                            value: destination.destination(
                                forNavigationTarget: target, connectionRoute: model.device.selectedConnectionRoute)
                        ) {
                            HStack(spacing: 12) {
                                Image(systemName: tab.id.systemImage)
                                    .foregroundStyle(.tint)
                                    .accessibilityHidden(true)
                                Text(tab.title)
                                    .lineLimit(nil)
                                    .fixedSize(horizontal: false, vertical: true)
                                    .frame(maxWidth: .infinity, alignment: .leading)
                            }
                            .frame(minHeight: 44)
                        }
                        .accessibilityIdentifier(tab.accessibilityIdentifier)
                    }
                }
                Section {
                    Button(action: openMusicPlayer) {
                        HStack(spacing: 12) {
                            Image(systemName: "music.note")
                                .accessibilityHidden(true)
                            Text(pevLocalizedText("music.settings.title"))
                                .lineLimit(nil)
                                .fixedSize(horizontal: false, vertical: true)
                                .frame(maxWidth: .infinity, alignment: .leading)
                        }
                        .frame(minHeight: 44)
                    }
                    .accessibilityIdentifier("more.music")
                    if model.music.isPlayerHidden {
                        Button(action: model.music.restorePlayer) {
                            HStack(spacing: 12) {
                                Image(systemName: "play.rectangle")
                                    .accessibilityHidden(true)
                                Text(pevLocalizedText("music.restore"))
                                    .lineLimit(nil)
                                    .fixedSize(horizontal: false, vertical: true)
                                    .frame(maxWidth: .infinity, alignment: .leading)
                            }
                            .frame(minHeight: 44)
                        }
                        .accessibilityIdentifier("music.restore")
                    }
                }
            }
            .scrollContentBackground(.hidden)
            .accessibilityIdentifier("more.screen")
        case .eucRide:
            EucRideRouteView(model: model)
        case .lighting:
            LightingRouteView(model: lighting, rideModel: model)
        case .eucPack(let packScreen):
            EucPackRouteView(
                device: model.device,
                packScreen: packScreen,
                selectedGroupIndex: destination.selectedBmsGroupIndex,
                navigate: navigate
            )
        case .eucTune:
            EucTuneRouteView(
                model: model,
                submitSetting: model.submitDeviceSetting,
                submitAction: model.submitDeviceAction
            )
        case .vescRide:
            VescRideRouteView(model: model)
        case .vescDebug:
            VescDebugRouteView(device: model.device, capture: model.capture)
        case .capture:
            CaptureRouteView(capture: model.capture)
        case .camera:
            CameraRouteContainerView(
                showsHeader: false,
                recordMediaReference: { fileName, generation, provenance, localURL in
                    model.recordCameraMediaReference(
                        captureFileName: fileName,
                        captureGeneration: generation,
                        provenance: provenance,
                        localURL: localURL
                    )
                },
                currentCaptureIdentity: model.currentCameraCaptureIdentity,
                sessionState: model.cameraSessionStateHandle
            )
        case .rideMap:
            RideMapRouteView(
                model: model, presentation: rideMapPresentation,
                { rideID in
                    rideMapPresentation.mode = .history
                    navigate(to: .rideMapDetail(rideID: rideID))
                })
        case .rideMapDetail(let rideID):
            RideMapRouteView(
                model: model,
                presentation: rideMapPresentation,
                initialHistoryID: rideID,
                detailOnly: true,
                closeDetail: closeRideMapDetail
            )
        case .devicePicker:
            DevicePickerRouteView(
                device: model.device,
                pair: pair,
                navigate: navigate,
                openSetup: { presentedSheet = .setup }
            )
            .accessibilityLabel(localizedAppText("picker.title"))
        }
    }

    private func appSectionTitle(for destination: CutoutAppRoute) -> String {
        switch destination {
        case .more:
            localizedAppText("navigation.section.more")
        case .eucRide, .vescRide:
            localizedAppText("navigation.section.ride")
        case .lighting:
            localizedAppText("navigation.section.lighting")
        case .eucPack:
            localizedAppText("navigation.section.pack")
        case .eucTune:
            localizedAppText("navigation.section.tune")
        case .vescDebug:
            localizedAppText("navigation.section.debug")
        case .capture:
            localizedAppText("navigation.section.capture")
        case .camera:
            localizedAppText("navigation.section.camera")
        case .rideMap, .rideMapDetail:
            localizedAppText("navigation.section.map")
        case .devicePicker:
            localizedAppText("picker.title")
        }
    }

    private var availableTabs: [PevScreenTab] {
        rootRoute.primaryNavigationTabs(for: model.device.selectedConnectionRoute)
    }

    private var tabSelection: Binding<PevScreenTabID> {
        Binding(
            get: {
                availableTabs.first(where: \.isSelected)?.id ?? .ride
            },
            set: { selectedID in
                guard selectedID != availableTabs.first(where: \.isSelected)?.id else { return }
                guard let target = availableTabs.first(where: { $0.id == selectedID })?.destinationTarget else {
                    return
                }
                selectTarget(target)
            }
        )
    }

    private var tabAccent: Color {
        #if os(iOS)
            switch Self.accentKind(selectedConnectionRoute: model.device.selectedConnectionRoute, route: route) {
            case .purple:
                Color(uiColor: TabAccentColors.purple)
            case .yellow:
                Color(uiColor: TabAccentColors.yellow)
            default:
                .primary
            }
        #else
            .primary
        #endif
    }

    static func accentKind(
        selectedConnectionRoute: DevicePickerConnectionRoute?,
        route: CutoutAppRoute
    ) -> PevAccent {
        switch selectedConnectionRoute {
        case .vescOnewheel:
            .purple
        case .electricUnicycle:
            .yellow
        case nil:
            switch route {
            case .vescRide, .vescDebug, .lighting(.vesc):
                .purple
            default:
                .yellow
            }
        }
    }

}

extension PevScreenTabID {
    fileprivate var systemImage: String {
        switch self {
        case .devices:
            "bolt.horizontal.circle"
        case .more:
            "ellipsis"
        case .camera:
            "video"
        case .ride:
            "speedometer"
        case .lighting:
            "lightbulb.2"
        case .pack:
            "battery.100percent"
        case .debug:
            "wrench.and.screwdriver"
        case .map:
            "map"
        case .tune:
            "slider.horizontal.3"
        case .logs:
            "doc.text"
        }
    }
}
