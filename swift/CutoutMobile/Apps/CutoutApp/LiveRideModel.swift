import CutoutMobile
import CutoutMobileFFI
import Foundation
import Observation

/// The Rust-owned ride snapshot and bounded route queries used by live presentation.
protocol LiveRideQuerying: Sendable {
    func currentSnapshot() -> MobileRideMapSnapshotDto?
    func currentSnapshot(atMs: UInt64) -> MobileRideMapSnapshotDto?
    func projectStoredPoints(
        rideID: String,
        budget: UInt32,
        viewport: MobileGeoBoundsDto?,
        privacy: MobileRideMapRoutePrivacyPolicy,
        cancellation: MobileRideMapProjectionCancellation?
    ) throws -> MobileRideMapRouteProjection
    func projectCurrentRoutePoints(
        budget: UInt32,
        rideID: String?,
        viewport: MobileGeoBoundsDto?,
        privacy: MobileRideMapRoutePrivacyPolicy,
        durableCancellation: MobileRideMapProjectionCancellation?,
        liveCancellation: MobileLiveRideMapProjectionCancellation?
    ) throws -> MobileRideMapRouteProjection
}

extension MobileRideMapState: LiveRideQuerying {}

enum LiveRideCommand: Sendable {
    case startGpsOnly(atMs: UInt64, musicHistoryPolicy: MobileMusicHistoryPolicyDto)
    case pause(expected: MobileRideMapCommandTokenDto, atMs: UInt64)
    case resume(expected: MobileRideMapCommandTokenDto, atMs: UInt64)
    case stop(expected: MobileRideMapCommandTokenDto, atMs: UInt64)
    case save(expected: MobileRideMapCommandTokenDto)
    case discard(expected: MobileRideMapCommandTokenDto)

    var resetsPoints: Bool {
        switch self {
        case .startGpsOnly, .discard: true
        case .pause, .resume, .stop, .save: false
        }
    }
}

/// Native lifetime and presentation for the Rust-owned live ride.
@MainActor
@Observable
final class LiveRideModel {
    private(set) var snapshot: MobileRideMapSnapshotDto?
    private(set) var storageError: String?
    private(set) var availability: MobileRideMapAvailability
    private(set) var error: MobileRideMapError?
    private(set) var displayPoints = [MobileRideMapRouteDisplayPoint]()
    private(set) var cameraRegion: MobileRideMapCameraRegion?
    private(set) var endpointMetadata = MobileRideMapRouteEndpointMetadata.empty
    private(set) var segments = [MobileRideMapSegmentDisplayMetadata]()
    private(set) var telemetryState: MobileRideMapTelemetryStateDto?
    private(set) var backgroundGapCount: UInt64 = 0
    private(set) var projectionVersion: UInt64 = 0
    private(set) var pointsTruncated = false
    private(set) var segmentsOmittedByBudget = false
    private(set) var lastDecision: MobileRideMapDecisionDto?
    private(set) var isMapVisible = false
    private(set) var isSceneActive = false

    private let state: (any LiveRideQuerying)?
    private let now: @MainActor () -> UInt64
    private let executeCommand: @MainActor (LiveRideCommand) async throws -> MobileRideMapSnapshotDto
    private var projectionTask: Task<Void, Never>?
    private var durationTask: Task<Void, Never>?
    private var liveCancellation: MobileLiveRideMapProjectionCancellation?
    private var durableCancellation: MobileRideMapProjectionCancellation?
    private var projectionGeneration: UInt64 = 0
    private var projectionEnabled = false
    private var projectionRequested = false
    private var didRestore = false
    private var snapshotReadTask: Task<Void, Never>?
    private var durationReadTask: Task<Void, Never>?
    private var pendingDurationRead: (atMs: UInt64, generation: UInt64)?
    private var errorReadTask: Task<Void, Never>?
    private var pendingError: MobileRideMapErrorEvent?
    private var snapshotGeneration: UInt64 = 0

    private struct ProjectionRequest {
        let rideID: String
        let generation: UInt64
        let liveCancellation: MobileLiveRideMapProjectionCancellation
        let durableCancellation: MobileRideMapProjectionCancellation
    }

    init(
        state: (any LiveRideQuerying)?,
        storageError: String?,
        availability: MobileRideMapAvailability,
        now: @escaping @MainActor () -> UInt64,
        executeCommand: @escaping @MainActor (LiveRideCommand) async throws -> MobileRideMapSnapshotDto = { _ in
            throw MobileRideMapError.noActiveRide
        }
    ) {
        self.state = state
        self.storageError = storageError
        self.availability = availability
        self.now = now
        self.executeCommand = executeCommand
    }

