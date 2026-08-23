import CutoutMobile
import CutoutMobileFFI

@MainActor
protocol CutoutSessionDriving: AnyObject {
    var rideSessionStateHandle: CutoutSessionStateHandle { get }
    var rideMapStateHandle: MobileRideMapState { get }
    var rideMapStorageError: String? { get }
    var onDisplayStateChange: ((RideDisplayState) -> Void)? { get set }
    var onPhaseChange: ((SessionConnectionPhase) -> Void)? { get set }
    var onReconnectScheduled: ((SessionConnectionRetry) -> Void)? { get set }
    var onCaptureEvent: ((CaptureEvent) -> Void)? { get set }
    var onScanStateChange: ((DevicePickerScanState) -> Void)? { get set }
    var onSettingsReadbackChange: ((SettingsReadback?) -> Void)? { get set }
    var onFaultHistoryReadbackChange: ((FaultHistoryReadback?) -> Void)? { get set }
    var onBmsSnapshotChange: ((BmsSnapshot?) -> Void)? { get set }
    var onPhoneLocationSnapshotChange: ((MobilePhoneLocationSnapshotDto, MonotonicMilliseconds) -> Void)? { get set }
    var onRideMapDecisionChange: ((MobileRideMapSnapshotDto, MobileRideMapDecisionDto) -> Void)? { get set }
    var onRideMapSnapshotChange: ((MobileRideMapSnapshotDto) -> Void)? { get set }
    var onRideMapErrorChange: ((MobileRideMapError) -> Void)? { get set }
    var onProtocolIdentityCandidateChange: ((DevicePickerDiscoveryCandidate?) -> Void)? { get set }
    var onBluetoothRestorationResolved: ((String?) -> Void)? { get set }
    var protocolIdentityCandidate: DevicePickerDiscoveryCandidate? { get }

    func start()
    func pair(platformIdentifier: String) -> Bool
    func pair(platformIdentifier: String, model: ElectricUnicycleModel) -> Bool
    func probe(platformIdentifier: String) -> Bool
    func recordOnly(platformIdentifier: String, note: String?, annotations: [String]) -> Bool
    func annotateCapture(label: String)
    func annotateCapture(key: String, value: String)
    func flushCapture() async -> Bool
    func disconnectAndScan()
    func now() -> MonotonicMilliseconds
}

extension CutoutSessionCore: CutoutSessionDriving {}

extension CutoutSessionDriving {
    var rideMapStorageError: String? { nil }
}
