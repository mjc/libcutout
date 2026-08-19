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

public protocol LiveActivityRideLifecycleManaging: Sendable {
    func start(snapshot: LiveActivityRideSnapshot) async throws
    func update(snapshot: LiveActivityRideSnapshot) async throws
    func end(reason: LiveActivityRideLifecycleEndReason) async throws
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
    private var hasReconciledInactiveState = false
    private var lastSnapshot: LiveActivityRideSnapshot?
    public private(set) var lastError: LiveActivityRideLifecycleError?
    private var latestRequestID: UInt64 = 0
    private var isOperationInFlight = false
    private var operationWaiters: [CheckedContinuation<Void, Never>] = []

    public init(manager: some LiveActivityRideLifecycleManaging) {
        self.manager = manager
    }

    public func reconcile(
        requestID: UInt64,
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

        if lastSnapshot == nil {
            await start(snapshot: snapshot)
            return
        }

        if lastSnapshot?.identity != snapshot.identity {
            guard await endIfNeeded(reason: .sessionEnded) else { return }
            await start(snapshot: snapshot)
            return
        }

        guard lastSnapshot != snapshot else {
            return
        }

        do {
            try await manager.update(snapshot: snapshot)
            lastSnapshot = snapshot
            lastError = nil
        } catch {
            lastError = Self.lifecycleError(from: error)
        }
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

    private func start(snapshot: LiveActivityRideSnapshot) async {
        do {
            try await manager.start(snapshot: snapshot)
            hasReconciledInactiveState = false
            lastSnapshot = snapshot
            lastError = nil
        } catch {
            lastSnapshot = nil
            lastError = Self.lifecycleError(from: error)
        }
    }

    private static func lifecycleError(from error: Error) -> LiveActivityRideLifecycleError {
        (error as? LiveActivityRideLifecycleError) ?? .requestFailed
    }

    private func endIfNeeded(reason: LiveActivityRideLifecycleEndReason) async -> Bool {
        guard hasReconciledInactiveState == false else {
            lastSnapshot = nil
            return true
        }

        do {
            try await manager.end(reason: reason)
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

    public func start(snapshot: LiveActivityRideSnapshot) async throws {
        try await state.start(snapshot: snapshot)
    }

    public func update(snapshot: LiveActivityRideSnapshot) async throws {
        try await state.update(snapshot: snapshot)
    }

    public func end(reason: LiveActivityRideLifecycleEndReason) async throws {
        try await state.end(reason: reason)
    }
}

@available(iOS 16.2, *)
private actor LiveActivityRideActivityKitState {
    private var activity: Activity<LiveActivityRideAttributes>?
    private var lastSnapshot: LiveActivityRideSnapshot?

    func start(snapshot: LiveActivityRideSnapshot) async throws {
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
            try await update(snapshot: snapshot)
            return
        }

        activity = nil
        do {
            activity = try Activity.request(
                attributes: LiveActivityRideAttributes(identity: snapshot.identity),
                content: content(snapshot: snapshot),
                pushType: nil
            )
            lastSnapshot = snapshot
        } catch {
            activity = nil
            lastSnapshot = nil
            throw LiveActivityRideLifecycleError.requestFailed
        }
    }

    func update(snapshot: LiveActivityRideSnapshot) async throws {
        guard let activity else {
            throw LiveActivityRideLifecycleError.activityUnavailable
        }

        await activity.update(content(snapshot: snapshot))
        lastSnapshot = snapshot
    }

    func end(reason _: LiveActivityRideLifecycleEndReason) async throws {
        let currentActivityID = activity?.id
        for currentActivity in Activity<LiveActivityRideAttributes>.activities {
            let finalContent = currentActivity.id == currentActivityID
                ? lastSnapshot.map(content(snapshot:)) ?? currentActivity.content
                : currentActivity.content
            await currentActivity.end(finalContent, dismissalPolicy: .immediate)
        }

        activity = nil
        lastSnapshot = nil
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

    public func start(snapshot _: LiveActivityRideSnapshot) async throws {
        throw LiveActivityRideLifecycleError.activityUnavailable
    }

    public func update(snapshot _: LiveActivityRideSnapshot) async throws {
        throw LiveActivityRideLifecycleError.activityUnavailable
    }

    public func end(reason _: LiveActivityRideLifecycleEndReason) async throws {}
}
#endif
