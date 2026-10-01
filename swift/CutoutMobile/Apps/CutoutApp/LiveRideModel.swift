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
    case pause(atMs: UInt64)
    case resume(atMs: UInt64)
    case stop(atMs: UInt64)
    case save
    case discard

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

    private let state: (any LiveRideQuerying)?
    private let now: @MainActor () -> UInt64
    private let executeCommand: @MainActor (LiveRideCommand) async throws -> MobileRideMapSnapshotDto
    private var restoreTask: Task<Void, Never>?
    private var restoreCancellation: MobileRideMapProjectionCancellation?
    private var projectionTask: Task<Void, Never>?
    private var durationTask: Task<Void, Never>?
    private var liveCancellation: MobileLiveRideMapProjectionCancellation?
    private var durableCancellation: MobileRideMapProjectionCancellation?
    private var projectionGeneration: UInt64 = 0
    private var projectionEnabled = false

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
        restoreTask?.cancel()
        restoreCancellation?.cancel()
        projectionTask?.cancel()
        durationTask?.cancel()
        liveCancellation?.cancel()
        durableCancellation?.cancel()
    }

    @discardableResult
    func restore() -> Task<Void, Never>? {
        guard let state else { return nil }
        snapshot = state.currentSnapshot()
        telemetryState = snapshot?.telemetryState
        updateDurationTicker()
        restoreTask?.cancel()
        restoreCancellation?.cancel()
        guard let rideID = snapshot?.rideID else { return nil }
        let generation = projectionGeneration
        let budget = MobileRideMapLimits.rustOwned.liveTailPointLimit
        let cancellation = MobileRideMapProjectionCancellation()
        restoreCancellation = cancellation
        restoreTask = Task { [weak self] in
            do {
                let projection = try await Self.runCancellableDetached(priority: .userInitiated) {
                    try state.projectStoredPoints(
                        rideID: rideID,
                        budget: budget,
                        viewport: nil,
                        privacy: .precise,
                        cancellation: cancellation
                    )
                }
                guard !Task.isCancelled, let self,
                    Self.shouldApplyRestoredProjection(
                        restorationGeneration: generation,
                        currentGeneration: self.projectionGeneration,
                        liveProjectionEnabled: self.projectionEnabled
                    ), self.snapshot?.rideID == rideID
                else { return }
                self.applyProjection(projection)
            } catch {
                guard !Task.isCancelled, let self,
                    Self.shouldApplyRestoredProjection(
                        restorationGeneration: generation,
                        currentGeneration: self.projectionGeneration,
                        liveProjectionEnabled: self.projectionEnabled
                    ), self.snapshot?.rideID == rideID
                else { return }
                self.error = appRideMapError(error)
                self.clearProjection()
            }
        }
        return restoreTask
    }

    func applySnapshot(_ next: MobileRideMapSnapshotDto) {
        guard accepts(next) else { return }
        if let previousRideID = snapshot?.rideID, previousRideID != next.rideID {
            invalidateProjection(clearPoints: true)
            error = nil
        }
        snapshot = next
        telemetryState = next.telemetryState
        updateDurationTicker()
        if restoreTask == nil { restore() }
    }

    func applyDecision(snapshot next: MobileRideMapSnapshotDto, decision: MobileRideMapDecisionDto) {
        guard accepts(next) else { return }
        error = nil
        snapshot = next
        lastDecision = decision
        switch decision {
        case let .pending(point):
            telemetryState = point.telemetryState
        case let .accepted(point):
            telemetryState = point.telemetryState
            requestProjection()
        case .rejected, .ignored, .storageError:
            break
        }
    }

    func applyError(_ event: MobileRideMapErrorEvent) {
        guard Self.shouldApplyError(context: event.context, currentSnapshot: snapshot) else { return }
        error = event.error
    }

    func setError(_ error: MobileRideMapError?) {
        self.error = error
    }

    func setAvailability(_ availability: MobileRideMapAvailability) {
        self.availability = availability
    }

    func applyCommandSnapshot(_ snapshot: MobileRideMapSnapshotDto, resetPoints: Bool) {
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

    func refreshDuration() {
        guard let next = state?.currentSnapshot(atMs: now()), next.state == .active else { return }
        snapshot = next
    }

    func invalidateProjection(clearPoints: Bool) {
        projectionGeneration &+= 1
        projectionEnabled = false
        restoreTask?.cancel()
        restoreCancellation?.cancel()
        liveCancellation?.cancel()
        durableCancellation?.cancel()
        if clearPoints {
            clearProjection()
            lastDecision = nil
        }
    }

    private func updateDurationTicker() {
        durationTask?.cancel()
        guard snapshot?.state == .active else {
            durationTask = nil
            return
        }
        durationTask = Task { [weak self] in
            while !Task.isCancelled {
                self?.refreshDuration()
                do { try await Task.sleep(for: .seconds(1)) } catch { return }
            }
        }
    }

    private func accepts(_ next: MobileRideMapSnapshotDto) -> Bool {
        guard let current = snapshot else { return true }
        guard next.revision >= current.revision else { return false }
        return next.revision != current.revision || next.rideID == current.rideID
    }

    private func requestProjection() {
        projectionGeneration &+= 1
        projectionEnabled = true
        liveCancellation?.cancel()
        durableCancellation?.cancel()
        guard projectionTask == nil, let state else { return }
        let budget = MobileRideMapLimits.rustOwned.liveTailPointLimit
        projectionTask = Task { [weak self] in
            defer {
                self?.projectionTask = nil
                self?.liveCancellation = nil
                self?.durableCancellation = nil
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
                }
                return
            }
        }
    }

    private func nextProjectionRequest() -> ProjectionRequest? {
        guard projectionEnabled, let rideID = snapshot?.rideID, !rideID.isEmpty else { return nil }
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

    static func shouldApplyRestoredProjection(
        restorationGeneration: UInt64,
        currentGeneration: UInt64,
        liveProjectionEnabled: Bool
    ) -> Bool {
        !liveProjectionEnabled && restorationGeneration == currentGeneration
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
