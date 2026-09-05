import CutoutMobileFFI
import Foundation

public enum LiveActivityRideLifecycleEndReason: String, Codable, Equatable, Hashable, Sendable {
    case disconnected
    case sessionEnded
    case unavailable
    case permissionDenied
    case platformUnsupported
}

public enum LiveActivityRideLifecycleError: Error, Equatable, Sendable {
    case authorizationDenied
    case requestFailed
    case activityUnavailable
}

public enum LiveActivityRideRecoveryResult: Equatable, Sendable {
    case noPersistedRide
    case adopted
    case ended(requiresUserAction: Bool)
}

public enum LiveActivityRideStartOutcome: Equatable, Sendable {
    case started(activityID: String)
    case adopted(activityID: String)

    var activityID: String {
        switch self {
        case let .started(activityID), let .adopted(activityID):
            activityID
        }
    }
}

public struct LiveActivityRideUpdateOutcome: Equatable, Sendable {
    public let activityID: String

    public init(activityID: String) {
        self.activityID = activityID
    }
}

public struct LiveActivityRideEndOutcome: Equatable, Sendable {
    public let activityIDs: [String]

    public init(activityIDs: [String]) {
        self.activityIDs = activityIDs
    }
}

public struct LiveActivityRideSessionIdentity: Codable, Equatable, Hashable, Sendable {
    public let platformIdentifier: String
    public let sessionID: String

    public init(platformIdentifier: String, sessionID: String) {
        self.platformIdentifier = platformIdentifier
        self.sessionID = sessionID
    }

    init(_ identity: MobileRideSessionIdentityDto) {
        self.init(
            platformIdentifier: identity.platformIdentifier,
            sessionID: identity.sessionId
        )
    }
}

public protocol LiveActivityRideLifecycleManaging: Sendable {
    func start(
        snapshot: LiveActivityRideSnapshot,
        rideSessionIdentity: LiveActivityRideSessionIdentity,
        staleAfterMilliseconds: UInt64
    ) async throws -> LiveActivityRideStartOutcome
    func update(
        snapshot: LiveActivityRideSnapshot,
        staleAfterMilliseconds: UInt64
    ) async throws -> LiveActivityRideUpdateOutcome
    func end(reason: LiveActivityRideLifecycleEndReason) async throws -> LiveActivityRideEndOutcome
}

struct LiveActivityRideReconciliation: Equatable, Sendable {
    let adoptedIndex: Int?
    let staleIndices: [Int]
}

func liveActivityRideReconciliation<Identity: Equatable & Sendable>(
    existingIdentities: [Identity],
    desiredIdentity: Identity
) -> LiveActivityRideReconciliation {
    let adoptedIndex = existingIdentities.firstIndex(of: desiredIdentity)
    return LiveActivityRideReconciliation(
        adoptedIndex: adoptedIndex,
        staleIndices: existingIdentities.indices.filter { $0 != adoptedIndex }
    )
}

