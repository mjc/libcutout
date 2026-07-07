import CutoutMobile
import SwiftUI

struct MockupScreenContainer: View {
    let screen: MockupScreen
    let devicePickerScanState: DevicePickerScanState?
    let rideState: EucRideScreenState?
    let rideTitle: String?
    let settingsReadback: SettingsReadback?
    let faultHistoryReadback: FaultHistoryReadback?
    let bmsSnapshot: BmsSnapshot?
    let vescRideSnapshot: VescRideSnapshot?
    let allowsFixtureFallback: Bool
    let captureStatusText: String?
    let isRecordOnlyCapture: Bool
    let disconnect: () -> Void
    let pair: (DevicePickerRow) -> Void
    let recordOnly: (DevicePickerRow, String) -> Void
    let selectScreen: (MockupScreenID) -> Void

    var body: some View {
        switch screen.id {
        case .devicePicker:
            DevicePickerView(
                scanState: devicePickerScanState,
                captureStatusText: captureStatusText,
                isRecordOnlyCapture: isRecordOnlyCapture,
                pair: pair,
                recordOnly: recordOnly
            )
        case .eucRide:
            EucRideScreenView(
                screen: screen,
                rideState: rideState,
                rideTitle: rideTitle,
                captureStatusText: captureStatusText,
                disconnect: disconnect,
                selectScreen: selectScreen
            )
        case .liveActivity:
            LiveActivityMockupView(screen: screen)
        case .bmsOverview, .bmsCellMap6S, .bmsCellMap40S, .bmsCellDetail, .bmsUnknownTopology, .bmsNoData:
            BmsMockupView(screen: screen, rideState: rideState, bmsSnapshot: bmsSnapshot, selectScreen: selectScreen)
        case .eucGarage:
            EucGarageMockupView(
                screen: screen,
                settingsReadback: settingsReadback,
                faultHistoryReadback: faultHistoryReadback,
                bmsSnapshot: bmsSnapshot
            )
        case .vescOnewheelRide:
            VescOnewheelRideMockupView(
                screen: screen,
                liveSnapshot: vescRideSnapshot,
                allowsFixtureFallback: allowsFixtureFallback,
                disconnect: disconnect
            )
        case .vescDebug:
            VescDebugMockupView(screen: screen)
        }
    }
}
