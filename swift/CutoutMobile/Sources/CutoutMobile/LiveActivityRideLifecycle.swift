public enum LiveActivityRideLifecycleEndReason: String, Codable, Equatable, Hashable, Sendable {
    case disconnected
    case sessionEnded
    case unavailable
    case permissionDenied
    case platformUnsupported
}

public protocol LiveActivityRideLifecycleManaging: AnyObject {
    func start(snapshot: LiveActivityRideSnapshot)
    func update(snapshot: LiveActivityRideSnapshot)
    func end(reason: LiveActivityRideLifecycleEndReason)
}

public final class LiveActivityRideLifecycleCoordinator {
    private let manager: any LiveActivityRideLifecycleManaging
    private var isActive = false
    private var lastSnapshot: LiveActivityRideSnapshot?

    public init(manager: some LiveActivityRideLifecycleManaging) {
        self.manager = manager
    }

    public func reconcile(
        snapshot: LiveActivityRideSnapshot?,
        shouldBeActive: Bool,
        endReason: LiveActivityRideLifecycleEndReason = .sessionEnded
    ) {
        guard shouldBeActive else {
            endIfNeeded(reason: endReason)
            return
        }

        guard let snapshot else {
            endIfNeeded(reason: endReason)
            return
        }

        if isActive == false {
            manager.start(snapshot: snapshot)
            isActive = true
            lastSnapshot = snapshot
            return
        }

        guard lastSnapshot != snapshot else {
            return
        }

        manager.update(snapshot: snapshot)
        lastSnapshot = snapshot
    }

    public func end(reason: LiveActivityRideLifecycleEndReason) {
        endIfNeeded(reason: reason)
    }

    private func endIfNeeded(reason: LiveActivityRideLifecycleEndReason) {
        guard isActive else {
            lastSnapshot = nil
            return
        }

        manager.end(reason: reason)
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
public final class LiveActivityRideActivityKitManager: LiveActivityRideLifecycleManaging {
    private let state = LiveActivityRideActivityKitState()

    public init() {}

    public func start(snapshot: LiveActivityRideSnapshot) {
        let state = state
        Task {
            await state.start(snapshot: snapshot)
        }
    }

    public func update(snapshot: LiveActivityRideSnapshot) {
        let state = state
        Task {
            await state.update(snapshot: snapshot)
        }
    }

    public func end(reason: LiveActivityRideLifecycleEndReason) {
        let state = state
        Task {
            await state.end(reason: reason)
        }
    }
}

@available(iOS 16.2, *)
private actor LiveActivityRideActivityKitState {
    private var activity: Activity<LiveActivityRideAttributes>?
    private var lastSnapshot: LiveActivityRideSnapshot?

    func start(snapshot: LiveActivityRideSnapshot) async {
        guard ActivityAuthorizationInfo().areActivitiesEnabled else {
            return
        }

        if let existing = Activity<LiveActivityRideAttributes>.activities.first {
            activity = existing
            await update(snapshot: snapshot)
            return
        }

        if activity != nil {
            await update(snapshot: snapshot)
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
        }
    }

    func update(snapshot: LiveActivityRideSnapshot) async {
        guard let activity else {
            await start(snapshot: snapshot)
            return
        }

        await activity.update(content(snapshot: snapshot))
        lastSnapshot = snapshot
    }

    func end(reason _: LiveActivityRideLifecycleEndReason) async {
        let currentActivity = activity
        let existingActivities = Activity<LiveActivityRideAttributes>.activities

        if let currentActivity {
            let finalContent = lastSnapshot.map(content(snapshot:)) ?? currentActivity.content
            await currentActivity.end(finalContent, dismissalPolicy: .immediate)
        }

        for existingActivity in existingActivities where existingActivity.id != currentActivity?.id {
            await existingActivity.end(existingActivity.content, dismissalPolicy: .immediate)
        }

        activity = nil
        lastSnapshot = nil
    }

    private func content(snapshot: LiveActivityRideSnapshot) -> ActivityContent<LiveActivityRideAttributes.ContentState> {
        ActivityContent(state: .init(snapshot: snapshot), staleDate: nil)
    }
}
#else
public final class LiveActivityRideActivityKitManager: LiveActivityRideLifecycleManaging {
    public init() {}

    public func start(snapshot _: LiveActivityRideSnapshot) {}

    public func update(snapshot _: LiveActivityRideSnapshot) {}

    public func end(reason _: LiveActivityRideLifecycleEndReason) {}
}
#endif
