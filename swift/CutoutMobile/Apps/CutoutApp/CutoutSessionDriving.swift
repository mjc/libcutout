import CutoutMobile
import CutoutMobileFFI

@MainActor
protocol CutoutSessionDriving: AnyObject {
    var rideSessionStateHandle: CutoutSessionStateHandle { get }
    /// The Rust-backed map adapter is optional while persistence is unavailable.
    var rideMapStateHandle: MobileRideMapState? { get }
    var onDisplayStateChange: ((RideDisplayState) -> Void)? { get set }
    var onPhaseChange: ((SessionConnectionPhase) -> Void)? { get set }
    var onReconnectScheduled: ((SessionConnectionRetry) -> Void)? { get set }
    var onCaptureEvent: ((CaptureEvent) -> Void)? { get set }
    var onScanStateChange: ((DevicePickerScanState) -> Void)? { get set }
    var onSettingsReadbackChange: ((SettingsReadback?) -> Void)? { get set }
    var onFaultHistoryReadbackChange: ((FaultHistoryReadback?) -> Void)? { get set }
    var onBmsSnapshotChange: ((BmsSnapshot?) -> Void)? { get set }
    var onPhoneLocationSnapshotChange: ((MobilePhoneLocationSnapshotDto, MonotonicMilliseconds) -> Void)? { get set }
    var onProtocolIdentityCandidateChange: ((DevicePickerDiscoveryCandidate?) -> Void)? { get set }
    var onBluetoothRestorationResolved: ((String?) -> Void)? { get set }
    var protocolIdentityCandidate: DevicePickerDiscoveryCandidate? { get }
    var electricUnicycleModel: ElectricUnicycleModel? { get }

    func start()
    func pair(platformIdentifier: String) -> Bool
    func pair(platformIdentifier: String, model: ElectricUnicycleModel) -> Bool
    func probe(platformIdentifier: String) -> Bool
    func recordOnly(platformIdentifier: String, note: String?, annotations: [String]) -> Bool
    func annotateCapture(label: String)
    func annotateCapture(key: String, value: String)
    func flushCapture() async -> Bool
    func disconnectAndScan()
    func setLights(_ state: LightState) -> Bool
    func now() -> MonotonicMilliseconds

    func resetRideMapLocationAdmission()
    func startRideMapGpsOnly(atMs: UInt64, lastConnectedVehicle: String?) throws -> MobileRideMapSnapshotDto
    func pauseRideMap(atMs: UInt64) throws -> MobileRideMapSnapshotDto
    func resumeRideMap(atMs: UInt64) throws -> MobileRideMapSnapshotDto
    func stopRideMap(atMs: UInt64) throws -> MobileRideMapSnapshotDto
    func saveRideMap() throws -> MobileRideMapSnapshotDto
    func discardRideMap() throws -> MobileRideMapSnapshotDto
}

extension CutoutSessionCore: CutoutSessionDriving {}

extension CutoutSessionDriving {
    var rideMapStateHandle: MobileRideMapState? { nil }

    var rideMapStorageError: String? {
        guard let state = rideMapStateHandle else { return "Rust ride database is unavailable" }
        guard case let .storageError(message)? = state.initializationError else { return nil }
        return message
    }

    var rideMapAvailability: MobileRideMapAvailability {
        rideMapStorageError == nil ? .ready : .storageUnavailable
    }

    private func requireRideMapState() throws -> MobileRideMapState {
        guard let state = rideMapStateHandle else {
            throw MobileRideMapError.storageError("Rust ride database is unavailable")
        }
        return state
    }

    func resetRideMapLocationAdmission() {}

    func startRideMapGpsOnly(atMs: UInt64, lastConnectedVehicle: String?) throws -> MobileRideMapSnapshotDto {
        try requireRideMapState().startGpsOnly(atMs: atMs, lastConnectedVehicle: lastConnectedVehicle)
    }

    func pauseRideMap(atMs: UInt64) throws -> MobileRideMapSnapshotDto {
        try requireRideMapState().pause(atMs: atMs)
    }

    func resumeRideMap(atMs: UInt64) throws -> MobileRideMapSnapshotDto {
        try requireRideMapState().resume(atMs: atMs)
    }

    func stopRideMap(atMs: UInt64) throws -> MobileRideMapSnapshotDto {
        try requireRideMapState().stop(atMs: atMs)
    }

    func saveRideMap() throws -> MobileRideMapSnapshotDto {
        try requireRideMapState().save()
    }

    func discardRideMap() throws -> MobileRideMapSnapshotDto {
        try requireRideMapState().discard()
    }
}
