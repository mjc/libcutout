import CutoutMobile
import CutoutMobileFFI

@MainActor
protocol CutoutSessionDriving: AnyObject {
    var onPhoneAlarmActionsAvailable: ((MobilePhoneAlarmActionsDto) -> Void)? { get set }
    var rideSessionStateHandle: CutoutSessionStateHandle { get }
    var connectionSnapshot: ConnectionSnapshot { get }
    /// The Rust-backed map adapter is optional while persistence is unavailable.
    var rideMapStateHandle: MobileRideMapState? { get }
    var onDisplayStateChange: ((RideDisplayState) -> Void)? { get set }
    var onPhaseChange: ((SessionConnectionPhase) -> Void)? { get set }
    var onReconnectScheduled: ((SessionConnectionRetry) -> Void)? { get set }
    var onCaptureEvent: ((CaptureEvent) -> Void)? { get set }
    var onScanStateChange: ((DevicePickerScanState) -> Void)? { get set }
    var onSettingsChange: ((DeviceSettings) -> Void)? { get set }
    var onFaultHistoryReadbackChange: ((FaultHistoryReadback?) -> Void)? { get set }
    var onBmsSnapshotChange: ((BmsSnapshot?) -> Void)? { get set }
    var onPhoneLocationSnapshotChange: ((MobilePhoneLocationSnapshotDto, MonotonicMilliseconds) -> Void)? { get set }
    var onRideMapDecisionChange: ((MobileRideMapSnapshotDto, MobileRideMapDecisionDto) -> Void)? { get set }
    var onRideMapSnapshotChange: ((MobileRideMapSnapshotDto) -> Void)? { get set }
    var onRideMapErrorChange: ((MobileRideMapErrorEvent) -> Void)? { get set }
    var onRideMapAvailabilityChange: ((MobileRideMapAvailability) -> Void)? { get set }
    var onProtocolIdentityCandidateChange: ((DevicePickerDiscoveryCandidate?) -> Void)? { get set }
    var onBluetoothRestorationResolved: ((String?) -> Void)? { get set }
    var protocolIdentityCandidate: DevicePickerDiscoveryCandidate? { get }
    var isRecordOnlyConnection: Bool { get }
    var electricUnicycleModel: ElectricUnicycleModel? { get }
    var settings: DeviceSettings { get }

    func start()
    func pair(platformIdentifier: String) -> Bool
    func pair(platformIdentifier: String, model: ElectricUnicycleModel) -> Bool
    func probe(platformIdentifier: String) -> Bool
    func recordOnly(platformIdentifier: String, note: String?, annotations: [String]) -> Bool
    func changeCaptureLabel(generation: CaptureGeneration, action: MobileCaptureLabelActionDto) throws
        -> [MobileCaptureLabelDto]
    func updateMusicCapturePolicy(_ policy: MobileMusicHistoryPolicyDto)
    func updateMusicCaptureObservation(_ observation: MobilePevcapMusicEventDto?)
    func flushCapture() async -> Bool
    func finishCapture() async -> Bool
    func disconnectAndScan()
    func submitDeviceSetting(token: ConnectionAttemptToken, id: DeviceSettingID, value: DeviceSettingValue) throws
    func submitDeviceAction(token: ConnectionAttemptToken, id: DeviceActionID) throws
    func setDeviceControlsValidation(token: ConnectionAttemptToken, authorized: Bool) throws
    func now() -> MonotonicMilliseconds
    @discardableResult
    func resetTripMeterForNewRide(token: ConnectionAttemptToken) -> Bool

    func resetRideMapLocationAdmission()
    func updateRideLocationDemand(for state: MobileRideMapStateDto)
    func startRideMapGpsOnly(atMs: UInt64) async throws -> MobileRideMapSnapshotDto
    func pauseRideMap(atMs: UInt64) async throws -> MobileRideMapSnapshotDto
    func resumeRideMap(atMs: UInt64) async throws -> MobileRideMapSnapshotDto
    func stopRideMap(atMs: UInt64) async throws -> MobileRideMapSnapshotDto
    func saveRideMap() async throws -> MobileRideMapSnapshotDto
    func discardRideMap() async throws -> MobileRideMapSnapshotDto
}

extension CutoutSessionCore: CutoutSessionDriving {}

extension CutoutSessionDriving {
    var connectionSnapshot: ConnectionSnapshot {
        rideSessionStateHandle.connectionAttemptSnapshot()
    }

    var onPhoneAlarmActionsAvailable: ((MobilePhoneAlarmActionsDto) -> Void)? {
        get { nil }
        set {}
    }

    func setDeviceControlsValidation(token: ConnectionAttemptToken, authorized: Bool) throws {
        throw DeviceSettingSubmissionError.ConnectionUnavailable
    }

    var isRecordOnlyConnection: Bool { false }
    var rideMapStateHandle: MobileRideMapState? { nil }

    @discardableResult
    func resetTripMeterForNewRide(token: ConnectionAttemptToken) -> Bool { false }

    var rideMapStorageError: String? {
        guard let state = rideMapStateHandle else { return "Rust ride database is unavailable" }
        guard case let .storageError(message)? = state.initializationError else { return nil }
        return message
    }

    var rideMapAvailability: MobileRideMapAvailability {
        guard let state = rideMapStateHandle else { return .storageUnavailable }
        if rideMapStorageError != nil { return .storageUnavailable }
        return state.isReady ? .ready : .checking
    }

    func resetRideMapLocationAdmission() {}
}
