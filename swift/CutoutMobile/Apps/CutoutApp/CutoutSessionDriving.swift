import CutoutMobile
import CutoutMobileFFI

@MainActor
protocol CutoutSessionDriving: AnyObject {
    var rideSessionStateHandle: CutoutSessionStateHandle { get }
    var rideMapStateHandle: MobileRideMapState { get }
    var rideMapStorageError: String? { get }
    var rideMapAvailability: MobileRideMapAvailability { get }
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
    var onRideMapAvailabilityChange: ((MobileRideMapAvailability) -> Void)? { get set }
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
    func resetRideMapLocationAdmission()
    func startRideMapGpsOnly(
        atMs: UInt64,
        lastConnectedVehicle: String?
    ) throws -> MobileRideMapSnapshotDto
    func pauseRideMap(atMs: UInt64) throws -> MobileRideMapSnapshotDto
    func resumeRideMap(atMs: UInt64) throws -> MobileRideMapSnapshotDto
    func stopRideMap(atMs: UInt64) throws -> MobileRideMapSnapshotDto
    func saveRideMap() throws -> MobileRideMapSnapshotDto
    func discardRideMap() throws -> MobileRideMapSnapshotDto
    func now() -> MonotonicMilliseconds
}

extension CutoutSessionCore: CutoutSessionDriving {}

extension CutoutSessionDriving {
    var rideMapStorageError: String? { nil }
    var rideMapAvailability: MobileRideMapAvailability { .checking }

    // Test and alternate drivers retain the legacy direct handle by default. The concrete
    // CutoutSessionCore implementation overrides these witnesses with its lifecycle barrier.
    func startRideMapGpsOnly(
        atMs: UInt64,
        lastConnectedVehicle: String?
    ) throws -> MobileRideMapSnapshotDto {
        try rideMapStateHandle.startGpsOnly(
            atMs: atMs,
            lastConnectedVehicle: lastConnectedVehicle
        )
    }

    func pauseRideMap(atMs: UInt64) throws -> MobileRideMapSnapshotDto {
        try rideMapStateHandle.pause(atMs: atMs)
    }

    func resumeRideMap(atMs: UInt64) throws -> MobileRideMapSnapshotDto {
        try rideMapStateHandle.resume(atMs: atMs)
    }

    func stopRideMap(atMs: UInt64) throws -> MobileRideMapSnapshotDto {
        try rideMapStateHandle.stop(atMs: atMs)
    }

    func saveRideMap() throws -> MobileRideMapSnapshotDto {
        try rideMapStateHandle.save()
    }

    func discardRideMap() throws -> MobileRideMapSnapshotDto {
        try rideMapStateHandle.discard()
    }
}
