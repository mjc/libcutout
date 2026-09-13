import CutoutMobile
import CutoutMobileFFI
import Foundation
import Observation

/// Owns live recording presentation and its asynchronous lifetime. Rust owns ride policy.
@MainActor
@Observable
final class RideRecordingModel {
    private(set) var snapshot: MobileRideMapSnapshotDto?
    var error: MobileRideMapError?
    private(set) var lastDecision: MobileRideMapDecisionDto?
    private(set) var displayPoints = [MobileRideMapRouteDisplayPoint]()
    private(set) var cameraRegion: MobileRideMapCameraRegion?
    private(set) var endpointMetadata = MobileRideMapRouteEndpointMetadata.empty
    private(set) var segments = [MobileRideMapSegmentDisplayMetadata]()
    var telemetryState: MobileRideMapTelemetryStateDto? { snapshot?.telemetryState }
    private(set) var backgroundGapCount: UInt64 = 0
    private(set) var projectionVersion: UInt64 = 0
    private(set) var pointsTruncated = false
    private(set) var segmentsOmittedByBudget = false
    private(set) var isCommandPending = false

    private let core: any CutoutSessionDriving
    private var projectionTask: Task<Void, Never>?
    private var projectionCancellation: MobileLiveRideMapProjectionCancellation?
    private var projectionGeneration: UInt64 = 0
    private var projectionEnabled = false
    private var durationTask: Task<Void, Never>?
    private var commandTask: Task<MobileRideMapSnapshotDto, Error>?
    private var commandGeneration: UInt64 = 0
    private var checkpointTask: Task<Void, Never>?
    private var checkpointGeneration: UInt64 = 0
    private var backgroundLease: RideBackgroundTask?
    var isCheckpointPending: Bool { checkpointTask != nil }

    init(core: any CutoutSessionDriving) {
        self.core = core
    }

    isolated deinit { cancel() }

    func cancel() {
        cancelCheckpoint()
        invalidateProjection(clearPoints: false)
        projectionTask?.cancel()
        durationTask?.cancel()
        durationTask = nil
        commandGeneration &+= 1
        commandTask?.cancel()
        commandTask = nil
        isCommandPending = false
    }

    /// Gives the existing worker time to settle writes queued before backgrounding.
    func checkpoint() {
        guard checkpointTask == nil else { return }
        checkpointGeneration &+= 1
        let generation = checkpointGeneration
        backgroundLease = RideBackgroundTask { [weak self] in self?.cancelCheckpoint() }
        let core = core
        checkpointTask = Task { [weak self] in
            do {
                try await core.checkpointRideMap()
            } catch let failure as MobileRideMapError {
                guard !Task.isCancelled, self?.checkpointGeneration == generation else { return }
                self?.error = failure
            } catch {
                guard !Task.isCancelled, self?.checkpointGeneration == generation else { return }
                self?.error = .storageError(error.localizedDescription)
            }
            guard !Task.isCancelled, self?.checkpointGeneration == generation else { return }
            self?.checkpointTask = nil
            self?.backgroundLease?.end()
            self?.backgroundLease = nil
        }
    }

    private func cancelCheckpoint() {
        checkpointGeneration &+= 1
        checkpointTask?.cancel()
        checkpointTask = nil
        backgroundLease?.end()
        backgroundLease = nil
    }

    var onRideChange: ((MobileRideMapSnapshotDto) -> Void)?

    func restore() {
        guard let snapshot = core.rideMapStateHandle?.currentSnapshot() else { return }
        applySnapshot(snapshot)
    }

    func applyDecision(snapshot: MobileRideMapSnapshotDto, decision: MobileRideMapDecisionDto) {
        guard applySnapshot(snapshot) else { return }
        error = nil
        lastDecision = decision
    }

    @discardableResult
    func applySnapshot(_ incoming: MobileRideMapSnapshotDto) -> Bool {
        if let snapshot, incoming.revision < snapshot.revision { return false }
        let prior = snapshot
        let changedRide = prior?.rideID != incoming.rideID
        let changedRoute = changedRide || prior?.summary.pointCount != incoming.summary.pointCount
        if changedRide { invalidateProjection(clearPoints: true) }
        snapshot = incoming
        if changedRide { onRideChange?(incoming) }
        if prior?.state != incoming.state { updateDurationTicker() }
        if changedRoute { requestProjection() }
        return true
    }

    func refreshDuration() async {
        guard let snapshot = await core.currentRideMapSnapshot(atMs: core.now().rawValue) else { return }
        guard !Task.isCancelled else { return }
        applySnapshot(snapshot)
    }

