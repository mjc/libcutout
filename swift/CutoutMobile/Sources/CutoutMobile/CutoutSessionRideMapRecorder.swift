import Foundation
import CutoutMobileFFI

struct RideMapDecisionBatch {
    let snapshot: MobileRideMapSnapshotDto?
    let decisions: [MobileRideMapDecisionDto]
}

protocol CutoutSessionRideMapRecording: AnyObject {
    var stateHandle: MobileRideMapState? { get }
    var initializationError: MobileRideMapError? { get }
    var isReady: Bool { get }
    func start(replayConnection: @escaping () -> Void)
    func beginBmsStorageSession()
    func observeConnection(
        at receivedAt: MonotonicMilliseconds,
        token: ConnectionAttemptToken,
        isConnectionCurrent: @escaping () -> Bool,
        connectionState: CutoutSessionStateHandle,
        resetTripMeter: @escaping () -> Bool
    )
    func persistBmsSamples(_ observations: [BmsRawVoltageObservation], deviceIdentity: String?)
    func ingestLocation(_ update: PhoneLocationUpdate)
    func startGpsOnly(atMs: UInt64) throws -> MobileRideMapSnapshotDto
    func pause(atMs: UInt64) throws -> MobileRideMapSnapshotDto
    func resume(atMs: UInt64) throws -> MobileRideMapSnapshotDto
    func stop(atMs: UInt64) throws -> MobileRideMapSnapshotDto
    func save() throws -> MobileRideMapSnapshotDto
    func discard() throws -> MobileRideMapSnapshotDto
}

/// Owns the Rust ride-map storage queue and its recording effects. Protocol truth remains in Core.
final class CutoutSessionRideMapRecorder: CutoutSessionRideMapRecording {
    private let state: MobileRideMapState?
    private let clock: MonotonicClock
    private let wallClock: () -> Date
    private let queue = DispatchQueue(label: "io.cutout.ridemap", qos: .utility)
    private let queueKey = DispatchSpecificKey<Void>()
    private let publishSnapshot: (MobileRideMapSnapshotDto) -> Void
    private let publishDecisions: (RideMapDecisionBatch) -> Void
    private let publishError: (MobileRideMapError, MobileRideMapErrorContext) -> Void
    private let publishAvailability: () -> Void
    private let recordDiagnostic: (String) -> Void
    private let onLocationDemand: (Bool) -> Void
    private var writePoller: DispatchSourceTimer?
    private var restorationStarted = false
    private var bmsStorageSessionIdentifier = UUID().uuidString

    var stateHandle: MobileRideMapState? {
        state
    }

    var initializationError: MobileRideMapError? {
        state?.initializationError
    }

    var isReady: Bool {
        state?.isReady ?? false
    }

    init(
        state: MobileRideMapState?,
        clock: MonotonicClock,
        wallClock: @escaping () -> Date,
        publishSnapshot: @escaping (MobileRideMapSnapshotDto) -> Void,
        publishDecisions: @escaping (RideMapDecisionBatch) -> Void,
        publishError: @escaping (MobileRideMapError, MobileRideMapErrorContext) -> Void,
        publishAvailability: @escaping () -> Void,
        recordDiagnostic: @escaping (String) -> Void,
        onLocationDemand: @escaping (Bool) -> Void
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
        queue.setSpecific(key: queueKey, value: ())
    }

    deinit {
        writePoller?.cancel()
    }

    func start(
        replayConnection: @escaping () -> Void
    ) {
        startWritePolling()
        startRestoration(replayConnection: replayConnection)
    }

    func beginBmsStorageSession() {
        bmsStorageSessionIdentifier = UUID().uuidString
    }

    func perform<T>(_ operation: () throws -> T) rethrows -> T {
        if DispatchQueue.getSpecific(key: queueKey) != nil {
            return try operation()
        }
        return try queue.sync(execute: operation)
    }

    func startGpsOnly(atMs: UInt64) throws -> MobileRideMapSnapshotDto {
        let snapshot = try perform { try requireState().startGpsOnly(atMs: atMs) }
        synchronizeLocationDemand()
        return snapshot
    }

    func pause(atMs: UInt64) throws -> MobileRideMapSnapshotDto {
        let snapshot = try perform { try requireState().pause(atMs: atMs) }
        synchronizeLocationDemand()
        return snapshot
    }

    func resume(atMs: UInt64) throws -> MobileRideMapSnapshotDto {
        let snapshot = try perform { try requireState().resume(atMs: atMs) }
        synchronizeLocationDemand()
        return snapshot
    }

    func stop(atMs: UInt64) throws -> MobileRideMapSnapshotDto {
        let snapshot = try perform { try requireState().stop(atMs: atMs) }
        synchronizeLocationDemand()
        return snapshot
    }

    func save() throws -> MobileRideMapSnapshotDto {
        let snapshot = try perform { try requireState().save() }
        synchronizeLocationDemand()
        return snapshot
    }

    func discard() throws -> MobileRideMapSnapshotDto {
        let snapshot = try perform { try requireState().discard() }
        synchronizeLocationDemand()
        return snapshot
    }

