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
                screen: screen,
                rideState: rideState,
                rideTitle: rideTitle,
                captureStatusText: captureStatusText,
                disconnect: disconnect,
                selectScreen: selectScreen
            )
        case .eucMap, .eucTune, .vescMap, .vescLogs:
            PevPlaceholderScreenView(screen: screen)
        case .liveActivity:
            ScrollView {
                VStack(alignment: .leading, spacing: 20) {
                    PevLiveActivityPresentationCard(
                        title: screen.title,
                        subtitle: screen.subtitle,
                        style: .expanded,
                        snapshot: LiveActivityRideSnapshot.demo(
                            identity: .demo(label: "Floatwheel Atom"),
                            glyph: .floatwheelAtom,
                            speed: .available(label: "Speed", value: "0.0", unit: "mph", source: .demo),
                            battery: .available(label: "Battery", value: "85", unit: "%", source: .demo),
                            packVoltage: .available(label: "Voltage", value: "61.8", unit: "V", source: .demo),
                            pwm: .unavailable(label: "PWM", unit: nil),
                            mode: .unavailable(label: "Mode", unit: nil),
                            duration: .unavailable(label: "Duration", unit: nil),
                            distance: .unavailable(label: "Distance", unit: "mi"),
                            headroom: .available(label: "Headroom", value: "18", unit: "%", source: .demo),
                            beeps: .unavailable(label: "Beeps", unit: nil),
                            temperature: .available(label: "Temperature", value: "27", unit: "°C", source: .demo)
                        )
                    )
                }
                .padding(24)
            }
            .background(PevLiveActivityPalette.background.ignoresSafeArea())
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
            VescDebugScreenView(screen: screen)
        }
    }
}