    isolated deinit {
        snapshotReadTask?.cancel()
        durationReadTask?.cancel()
        errorReadTask?.cancel()
        projectionTask?.cancel()
        durationTask?.cancel()
        liveCancellation?.cancel()
        durableCancellation?.cancel()
    }

    @discardableResult
    func restore() -> Task<Void, Never>? {
        guard let state else { return nil }
        if let snapshotReadTask { return snapshotReadTask }
        didRestore = true
        let generation = snapshotGeneration
        snapshotReadTask = Task { [weak self] in
            defer { self?.snapshotReadTask = nil }
            let next = await Task.detached(priority: .utility) { state.currentSnapshot() }.value
            guard !Task.isCancelled else { return }
            let projection = self?.applyRestoredSnapshot(next, generation: generation)
            await projection?.value
        }
        return snapshotReadTask
    }

    private func applyRestoredSnapshot(
        _ next: MobileRideMapSnapshotDto?, generation: UInt64
    ) -> Task<Void, Never>? {
        guard generation == snapshotGeneration else { return nil }
        if let next {
            applySnapshot(next)
            if snapshot?.rideID.isEmpty == false {
                if projectionTask == nil { requestProjection() }
                return projectionTask
            }
        } else {
            snapshot = nil
            telemetryState = nil
            updateDurationTicker()
            invalidateProjection(clearPoints: true)
        }
        return nil
    }

    func applySnapshot(_ next: MobileRideMapSnapshotDto) {
        guard accepts(next) else { return }
        snapshotGeneration &+= 1
        let previousRideID = snapshot?.rideID
        let routeProgressChanged =
            previousRideID != next.rideID
            || snapshot?.summary.pointCount != next.summary.pointCount
        if let previousRideID, previousRideID != next.rideID {
            invalidateProjection(clearPoints: true)
            error = nil
        }
        snapshot = next
        telemetryState = next.telemetryState
        updateDurationTicker()
        if !next.rideID.isEmpty, routeProgressChanged {
            didRestore = true
            requestProjection()
        } else if !didRestore {
            _ = restore()
        }
    }

    func applyDecision(snapshot next: MobileRideMapSnapshotDto, decision: MobileRideMapDecisionDto) {
        guard accepts(next) else { return }
        snapshotGeneration &+= 1
        let routeProgressChanged =
            snapshot?.rideID != next.rideID
            || snapshot?.summary.pointCount != next.summary.pointCount
        error = nil
        if routeProgressChanged {
            applySnapshot(next)
        } else {
            snapshot = next
        }
        lastDecision = decision
        switch decision {
        case let .pending(point):
            telemetryState = point.telemetryState
        case let .accepted(point):
            telemetryState = point.telemetryState
            if !routeProgressChanged { requestProjection() }
        case .rejected, .ignored, .storageError:
            break
        }
    }

    @discardableResult
    func applyError(_ event: MobileRideMapErrorEvent) -> Task<Void, Never>? {
        guard let state else {
            if Self.shouldApplyError(context: event.context, currentSnapshot: snapshot) { error = event.error }
            return nil
        }
        if let errorReadTask {
            pendingError = event
            return errorReadTask
        }
        errorReadTask = Task { [weak self] in
            var nextEvent: MobileRideMapErrorEvent? = event
            while let currentEvent = nextEvent, let generation = self?.snapshotGeneration {
                let current = await Task.detached(priority: .utility) { state.currentSnapshot() }.value
                guard let self else { return }
                guard !Task.isCancelled else {
                    self.errorReadTask = nil
                    return
                }
                if generation != self.snapshotGeneration {
                    // Commands can change authoritative absence while the query waits.
                    // Retry the owned event without dropping the latest pending event.
                    continue
                }
                if Self.shouldApplyError(context: currentEvent.context, currentSnapshot: current) {
                    self.error = currentEvent.error
                }
                nextEvent = self.pendingError
                self.pendingError = nil
            }
            self?.errorReadTask = nil
        }
        return errorReadTask
    }

    func setError(_ error: MobileRideMapError?) {
        self.error = error
    }

    func setAvailability(_ availability: MobileRideMapAvailability) {
        self.availability = availability
    }

