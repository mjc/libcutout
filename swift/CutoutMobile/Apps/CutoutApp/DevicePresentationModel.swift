import CutoutMobile
import CutoutMobileFFI
import Observation

/// App-lifetime presentation of the one Rust-backed device session.
@MainActor
@Observable
final class DevicePresentationModel {
    var displayState = RideDisplayState()
    var phase = SessionConnectionPhase.starting
    var scanState: DevicePickerScanState?
    var connectionState = ConnectionState.picker
    var faultHistoryReadback: FaultHistoryReadback?
    var bmsSnapshot: BmsSnapshot?
    var phoneLocationReadback = PhoneLocationReadback(
        snapshot: MobilePhoneLocationSnapshotDto(latestSample: nil, gpsSpeed: nil)
    )
    var hasSavedDevice = false
    var settings: DeviceSettings?

    var selectedRideTitle: String? { connectionState.selection?.title }
    var selectedRideIdentifier: String? { connectionState.selection?.platformIdentifier }
    var selectedConnectionRoute: DevicePickerConnectionRoute? { connectionState.selection?.route }
    var speed: SpeedReadout { displayState.speed }
    var rideState: EucRideScreenState { EucRideScreenState(phase: phase, displayState: displayState) }
    var eucRidePresentationState: EucRideScreenState? {
        guard selectedRideTitle != nil || phase != .starting || displayState.notificationCount != 0 else {
            return nil
        }
        return rideState
    }
    var vescRideSnapshot: VescRideSnapshot? {
        VescRideSnapshot(displayState: displayState, title: selectedRideTitle)
    }
    var connectionStatusText: String { connectionState.statusText ?? phase.displayText }
}
