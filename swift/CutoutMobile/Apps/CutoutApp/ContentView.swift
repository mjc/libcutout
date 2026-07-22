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

    private let catalog = PevScreenCatalog.live

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

                DevicePickerView(
                    scanState: model.devicePickerScanState,
                    connectionPhase: model.phase,
                    captureStatusText: model.captureStatusText,
                    isRecordOnlyCapture: model.isRecordOnlyCapture,
                    hasSavedDevice: model.hasSavedDevice,
                    pair: pair,
                    forgetSavedDevice: model.forgetSavedDevice,
                    recordOnly: { row, deviceKind in
                        if model.recordOnly(platformIdentifier: row.id, deviceKind: deviceKind) {
                            navigate(to: model.isRecordOnlyCapture ? .capture : .eucRide)
                        }
                    }
                )
                .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .top)
                .accessibilityLabel("Choose device")
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
            if case .failed = phase {
                navigate(to: .devicePicker)
                return
            }
            openRideScreen(ifNeededFor: phase)
        }
        .onChange(of: model.bmsSnapshot?.accessibilityAlertLevel) { _, level in
            if let announcement = level?.accessibilityAnnouncement {
                AccessibilityNotification.Announcement(announcement).post()
            }
        }
    }

    private func pair(_ row: DevicePickerRow) {
        guard row.isSupported else { return }

        connectionAnnouncements.beginUserInitiatedAttempt()
        guard model.pair(platformIdentifier: row.id) else { return }
    }

    private func openRideScreen(ifNeededFor phase: SessionConnectionPhase) {
        guard !model.isRecordOnlyCapture else { return }
        guard model.selectedRideTitle != nil else { return }
        guard phase.opensRideScreen else { return }
        guard route == .devicePicker else { return }
        navigate(to: CutoutAppRoute.route(for: model.selectedConnectionRoute))
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
            TabView(selection: tabSelection) {
                ForEach(availableTabs) { tab in
                    if let target = tab.destinationTarget {
                        let tabRoute = contentRoute(for: tab, destination: destination, target: target)
                        Tab(value: tab.id) {
                            connectedDestination(for: tabRoute)
                        } label: {
                            Label(tab.title, systemImage: tab.id.systemImage)
                        }
                        .accessibilityIdentifier(tab.accessibilityIdentifier)
                    }
                }
            }
            .tint(tabAccent)
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

    private func contentRoute(
        for tab: PevScreenTab,
        destination: CutoutAppRoute,
        target: PevNavigationTarget
    ) -> CutoutAppRoute {
        if tab.id == .pack, case .eucPack = destination {
            return destination
        }
        return CutoutAppRoute.route(forNavigationTarget: target)
    }

    @ViewBuilder
    private func routedContent(for destination: CutoutAppRoute) -> some View {
        switch destination {
        case .eucRide:
            TimelineView(.periodic(from: .now, by: 1)) { _ in
                EucRideScreenView(
                    rideState: model.selectedRideTitle == nil && model.phase == .starting && model.displayState.notificationCount == 0
                        ? nil
                        : model.rideState,
                    rideTitle: model.selectedRideTitle,
                    now: model.currentMonotonicTime,
                    captureStatusText: model.captureStatusText,
                    phoneLocationReadback: model.phoneLocationReadback
                )
                .accessibilityElement(children: .contain)
                .accessibilityIdentifier("dashboard.screen.eucRide")
            }
        case .eucPack(let packScreen):
            if let screen = bmsScreen(for: packScreen) {
                BmsScreenView(
                    screen: screen,
                    rideState: model.rideState,
                    bmsSnapshot: model.bmsSnapshot,
                    selectedGroupIndex: destination.selectedBmsGroupIndex,
                    showGroupDetail: { groupIndex in
                        navigate(to: .eucPack(.bmsCellDetail(groupIndex)))
                    },
                    showCellMap: {
                        navigate(to: .eucPack(.root))
                    }
                )
                .accessibilityElement(children: .contain)
                .accessibilityIdentifier("dashboard.screen.\(screen.id.rawValue)")
            }
        case .vescRide:
            TimelineView(.periodic(from: .now, by: 1)) { _ in
                VescRideScreenView(
                    liveSnapshot: model.vescRideSnapshot,
                    phase: model.phase,
                    now: model.currentMonotonicTime,
                    captureStatusText: model.captureStatusText
                )
                .accessibilityElement(children: .contain)
                .accessibilityIdentifier("dashboard.screen.vescRide")
            }
        case .vescDebug:
            VescDebugScreenView(
                snapshot: model.vescRideSnapshot,
                phase: model.phase,
                notificationCount: model.displayState.notificationCount,
                captureStatusText: model.captureStatusText
            )
            .accessibilityElement(children: .contain)
            .accessibilityIdentifier("dashboard.screen.vescDebug")
        case .capture:
            CaptureRecordingScreen(
                deviceKind: model.recordOnlyDeviceKind,
                captureStatusText: model.captureStatusText,
                activeLabels: model.activeCaptureLabels,
                disconnect: disconnectAndReturnToPicker,
                startCaptureLabel: model.startCaptureLabel,
                stopCaptureLabel: model.stopCaptureLabel
            )
        case .devicePicker:
            EmptyView()
        }
    }

    private func appSectionTitle(for destination: CutoutAppRoute) -> String {
        switch destination {
        case .eucRide, .vescRide:
            "Ride"
        case .eucPack:
            "Pack"
        case .vescDebug:
            "Debug"
        case .capture:
            "Capture"
        case .devicePicker:
            "Ride"
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

    private func bmsScreen(for screen: EucPackScreen) -> PevScreen? {
        if let screenID = screen.screenID {
            catalog.screen(id: screenID).map {
                catalog.presentedScreen(for: $0, liveBmsSnapshot: model.bmsSnapshot)
            }
        } else {
            catalog.presentedBmsScreen(liveBmsSnapshot: model.bmsSnapshot)
        }
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