public actor LiveActivityRideLifecycleCoordinator {
    private let manager: any LiveActivityRideLifecycleManaging
    private let sessionState: CutoutSessionStateHandle
    private let markerStore: RideSessionMarkerStore
    private var hasReconciledInactiveState = false
    private var lastSnapshot: LiveActivityRideSnapshot?
    public private(set) var lastError: LiveActivityRideLifecycleError?
    private var latestRequestID: UInt64 = 0
    private var isOperationInFlight = false
    private var operationWaiters: [CheckedContinuation<Void, Never>] = []
    private var operationQueueObservers: [(depth: Int, continuation: CheckedContinuation<Void, Never>)] = []

    public init(
        manager: some LiveActivityRideLifecycleManaging,
        sessionState: CutoutSessionStateHandle = CutoutSessionStateHandle(),
        markerStore: RideSessionMarkerStore = RideSessionMarkerStore()
    ) {
        self.manager = manager
        self.sessionState = sessionState
        self.markerStore = markerStore
    }

    /// Reconciles an opaque persisted Rust marker with `CoreBluetooth` restoration state.
    ///
    /// Reports whether Rust adopted the persisted ride or ended it before user action is required.
    public func recoverPersistedRide(
        requestID: UInt64,
        restoredPlatformIdentifier: String?,
        snapshot: LiveActivityRideSnapshot?
    ) async -> LiveActivityRideRecoveryResult {
        guard accept(requestID: requestID) else { return .noPersistedRide }
        await beginOperation()
        defer { finishOperation() }
        guard requestID == latestRequestID else { return .noPersistedRide }
        guard let marker = markerStore.marker else { return .noPersistedRide }

        do {
            let decision = try sessionState.recoverRideSessionMarker(
                marker: marker,
                restoredPlatformIdentifier: restoredPlatformIdentifier
            )
            let result: LiveActivityRideRecoveryResult = switch decision.effect {
            case .startActivity:
                .adopted
            case .endActivity:
                .ended(requiresUserAction: restoredPlatformIdentifier != nil)
            default:
                .ended(requiresUserAction: false)
            }
            persistSessionMarker()
            await execute(
                effect: decision.effect,
                snapshot: snapshot,
                endReason: .sessionEnded,
                staleAfterMilliseconds: decision.snapshot.staleAfterMs
            )
            return result
        } catch {
            try? markerStore.clear()
            lastError = Self.lifecycleError(from: error)
            return .ended(requiresUserAction: false)
        }
    }

    public func reconcile(
        requestID: UInt64,
        platformIdentifier: String? = nil,
        monotonicTimeMs: UInt64 = 0,
        snapshot: LiveActivityRideSnapshot?,
        shouldBeActive: Bool,
        endReason: LiveActivityRideLifecycleEndReason = .sessionEnded
    ) async {
        guard accept(requestID: requestID) else { return }
        await beginOperation()
        defer { finishOperation() }
        guard requestID == latestRequestID else { return }

        guard shouldBeActive else {
            _ = await endIfNeeded(reason: endReason)
            return
        }

        guard let snapshot else {
            _ = await endIfNeeded(reason: endReason)
            return
        }

        let platformIdentifier = platformIdentifier ?? snapshot.identity.label
        let rustIdentity = sessionState.rideSessionSnapshot().identity
        if lastSnapshot == nil || rustIdentity?.platformIdentifier != platformIdentifier {
            await apply(
                input: .start(platformIdentifier: platformIdentifier),
                snapshot: snapshot,
                endReason: endReason
            )
            return
        }

        if sessionState.rideSessionSnapshot().phase == .reconnecting {
            await apply(input: .bluetoothConnected, snapshot: snapshot, endReason: endReason)
        }
        guard lastSnapshot != snapshot else {
            return
        }
        await apply(
            input: .telemetryObserved(atMs: monotonicTimeMs),
            snapshot: snapshot,
            endReason: endReason
        )
    }

    public func transportDisconnected(
        requestID: UInt64,
        atMs: UInt64,
        snapshot: LiveActivityRideSnapshot
    ) async {
        guard accept(requestID: requestID) else { return }
        await beginOperation()
        defer { finishOperation() }
        guard requestID == latestRequestID else { return }
        await apply(
            input: .bluetoothDisconnected(atMs: atMs),
            snapshot: snapshot,
            endReason: .sessionEnded
        )
    }

    public func reconnectExhausted(
        requestID: UInt64,
        snapshot: LiveActivityRideSnapshot
    ) async {
        await terminate(
            requestID: requestID,
            input: .reconnectExhausted,
            snapshot: snapshot
        )
    }

    public func unrecoverableSessionFailure(
        requestID: UInt64,
        snapshot: LiveActivityRideSnapshot
    ) async {
        await terminate(
            requestID: requestID,
            input: .unrecoverableSessionFailure,
            snapshot: snapshot
        )
    }

    private func terminate(
        requestID: UInt64,
        input: MobileRideSessionInputDto,
        snapshot: LiveActivityRideSnapshot
    ) async {
        guard accept(requestID: requestID) else { return }
        await beginOperation()
        defer { finishOperation() }
        guard requestID == latestRequestID else { return }
        await apply(
            input: input,
            snapshot: snapshot,
            endReason: .unavailable
        )
    }

    public func appDidEnterBackground(
        requestID: UInt64,
        snapshot: LiveActivityRideSnapshot,
        captureFlush: @escaping @Sendable () async -> Bool
    ) async {
        guard accept(requestID: requestID) else { return }
        await beginOperation()
        defer { finishOperation() }
        guard requestID == latestRequestID else { return }
        await apply(
            input: .appBackgrounded,
            snapshot: snapshot,
            endReason: .sessionEnded,
            captureFlush: captureFlush
        )
    }

    public func appDidBecomeActive(
        requestID: UInt64,
        snapshot: LiveActivityRideSnapshot
    ) async {
        guard accept(requestID: requestID) else { return }
        await beginOperation()
        defer { finishOperation() }
        guard requestID == latestRequestID else { return }
        await apply(
            input: .appForegrounded,
            snapshot: snapshot,
            endReason: .sessionEnded
        )
    }

    public func end(requestID: UInt64, reason: LiveActivityRideLifecycleEndReason) async {
        guard accept(requestID: requestID) else { return }
        await beginOperation()
        defer { finishOperation() }
        guard requestID == latestRequestID else { return }
        _ = await endIfNeeded(reason: reason)
    }

    private func accept(requestID: UInt64) -> Bool {
        guard requestID > latestRequestID else { return false }
        latestRequestID = requestID
        return true
    }

    private func beginOperation() async {
        guard isOperationInFlight else {
            isOperationInFlight = true
            return
        }

        await withCheckedContinuation { continuation in
            operationWaiters.append(continuation)
            resumeReadyOperationQueueObservers()
        }
    }

    internal func waitForOperationQueueDepthForTesting(_ depth: Int) async {
        guard operationWaiters.count < depth else { return }
        await withCheckedContinuation { continuation in
            operationQueueObservers.append((depth: depth, continuation: continuation))
            resumeReadyOperationQueueObservers()
        }
    }

    private func resumeReadyOperationQueueObservers() {
        let ready = operationQueueObservers.filter { operationWaiters.count >= $0.depth }
        operationQueueObservers.removeAll { operationWaiters.count >= $0.depth }
        for observer in ready {
            observer.continuation.resume()
        }
    }

    private func finishOperation() {
        guard operationWaiters.isEmpty == false else {
            isOperationInFlight = false
            return
        }

        operationWaiters.removeFirst().resume()
    }

    private func apply(
        input: MobileRideSessionInputDto,
        snapshot: LiveActivityRideSnapshot?,
        endReason: LiveActivityRideLifecycleEndReason,
        captureFlush: (@Sendable () async -> Bool)? = nil
    ) async {
        do {
            let decision = try reduce(input)
            await execute(
                effect: decision.effect,
                snapshot: snapshot,
                endReason: endReason,
                staleAfterMilliseconds: decision.snapshot.staleAfterMs,
                captureFlush: captureFlush
            )
        } catch {
            lastError = Self.lifecycleError(from: error)
        }
    }

    private func execute(
        effect: MobileRideSessionEffectDto,
        snapshot: LiveActivityRideSnapshot?,
        endReason: LiveActivityRideLifecycleEndReason,
        staleAfterMilliseconds: UInt64,
        captureFlush: (@Sendable () async -> Bool)? = nil
    ) async {
        switch effect {
        case .none:
            return
        case let .startActivity(identity):
            guard let snapshot else {
                lastError = .requestFailed
                return
            }
            do {
                let outcome = try await manager.start(
                    snapshot: snapshot,
                    rideSessionIdentity: LiveActivityRideSessionIdentity(identity),
                    staleAfterMilliseconds: staleAfterMilliseconds
                )
                hasReconciledInactiveState = false
                lastSnapshot = snapshot
                lastError = nil
                await apply(
                    input: .activityStarted(identity: identity, activityId: outcome.activityID),
                    snapshot: snapshot,
                    endReason: endReason
                )
            } catch {
                lastSnapshot = nil
                lastError = Self.lifecycleError(from: error)
                _ = try? reduce(.activityUnavailable(identity: identity))
            }
        case .updateActivity, .markActivityStale:
            guard let snapshot else {
                lastError = .requestFailed
                return
            }
            do {
                let updateStaleAfterMilliseconds: UInt64 = switch effect {
                case .markActivityStale:
                    0
                default:
                    staleAfterMilliseconds
                }
                _ = try await manager.update(
                    snapshot: snapshot,
                    staleAfterMilliseconds: updateStaleAfterMilliseconds
                )
                lastSnapshot = snapshot
                lastError = nil
            } catch {
                lastError = Self.lifecycleError(from: error)
            }
        case let .endActivity(identity, reason):
            do {
                _ = try await manager.end(reason: Self.endReason(from: reason, fallback: endReason))
                lastSnapshot = nil
                lastError = nil
                hasReconciledInactiveState = true
                await apply(
                    input: .activityEnded(identity: identity),
                    snapshot: snapshot,
                    endReason: endReason
                )
            } catch {
                lastError = Self.lifecycleError(from: error)
                _ = try? reduce(.activityUnavailable(identity: identity))
            }
        case .requestCaptureFlush:
            _ = await captureFlush?()
        }
    }

    private static func lifecycleError(from error: Error) -> LiveActivityRideLifecycleError {
        (error as? LiveActivityRideLifecycleError) ?? .requestFailed
    }

    private static func endReason(
        from reason: MobileRideSessionEndReasonDto,
        fallback: LiveActivityRideLifecycleEndReason
    ) -> LiveActivityRideLifecycleEndReason {
        switch reason {
        case .userDisconnect:
            .disconnected
        case .replacedByNewSession:
            .sessionEnded
        case .userStop, .reconnectExhausted, .appReset, .unrecoverableSessionFailure:
            fallback
        }
    }

    private func endIfNeeded(reason: LiveActivityRideLifecycleEndReason) async -> Bool {
        if let snapshot = lastSnapshot {
            let input: MobileRideSessionInputDto = reason == .disconnected ? .userDisconnected : .userStopped
            do {
                let decision = try reduce(input)
                if decision.effect != .none {
                    await execute(
                        effect: decision.effect,
                        snapshot: snapshot,
                        endReason: reason,
                        staleAfterMilliseconds: decision.snapshot.staleAfterMs
                    )
                    return lastError == nil
                }
            } catch {
                lastError = Self.lifecycleError(from: error)
                return false
            }
        }

        guard hasReconciledInactiveState == false else {
            lastSnapshot = nil
            return true
        }

        do {
            _ = try await manager.end(reason: reason)
            lastError = nil
            hasReconciledInactiveState = true
            lastSnapshot = nil
            return true
        } catch {
            lastError = Self.lifecycleError(from: error)
            return false
        }
    }

    private func reduce(_ input: MobileRideSessionInputDto) throws -> MobileRideSessionDecisionDto {
        let decision = try sessionState.reduceRideSession(input: input)
        persistSessionMarker()
        return decision
    }

    private func persistSessionMarker() {
        guard let marker = try? sessionState.exportRideSessionMarker() else {
            try? markerStore.clear()
            return
        }
        markerStore.save(marker)
    }
}

