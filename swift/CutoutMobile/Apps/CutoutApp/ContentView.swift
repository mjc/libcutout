import Accessibility
import CutoutMobile
import Foundation
import SwiftUI
#if os(iOS)
import UIKit
#endif

struct ContentView: View {
    let model: CutoutAppModel
    @Binding private var navigationPath: [CutoutAppRoute]
    @AccessibilityFocusState private var focusedRoute: CutoutAppRoute?
    @State private var connectionAnnouncements = ConnectionAccessibilityAnnouncements()

    init(model: CutoutAppModel, navigationPath: Binding<[CutoutAppRoute]>) {
        self.model = model
        _navigationPath = navigationPath
    }

    private var route: CutoutAppRoute {
        navigationPath.last ?? .devicePicker
    }

    var body: some View {
        NavigationStack(path: $navigationPath) {
            ZStack {
                PevColors.pageBackground
                    .ignoresSafeArea()

                DevicePickerRouteView(model: model, pair: pair, navigate: navigate)
                .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .top)
                .accessibilityLabel(localizedAppText("picker.title"))
                .accessibilityFocused($focusedRoute, equals: .devicePicker)
            }
            .navigationDestination(for: CutoutAppRoute.self) { destination in
                destinationContent(for: destination)
                    .navigationBarBackButtonHidden(true)
            }
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .background(PevColors.pageBackground.ignoresSafeArea())
        .onChange(of: route, initial: true) { _, route in
            focusedRoute = route
        }
        .onChange(of: model.captureStatus) { _, status in
            if let announcement = status?.accessibilityAnnouncement {
                AccessibilityNotification.Announcement(announcement).post()
            }
        }
        .onChange(of: model.phase) { _, phase in
            if let announcement = connectionAnnouncements.next(for: phase) {
                AccessibilityNotification.Announcement(announcement).post()
            }
        }
        .onChange(of: model.connectionState) { _, state in
            if let announcement = connectionAnnouncements.next(for: state) {
                AccessibilityNotification.Announcement(announcement).post()
            }
            switch state.navigationIntent(isRecordOnlyCapture: model.isRecordOnlyCapture) {
            case .returnToPicker:
                navigate(to: .devicePicker)
            case let .openRide(connectionRoute) where route == .devicePicker:
                navigate(to: CutoutAppRoute.route(for: connectionRoute))
            case .stay, .openRide:
                break
            }
        }
        .onChange(of: model.devicePickerScanState?.status) { _, _ in
            guard let scanState = model.devicePickerScanState,
                  let announcement = connectionAnnouncements.next(for: scanState) else {
                return
            }
            AccessibilityNotification.Announcement(announcement).post()
        }
        .onChange(of: model.bmsSnapshot?.accessibilityAlertLevel) { _, level in
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

    private func pair(_ row: DevicePickerRow) {
        guard row.isSupported else { return }

        connectionAnnouncements.beginUserInitiatedAttempt()
        guard model.pair(platformIdentifier: row.id) else { return }
    }

    private func selectTarget(_ target: PevNavigationTarget) {
        navigate(to: CutoutAppRoute.route(forNavigationTarget: target))
    }

    private func navigate(to route: CutoutAppRoute) {
        navigationPath = CutoutAppRoute.navigationPath(for: route)
    }

    private func disconnectAndReturnToPicker() {
        model.disconnectTransport()
        navigate(to: .devicePicker)
    }

    private func finishCaptureAndReturnToPicker() {
        guard model.finishCapture() else { return }
        navigate(to: .devicePicker)
    }

    @ViewBuilder
    private func destinationContent(for destination: CutoutAppRoute) -> some View {
        if destination == .capture {
            ZStack {
                PevColors.pageBackground
                    .ignoresSafeArea()
                routedContent(for: destination)
            }
            .accessibilityFocused($focusedRoute, equals: destination)
        } else {
            let tabs = TabView(selection: tabSelection) {
                ForEach(availableTabs) { tab in
                    if let tabRoute = destination.destination(for: tab) {
                        Tab(value: tab.id) {
                            connectedDestination(for: tabRoute)
                                .id(tabRoute)
                        } label: {
                            Label(tab.title, systemImage: tab.id.systemImage)
                        }
                        .accessibilityIdentifier(tab.accessibilityIdentifier)
                    }
                }
            }
            .tint(tabAccent)
#if os(iOS)
            tabs
                .toolbarBackground(PevColors.pageBackground, for: .tabBar)
                .toolbarBackground(.visible, for: .tabBar)
#else
            tabs
#endif
        }
    }

    private func connectedDestination(for destination: CutoutAppRoute) -> some View {
        ZStack {
            PevColors.pageBackground
                .ignoresSafeArea()

            PevAppShell(
                sectionTitle: appSectionTitle(for: destination),
                disconnect: disconnectAndReturnToPicker
            ) {
                routedContent(for: destination)
            }
            .accessibilityElement(children: .contain)
            .accessibilityLabel(appSectionTitle(for: destination))
            .accessibilityFocused($focusedRoute, equals: destination)
        }
    }

    @ViewBuilder
    private func routedContent(for destination: CutoutAppRoute) -> some View {
        switch destination {
        case .eucRide:
            EucRideRouteView(model: model)
        case .eucPack(let packScreen):
            EucPackRouteView(
                model: model,
                packScreen: packScreen,
                selectedGroupIndex: destination.selectedBmsGroupIndex,
                navigate: navigate
            )
        case .vescRide:
            VescRideRouteView(model: model)
        case .vescDebug:
            VescDebugRouteView(model: model)
        case .capture:
            CaptureRouteView(model: model, finishCapture: finishCaptureAndReturnToPicker)
        case .devicePicker:
            EmptyView()
        }
    }

    private func appSectionTitle(for destination: CutoutAppRoute) -> String {
        switch destination {
        case .eucRide, .vescRide:
            localizedAppText("navigation.section.ride")
        case .eucPack:
            localizedAppText("navigation.section.pack")
        case .vescDebug:
            localizedAppText("navigation.section.debug")
        case .capture:
            localizedAppText("navigation.section.capture")
        case .devicePicker:
            localizedAppText("navigation.section.ride")
        }
    }

    private var availableTabs: [PevScreenTab] {
        route.availableNavigationTabs
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
        switch route {
        case .vescRide, .vescDebug:
            Color(uiColor: UIColor { traits in
                traits.userInterfaceStyle == .dark
                    ? .systemPurple
                    : UIColor(red: 0.34, green: 0.08, blue: 0.52, alpha: 1)
            })
        default:
            Color(uiColor: UIColor { traits in
                traits.userInterfaceStyle == .dark
                    ? .systemYellow
                    : UIColor(red: 0.45, green: 0.25, blue: 0.0, alpha: 1)
                })
        }
        #else
        .primary
        #endif
    }

}

struct DevicePickerRouteView: View {
    let model: CutoutAppModel
    let pair: (DevicePickerRow) -> Void
    let navigate: (CutoutAppRoute) -> Void

    var body: some View {
        DevicePickerView(
            scanState: model.devicePickerScanState,
            connectionPhase: model.phase,
            captureStatusText: model.captureStatusText,
            hasSavedDevice: model.hasSavedDevice,
            pair: pair,
            forgetSavedDevice: model.forgetSavedDevice,
            probe: { row in model.startProbe(platformIdentifier: row.id) },
            recordOnly: { row, deviceKind in
                guard model.recordOnly(platformIdentifier: row.id, deviceKind: deviceKind) else { return false }
                navigate(model.isRecordOnlyCapture ? .capture : .eucRide)
                return true
            }
        )
    }
}

private struct EucRideRouteView: View {
    let model: CutoutAppModel

    var body: some View {
        TimelineView(.periodic(from: .now, by: 1)) { _ in
            EucRideScreenView(
                rideState: model.eucRidePresentationState,
                rideTitle: model.selectedRideTitle,
                now: model.currentMonotonicTime,
                captureStatusText: model.captureStatusText,
                connectionStatusText: model.connectionStatusText,
                phoneLocationReadback: model.phoneLocationReadback
            )
            .accessibilityElement(children: .contain)
            .accessibilityIdentifier("dashboard.screen.eucRide")
        }
    }
}

struct CaptureRouteView: View {
    let model: CutoutAppModel
    let finishCapture: () -> Void

    var body: some View {
        CaptureRecordingScreen(
            deviceKind: model.recordOnlyDeviceKind,
            captureStatusText: model.captureStatusText,
            captureStatusTone: model.captureStatus?.statusStripTone ?? .nominal,
            captureProgress: model.captureProgress,
            activeLabels: model.activeCaptureLabels,
            isFinishing: model.isFinishingCapture,
            finishCapture: finishCapture,
            startCaptureLabel: model.startCaptureLabel,
            stopCaptureLabel: model.stopCaptureLabel
        )
    }
}

struct EucPackRouteView: View {
    let model: CutoutAppModel
    let packScreen: EucPackScreen
    let selectedGroupIndex: Int?
    let navigate: (CutoutAppRoute) -> Void

    private let catalog = PevScreenCatalog.live

    var body: some View {
        if let screen = bmsScreen {
            let rideState = screen.bmsContentOrUnavailable.kind == .noData ? model.rideState : nil
            BmsScreenView(
                screen: screen,
                rideState: rideState,
                bmsSnapshot: model.bmsSnapshot,
                selectedGroupIndex: selectedGroupIndex,
                showGroupDetail: { groupIndex in
                    navigate(.eucPack(.bmsCellDetail(groupIndex)))
                },
                showCellMap: {
                    navigate(.eucPack(.root))
                }
            )
            .accessibilityElement(children: .contain)
            .accessibilityIdentifier("dashboard.screen.\(screen.id.rawValue)")
            .onChange(of: model.bmsSnapshot?.groups.map(\.index), initial: true) { _, groupIndices in
                guard !packScreen.hasAvailableSelectedGroup(in: groupIndices) else { return }
                navigate(.eucPack(.root))
            }
        }
    }

    private var bmsScreen: PevScreen? {
        if let screenID = packScreen.screenID {
            catalog.screen(id: screenID).map {
                catalog.presentedScreen(for: $0, liveBmsSnapshot: model.bmsSnapshot)
            }
        } else {
            catalog.presentedBmsScreen(liveBmsSnapshot: model.bmsSnapshot)
        }
    }
}

private struct VescRideRouteView: View {
    let model: CutoutAppModel

    var body: some View {
        TimelineView(.periodic(from: .now, by: 1)) { _ in
            VescRideScreenView(
                liveSnapshot: model.vescRideSnapshot,
                phase: model.phase,
                now: model.currentMonotonicTime,
                captureStatusText: model.captureStatusText,
                connectionStatusText: model.connectionStatusText
            )
            .accessibilityElement(children: .contain)
            .accessibilityIdentifier("dashboard.screen.vescRide")
        }
    }
}

private struct VescDebugRouteView: View {
    let model: CutoutAppModel

    var body: some View {
        VescDebugScreenView(
            snapshot: model.vescRideSnapshot,
            phase: model.phase,
            notificationCount: model.displayState.notificationCount,
            captureStatusText: model.captureStatusText,
            connectionStatusText: model.connectionStatusText
        )
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("dashboard.screen.vescDebug")
    }
}

private extension PevScreenTabID {
    var systemImage: String {
        switch self {
        case .ride:
            "speedometer"
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