    func observeConnection(
        at receivedAt: MonotonicMilliseconds,
        token: ConnectionAttemptToken,
        isConnectionCurrent: @escaping () -> Bool,
        connectionState: CutoutSessionStateHandle,
        resetTripMeter: @escaping () -> Bool
    ) {
        guard isReady else { return }
        queue.async { [weak self] in
            guard let self,
                  isConnectionCurrent()
            else { return }
            do {
                let previousRideID = self.state?.currentSnapshot(atMs: receivedAt.rawValue)?.rideID
                guard let state = self.state else { return }
                let snapshot = try state.ensureRecordingForVerifiedConnection(
                    connectionState: connectionState,
                    token: token,
                    atMs: receivedAt.rawValue
                )
                if snapshot?.rideID != previousRideID {
                    _ = resetTripMeter()
                }
                _ = try state.observeTelemetry(atMs: receivedAt.rawValue)
                if let snapshot = state.currentSnapshot(atMs: receivedAt.rawValue) {
                    self.publishSnapshot(snapshot)
                }
                self.synchronizeLocationDemand()
            } catch let error as MobileRideMapError where error == .staleConnection {
                return
            } catch let error as MobileRideMapError {
                let snapshot = self.state?.currentSnapshot(atMs: receivedAt.rawValue)
                if let snapshot {
                    self.publishSnapshot(snapshot)
                }
                self.publishError(error, MobileRideMapErrorContext(snapshot: snapshot))
                self.recordDiagnostic("ride_map_connection_error=\(error)")
            } catch {
                self.recordDiagnostic("ride_map_connection_error=\(error)")
            }
        }
    }

    func persistBmsSamples(
        _ observations: [BmsRawVoltageObservation],
        deviceIdentity: String?
    ) {
        guard let state,
              state.initializationError == nil,
              state.isReady,
              let deviceIdentity,
              let wallClockMilliseconds = unixMilliseconds(for: wallClock())
        else { return }
        let samples = bmsStorageSamples(
            observations: observations,
            wallClockMilliseconds: wallClockMilliseconds,
            sessionIdentifier: bmsStorageSessionIdentifier
        )
        guard !samples.isEmpty else { return }

        queue.async { [weak self] in
            guard let self else { return }
            do {
                try state.recordBmsVoltageSamples(deviceIdentity: deviceIdentity, samples: samples)
            } catch {
                self.recordDiagnostic("bms_storage_error=\(error)")
            }
        }
    }

    func ingestLocation(_ update: PhoneLocationUpdate) {
        guard let state,
              state.initializationError == nil,
              state.isReady,
              let receiptWallClockUnixMs = unixMilliseconds(for: update.receiptWallClock)
        else { return }

        queue.async { [weak self] in
            guard let self else { return }
            let recordingToken = state
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
                self.publishDecisionBatch(decisions)
            } catch let error as MobileRideMapError {
                self.publishError(error, errorContext)
                self.recordDiagnostic("ride_map_ingest_error=\(error)")
            } catch {
                self.recordDiagnostic("ride_map_ingest_error=\(error)")
            }
        }
    }

    private func startWritePolling() {
        queue.async { [weak self] in
            guard let self, self.writePoller == nil else { return }
            let timer = DispatchSource.makeTimerSource(queue: self.queue)
            timer.schedule(
                deadline: .now() + .milliseconds(100),
                repeating: .milliseconds(100),
                leeway: .milliseconds(25)
            )
            timer.setEventHandler { [weak self] in
                self?.drainLocationWrites()
            }
            self.writePoller = timer
            timer.resume()
        }
    }

    private func startRestoration(replayConnection: @escaping () -> Void) {
        queue.async { [weak self] in
            guard let self, !self.restorationStarted else { return }
            self.restorationStarted = true
            guard let state = self.state else {
                self.publishAvailability()
                return
            }
            do {
                if let snapshot = try state.restore(atMs: self.clock.now().rawValue) {
                    self.publishSnapshot(snapshot)
                }
                self.publishAvailability()
                self.synchronizeLocationDemand()
                replayConnection()
            } catch let error as MobileRideMapError {
                self.restorationStarted = false
                self.publishError(error, MobileRideMapErrorContext(snapshot: nil))
                self.publishAvailability()
            } catch {
                self.restorationStarted = false
                self.publishError(
                    .storageError(String(describing: error)),
                    MobileRideMapErrorContext(snapshot: nil)
                )
                self.publishAvailability()
            }
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
        publishDecisions(RideMapDecisionBatch(
            snapshot: state?.currentSnapshot(atMs: clock.now().rawValue),
            decisions: decisions
        ))
    }

    private func synchronizeLocationDemand() {
        let active = perform {
            state?.currentSnapshot(atMs: clock.now().rawValue)?.state == .active
        }
        onLocationDemand(active)
    }

    private func requireState() throws -> MobileRideMapState {
        guard let state else {
            throw MobileRideMapError.storageError("Rust ride database is unavailable")
        }
        return state
    }
}
