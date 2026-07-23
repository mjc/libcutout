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

public actor LiveActivityRideLifecycleCoordinator {
    private let manager: any LiveActivityRideLifecycleManaging
    private var isActive = false
    private var lastSnapshot: LiveActivityRideSnapshot?
    public private(set) var lastError: LiveActivityRideLifecycleError?
    private var isOperationInFlight = false
    private var operationWaiters: [CheckedContinuation<Void, Never>] = []

    public init(manager: some LiveActivityRideLifecycleManaging) {
        self.manager = manager
    }

    public func reconcile(
        snapshot: LiveActivityRideSnapshot?,
        shouldBeActive: Bool,
        endReason: LiveActivityRideLifecycleEndReason = .sessionEnded
    ) async {
        await beginOperation()
        defer { finishOperation() }

        guard shouldBeActive else {
            await endIfNeeded(reason: endReason)
            return
        }

        guard let snapshot else {
            await endIfNeeded(reason: endReason)
            return
        }

        if isActive == false {
            await start(snapshot: snapshot)
            return
        }

        if lastSnapshot?.identity != snapshot.identity {
            await endIfNeeded(reason: .sessionEnded)
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
            isActive = false
            lastSnapshot = nil
            lastError = Self.lifecycleError(from: error)
        }
    }

    public func end(reason: LiveActivityRideLifecycleEndReason) async {
        await beginOperation()
        defer { finishOperation() }
        await endIfNeeded(reason: reason)
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
            isActive = true
            lastSnapshot = snapshot
            lastError = nil
        } catch {
            isActive = false
            lastSnapshot = nil
            lastError = Self.lifecycleError(from: error)
        }
    }

    private static func lifecycleError(from error: Error) -> LiveActivityRideLifecycleError {
        (error as? LiveActivityRideLifecycleError) ?? .requestFailed
    }

    private func endIfNeeded(reason: LiveActivityRideLifecycleEndReason) async {
        guard isActive else {
            lastSnapshot = nil
            return
        }

        try? await manager.end(reason: reason)
        isActive = false
        lastSnapshot = nil
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
        if let existing = existingActivities.first(where: { $0.attributes.identity == snapshot.identity }) {
            activity = existing
            try await update(snapshot: snapshot)
            return
        }

        if activity != nil {
            if let activeActivity = activity {
                await activeActivity.end(activeActivity.content, dismissalPolicy: .immediate)
            }
            activity = nil
            try await start(snapshot: snapshot)
            return
        }

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
        let currentActivity = activity

        if let currentActivity {
            let finalContent = lastSnapshot.map(content(snapshot:)) ?? currentActivity.content
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
