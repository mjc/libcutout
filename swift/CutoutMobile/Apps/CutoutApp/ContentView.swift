import Accessibility
import CutoutMobile
import Foundation
import SwiftUI

struct ContentView: View {
    let model: CutoutAppModel
    @Binding private var navigationPath: [CutoutAppRoute]
    @AccessibilityFocusState private var focusedRoute: CutoutAppRoute?

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
                    pair: pair,
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
                ZStack {
                    PevColors.pageBackground
                        .ignoresSafeArea()

                    PevAppShell(
                        sectionTitle: appSectionTitle,
                        tabs: appTabs,
                        connectionPhase: model.phase,
                        selectedColor: appSelectedColor,
                        unselectedColor: PevColors.muted,
                        disconnect: disconnectAndReturnToPicker,
                        selectTarget: selectTarget
                    ) {
                        routedContent
                    }
                    .accessibilityElement(children: .contain)
                    .accessibilityLabel(appSectionTitle)
                    .accessibilityFocused($focusedRoute, equals: destination)
                }
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
            if let announcement = phase.accessibilityAnnouncement {
                AccessibilityNotification.Announcement(announcement).post()
            }
            if case .failed = phase {
                disconnectAndReturnToPicker()
                return
            }
            if route.requiresLiveSession && phase != .live {
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

        guard model.pair(platformIdentifier: row.id) else { return }
    }

    private func openRideScreen(ifNeededFor phase: SessionConnectionPhase) {
        guard !model.isRecordOnlyCapture else { return }
        guard model.selectedRideTitle != nil else { return }
        guard phase.opensRideScreen else { return }
        guard route == .devicePicker else { return }
        navigate(to: CutoutAppRoute.route(for: model.selectedConnectionRoute))
    }

    private func selectScreen(_ screenID: PevScreenID) {
        navigate(to: CutoutAppRoute.route(for: screenID))
    }

    private func selectTarget(_ target: PevNavigationTarget) {
        navigate(to: CutoutAppRoute.route(forNavigationTarget: target))
    }

    private func navigate(to route: CutoutAppRoute) {
        navigationPath = CutoutAppRoute.navigationPath(for: route)
    }

    private func disconnectAndReturnToPicker() {
        model.disconnectAndSearch()
        navigate(to: .devicePicker)
    }

    @ViewBuilder
    private var routedContent: some View {
        if let screen = screen(for: route) {
            if screen.id == .eucRide || screen.id == .vescRide {
                TimelineView(.periodic(from: .now, by: 1)) { _ in
                    screenContainer(screen, now: model.currentMonotonicTime)
                }
            } else {
                screenContainer(screen, now: model.currentMonotonicTime)
            }
        } else if route == .capture {
            CaptureRecordingScreen(
                deviceKind: model.recordOnlyDeviceKind,
                captureStatusText: model.captureStatusText,
                activeLabels: model.activeCaptureLabels,
                disconnect: disconnectAndReturnToPicker,
                startCaptureLabel: model.startCaptureLabel,
                stopCaptureLabel: model.stopCaptureLabel
            )
        }
    }

    private func screenContainer(_ screen: PevScreen, now: MonotonicMilliseconds) -> some View {
        PevScreenContainer(
            screen: screen,
            rideState: model.selectedRideTitle == nil && model.phase == .starting && model.displayState.notificationCount == 0
                ? nil
                : model.rideState,
            rideTitle: model.selectedRideTitle,
            settingsReadback: model.settingsReadback,
            faultHistoryReadback: model.faultHistoryReadback,
            bmsSnapshot: model.bmsSnapshot,
            phoneLocationReadback: model.phoneLocationReadback,
            vescSnapshot: model.vescRideSnapshot,
            now: now,
            connectionPhase: model.phase,
            notificationCount: model.displayState.notificationCount,
            captureStatusText: model.captureStatusText,
            disconnect: disconnectAndReturnToPicker
        )
    }

    private var appSectionTitle: String {
        switch route {
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

    private var appTabs: [PevScreenTab] {
        route.navigationTabs
    }

    private var appSelectedColor: Color {
        switch route {
        case .vescRide, .vescDebug:
            PevColors.purple
        default:
            PevColors.yellow
        }
    }

    private func screen(for route: CutoutAppRoute) -> PevScreen? {
        switch route {
        case .devicePicker:
            nil
        case .eucRide:
            catalog.screen(id: .eucRide)
        case .eucPack(let screenID):
            catalog.screen(id: screenID).map {
                catalog.presentedScreen(for: $0, liveBmsSnapshot: model.bmsSnapshot)
            }
        case .vescRide:
            catalog.screen(id: .vescRide)
        case .vescDebug:
            catalog.screen(id: .vescDebug)
        case .capture:
            nil
        }
    }
}