#if canImport(ActivityKit) && !os(macOS)
@preconcurrency import ActivityKit

@available(iOS 16.2, *)
public struct LiveActivityRideAttributes: ActivityAttributes, Codable, Hashable, Sendable {
    public struct ContentState: Codable, Hashable, Sendable {
        public let snapshot: LiveActivityRideSnapshot
        public let staleAt: Date?

        public init(snapshot: LiveActivityRideSnapshot, staleAt: Date? = nil) {
            self.snapshot = snapshot
            self.staleAt = staleAt
        }

        public func presentationSnapshot(isStale: Bool, now: Date) -> LiveActivityRideSnapshot {
            snapshot.presented(isStale: isStale || staleAt.map { now >= $0 } == true)
        }
    }

    public let identity: LiveActivityRideIdentity
    public let rideSessionIdentity: LiveActivityRideSessionIdentity

    public init(
        identity: LiveActivityRideIdentity,
        rideSessionIdentity: LiveActivityRideSessionIdentity
    ) {
        self.identity = identity
        self.rideSessionIdentity = rideSessionIdentity
    }
}

@available(iOS 16.2, *)
public actor LiveActivityRideActivityKitManager: LiveActivityRideLifecycleManaging {
    private let state = LiveActivityRideActivityKitState()

    public init() {}

    public func start(
        snapshot: LiveActivityRideSnapshot,
        rideSessionIdentity: LiveActivityRideSessionIdentity,
        staleAfterMilliseconds: UInt64
    ) async throws -> LiveActivityRideStartOutcome {
        try await state.start(
            snapshot: snapshot,
            rideSessionIdentity: rideSessionIdentity,
            staleAfterMilliseconds: staleAfterMilliseconds
        )
    }

    public func update(
        snapshot: LiveActivityRideSnapshot,
        staleAfterMilliseconds: UInt64
    ) async throws -> LiveActivityRideUpdateOutcome {
        try await state.update(
            snapshot: snapshot,
            staleAfterMilliseconds: staleAfterMilliseconds
        )
    }

    public func end(reason: LiveActivityRideLifecycleEndReason) async throws -> LiveActivityRideEndOutcome {
        try await state.end(reason: reason)
    }
}

