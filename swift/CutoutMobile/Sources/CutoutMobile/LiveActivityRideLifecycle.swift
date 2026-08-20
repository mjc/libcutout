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

public protocol LiveActivityRideLifecycleManaging: Sendable {
    func start(snapshot: LiveActivityRideSnapshot) async throws -> LiveActivityRideStartOutcome
    func update(snapshot: LiveActivityRideSnapshot) async throws -> LiveActivityRideUpdateOutcome
    func end(reason: LiveActivityRideLifecycleEndReason) async throws -> LiveActivityRideEndOutcome
}

struct LiveActivityRideReconciliation: Equatable, Sendable {
    let adoptedIndex: Int?
    let staleIndices: [Int]
}

func liveActivityRideReconciliation(
    existingIdentities: [LiveActivityRideIdentity],
    desiredIdentity: LiveActivityRideIdentity
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
    private var hasReconciledInactiveState = false
    private var lastSnapshot: LiveActivityRideSnapshot?
    public private(set) var lastError: LiveActivityRideLifecycleError?
    private var latestRequestID: UInt64 = 0
    private var isOperationInFlight = false
    private var operationWaiters: [CheckedContinuation<Void, Never>] = []

    public init(
        manager: some LiveActivityRideLifecycleManaging,
        sessionState: CutoutSessionStateHandle = CutoutSessionStateHandle()
    ) {
        self.manager = manager
        self.sessionState = sessionState
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

        guard lastSnapshot != snapshot else {
            return
        }
        await apply(
            input: .telemetryObserved(atMs: monotonicTimeMs),
            snapshot: snapshot,
            endReason: endReason
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
        snapshot: LiveActivityRideSnapshot,
        endReason: LiveActivityRideLifecycleEndReason
    ) async {
        do {
            let decision = try sessionState.reduceRideSession(input: input)
            await execute(effect: decision.effect, snapshot: snapshot, endReason: endReason)
        } catch {
            lastError = Self.lifecycleError(from: error)
        }
    }

    private func execute(
        effect: MobileRideSessionEffectDto,
        snapshot: LiveActivityRideSnapshot,
        endReason: LiveActivityRideLifecycleEndReason
    ) async {
        switch effect {
        case .none:
            return
        case let .startActivity(identity):
            do {
                let outcome = try await manager.start(snapshot: snapshot)
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
                _ = try? sessionState.reduceRideSession(input: .activityUnavailable(identity: identity))
            }
        case .updateActivity, .markActivityStale:
            do {
                _ = try await manager.update(snapshot: snapshot)
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
                _ = try? sessionState.reduceRideSession(input: .activityUnavailable(identity: identity))
            }
        case .requestCaptureFlush:
            return
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
                let decision = try sessionState.reduceRideSession(input: input)
                if decision.effect != .none {
                    await execute(effect: decision.effect, snapshot: snapshot, endReason: reason)
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
}

#if canImport(ActivityKit) && !os(macOS)
@preconcurrency import ActivityKit

@available(iOS 16.2, *)
public struct LiveActivityRideAttributes: ActivityAttributes, Codable, Hashable, Sendable {
    public struct ContentState: Codable, Hashable, Sendable {
        public let snapshot: LiveActivityRideSnapshot

        public init(snapshot: LiveActivityRideSnapshot) {
            self.snapshot = snapshot
        }
    }

    public let identity: LiveActivityRideIdentity

    public init(identity: LiveActivityRideIdentity) {
        self.identity = identity
    }
}

@available(iOS 16.2, *)
public actor LiveActivityRideActivityKitManager: LiveActivityRideLifecycleManaging {
    private let state = LiveActivityRideActivityKitState()

    public init() {}

    public func start(snapshot: LiveActivityRideSnapshot) async throws -> LiveActivityRideStartOutcome {
        try await state.start(snapshot: snapshot)
    }

    public func update(snapshot: LiveActivityRideSnapshot) async throws -> LiveActivityRideUpdateOutcome {
        try await state.update(snapshot: snapshot)
    }

    public func end(reason: LiveActivityRideLifecycleEndReason) async throws -> LiveActivityRideEndOutcome {
        try await state.end(reason: reason)
    }
}

@available(iOS 16.2, *)
private actor LiveActivityRideActivityKitState {
    private var activity: Activity<LiveActivityRideAttributes>?
    private var lastSnapshot: LiveActivityRideSnapshot?

    func start(snapshot: LiveActivityRideSnapshot) async throws -> LiveActivityRideStartOutcome {
        guard ActivityAuthorizationInfo().areActivitiesEnabled else {
            throw LiveActivityRideLifecycleError.authorizationDenied
        }

        let existingActivities = Activity<LiveActivityRideAttributes>.activities
        let reconciliation = liveActivityRideReconciliation(
            existingIdentities: existingActivities.map(\.attributes.identity),
            desiredIdentity: snapshot.identity
        )
        for staleIndex in reconciliation.staleIndices {
            let staleActivity = existingActivities[staleIndex]
            await staleActivity.end(staleActivity.content, dismissalPolicy: .immediate)
        }

        if let adoptedIndex = reconciliation.adoptedIndex {
            activity = existingActivities[adoptedIndex]
            _ = try await update(snapshot: snapshot)
            return .adopted(activityID: existingActivities[adoptedIndex].id)
        }

        activity = nil
        do {
            let startedActivity = try Activity.request(
                attributes: LiveActivityRideAttributes(identity: snapshot.identity),
                content: content(snapshot: snapshot),
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

    func update(snapshot: LiveActivityRideSnapshot) async throws -> LiveActivityRideUpdateOutcome {
        guard let activity else {
            throw LiveActivityRideLifecycleError.activityUnavailable
        }

        await activity.update(content(snapshot: snapshot))
        lastSnapshot = snapshot
        return LiveActivityRideUpdateOutcome(activityID: activity.id)
    }

    func end(reason _: LiveActivityRideLifecycleEndReason) async throws -> LiveActivityRideEndOutcome {
        let currentActivityID = activity?.id
        let activities = Activity<LiveActivityRideAttributes>.activities
        for currentActivity in activities {
            let finalContent = currentActivity.id == currentActivityID
                ? lastSnapshot.map(content(snapshot:)) ?? currentActivity.content
                : currentActivity.content
            await currentActivity.end(finalContent, dismissalPolicy: .immediate)
        }

        activity = nil
        lastSnapshot = nil
        return LiveActivityRideEndOutcome(activityIDs: activities.map(\.id))
    }

    private func content(snapshot: LiveActivityRideSnapshot) -> ActivityContent<LiveActivityRideAttributes.ContentState> {
        ActivityContent(
            state: .init(snapshot: snapshot),
            staleDate: LiveActivityRideFreshnessPolicy.staleDate(after: Date())
        )
    }
}
#else
public actor LiveActivityRideActivityKitManager: LiveActivityRideLifecycleManaging {
    public init() {}

    public func start(snapshot _: LiveActivityRideSnapshot) async throws -> LiveActivityRideStartOutcome {
        throw LiveActivityRideLifecycleError.activityUnavailable
    }

    public func update(snapshot _: LiveActivityRideSnapshot) async throws -> LiveActivityRideUpdateOutcome {
        throw LiveActivityRideLifecycleError.activityUnavailable
    }

    public func end(reason _: LiveActivityRideLifecycleEndReason) async throws -> LiveActivityRideEndOutcome {
        LiveActivityRideEndOutcome(activityIDs: [])
    }
}
#endif
