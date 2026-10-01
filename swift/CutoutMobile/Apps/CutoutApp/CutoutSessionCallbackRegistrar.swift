import CutoutMobile
import CutoutMobileFFI

@MainActor
struct CutoutSessionCallbackRegistrar {
    struct Handlers {
        let displayState: (RideDisplayState) -> Void
        let phase: (SessionConnectionPhase) -> Void
        let reconnectScheduled: (SessionConnectionRetry) -> Void
        let captureEvent: (CaptureEvent) -> Void
        let scanState: (DevicePickerScanState) -> Void
        let settings: (DeviceSettings) -> Void
        let faultHistory: (FaultHistoryReadback?) -> Void
        let bmsSnapshot: (BmsSnapshot?) -> Void
        let phoneLocation: (MobilePhoneLocationSnapshotDto, MonotonicMilliseconds) -> Void
        let rideMapDecision: (MobileRideMapSnapshotDto, MobileRideMapDecisionDto) -> Void
        let rideMapSnapshot: (MobileRideMapSnapshotDto) -> Void
        let rideMapError: (MobileRideMapErrorEvent) -> Void
        let rideMapAvailability: (MobileRideMapAvailability) -> Void
        let protocolIdentity: (DevicePickerDiscoveryCandidate?) -> Void
        let bluetoothRestoration: (String?) -> Void
        let phoneAlarmActions: (MobilePhoneAlarmActionsDto) -> Void
    }

    private let core: any CutoutSessionDriving

    init(core: any CutoutSessionDriving) {
        self.core = core
    }

    func install(_ handlers: Handlers) {
        core.onDisplayStateChange = handlers.displayState
        core.onPhaseChange = handlers.phase
        core.onReconnectScheduled = handlers.reconnectScheduled
        core.onScanStateChange = handlers.scanState
        core.onSettingsChange = handlers.settings
        core.onPhoneAlarmActionsAvailable = handlers.phoneAlarmActions
        core.onFaultHistoryReadbackChange = handlers.faultHistory
        core.onBmsSnapshotChange = handlers.bmsSnapshot
        core.onPhoneLocationSnapshotChange = handlers.phoneLocation
        core.onRideMapDecisionChange = handlers.rideMapDecision
        core.onRideMapSnapshotChange = handlers.rideMapSnapshot
        core.onRideMapErrorChange = handlers.rideMapError
        core.onRideMapAvailabilityChange = handlers.rideMapAvailability
        core.onProtocolIdentityCandidateChange = handlers.protocolIdentity
        core.onBluetoothRestorationResolved = handlers.bluetoothRestoration
        core.onCaptureEvent = handlers.captureEvent
    }
}