    private func updateDurationTicker() {
        durationTask?.cancel()
        durationTask = nil
        guard snapshot?.state == .active else { return }
        let core = core
        durationTask = Task { [weak self] in
            while !Task.isCancelled {
                let snapshot = await core.currentRideMapSnapshot(atMs: core.now().rawValue)
                guard !Task.isCancelled, self != nil else { return }
                if let snapshot { self?.applySnapshot(snapshot) }
                do { try await Task.sleep(for: .seconds(1)) }
                catch { return }
            }
        }
    }

    /// Coalesces restore and live requests through one cancellable projection.
    private func requestProjection() {
        projectionGeneration &+= 1
        projectionEnabled = true
        projectionCancellation?.cancel()
        guard projectionTask == nil, let state = core.rideMapStateHandle else { return }
        let budget = MobileRideMapLimits.rustOwned.liveTailPointLimit
        projectionTask = Task { [weak self] in
            while !Task.isCancelled {
                guard let generation = self?.beginProjection() else { return }
                let cancellation = MobileLiveRideMapProjectionCancellation()
                self?.projectionCancellation = cancellation
                let result = await Task.detached(priority: .userInitiated) {
                    Result { try state.projectPoints(budget: budget, cancellation: cancellation) }
                }.value
                guard !Task.isCancelled else { return }
                guard self?.finishProjection(result, generation: generation) == true else { return }
            }
        }
    }

    private func beginProjection() -> UInt64? {
        guard projectionEnabled else {
            projectionTask = nil
            projectionCancellation = nil
            return nil
        }
        return projectionGeneration
    }

    private func finishProjection(
        _ result: Result<MobileRideMapRouteProjection, Error>, generation: UInt64
    ) -> Bool {
        guard projectionEnabled else {
            projectionTask = nil
            projectionCancellation = nil
            return false
        }
        guard generation == projectionGeneration else { return true }
        projectionTask = nil
        projectionCancellation = nil
        switch result {
        case let .success(projection): applyLiveProjection(projection)
        case let .failure(failure):
            error = Self.mapError(failure)
            clearLiveProjectionState()
        }
        return false
    }

    private func applyLiveProjection(_ projection: MobileRideMapRouteProjection) {
        projectionVersion &+= 1
        displayPoints = projection.points
        cameraRegion = projection.cameraRegion
        endpointMetadata = projection.endpointMetadata
        segments = projection.segments
        backgroundGapCount = projection.backgroundGapCount
        pointsTruncated = projection.pointsOmittedByBudget
        segmentsOmittedByBudget = projection.segmentsOmittedByBudget
    }

    private func clearLiveProjectionState() {
        projectionVersion &+= 1
        displayPoints.removeAll(keepingCapacity: true)
        cameraRegion = nil
        endpointMetadata = .empty
        segments.removeAll(keepingCapacity: true)
        backgroundGapCount = 0
        pointsTruncated = false
        segmentsOmittedByBudget = false
    }

    func invalidateProjection(clearPoints: Bool) {
        projectionGeneration &+= 1
        projectionEnabled = false
        projectionCancellation?.cancel()
        if clearPoints {
            clearLiveProjectionState()
            lastDecision = nil
        }
    }

    func command(_ event: MobileRideEventDto, lastConnectedVehicle: String? = nil) async -> Bool {
        let expected = snapshot?.commandToken
        let atMs = core.now().rawValue
        let core = core
        return await performCommand {
            if event == .start { core.resetRideMapLocationAdmission() }
            return try await core.applyRideMapCommand(
                expected: expected, event: event, atMs: atMs,
                lastConnectedVehicle: lastConnectedVehicle
            )
        }
    }

    func performCommand(
        _ operation: @escaping @MainActor () async throws -> MobileRideMapSnapshotDto
    ) async -> Bool {
        guard !isCommandPending else { return false }
        commandGeneration &+= 1
        let generation = commandGeneration
        isCommandPending = true
        let task = Task {
            try Task.checkCancellation()
            return try await operation()
        }
        commandTask = task
        defer {
            if generation == commandGeneration {
                commandTask = nil
                isCommandPending = false
            }
        }
        do {
            let snapshot = try await withTaskCancellationHandler {
                try await task.value
            } onCancel: {
                task.cancel()
            }
            guard !Task.isCancelled, !task.isCancelled, generation == commandGeneration else { return false }
            guard applySnapshot(snapshot) else { return false }
            error = nil
            return true
        } catch {
            guard !Task.isCancelled, !task.isCancelled, generation == commandGeneration else { return false }
            self.error = Self.mapError(error)
            return false
        }
    }

    private static func mapError(_ error: Error) -> MobileRideMapError {
        error as? MobileRideMapError ?? .storageError(String(describing: error))
    }
}
