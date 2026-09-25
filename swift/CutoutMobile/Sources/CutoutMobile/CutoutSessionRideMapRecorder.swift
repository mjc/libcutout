import CutoutMobileFFI
import Foundation

struct RideMapDecisionBatch: Sendable {
    let outcomes: [MobileRideMapOutcomeDto]
}

private struct PendingRideMapConnectionAdmission {
    let admission: MobileRideMapConnectionAdmission
    let previousRideID: String?
    let atMs: UInt64
    let token: ConnectionAttemptToken
    let connectionState: CutoutSessionStateHandle
    let resetTripMeter: @Sendable (ConnectionAttemptToken) -> Void
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
    private var pendingConnectionAdmissions: [PendingRideMapConnectionAdmission] = []
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

    func pause(atMs: UInt64) async throws -> MobileRideMapSnapshotDto {
        let snapshot = try await transition(.pause, atMs: atMs)
        synchronizeLocationDemand()
        return snapshot
    }

    func resume(atMs: UInt64) async throws -> MobileRideMapSnapshotDto {
        let snapshot = try await transition(.resume, atMs: atMs)
        synchronizeLocationDemand()
        return snapshot
    }

    func stop(atMs: UInt64) async throws -> MobileRideMapSnapshotDto {
        let snapshot = try await transition(.stop, atMs: atMs)
        synchronizeLocationDemand()
        return snapshot
    }

    func save() async throws -> MobileRideMapSnapshotDto {
        let snapshot = try await transition(.save, atMs: clock.now().rawValue)
        synchronizeLocationDemand()
        return snapshot
    }

    func discard() async throws -> MobileRideMapSnapshotDto {
        let snapshot = try await transition(.discard, atMs: clock.now().rawValue)
        synchronizeLocationDemand()
        return snapshot
    }

    private func transition(
        _ event: MobileRideEventDto,
        atMs: UInt64
    ) async throws -> MobileRideMapSnapshotDto {
        let state = try requireState()
        let command = try state.beginLifecycleCommand(event: event, atMs: atMs)
        // Rust has accepted this mutation and holds its lifecycle barrier until the command
        // reaches a terminal result. Keep polling even if the UI task that requested it is
        // cancelled; otherwise cancellation makes Task.sleep return immediately on every pass.
        let completion = Task {
            while true {
                switch try state.pollLifecycleCommand(command) {
                case .pending:
                    try await Task.sleep(nanoseconds: 10_000_000)
                case let .completed(snapshot):
                    return snapshot
                }
            }
        }
        return try await completion.value
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
                    let admission = try state.beginVerifiedConnectionAdmission(
                        connectionState: connectionState,
                        token: token,
                        atMs: receivedAt.rawValue
                    )
                    recorder.pendingConnectionAdmissions.append(
                        PendingRideMapConnectionAdmission(
                            admission: admission,
                            previousRideID: previousRideID,
                            atMs: receivedAt.rawValue,
                            token: token,
                            connectionState: connectionState,
                            resetTripMeter: resetTripMeter
                        )
                    )
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
                    try state.queueBmsVoltageSamples(deviceIdentity: deviceIdentity, samples: samples)
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
                    let outcomes = try state.ingestLocationBatchOutcomes(
                        recordingToken: recordingToken,
                        receiptMonotonicMs: update.receiptMonotonic.rawValue,
                        receiptWallClockUnixMs: receiptWallClockUnixMs,
                        samples: update.samples
                    )
                    recorder.publishDecisionBatch(outcomes)
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
                recorder.drainConnectionAdmissions()
                recorder.drainLocationWrites()
            }
        }
        writePoller = timer
        timer.resume()
    }

    private func drainConnectionAdmissions() {
        guard let state else { return }
        var remaining = [PendingRideMapConnectionAdmission]()
        remaining.reserveCapacity(pendingConnectionAdmissions.count)
        for pending in pendingConnectionAdmissions {
            do {
                switch try state.pollVerifiedConnectionAdmission(pending.admission) {
                case .pending:
                    remaining.append(pending)
                case let .completed(admissionSnapshot):
                    let connectionIsCurrent = pending.connectionState
                        .verifiedConnectionAttemptIsCurrent(token: pending.token)
                    if connectionIsCurrent,
                        admissionSnapshot?.rideID != pending.previousRideID
                    {
                        pending.resetTripMeter(pending.token)
                    }
                    if connectionIsCurrent {
                        _ = try state.observeTelemetry(atMs: pending.atMs)
                    }
                    if let snapshot = state.currentSnapshot(atMs: pending.atMs) ?? admissionSnapshot {
                        publishSnapshot(snapshot)
                    }
                    synchronizeLocationDemand()
                }
            } catch let error as MobileRideMapError where error == .staleConnection {
                continue
            } catch let error as MobileRideMapError {
                let snapshot = state.currentSnapshot(atMs: pending.atMs)
                if let snapshot { publishSnapshot(snapshot) }
                publishError(error, MobileRideMapErrorContext(snapshot: snapshot))
                recordDiagnostic("ride_map_connection_error=\(error)")
            } catch {
                recordDiagnostic("ride_map_connection_error=\(error)")
            }
        }
        pendingConnectionAdmissions = remaining
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
        drainBmsVoltageWrites()
        guard let state,
            state.initializationError == nil,
            state.isReady,
            state.hasPendingLocationWrites
                || state.currentSnapshot(atMs: clock.now().rawValue)?.state.isOpen == true
        else { return }
        publishDecisionBatch(state.pollLocationWriteOutcomes(atMs: clock.now().rawValue))
    }

    private func drainBmsVoltageWrites() {
        guard let state else { return }
        for outcome in state.pollBmsVoltageWrites() {
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
