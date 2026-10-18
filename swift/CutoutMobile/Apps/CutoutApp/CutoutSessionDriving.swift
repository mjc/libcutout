import CutoutMobile
import CutoutMobileFFI

@MainActor
protocol CutoutSessionDriving: AnyObject {
    var onPhoneAlarmActionsAvailable: ((MobilePhoneAlarmActionsDto) -> Void)? { get set }
    var rideSessionStateHandle: CutoutSessionStateHandle { get }
    /// The Rust-backed map adapter is optional while persistence is unavailable.
    var rideMapStateHandle: MobileRideMapState? { get }
    var onDisplayStateChange: ((RideDisplayState) -> Void)? { get set }
    var onPhaseChange: ((SessionConnectionPresentation) -> Void)? { get set }
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
    var electricUnicycleModel: ElectricUnicycleModel? { get }
    var settings: DeviceSettings { get }

    func setPresentationActive(_ active: Bool)
    func start()
    func pair(platformIdentifier: String) -> Bool
    func pair(platformIdentifier: String, model: ElectricUnicycleModel) -> Bool
    func probe(platformIdentifier: String) -> Bool
    func recordOnly(platformIdentifier: String, note: String?, annotations: [String]) -> Bool
    func changeCaptureLabel(generation: CaptureGeneration, action: MobileCaptureLabelActionDto) throws
        -> [MobileCaptureLabelDto]
    func recordCaptureSettingsObservation(
        generation: CaptureGeneration, draft: MobileCaptureSettingsDraftDto
    ) async throws -> MobileCaptureSettingsObservationDto
    func captureSettingsObservations(generation: CaptureGeneration) throws -> [MobileCaptureSettingsObservationDto]
    func captureSettingsReadbackBatch(
        generation: CaptureGeneration, requests: [MobileCaptureResearchReadbackRequestDto]
    ) async throws -> [MobileCaptureResearchReadbackDto]
    func annotateCapture(key: String, value: String) -> Bool
    func flushCapture() async -> Bool
    func finishCapture() async -> Bool
    func setAutomaticDiagnosticRecordingEnabled(_ enabled: Bool) async throws
    func disconnectAndScan()
    func disconnectAndScan(expectedGeneration: UInt64) -> Bool
    func submitDeviceSetting(token: ConnectionAttemptToken, id: DeviceSettingID, value: DeviceSettingValue) throws
    func submitDeviceAction(token: ConnectionAttemptToken, id: DeviceActionID) throws
    func setDeviceControlsValidation(token: ConnectionAttemptToken, authorized: Bool) throws
    func now() -> MonotonicMilliseconds
    @discardableResult
    func resetTripMeterForNewRide(token: ConnectionAttemptToken) -> Bool

    func resetRideMapLocationAdmission()
    func startRideMapGpsOnly(atMs: UInt64, musicHistoryPolicy: MobileMusicHistoryPolicyDto) async throws
        -> MobileRideMapSnapshotDto
    func pauseRideMap(expected: MobileRideMapCommandTokenDto, atMs: UInt64) async throws -> MobileRideMapSnapshotDto
    func prepareRideMapForDisconnect(
        expectedRecordingToken: MobileRideMapRecordingTokenDto?,
        connectionGeneration: UInt64
    ) async throws
    func resumeRideMap(expected: MobileRideMapCommandTokenDto, atMs: UInt64) async throws -> MobileRideMapSnapshotDto
    func stopRideMap(expected: MobileRideMapCommandTokenDto, atMs: UInt64) async throws -> MobileRideMapSnapshotDto
    func saveRideMap(expected: MobileRideMapCommandTokenDto) async throws -> MobileRideMapSnapshotDto
    func discardRideMap(expected: MobileRideMapCommandTokenDto) async throws -> MobileRideMapSnapshotDto
    func checkpointRideMap() async throws
}

extension CutoutSessionCore: CutoutSessionDriving {}

extension CutoutSessionDriving {

    func setAutomaticDiagnosticRecordingEnabled(_ enabled: Bool) async throws {
        let state = rideSessionStateHandle
        _ = try await Task.detached(priority: .userInitiated) {
            try state.setAutomaticDiagnosticRecordingEnabled(enabled: enabled)
        }.value
    }

    func setPresentationActive(_ active: Bool) {}
    func checkpointRideMap() async throws {}
    func prepareRideMapForDisconnect(
        expectedRecordingToken: MobileRideMapRecordingTokenDto?,
        connectionGeneration: UInt64
    ) async throws {}

    func disconnectAndScan(expectedGeneration: UInt64) -> Bool {
        guard connectionSnapshot.generation == expectedGeneration else { return false }
        disconnectAndScan()
        return true
    }

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

    private func requireRideMapState() throws -> MobileRideMapState {
        guard let state = rideMapStateHandle else {
            throw MobileRideMapError.storageError("Rust ride database is unavailable")
        }
        return state
    }

    func resetRideMapLocationAdmission() {}

    func startRideMapGpsOnly(atMs: UInt64) throws -> MobileRideMapSnapshotDto {
        try requireRideMapState().startGpsOnly(atMs: atMs)
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
