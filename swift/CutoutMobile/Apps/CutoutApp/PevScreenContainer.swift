import CutoutMobile
import SwiftUI

struct PevScreenContainer: View {
    let screen: PevScreen
    let rideState: EucRideScreenState?
    let rideTitle: String?
    let settingsReadback: SettingsReadback?
    let faultHistoryReadback: FaultHistoryReadback?
    let bmsSnapshot: BmsSnapshot?
    let phoneLocationReadback: PhoneLocationReadback
    let vescSnapshot: VescRideSnapshot?
    let now: MonotonicMilliseconds
    let connectionPhase: SessionConnectionPhase
    let notificationCount: UInt64
    let captureStatusText: String?
    let disconnect: () -> Void

    var body: some View {
        Group {
            switch screen.id {
            case .eucRide:
                EucRideScreenView(
                    rideState: rideState,
                    rideTitle: rideTitle,
                    now: now,
                    captureStatusText: captureStatusText,
                    phoneLocationReadback: phoneLocationReadback,
                    disconnect: disconnect,
                )
            case .bmsOverview, .bmsCellMap6S, .bmsCellMap40S, .bmsCellDetail, .bmsUnknownTopology, .bmsNoData:
                BmsScreenView(screen: screen, rideState: rideState, bmsSnapshot: bmsSnapshot)
            case .eucGarage:
                EucGarageScreenView(
                    screen: screen,
                    settingsReadback: settingsReadback,
                    faultHistoryReadback: faultHistoryReadback,
                    bmsSnapshot: bmsSnapshot
                )
            case .vescRide:
                VescRideScreenView(
                    liveSnapshot: vescSnapshot,
                    phase: connectionPhase,
                    now: now,
                    captureStatusText: captureStatusText,
                    disconnect: disconnect
                )
            case .vescDebug:
                VescDebugScreenView(
                    snapshot: vescSnapshot,
                    phase: connectionPhase,
                    notificationCount: notificationCount,
                    captureStatusText: captureStatusText
                )
            }
        }
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("dashboard.screen.\(screen.id.rawValue)")
    }
}
