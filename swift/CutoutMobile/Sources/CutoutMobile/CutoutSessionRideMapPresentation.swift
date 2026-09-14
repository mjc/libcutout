import CutoutMobileFFI
import Foundation

/// Presents Rust ride-map results without owning the ride-map database or admission rules.
final class CutoutSessionRideMapPresentation {
    private let onSnapshot: (MobileRideMapSnapshotDto) -> Void
    private let onDecision: (MobileRideMapSnapshotDto, MobileRideMapDecisionDto) -> Void
    private let onError: (MobileRideMapErrorEvent) -> Void
    private let onAvailability: (MobileRideMapAvailability) -> Void
    private(set) var latestSnapshot: MobileRideMapSnapshotDto?

    init(
        onSnapshot: @escaping (MobileRideMapSnapshotDto) -> Void,
        onDecision: @escaping (MobileRideMapSnapshotDto, MobileRideMapDecisionDto) -> Void,
        onError: @escaping (MobileRideMapErrorEvent) -> Void,
        onAvailability: @escaping (MobileRideMapAvailability) -> Void
    ) {
        self.onSnapshot = onSnapshot
        self.onDecision = onDecision
        self.onError = onError
        self.onAvailability = onAvailability
    }

    func publishSnapshot(_ snapshot: MobileRideMapSnapshotDto) {
        guard Self.shouldPublishSnapshot(current: latestSnapshot, incoming: snapshot) else {
            return
        }
        latestSnapshot = snapshot
        onSnapshot(snapshot)
    }

    static func shouldPublishSnapshot(
        current: MobileRideMapSnapshotDto?,
        incoming: MobileRideMapSnapshotDto
    ) -> Bool {
        guard let current else { return true }
        return current.rideID != incoming.rideID || incoming.revision >= current.revision
    }

    func publishDecision(
        _ decision: MobileRideMapDecisionDto,
        for snapshot: MobileRideMapSnapshotDto
    ) {
        onDecision(snapshot, decision)
    }

    func publishError(_ event: MobileRideMapErrorEvent) {
        onError(event)
    }

    func publishAvailability(_ availability: MobileRideMapAvailability) {
        onAvailability(availability)
    }
}