    func setMapVisible(_ visible: Bool) {
        guard isMapVisible != visible else { return }
        isMapVisible = visible
        if snapshot?.state == .paused {
            if visible { refreshDuration() } else { pendingDurationRead = nil }
            updateDurationTicker()
        }
        updateProjectionAvailability()
    }

    func setSceneActive(_ active: Bool) {
        guard isSceneActive != active else { return }
        isSceneActive = active
        if active { refreshDuration() } else { pendingDurationRead = nil }
        updateDurationTicker()
        updateProjectionAvailability()
    }

    func applyCommandSnapshot(_ snapshot: MobileRideMapSnapshotDto, resetPoints: Bool) {
        snapshotGeneration &+= 1
        self.snapshot = snapshot
        error = nil
        updateDurationTicker()
        if resetPoints { invalidateProjection(clearPoints: true) }
        telemetryState = snapshot.telemetryState
    }

    func perform(_ command: LiveRideCommand) async -> MobileRideMapSnapshotDto? {
        do {
            let next = try await executeCommand(command)
            applyCommandSnapshot(next, resetPoints: command.resetsPoints)
            return next
        } catch {
            setError(appRideMapError(error))
            return nil
        }
    }

    @discardableResult
    func refreshDuration() -> Task<Void, Never>? {
        guard allowsSnapshotRefresh, let state else { return nil }
        pendingDurationRead = (now(), snapshotGeneration)
        if let durationReadTask { return durationReadTask }
        durationReadTask = Task { [weak self] in
            defer { self?.durationReadTask = nil }
            while let pending = self?.takePendingDurationRead() {
                let next = await Task.detached(priority: .utility) {
                    state.currentSnapshot(atMs: pending.atMs)
                }.value
                guard !Task.isCancelled, let self else { return }
                guard self.allowsSnapshotRefresh, pending.generation == self.snapshotGeneration,
                    let next, next.state == .active || (next.state == .paused && self.isMapVisible)
                else { continue }
                if self.snapshot?.rideID != next.rideID || self.snapshot?.summary.pointCount != next.summary.pointCount
                {
                    self.applySnapshot(next)
                } else {
                    self.snapshot = next
                }
            }
        }
        return durationReadTask
    }

    private var allowsSnapshotRefresh: Bool {
        isSceneActive && (snapshot?.state != .paused || isMapVisible)
    }

    private func takePendingDurationRead() -> (atMs: UInt64, generation: UInt64)? {
        guard allowsSnapshotRefresh else { return nil }
        defer { pendingDurationRead = nil }
        return pendingDurationRead
    }

    func invalidateProjection(clearPoints: Bool) {
        projectionGeneration &+= 1
        projectionEnabled = isMapVisible && isSceneActive
        liveCancellation?.cancel()
        durableCancellation?.cancel()
        if clearPoints {
            projectionRequested = false
            clearProjection()
            lastDecision = nil
        }
    }

    private func updateDurationTicker() {
        guard allowsSnapshotRefresh, snapshot?.state == .active || snapshot?.state == .paused else {
            durationTask?.cancel()
            durationTask = nil
            return
        }
        guard durationTask == nil else { return }
        durationTask = Task { [weak self] in
            while !Task.isCancelled {
                do { try await Task.sleep(for: .seconds(1)) } catch { return }
                self?.refreshDuration()
            }
        }
    }

    private func accepts(_ next: MobileRideMapSnapshotDto) -> Bool {
        guard let current = snapshot else { return true }
        guard next.revision >= current.revision else { return false }
        return next.revision != current.revision || next.rideID == current.rideID
    }

    private func requestProjection() {
        projectionRequested = true
        guard projectionEnabled else { return }
        projectionGeneration &+= 1
        liveCancellation?.cancel()
        durableCancellation?.cancel()
        startProjectionIfNeeded()
    }

    private func updateProjectionAvailability() {
        let enabled = isMapVisible && isSceneActive
        guard projectionEnabled != enabled else { return }
        projectionEnabled = enabled
        guard enabled else {
            projectionGeneration &+= 1
            liveCancellation?.cancel()
            durableCancellation?.cancel()
            return
        }
        if projectionRequested {
            projectionGeneration &+= 1
            startProjectionIfNeeded()
        }
    }

