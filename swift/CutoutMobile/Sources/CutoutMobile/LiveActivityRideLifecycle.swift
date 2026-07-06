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
import ActivityKit

@available(iOS 16.1, *)
public struct LiveActivityRideAttributes: ActivityAttributes, Codable, Hashable, Sendable {
    public struct ContentState: Codable, Hashable, Sendable {}

    public let identity: LiveActivityRideIdentity

    public init(identity: LiveActivityRideIdentity) {
        self.identity = identity
    }
}

@available(iOS 16.1, *)
public final class LiveActivityRideActivityKitManager: LiveActivityRideLifecycleManaging {
    private var activity: Activity<LiveActivityRideAttributes>?

    public init() {}

    public func start(snapshot: LiveActivityRideSnapshot) {
        guard activity == nil else {
            update(snapshot: snapshot)
            return
        }

        Task {
            do {
                let requested = try Activity.request(
                    attributes: LiveActivityRideAttributes(identity: snapshot.identity),
                    contentState: LiveActivityRideAttributes.ContentState()
                )
                activity = requested
            } catch {
                activity = nil
            }
        }
    }

    public func update(snapshot: LiveActivityRideSnapshot) {
        Task {
            await activity?.update(
                using: LiveActivityRideAttributes.ContentState()
            )
        }
    }

    public func end(reason _: LiveActivityRideLifecycleEndReason) {
        Task {
            await activity?.end(
                LiveActivityRideAttributes.ContentState(),
                dismissalPolicy: .immediate
            )
            activity = nil
        }
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
