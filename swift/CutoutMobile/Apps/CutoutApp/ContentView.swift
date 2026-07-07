import Foundation
import CutoutMobile
import SwiftUI

struct ContentView: View {
    @ObservedObject var model: CutoutAppModel
    @State private var route: CutoutAppRoute

    private let catalog = MockupScreenCatalog.v2

    init(model: CutoutAppModel) {
        self.model = model
        _route = State(initialValue: CutoutAppRoute.initialRoute())
    }

    var body: some View {
        ZStack {
            MockupColors.pageBackground
                .ignoresSafeArea()

            if route == .devicePicker {
                DevicePickerView(
                    scanState: model.devicePickerScanState,
                    captureStatusText: model.captureStatusText,
                    isRecordOnlyCapture: model.isRecordOnlyCapture,
                    pair: pair,
                    recordOnly: { row, deviceKind in
                        if model.recordOnly(platformIdentifier: row.id, deviceKind: deviceKind) {
                            route = model.isRecordOnlyCapture ? .capture : .ride
                        }
                    }
                )
                .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .top)
            } else if let screen = screen(for: route) {
                MockupScreenContainer(
                    screen: screen,
                    devicePickerScanState: model.devicePickerScanState,
                    rideState: model.selectedRideTitle == nil && model.phase == .starting && model.displayState.notificationCount == 0
                        ? nil
                        : model.rideState,
                    rideTitle: model.selectedRideTitle,
                    settingsReadback: model.settingsReadback,
                    faultHistoryReadback: model.faultHistoryReadback,
                    bmsSnapshot: model.bmsSnapshot,
                    vescRideSnapshot: model.vescRideSnapshot,
                    allowsFixtureFallback: route.allowsFixtureFallback,
                    captureStatusText: model.captureStatusText,
                    isRecordOnlyCapture: model.isRecordOnlyCapture,
                    disconnect: {
                        model.disconnectAndSearch()
                        route = .devicePicker
                    },
                    pair: pair,
                    recordOnly: { row, deviceKind in
                        if model.recordOnly(platformIdentifier: row.id, deviceKind: deviceKind) {
                            route = model.isRecordOnlyCapture ? .capture : .ride
                        }
                    },
                    selectScreen: selectScreen
                )
                .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .top)
            } else if route == .capture {
                CaptureRecordingScreen(
                    deviceKind: model.recordOnlyDeviceKind,
                    captureStatusText: model.captureStatusText,
                    activeLabels: model.activeCaptureLabels,
                    disconnect: {
                        model.disconnectAndSearch()
                        route = .devicePicker
                    },
                    startCaptureLabel: model.startCaptureLabel,
                    stopCaptureLabel: model.stopCaptureLabel
                )
            }
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .background(MockupColors.pageBackground.ignoresSafeArea())
        .onChange(of: model.phase) { _, phase in
            if case .failed = phase {
                model.disconnectAndSearch()
                route = .devicePicker
                return
            }
            openRideScreen(ifNeededFor: phase)
        }
    }

    private func pair(_ row: DevicePickerRow) {
        guard row.isSupported else { return }

        guard model.pair(platformIdentifier: row.id) else { return }
        route = CutoutAppRoute.route(for: row.connectionRoute)
    }

    private func openRideScreen(ifNeededFor phase: SessionConnectionPhase) {
        guard !model.isRecordOnlyCapture else { return }
        guard phase.opensRideScreen else { return }
        guard route == .devicePicker else { return }
        route = .ride
    }

    private func selectScreen(_ screenID: MockupScreenID) {
        route = CutoutAppRoute.route(for: screenID)
    }

    private func screen(for route: CutoutAppRoute) -> MockupScreen? {
        switch route {
        case .devicePicker:
            nil
        case .ride:
            catalog.screen(id: .eucRide)
        case .liveActivity:
            catalog.screen(id: .liveActivity)
        case .pack:
            catalog.screen(id: .bmsOverview).map {
                catalog.presentedScreen(for: $0, liveBmsSnapshot: model.bmsSnapshot, fixtureFallback: false)
            }
        case .vescRide:
            catalog.screen(id: .vescOnewheelRide)
        case .capture:
            nil
        case .mockup(let screenID):
            catalog.screen(id: screenID)
        }
    }

}
