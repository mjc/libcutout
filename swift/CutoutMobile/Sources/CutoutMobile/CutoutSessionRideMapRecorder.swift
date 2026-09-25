import CutoutMobileFFI
import Foundation

struct RideMapDecisionBatch: Sendable {
    let snapshot: MobileRideMapSnapshotDto?
    let decisions: [MobileRideMapDecisionDto]
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
        connectionState: CutoutSessionStateHandle,
        resetTripMeter: @escaping @Sendable (ConnectionAttemptToken) -> Void
    )
    func ingestLocation(_ update: PhoneLocationUpdate)
    func startGpsOnly(atMs: UInt64) async throws -> MobileRideMapSnapshotDto
    func pause(atMs: UInt64) async throws -> MobileRideMapSnapshotDto
    func resume(atMs: UInt64) async throws -> MobileRideMapSnapshotDto
    func stop(atMs: UInt64) async throws -> MobileRideMapSnapshotDto
    func save() async throws -> MobileRideMapSnapshotDto
    func discard() async throws -> MobileRideMapSnapshotDto
}

/// Owns Rust ride-map state and storage effects on one serial executor. Protocol truth remains in Core.
actor CutoutSessionRideMapRecorder: CutoutSessionRideMapRecording {
    private nonisolated let queue = DispatchSerialQueue(label: "io.cutout.ridemap", qos: .utility)
    private let state: MobileRideMapState?
    private let clock: MonotonicClock
    private let wallClock: @Sendable () -> Date
    private let publishSnapshot: @Sendable (MobileRideMapSnapshotDto) -> Void
    private let publishDecisions: @Sendable (RideMapDecisionBatch) -> Void
    private let publishError: @Sendable (MobileRideMapError, MobileRideMapErrorContext) -> Void
    private let publishAvailability: @Sendable (MobileRideMapError?, Bool) -> Void
    private let recordDiagnostic: @Sendable (String) -> Void
    private let onLocationDemand: @Sendable (Bool) -> Void
    private var writePoller: DispatchSourceTimer?
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
        onLocationDemand: @escaping @Sendable (Bool) -> Void
    ) {
        self.state = state
        self.clock = clock
        self.wallClock = wallClock
        self.publishSnapshot = publishSnapshot
        self.publishDecisions = publishDecisions
        self.publishError = publishError
        self.publishAvailability = publishAvailability
        self.recordDiagnostic = recordDiagnostic
        self.onLocationDemand = onLocationDemand
    }

    deinit {
        writePoller?.cancel()
    }

    nonisolated func start(
        replayConnection: @escaping @Sendable () -> Void
    ) {
        queue.async { [self] in
            assumeIsolated { recorder in
                recorder.startWritePolling()
                recorder.startRestoration(replayConnection: replayConnection)
            }
        }
    }

    func startGpsOnly(atMs: UInt64) throws -> MobileRideMapSnapshotDto {
        let snapshot = try requireState().startGpsOnly(atMs: atMs)
        synchronizeLocationDemand()
        return snapshot
    }

    func pause(atMs: UInt64) throws -> MobileRideMapSnapshotDto {
        let snapshot = try requireState().pause(atMs: atMs)
        synchronizeLocationDemand()
        return snapshot
    }

    func resume(atMs: UInt64) throws -> MobileRideMapSnapshotDto {
        let snapshot = try requireState().resume(atMs: atMs)
        synchronizeLocationDemand()
        return snapshot
    }

    func stop(atMs: UInt64) throws -> MobileRideMapSnapshotDto {
        let snapshot = try requireState().stop(atMs: atMs)
        synchronizeLocationDemand()
        return snapshot
    }

    func save() throws -> MobileRideMapSnapshotDto {
        let snapshot = try requireState().save()
        synchronizeLocationDemand()
        return snapshot
    }

    func discard() throws -> MobileRideMapSnapshotDto {
        let snapshot = try requireState().discard()
        synchronizeLocationDemand()
        return snapshot
    }

    nonisolated func observeConnection(
        at receivedAt: MonotonicMilliseconds,
        token: ConnectionAttemptToken,
        connectionState: CutoutSessionStateHandle,
        resetTripMeter: @escaping @Sendable (ConnectionAttemptToken) -> Void
    ) {
        queue.async { [weak self] in
            guard let self else { return }
            self.assumeIsolated { recorder in
                guard recorder.state?.isReady == true else { return }
                do {
                    let previousRideID = recorder.state?.currentSnapshot(atMs: receivedAt.rawValue)?.rideID
                    guard let state = recorder.state else { return }
                    let snapshot = try state.ensureRecordingForVerifiedConnection(
                        connectionState: connectionState,
                        token: token,
                        atMs: receivedAt.rawValue
                    )
                    if snapshot?.rideID != previousRideID {
                        resetTripMeter(token)
                    }
                    _ = try state.observeTelemetry(atMs: receivedAt.rawValue)
                    if let snapshot = state.currentSnapshot(atMs: receivedAt.rawValue) {
                        recorder.publishSnapshot(snapshot)
                    }
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
        queue.async { [weak self] in
            guard let self else { return }
            self.assumeIsolated { recorder in
                guard let state = recorder.state,
                    state.initializationError == nil,
                    state.isReady,
                    let deviceIdentity,
                    let wallClockMilliseconds = unixMilliseconds(for: recorder.wallClock())
                else { return }
                let samples = bmsStorageSamples(
                    observations: observations,
                    wallClockMilliseconds: wallClockMilliseconds,
                    sessionIdentifier: sessionIdentifier
                )
                guard !samples.isEmpty else { return }
                do {
                    try state.recordBmsVoltageSamples(deviceIdentity: deviceIdentity, samples: samples)
                } catch {
                    recorder.recordDiagnostic("bms_storage_error=\(error)")
                }
            }
        }
    }

    nonisolated func ingestLocation(_ update: PhoneLocationUpdate) {
        queue.async { [weak self] in
            guard let self else { return }
            self.assumeIsolated { recorder in
                guard let state = recorder.state,
                    state.initializationError == nil,
                    state.isReady,
                    let receiptWallClockUnixMs = unixMilliseconds(for: update.receiptWallClock)
                else { return }
                let recordingToken =
                    state
                    .currentSnapshot(atMs: update.receiptMonotonic.rawValue)?
                    .recordingToken
                let errorContext = MobileRideMapErrorContext(recordingToken: recordingToken)
                do {
                    let decisions = try state.ingestLocationBatch(
                        recordingToken: recordingToken,
                        receiptMonotonicMs: update.receiptMonotonic.rawValue,
                        receiptWallClockUnixMs: receiptWallClockUnixMs,
                        samples: update.samples
                    )
                    recorder.publishDecisionBatch(decisions)
                } catch let error as MobileRideMapError {
                    recorder.publishError(error, errorContext)
                    recorder.recordDiagnostic("ride_map_ingest_error=\(error)")
                } catch {
                    recorder.recordDiagnostic("ride_map_ingest_error=\(error)")
                }
            }
        }
    }

    private func startWritePolling() {
        guard writePoller == nil else { return }
        let timer = DispatchSource.makeTimerSource(queue: queue)
        timer.schedule(
            deadline: .now() + .milliseconds(100),
            repeating: .milliseconds(100),
            leeway: .milliseconds(25)
        )
        timer.setEventHandler { [weak self] in
            self?.assumeIsolated { recorder in
                recorder.drainLocationWrites()
            }
        }
        writePoller = timer
        timer.resume()
    }

    private func startRestoration(replayConnection: @escaping @Sendable () -> Void) {
        guard !restorationStarted else { return }
        restorationStarted = true
        guard let state else {
            publishAvailability(nil, false)
            return
        }
        do {
            if let snapshot = try state.restore(atMs: clock.now().rawValue) {
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
        guard let state,
            state.initializationError == nil,
            state.isReady,
            state.hasPendingLocationWrites
                || state.currentSnapshot(atMs: clock.now().rawValue)?.state.isOpen == true
        else { return }
        publishDecisionBatch(state.pollLocationWrites())
    }

    private func publishDecisionBatch(_ decisions: [MobileRideMapDecisionDto]) {
        publishDecisions(
            RideMapDecisionBatch(
                snapshot: state?.currentSnapshot(atMs: clock.now().rawValue),
                decisions: decisions
            ))
    }

    private func synchronizeLocationDemand() {
        let active = state?.currentSnapshot(atMs: clock.now().rawValue)?.state == .active
        onLocationDemand(active)
    }

    private func requireState() throws -> MobileRideMapState {
        guard let state else {
            throw MobileRideMapError.storageError("Rust ride database is unavailable")
        }
        return state
    }
}