    private func startProjectionIfNeeded() {
        guard projectionEnabled, projectionRequested else { return }
        guard let rideID = snapshot?.rideID, !rideID.isEmpty else {
            projectionRequested = false
            return
        }
        guard let state else {
            projectionRequested = false
            return
        }
        guard projectionTask == nil else { return }
        let budget = MobileRideMapLimits.rustOwned.liveTailPointLimit
        projectionTask = Task { [weak self] in
            defer {
                self?.projectionTask = nil
                self?.liveCancellation = nil
                self?.durableCancellation = nil
                if self?.projectionEnabled == true, self?.projectionRequested == true {
                    self?.startProjectionIfNeeded()
                }
            }
            while let request = self?.nextProjectionRequest() {
                do {
                    let projection = try await Self.runCancellableDetached(priority: .userInitiated) {
                        try state.projectCurrentRoutePoints(
                            budget: budget,
                            rideID: request.rideID,
                            viewport: nil,
                            privacy: .precise,
                            durableCancellation: request.durableCancellation,
                            liveCancellation: request.liveCancellation
                        )
                    }
                    guard let self else { break }
                    guard self.projectionEnabled else { break }
                    guard
                        Self.shouldApplyProjection(
                            generation: request.generation,
                            currentGeneration: self.projectionGeneration,
                            enabled: self.projectionEnabled,
                            rideID: request.rideID,
                            currentRideID: self.snapshot?.rideID
                        )
                    else { continue }
                    self.applyProjection(projection)
                    self.projectionRequested = false
                } catch {
                    guard let self else { break }
                    guard self.projectionEnabled else { break }
                    guard
                        Self.shouldApplyProjection(
                            generation: request.generation,
                            currentGeneration: self.projectionGeneration,
                            enabled: self.projectionEnabled,
                            rideID: request.rideID,
                            currentRideID: self.snapshot?.rideID
                        )
                    else { continue }
                    self.error = appRideMapError(error)
                    self.clearProjection()
                    self.projectionRequested = false
                }
                return
            }
        }
    }

    private func nextProjectionRequest() -> ProjectionRequest? {
        guard projectionEnabled, projectionRequested,
            let rideID = snapshot?.rideID, !rideID.isEmpty
        else { return nil }
        let request = ProjectionRequest(
            rideID: rideID,
            generation: projectionGeneration,
            liveCancellation: MobileLiveRideMapProjectionCancellation(),
            durableCancellation: MobileRideMapProjectionCancellation()
        )
        liveCancellation = request.liveCancellation
        durableCancellation = request.durableCancellation
        return request
    }

    private func applyProjection(_ projection: MobileRideMapRouteProjection) {
        projectionVersion &+= 1
        displayPoints = projection.points
        cameraRegion = projection.canonicalCameraRegion ?? projection.cameraRegion
        endpointMetadata = projection.endpointMetadata
        segments = projection.segments
        backgroundGapCount = projection.backgroundGapCount
        pointsTruncated = projection.pointsOmittedByBudget
        segmentsOmittedByBudget = projection.segmentsOmittedByBudget
    }

    private func clearProjection() {
        projectionVersion &+= 1
        displayPoints.removeAll(keepingCapacity: true)
        cameraRegion = nil
        endpointMetadata = .empty
        segments.removeAll(keepingCapacity: true)
        telemetryState = nil
        backgroundGapCount = 0
        pointsTruncated = false
        segmentsOmittedByBudget = false
    }

    static func shouldApplyProjection(
        generation: UInt64,
        currentGeneration: UInt64,
        enabled: Bool,
        rideID: String,
        currentRideID: String?
    ) -> Bool {
        enabled && generation == currentGeneration && currentRideID == rideID
    }

    static func shouldApplyError(
        context: MobileRideMapErrorContext,
        currentSnapshot: MobileRideMapSnapshotDto?
    ) -> Bool {
        guard let currentSnapshot else { return context.rideID == nil }
        guard context.rideID == currentSnapshot.rideID else { return false }
        guard let generation = context.generation else { return true }
        return currentSnapshot.recordingToken?.generation == generation
    }

    private nonisolated static func runCancellableDetached<Success: Sendable>(
        priority: TaskPriority,
        operation: @escaping @Sendable () throws -> Success
    ) async throws -> Success {
        let task = Task.detached(priority: priority) {
            try Task.checkCancellation()
            let result = try operation()
            try Task.checkCancellation()
            return result
        }
        return try await withTaskCancellationHandler(
            operation: { try await task.value },
            onCancel: { task.cancel() }
        )
    }
}
