import CutoutMobileFFI
import Foundation

struct RideMapDecisionBatch: Sendable {
    let outcomes: [MobileRideMapOutcomeDto]
}

protocol CutoutSessionRideMapRecording: AnyObject {
    func start(replayConnection: @escaping @Sendable () -> Void)
    func persistBmsSamples(
        _ observations: [BmsRawVoltageObservation],
        deviceIdentity: String?,
        sessionIdentifier: String
    )
    func observeConnection(
        at receivedAt: MonotonicMilliseconds,
        token: ConnectionAttemptToken,
        speedObservation: MobileRideMapSpeedObservationDto?,
        connectionState: CutoutSessionStateHandle,
        resetTripMeter: @escaping @Sendable (ConnectionAttemptToken) -> Void
    )
    func ingestLocation(_ update: PhoneLocationUpdate)
    func startGpsOnly(atMs: UInt64, musicHistoryPolicy: MobileMusicHistoryPolicyDto) async throws
        -> MobileRideMapSnapshotDto
    func pause(expected: MobileRideMapCommandTokenDto, atMs: UInt64) async throws -> MobileRideMapSnapshotDto
    func prepareForDisconnect(expected: MobileRideMapRecordingTokenDto?) async throws
    func resume(expected: MobileRideMapCommandTokenDto, atMs: UInt64) async throws -> MobileRideMapSnapshotDto
    func stop(expected: MobileRideMapCommandTokenDto, atMs: UInt64) async throws -> MobileRideMapSnapshotDto
    func save(expected: MobileRideMapCommandTokenDto) async throws -> MobileRideMapSnapshotDto
    func discard(expected: MobileRideMapCommandTokenDto) async throws -> MobileRideMapSnapshotDto
    func checkpoint() async throws
}

