import Foundation

/// Presents Rust ride-map results without owning the ride-map database or admission rules.
final class CutoutSessionRideMapPresentation {
    private let onSnapshot: (MobileRideMapSnapshotDto) -> Void
    private let onDecision: (MobileRideMapSnapshotDto, MobileRideMapDecisionDto) -> Void
    private let onError: (MobileRideMapErrorEvent) -> Void
    private let onAvailability: (MobileRideMapAvailability) -> Void
    private let onLocationDemand: (MobileRideMapStateDto) -> Void
    private(set) var latestSnapshot: MobileRideMapSnapshotDto?

    init(
        onSnapshot: @escaping (MobileRideMapSnapshotDto) -> Void,
        onDecision: @escaping (MobileRideMapSnapshotDto, MobileRideMapDecisionDto) -> Void,
        onError: @escaping (MobileRideMapErrorEvent) -> Void,
        onAvailability: @escaping (MobileRideMapAvailability) -> Void,
        onLocationDemand: @escaping (MobileRideMapStateDto) -> Void
    ) {
        self.onSnapshot = onSnapshot
        self.onDecision = onDecision
        self.onError = onError
        self.onAvailability = onAvailability
        self.onLocationDemand = onLocationDemand
    }

    func publishSnapshot(_ snapshot: MobileRideMapSnapshotDto) {
        latestSnapshot = snapshot
        onLocationDemand(snapshot.state)
        onSnapshot(snapshot)
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