@available(iOS 16.2, *)
private actor LiveActivityRideActivityKitState {
    private var activity: Activity<LiveActivityRideAttributes>?
    private var lastSnapshot: LiveActivityRideSnapshot?

    func start(
        snapshot: LiveActivityRideSnapshot,
        rideSessionIdentity: LiveActivityRideSessionIdentity,
        staleAfterMilliseconds: UInt64
    ) async throws -> LiveActivityRideStartOutcome {
        guard ActivityAuthorizationInfo().areActivitiesEnabled else {
            throw LiveActivityRideLifecycleError.authorizationDenied
        }

        let existingActivities = Activity<LiveActivityRideAttributes>.activities
        let reconciliation = liveActivityRideReconciliation(
            existingIdentities: existingActivities.map(\.attributes.rideSessionIdentity),
            desiredIdentity: rideSessionIdentity
        )
        for staleIndex in reconciliation.staleIndices {
            let staleActivity = existingActivities[staleIndex]
            await staleActivity.end(staleActivity.content, dismissalPolicy: .immediate)
        }

        if let adoptedIndex = reconciliation.adoptedIndex {
            activity = existingActivities[adoptedIndex]
            _ = try await update(
                snapshot: snapshot,
                staleAfterMilliseconds: staleAfterMilliseconds
            )
            return .adopted(activityID: existingActivities[adoptedIndex].id)
        }

        activity = nil
        do {
            let startedActivity = try Activity.request(
                attributes: LiveActivityRideAttributes(
                    identity: snapshot.identity,
                    rideSessionIdentity: rideSessionIdentity
                ),
                content: content(
                    snapshot: snapshot,
                    staleAfterMilliseconds: staleAfterMilliseconds
                ),
                pushType: nil
            )
            activity = startedActivity
            lastSnapshot = snapshot
            return .started(activityID: startedActivity.id)
        } catch {
            activity = nil
            lastSnapshot = nil
            throw LiveActivityRideLifecycleError.requestFailed
        }
    }

    func update(
        snapshot: LiveActivityRideSnapshot,
        staleAfterMilliseconds: UInt64
    ) async throws -> LiveActivityRideUpdateOutcome {
        guard let activity else {
            throw LiveActivityRideLifecycleError.activityUnavailable
        }

        await activity.update(
            content(
                snapshot: snapshot,
                staleAfterMilliseconds: staleAfterMilliseconds
            )
        )
        lastSnapshot = snapshot
        return LiveActivityRideUpdateOutcome(activityID: activity.id)
    }

    func end(reason _: LiveActivityRideLifecycleEndReason) async throws -> LiveActivityRideEndOutcome {
        let currentActivityID = activity?.id
        let activities = Activity<LiveActivityRideAttributes>.activities
        for currentActivity in activities {
            let finalContent = currentActivity.id == currentActivityID
                ? lastSnapshot.map {
                    content(snapshot: $0, staleAfterMilliseconds: 0)
                } ?? currentActivity.content
                : currentActivity.content
            await currentActivity.end(finalContent, dismissalPolicy: .immediate)
        }

        activity = nil
        lastSnapshot = nil
        return LiveActivityRideEndOutcome(activityIDs: activities.map(\.id))
    }

    private func content(
        snapshot: LiveActivityRideSnapshot,
        staleAfterMilliseconds: UInt64
    ) -> ActivityContent<LiveActivityRideAttributes.ContentState> {
        let staleAt = Date().addingTimeInterval(TimeInterval(staleAfterMilliseconds) / 1_000)
        return ActivityContent(
            state: LiveActivityRideAttributes.ContentState(
                snapshot: snapshot,
                staleAt: staleAt
            ),
            staleDate: staleAt
        )
    }
}
#else
public actor LiveActivityRideActivityKitManager: LiveActivityRideLifecycleManaging {
    public init() {}

    public func start(
        snapshot _: LiveActivityRideSnapshot,
        rideSessionIdentity _: LiveActivityRideSessionIdentity,
        staleAfterMilliseconds _: UInt64
    ) async throws -> LiveActivityRideStartOutcome {
        throw LiveActivityRideLifecycleError.activityUnavailable
    }

    public func update(
        snapshot _: LiveActivityRideSnapshot,
        staleAfterMilliseconds _: UInt64
    ) async throws -> LiveActivityRideUpdateOutcome {
        throw LiveActivityRideLifecycleError.activityUnavailable
    }

    public func end(reason _: LiveActivityRideLifecycleEndReason) async throws -> LiveActivityRideEndOutcome {
        LiveActivityRideEndOutcome(activityIDs: [])
    }
}
#endif