/// Owns Rust ride-map state and storage effects on one serial executor. Protocol truth remains in Core.
actor CutoutSessionRideMapRecorder: CutoutSessionRideMapRecording {
    private nonisolated let queue = DispatchSerialQueue(label: "io.cutout.ridemap", qos: .utility)
    private nonisolated let recordingDispatchLock = NSLock()
    private let state: MobileRideMapState?
    private let clock: MonotonicClock
    private let wallClock: @Sendable () -> Date
    private let publishSnapshot: @Sendable (MobileRideMapSnapshotDto) -> Void
    private let publishDecisions: @Sendable (RideMapDecisionBatch) -> Void
    private let publishError: @Sendable (MobileRideMapError, MobileRideMapErrorContext) -> Void
    private let publishAvailability: @Sendable (MobileRideMapError?, Bool) -> Void
    private let recordDiagnostic: @Sendable (String) -> Void
    private let onLocationDemandChanged: @Sendable () -> Void
    private var restorationStarted = false
    nonisolated var unownedExecutor: UnownedSerialExecutor {
        queue.asUnownedSerialExecutor()
    }

    init(
        state: MobileRideMapState?,
        clock: MonotonicClock,
        wallClock: @escaping @Sendable () -> Date,
        publishSnapshot: @escaping @Sendable (MobileRideMapSnapshotDto) -> Void,
        publishDecisions: @escaping @Sendable (RideMapDecisionBatch) -> Void,
        publishError: @escaping @Sendable (MobileRideMapError, MobileRideMapErrorContext) -> Void,
        publishAvailability: @escaping @Sendable (MobileRideMapError?, Bool) -> Void,
        recordDiagnostic: @escaping @Sendable (String) -> Void,
        onLocationDemandChanged: @escaping @Sendable () -> Void
    ) {
        self.state = state
        self.clock = clock
        self.wallClock = wallClock
        self.publishSnapshot = publishSnapshot
        self.publishDecisions = publishDecisions
        self.publishError = publishError
        self.publishAvailability = publishAvailability
        self.recordDiagnostic = recordDiagnostic
        self.onLocationDemandChanged = onLocationDemandChanged
    }

    nonisolated func start(
        replayConnection: @escaping @Sendable () -> Void
    ) {
        queue.async { [self] in
            assumeIsolated { recorder in
                _ = Task {
                    await recorder.startRestoration(replayConnection: replayConnection)
                }
            }
        }
    }

    func startGpsOnly(atMs: UInt64, musicHistoryPolicy: MobileMusicHistoryPolicyDto) async throws
        -> MobileRideMapSnapshotDto
    {
        let state = try requireState()
        let snapshot = try await state.startGpsOnlyCommand(atMs: atMs, musicHistoryPolicy: musicHistoryPolicy)
        synchronizeLocationDemand()
        return snapshot
    }

    func pause(expected: MobileRideMapCommandTokenDto, atMs: UInt64) async throws -> MobileRideMapSnapshotDto {
        let snapshot = try await transition(.pause, expected: expected, atMs: atMs)
        synchronizeLocationDemand()
        return snapshot
    }

    func prepareForDisconnect(expected: MobileRideMapRecordingTokenDto?) async throws {
        guard state != nil else { return }
        guard
            let snapshot = try requireState().prepareDisconnect(
                expected: expected,
                atMs: clock.now().rawValue
            )
        else {
            return
        }
        synchronizeLocationDemand()
        publishSnapshot(snapshot)
    }

    func resume(expected: MobileRideMapCommandTokenDto, atMs: UInt64) async throws -> MobileRideMapSnapshotDto {
        let snapshot = try await transition(.resume, expected: expected, atMs: atMs)
        synchronizeLocationDemand()
        return snapshot
    }

    func stop(expected: MobileRideMapCommandTokenDto, atMs: UInt64) async throws -> MobileRideMapSnapshotDto {
        let snapshot = try await transition(.stop, expected: expected, atMs: atMs)
        synchronizeLocationDemand()
        return snapshot
    }

    func save(expected: MobileRideMapCommandTokenDto) async throws -> MobileRideMapSnapshotDto {
        let snapshot = try await transition(.save, expected: expected, atMs: clock.now().rawValue)
        synchronizeLocationDemand()
        return snapshot
    }

    func discard(expected: MobileRideMapCommandTokenDto) async throws -> MobileRideMapSnapshotDto {
        let snapshot = try await transition(.discard, expected: expected, atMs: clock.now().rawValue)
        synchronizeLocationDemand()
        return snapshot
    }

    func checkpoint() async throws {
        let state = try requireState()
        let outcomes = try await state.checkpoint()
        guard !Task.isCancelled else { return }
        if !outcomes.isEmpty {
            publishDecisions(RideMapDecisionBatch(outcomes: outcomes))
        }
    }

    private func transition(
        _ event: MobileRideEventDto,
        expected: MobileRideMapCommandTokenDto,
        atMs: UInt64
    ) async throws -> MobileRideMapSnapshotDto {
        let state = try requireState()
        let snapshot = try await state.performLifecycleCommand(event: event, expected: expected, atMs: atMs)
        drainLocationWrites()
        return snapshot
    }

    nonisolated func observeConnection(
        at receivedAt: MonotonicMilliseconds,
        token: ConnectionAttemptToken,
        speedObservation: MobileRideMapSpeedObservationDto?,
        connectionState: CutoutSessionStateHandle,
        resetTripMeter: @escaping @Sendable (ConnectionAttemptToken) -> Void
    ) {
        recordingDispatchLock.lock()
        defer { recordingDispatchLock.unlock() }
        guard let permit = reserveRecordingWork(observationCount: 1) else { return }
        queue.async { [weak self] in
            defer { permit.release() }
            guard let self else { return }
            self.assumeIsolated { recorder in
                guard recorder.state?.isReady == true else { return }
                do {
                    let previousRideID = recorder.state?.currentSnapshot(atMs: receivedAt.rawValue)?.rideID
                    guard let state = recorder.state else { return }
                    let admission = try state.beginVerifiedConnectionAdmission(
                        connectionState: connectionState,
                        token: token,
                        atMs: receivedAt.rawValue,
                        musicHistoryPolicy: MusicHistoryPolicyStore().policy
                    )
                    let admissionSnapshot = try state.waitVerifiedConnectionAdmission(admission)
                    if let admissionSnapshot, admissionSnapshot.rideID != previousRideID {
                        resetTripMeter(token)
                    }
                    _ = try state.observeTelemetryForVerifiedConnection(
                        connectionState: connectionState,
                        token: token,
                        atMs: receivedAt.rawValue,
                        speedObservation: speedObservation
                    )
                    if let snapshot = state.currentSnapshot(atMs: receivedAt.rawValue) ?? admissionSnapshot {
                        recorder.publishSnapshot(snapshot)
                    }
                    recorder.drainLocationWrites()
                    recorder.synchronizeLocationDemand()
                } catch let error as MobileRideMapError where error == .staleConnection {
                    return
                } catch let error as MobileRideMapError {
                    let snapshot = self.state?.currentSnapshot(atMs: receivedAt.rawValue)
                    if let snapshot {
                        recorder.publishSnapshot(snapshot)
                    }
                    recorder.publishError(error, MobileRideMapErrorContext(snapshot: snapshot))
                    recorder.recordDiagnostic("ride_map_connection_error=\(error)")
                } catch {
                    recorder.recordDiagnostic("ride_map_connection_error=\(error)")
                }
            }
        }
    }

    nonisolated func persistBmsSamples(
        _ observations: [BmsRawVoltageObservation],
        deviceIdentity: String?,
        sessionIdentifier: String
    ) {
        guard !observations.isEmpty,
            let wallClockMilliseconds = unixMilliseconds(for: wallClock())
        else { return }
        recordingDispatchLock.lock()
        defer { recordingDispatchLock.unlock() }
        guard let permit = reserveRecordingWork(observationCount: UInt64(observations.count)) else { return }
        queue.async { [weak self] in
            defer { permit.release() }
            guard let self else { return }
            self.assumeIsolated { recorder in
                guard let state = recorder.state,
                    state.initializationError == nil,
                    state.isReady,
                    let deviceIdentity
                else { return }
                let samples = bmsStorageSamples(
                    observations: observations,
                    wallClockMilliseconds: wallClockMilliseconds,
                    sessionIdentifier: sessionIdentifier
                )
                guard !samples.isEmpty else { return }
                do {
                    try state.queueBmsVoltageSamples(deviceIdentity: deviceIdentity, samples: samples)
                    recorder.drainBmsVoltageWrites()
                } catch {
                    recorder.recordDiagnostic("bms_storage_error=\(error)")
                }
            }
        }
    }

    nonisolated func ingestLocation(_ update: PhoneLocationUpdate) {
        // Native dispatch order must match Rust receipt order even if a future producer
        // calls from a second delegate queue. This lock only bridges ownership transfer.
        recordingDispatchLock.lock()
        defer { recordingDispatchLock.unlock() }
        guard let state,
            state.initializationError == nil,
            state.isReady,
            let receiptWallClockUnixMs = unixMilliseconds(for: update.receiptWallClock)
        else { return }
        do {
            // Only the Rust receipt enters the executor; the native samples do not accumulate
            // in Dispatch closures. Saturation waits for capacity rather than dropping data.
            let callback = try state.admitLocationCallback(
                receiptMonotonicMs: update.receiptMonotonic.rawValue,
                receiptWallClockUnixMs: receiptWallClockUnixMs,
                samples: update.samples
            )
            queue.async { [weak self] in
                guard let self else { return }
                self.assumeIsolated { recorder in
                    do {
                        recorder.publishDecisionBatch(try state.finishLocationCallback(callback))
                    } catch let error as MobileRideMapError {
                        recorder.publishError(
                            error, MobileRideMapErrorContext(recordingToken: callback.recordingToken()))
                        recorder.recordDiagnostic("ride_map_ingest_error=\(error)")
                    } catch {
                        recorder.recordDiagnostic("ride_map_ingest_error=\(error)")
                    }
                }
            }
        } catch let error as MobileRideMapError {
            queue.async { [weak self] in
                self?.assumeIsolated { recorder in
                    recorder.publishError(error, MobileRideMapErrorContext(snapshot: nil))
                    recorder.recordDiagnostic("ride_map_ingest_error=\(error)")
                }
            }
        } catch {
            recordDiagnostic("ride_map_ingest_error=\(error)")
        }
    }

    private func startRestoration(replayConnection: @escaping @Sendable () -> Void) async {
        guard !restorationStarted else { return }
        restorationStarted = true
        guard let state else {
            publishAvailability(nil, false)
            return
        }
        do {
            if let snapshot = try await state.restoreCommand(atMs: clock.now().rawValue) {
                publishSnapshot(snapshot)
            }
            publishAvailability(state.initializationError, state.isReady)
            synchronizeLocationDemand()
            replayConnection()
        } catch let error as MobileRideMapError {
            restorationStarted = false
            publishError(error, MobileRideMapErrorContext(snapshot: nil))
            publishAvailability(state.initializationError, state.isReady)
        } catch {
            restorationStarted = false
            publishError(
                .storageError(String(describing: error)),
                MobileRideMapErrorContext(snapshot: nil)
            )
            publishAvailability(state.initializationError, state.isReady)
        }
    }

    private func drainLocationWrites() {
        drainBmsVoltageWrites()
        guard let state,
            state.initializationError == nil,
            state.isReady,
            state.hasPendingLocationWrites
        else { return }
        publishDecisionBatch(state.pollLocationWriteOutcomes(atMs: clock.now().rawValue))
    }

    private func drainBmsVoltageWrites() {
        guard let state else { return }
        for outcome in state.finishBmsVoltageWrites() {
            if let error = outcome.error {
                recordDiagnostic("bms_storage_error request=\(outcome.requestId) error=\(error)")
            }
        }
    }

    private func publishDecisionBatch(_ outcomes: [MobileRideMapOutcomeDto]) {
        guard !outcomes.isEmpty else { return }
        publishDecisions(RideMapDecisionBatch(outcomes: outcomes))
    }

    private func synchronizeLocationDemand() {
        guard state?.takeLocationAcquisitionChange() == true else { return }
        onLocationDemandChanged()
    }

    private nonisolated func reserveRecordingWork(observationCount: UInt64) -> MobileRideMapRecordingWorkPermit? {
        guard let state else { return nil }
        do {
            return try state.reserveRecordingWork(observationCount: observationCount)
        } catch {
            recordDiagnostic("ride_map_callback_admission_error=\(error)")
            return nil
        }
    }

    private func requireState() throws -> MobileRideMapState {
        guard let state else {
            throw MobileRideMapError.storageError("Rust ride database is unavailable")
        }
        return state
    }
}
