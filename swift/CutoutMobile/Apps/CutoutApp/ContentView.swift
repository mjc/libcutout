import Foundation
import CutoutMobile
import SwiftUI

struct ContentView: View {
    @ObservedObject var model: CutoutAppModel
    @State private var route: CutoutAppRoute

    private let catalog = PevScreenCatalog.v2

    init(model: CutoutAppModel) {
        self.model = model
        _route = State(initialValue: CutoutAppRoute.initialRoute())
    }

    var body: some View {
        ZStack {
            PevColors.pageBackground
                .ignoresSafeArea()

            if route == .devicePicker {
                DevicePickerView(
                    scanState: model.devicePickerScanState,
                    connectionPhase: model.phase,
                    captureStatusText: model.captureStatusText,
                    isRecordOnlyCapture: model.isRecordOnlyCapture,
                    pair: pair,
                    recordOnly: { row, deviceKind in
                        if model.recordOnly(platformIdentifier: row.id, deviceKind: deviceKind) {
                            route = model.isRecordOnlyCapture ? .capture : .eucRide
                        }
                    }
                )
                .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .top)
            } else {
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
            }
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .background(PevColors.pageBackground.ignoresSafeArea())
        .onChange(of: model.phase) { _, phase in
            if case .failed = phase {
                model.disconnectAndSearch()
                return
            }
            openRideScreen(ifNeededFor: phase)
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
        route = CutoutAppRoute.route(for: model.selectedConnectionRoute)
    }

    private func selectScreen(_ screenID: PevScreenID) {
        route = CutoutAppRoute.route(for: screenID)
    }

    private func selectTarget(_ target: PevNavigationTarget) {
        switch target {
        case .screen(let screenID):
            selectScreen(screenID)
        case .vescRide:
            route = .vescRide
        }
    }

    private func disconnectAndReturnToPicker() {
        model.disconnectAndSearch()
        route = .devicePicker
    }

    @ViewBuilder
    private var routedContent: some View {
        if let screen = screen(for: route) {
            PevScreenContainer(
                screen: screen,
                devicePickerScanState: model.devicePickerScanState,
                rideState: model.selectedRideTitle == nil && model.phase == .starting && model.displayState.notificationCount == 0
                    ? nil
                    : model.rideState,
                rideTitle: model.selectedRideTitle,
                settingsReadback: model.settingsReadback,
                faultHistoryReadback: model.faultHistoryReadback,
                bmsSnapshot: model.bmsSnapshot,
                captureStatusText: model.captureStatusText,
                isRecordOnlyCapture: model.isRecordOnlyCapture,
                disconnect: disconnectAndReturnToPicker,
                pair: pair,
                recordOnly: { row, deviceKind in
                    if model.recordOnly(platformIdentifier: row.id, deviceKind: deviceKind) {
                        route = model.isRecordOnlyCapture ? .capture : .eucRide
                    }
                },
                selectScreen: selectScreen
            )
        } else if route == .vescRide {
            TimelineView(.periodic(from: .now, by: 1)) { _ in
                VescRideScreenView(
                    liveSnapshot: model.vescRideSnapshot,
                    phase: model.phase,
                    now: model.currentMonotonicTime,
                    captureStatusText: model.captureStatusText,
                    disconnect: disconnectAndReturnToPicker,
                    selectScreen: selectScreen
                )
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

    private var appSectionTitle: String {
        switch route {
        case .eucRide, .vescRide:
            "Ride"
        case .eucMap, .vescMap:
            "Map"
        case .eucTune:
            "Tune"
        case .eucPack:
            "Pack"
        case .vescDebug:
            "Debug"
        case .vescLogs:
            "Logs"
        case .liveActivity:
            "Live Activity"
        case .capture:
            "Capture"
        case .devicePicker:
            "Ride"
        }
    }

    private var appTabs: [PevScreenTab] {
        switch route {
        case .vescRide, .vescDebug, .vescMap, .vescLogs:
            PevRideTabs.vescRideTabs(selected: selectedScreenID)
        default:
            PevRideTabs.eucRideTabs(selected: selectedScreenID)
        }
    }

    private var selectedScreenID: PevScreenID? {
        switch route {
        case .eucRide:
            .eucRide
        case .eucMap:
            .eucMap
        case .eucTune:
            .eucTune
        case .eucPack(let screenID):
            screenID
        case .vescRide:
            nil
        case .vescDebug:
            .vescDebug
        case .vescMap:
            .vescMap
        case .vescLogs:
            .vescLogs
        case .devicePicker, .liveActivity, .capture:
            nil
        }
    }

    private var appSelectedColor: Color {
        switch route {
        case .vescRide, .vescDebug, .vescMap, .vescLogs:
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
        case .eucMap:
            catalog.screen(id: .eucMap)
        case .eucTune:
            catalog.screen(id: .eucTune)
        case .liveActivity:
            catalog.screen(id: .liveActivity)
        case .eucPack(let screenID):
            catalog.screen(id: screenID).map {
                catalog.presentedScreen(for: $0, liveBmsSnapshot: model.bmsSnapshot, previewFallback: false)
            }
        case .vescRide:
            nil
        case .vescDebug:
            catalog.screen(id: .vescDebug)
        case .vescMap:
            catalog.screen(id: .vescMap)
        case .vescLogs:
            catalog.screen(id: .vescLogs)
        case .capture:
            nil
        }
    }
}
