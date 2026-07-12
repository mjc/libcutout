import CutoutMobile
import SwiftUI

struct PevScreenContainer: View {
    let screen: PevScreen
    let devicePickerScanState: DevicePickerScanState?
    let rideState: EucRideScreenState?
    let rideTitle: String?
    let settingsReadback: SettingsReadback?
    let faultHistoryReadback: FaultHistoryReadback?
    let bmsSnapshot: BmsSnapshot?
    let captureStatusText: String?
    let isRecordOnlyCapture: Bool
    let disconnect: () -> Void
    let pair: (DevicePickerRow) -> Void
    let recordOnly: (DevicePickerRow, String) -> Void
    let selectScreen: (PevScreenID) -> Void

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
                rideState: rideState,
                rideTitle: rideTitle,
                captureStatusText: captureStatusText,
                disconnect: disconnect,
                selectScreen: selectScreen
            )
        case .eucMap, .eucTune, .vescMap, .vescLogs:
            PevPlaceholderScreenView(screen: screen)
        case .liveActivity:
            EmptyView()
        case .bmsOverview, .bmsCellMap6S, .bmsCellMap40S, .bmsCellDetail, .bmsUnknownTopology, .bmsNoData:
            BmsScreenView(screen: screen, rideState: rideState, bmsSnapshot: bmsSnapshot, selectScreen: selectScreen)
        case .eucGarage:
            EucGarageScreenView(
                screen: screen,
                settingsReadback: settingsReadback,
                faultHistoryReadback: faultHistoryReadback,
                bmsSnapshot: bmsSnapshot
            )
        case .vescDebug:
            EmptyView()
        }
    }
}
