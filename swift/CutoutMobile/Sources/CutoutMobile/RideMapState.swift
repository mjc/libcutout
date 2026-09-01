import CutoutMobileFFI
import Foundation

public enum MobileRideMapAvailability: Equatable, Hashable, Sendable {
    case checking
    case ready
    case permissionRequired
    case denied
    case restricted
    case servicesDisabled
    case locationUnavailable
    case storageUnavailable
}

/// Errors surfaced by the map presentation adapter.
public enum MobileRideMapError: Error, Equatable, Hashable, Sendable {
    case alreadyRecording
    case noActiveRide
    case invalidTransition
    case invalidLocation
    case invalidRouteProjection
    case rideNotFound
    case cancelled
    case storageError(String)
}



/// Swift-owned handle for cancelling one Rust durable route projection.
public final class MobileRideMapProjectionCancellation: @unchecked Sendable {
    fileprivate let ffi: MobileRouteProjectionCancellation

    public init() {
        ffi = MobileRouteProjectionCancellation()
    }

    public func cancel() {
        ffi.cancel()
    }
}

/// Swift-owned handle for cancelling one live in-memory route projection.
public final class MobileLiveRideMapProjectionCancellation: @unchecked Sendable {
    fileprivate let ffi: MobileLiveRouteProjectionCancellation

    public init() {
        ffi = MobileLiveRouteProjectionCancellation()
    }

    public func cancel() {
        ffi.cancel()
    }
}

public enum MobileRideMapStateDto: Equatable, Hashable, Sendable {
    case draft
    case active
    case paused
    case stopped
    case interrupted
    case discarded
    case saved
    case imported
}

public enum MobileRideMapTelemetryStateDto: Equatable, Hashable, Sendable {
    case gpsOnly
    case associatedNoTelemetry
    case associatedFresh
    case associatedStale
    case unknown
}

public enum MobileRideMapTelemetryObservation: Equatable, Hashable, Sendable {
    case observed
    case alreadyObserved
    case notAssociated
    case timestampOutOfOrder
    case rideNotOpen
    case unknown
}

public struct MobileRideMapSummaryDto: Equatable, Hashable, Sendable {
    public let pointCount: UInt64
    public let distanceMeters: Double
    public let durationMilliseconds: UInt64
    public let averageSpeedMillimetresPerSecond: UInt64?

    public init(
        pointCount: UInt64,
        distanceMeters: Double,
        durationMilliseconds: UInt64,
        averageSpeedMillimetresPerSecond: UInt64? = nil
    ) {
        self.pointCount = pointCount
        self.distanceMeters = distanceMeters
        self.durationMilliseconds = durationMilliseconds
        self.averageSpeedMillimetresPerSecond = averageSpeedMillimetresPerSecond
    }
}

public struct MobileRideMapPointDto: Equatable, Hashable, Sendable {
    public let sequence: UInt64
    public let segmentId: UInt64
    public let startReason: MobileRideMapSegmentStartReason
    public let latitudeDegrees: Double
    public let longitudeDegrees: Double
    public let wallClockUnixMs: UInt64
    public let monotonicMs: UInt64
    public let horizontalAccuracyMeters: Double?
    public let telemetryState: MobileRideMapTelemetryStateDto

    public init(
        sequence: UInt64,
        segmentId: UInt64,
        startReason: MobileRideMapSegmentStartReason,
        latitudeDegrees: Double,
        longitudeDegrees: Double,
        wallClockUnixMs: UInt64,
        monotonicMs: UInt64,
        horizontalAccuracyMeters: Double?,
        telemetryState: MobileRideMapTelemetryStateDto
    ) {
        self.sequence = sequence
        self.segmentId = segmentId
        self.startReason = startReason
        self.latitudeDegrees = latitudeDegrees
        self.longitudeDegrees = longitudeDegrees
        self.wallClockUnixMs = wallClockUnixMs
        self.monotonicMs = monotonicMs
        self.horizontalAccuracyMeters = horizontalAccuracyMeters
        self.telemetryState = telemetryState
    }
}

public struct MobileRideMapPointBatchDto: Equatable, Hashable, Sendable {
    public let points: [MobileRideMapPointDto]
    public let nextCursor: UInt64?
    public let hasMore: Bool

    public init(points: [MobileRideMapPointDto], nextCursor: UInt64?, hasMore: Bool) {
        self.points = points
        self.nextCursor = nextCursor
        self.hasMore = hasMore
    }
}

public enum MobileRideMapRoutePrivacyPolicy: Equatable, Hashable, Sendable {
    case precise
    case grid(e7: UInt32)
}

public enum MobileRideMapRoutePrivacyClass: Equatable, Hashable, Sendable {
    case precise
    case gridRedacted
}

/// Rust's reason for starting a route segment.
///
/// Segment identity alone is not enough to decide whether a route should be drawn as a gap:
/// resuming a paused ride and crossing an import boundary are different from a background
/// location gap. The map presentation uses `isBackgroundGap` rather than inferring semantics from
/// adjacent segment IDs.
public enum MobileRideMapSegmentStartReason: Equatable, Hashable, Sendable {
    case initial
    case resume
    case backgroundGap
    case importBoundary
    case unknown

    public var isBackgroundGap: Bool {
        self == .backgroundGap
    }

    public var accessibilityLabel: String {
        switch self {
        case .initial:
            return pevLocalizedText("ride_map.segment.initial")
        case .resume:
            return pevLocalizedText("ride_map.segment.resume")
        case .backgroundGap:
            return pevLocalizedText("ride_map.segment.background_gap")
        case .importBoundary:
            return pevLocalizedText("ride_map.segment.import_boundary")
        case .unknown:
            return pevLocalizedText("ride_map.segment.unknown")
        }
    }

    /// Accessibility text for a singleton retained by the bounded display projection.
    ///
    /// The wording intentionally describes the displayed point rather than claiming that the
    /// canonical source segment contains one point or that this point is a route endpoint.
    public var retainedSingletonAccessibilityLabel: String {
        switch self {
        case .initial:
            return pevLocalizedText("ride_map.segment.singleton_initial")
        case .resume:
            return pevLocalizedText("ride_map.segment.singleton_resume")
        case .backgroundGap:
            return pevLocalizedText("ride_map.segment.singleton_background_gap")
        case .importBoundary:
            return pevLocalizedText("ride_map.segment.singleton_import_boundary")
        case .unknown:
            return pevLocalizedText("ride_map.segment.singleton_unknown")
        }
    }
}

public struct MobileRideMapRouteDisplayPoint: Equatable, Hashable, Sendable {
    public let sequence: UInt64
    public let segmentId: UInt64
    public let latitudeDegrees: Double
    public let longitudeDegrees: Double
    public let privacyClass: MobileRideMapRoutePrivacyClass

    init(
        sequence: UInt64,
        segmentId: UInt64,
        latitudeDegrees: Double,
        longitudeDegrees: Double,
        privacyClass: MobileRideMapRoutePrivacyClass
    ) {
        self.sequence = sequence
        self.segmentId = segmentId
        self.latitudeDegrees = latitudeDegrees
        self.longitudeDegrees = longitudeDegrees
        self.privacyClass = privacyClass
    }
}

/// Bounded metadata for one segment represented by a route projection.
public struct MobileRideMapSegmentDisplayMetadata: Equatable, Hashable, Sendable, Identifiable {
    public let segmentId: UInt64
    public let startReason: MobileRideMapSegmentStartReason
    /// Number of points retained for this segment in the bounded display projection.
    /// This is not the cardinality of the canonical source segment.
    public let visiblePointCount: UInt64
    /// Number of points in the canonical source segment, when the projection has durable data.
    /// Live-tail projections may leave this unknown after older points are evicted.
    public let canonicalPointCount: UInt64?
    public let firstVisibleSequence: UInt64?
    public let lastVisibleSequence: UInt64?

    public var id: UInt64 { segmentId }

    public var isBackgroundGap: Bool {
        startReason.isBackgroundGap
    }

    /// Whether exactly one point from this segment was retained for display.
    ///
    /// A retained singleton does not imply that the canonical source segment contains one point.
    public var isRetainedSingleton: Bool {
        visiblePointCount == 1
    }

    /// Whether the canonical source segment contains exactly one point.
    public var isCanonicalSingleton: Bool {
        canonicalPointCount == 1
    }

    /// Accessibility text that distinguishes a canonical singleton from a display-only LOD
    /// singleton. A missing canonical count is intentionally left as the generic retained-point
    /// wording used by live-tail projections.
    public var singletonAccessibilityLabel: String? {
        guard isRetainedSingleton else { return nil }
        guard let canonicalPointCount else {
            return startReason.retainedSingletonAccessibilityLabel
        }
        if canonicalPointCount == 1 {
            return pevLocalizedText(
                "ride_map.segment.canonical_singleton",
                startReason.accessibilityLabel
            )
        }
        return pevLocalizedText(
            "ride_map.segment.display_singleton",
            startReason.accessibilityLabel,
            canonicalPointCount.formatted()
        )
    }

    /// Counts displayed segments that represent a background location gap.
    ///
    /// This is a visible-segment helper for secondary presentation diagnostics. Canonical route
    /// truth must use `MobileRideMapRouteProjection.backgroundGapCount`, which covers omitted
    /// segments too.
    public static func visibleBackgroundGapCount(
        for segments: [MobileRideMapSegmentDisplayMetadata]
    ) -> UInt64 {
        UInt64(segments.lazy.filter(\.isBackgroundGap).count)
    }

    init(
        segmentId: UInt64,
        startReason: MobileRideMapSegmentStartReason,
        visiblePointCount: UInt64,
        canonicalPointCount: UInt64? = nil,
        firstVisibleSequence: UInt64?,
        lastVisibleSequence: UInt64?
    ) {
        self.segmentId = segmentId
        self.startReason = startReason
        self.visiblePointCount = visiblePointCount
        self.canonicalPointCount = canonicalPointCount
        self.firstVisibleSequence = firstVisibleSequence
        self.lastVisibleSequence = lastVisibleSequence
    }
}

public struct MobileRideMapRouteProjection: Equatable, Hashable, Sendable {
    public let points: [MobileRideMapRouteDisplayPoint]
    public let segments: [MobileRideMapSegmentDisplayMetadata]
    public let sourcePointCount: UInt64
    public let sourceSegmentCount: UInt64
    public let candidatePointCount: UInt64
    public let candidateSegmentCount: UInt64
    public let displayedSegmentCount: UInt64
    /// Number of canonical BackgroundGap segments in the complete route.
    public let backgroundGapCount: UInt64
    public let canonicalStartSequence: UInt64?
    public let canonicalEndSequence: UInt64?
    public let canonicalStartVisible: Bool
    public let canonicalEndVisible: Bool

    init(
        points: [MobileRideMapRouteDisplayPoint],
        segments: [MobileRideMapSegmentDisplayMetadata],
        sourcePointCount: UInt64,
        sourceSegmentCount: UInt64,
        candidatePointCount: UInt64,
        candidateSegmentCount: UInt64,
        displayedSegmentCount: UInt64,
        backgroundGapCount: UInt64,
        canonicalStartSequence: UInt64? = nil,
        canonicalEndSequence: UInt64? = nil,
        canonicalStartVisible: Bool = false,
        canonicalEndVisible: Bool = false
    ) {
        self.points = points
        self.segments = segments
        self.sourcePointCount = sourcePointCount
        self.sourceSegmentCount = sourceSegmentCount
        self.candidatePointCount = candidatePointCount
        self.candidateSegmentCount = candidateSegmentCount
        self.displayedSegmentCount = displayedSegmentCount
        self.backgroundGapCount = backgroundGapCount
        self.canonicalStartSequence = canonicalStartSequence
        self.canonicalEndSequence = canonicalEndSequence
        self.canonicalStartVisible = canonicalStartVisible
        self.canonicalEndVisible = canonicalEndVisible
    }

    public var endpointMetadata: MobileRideMapRouteEndpointMetadata {
        MobileRideMapRouteEndpointMetadata(
            canonicalStartSequence: canonicalStartSequence,
            canonicalEndSequence: canonicalEndSequence,
            canonicalStartVisible: canonicalStartVisible,
            canonicalEndVisible: canonicalEndVisible
        )
    }

    /// Whether display LOD omitted candidate points after viewport filtering.
    public var pointsOmittedByBudget: Bool {
        UInt64(points.count) < candidatePointCount
    }

    /// Whether route segments were omitted by the bounded display projection.
    public var segmentsOmittedByBudget: Bool {
        displayedSegmentCount < candidateSegmentCount
    }

}

/// Canonical endpoint identity and viewport visibility for a bounded route projection.
///
/// The Rust projection owns which points are the route endpoints. Swift only decides whether
/// to annotate a displayed point when its canonical sequence matches this metadata.
public struct MobileRideMapRouteEndpointMetadata: Equatable, Hashable, Sendable {
    public let canonicalStartSequence: UInt64?
    public let canonicalEndSequence: UInt64?
    public let canonicalStartVisible: Bool
    public let canonicalEndVisible: Bool

    init(
        canonicalStartSequence: UInt64? = nil,
        canonicalEndSequence: UInt64? = nil,
        canonicalStartVisible: Bool = false,
        canonicalEndVisible: Bool = false
    ) {
        self.canonicalStartSequence = canonicalStartSequence
        self.canonicalEndSequence = canonicalEndSequence
        self.canonicalStartVisible = canonicalStartVisible && canonicalStartSequence != nil
        self.canonicalEndVisible = canonicalEndVisible && canonicalEndSequence != nil
    }

    public static let empty = Self()
}

public struct MobileRideMapSnapshotDto: Equatable, Hashable, Sendable {
    public let rideID: String
    public let state: MobileRideMapStateDto
    public let summary: MobileRideMapSummaryDto
    public let segmentCount: UInt64
    public let associatedVehicle: String?

    public init(
        rideID: String,
        state: MobileRideMapStateDto,
        summary: MobileRideMapSummaryDto,
        segmentCount: UInt64,
        associatedVehicle: String?
    ) {
        self.rideID = rideID
        self.state = state
        self.summary = summary
        self.segmentCount = segmentCount
        self.associatedVehicle = associatedVehicle
    }
}

public struct MobileRideMapHistorySummaryDto: Equatable, Hashable, Sendable {
    public let rideID: String
    public let state: MobileRideMapStateDto
    public let summary: MobileRideMapSummaryDto
    public let segmentCount: UInt64
    public let createdAtMilliseconds: UInt64
    public let candidateVehicle: String?
    public let associatedVehicle: String?
    public let candidateVehicleName: String?
    public let associatedVehicleName: String?
    /// Rust-derived telemetry provenance for the latest durable route evidence.
    public let telemetryState: MobileRideMapTelemetryStateDto

    public init(
        rideID: String,
        state: MobileRideMapStateDto,
        summary: MobileRideMapSummaryDto,
        segmentCount: UInt64,
        createdAtMilliseconds: UInt64,
        candidateVehicle: String?,
        associatedVehicle: String?,
        candidateVehicleName: String? = nil,
        associatedVehicleName: String? = nil,
        telemetryState: MobileRideMapTelemetryStateDto
    ) {
        self.rideID = rideID
        self.state = state
        self.summary = summary
        self.segmentCount = segmentCount
        self.createdAtMilliseconds = createdAtMilliseconds
        self.candidateVehicle = candidateVehicle
        self.associatedVehicle = associatedVehicle
        self.candidateVehicleName = candidateVehicleName
        self.associatedVehicleName = associatedVehicleName
        self.telemetryState = telemetryState
    }

    /// Returns the persisted display name for the confirmed vehicle, falling back to the ride's
    /// candidate vehicle name when association was not completed.
    public var vehicleDisplayName: String? {
        if associatedVehicle != nil {
            return associatedVehicleName
        }
        return candidateVehicleName
    }

}

public struct MobileRideMapHistoryVehicleOptionDto: Equatable, Hashable, Sendable, Identifiable {
    public var platformIdentifier: String
    public var displayName: String?

    public init(platformIdentifier: String, displayName: String?) {
        self.platformIdentifier = platformIdentifier
        self.displayName = displayName
    }

    public var id: String { platformIdentifier }
}

public struct MobileRideMapHistoryPageDto: Equatable, Hashable, Sendable {
    public let summaries: [MobileRideMapHistorySummaryDto]
    public let nextCursor: MobileRideCursorDto?

    public init(summaries: [MobileRideMapHistorySummaryDto], nextCursor: MobileRideCursorDto?) {
        self.summaries = summaries
        self.nextCursor = nextCursor
    }
}

/// Rust-enforced bounds for the history overview's contextual route projection.
///
/// The application passes this value across the FFI boundary; it never pages raw route points
/// into Swift. Keeping the limits in one value makes the memory contract visible at the call site
/// and prevents a new history surface from accidentally requesting an unbounded projection.
public struct MobileRideMapHistoryContextBudget: Equatable, Hashable, Sendable {
    public let historyPageLimit: UInt32
    public let maxRoutes: UInt32
    public let perRouteBudget: UInt32
    public let totalPointBudget: UInt32

    public init(
        historyPageLimit: UInt32,
        maxRoutes: UInt32,
        perRouteBudget: UInt32,
        totalPointBudget: UInt32
    ) {
        self.historyPageLimit = historyPageLimit
        self.maxRoutes = maxRoutes
        self.perRouteBudget = perRouteBudget
        self.totalPointBudget = totalPointBudget
    }

    /// The bounded overview contract: at most eight surrounding rides and 4,096 display points.
    public static let overview = Self(
        historyPageLimit: 50,
        maxRoutes: 8,
        perRouteBudget: 512,
        totalPointBudget: 4_096
    )
}

/// One bounded contextual route returned by Rust for a history overview.
public struct MobileRideMapHistoryContextRoute: Equatable, Hashable, Sendable, Identifiable {
    public let rideID: String
    public let projection: MobileRideMapRouteProjection

    public init(rideID: String, projection: MobileRideMapRouteProjection) {
        self.rideID = rideID
        self.projection = projection
    }

    public var id: String { rideID }
}

/// Bounded surrounding-route context for a selected history route.
public struct MobileRideMapHistoryContextProjection: Equatable, Hashable, Sendable {
    public let routes: [MobileRideMapHistoryContextRoute]
    public let sourceHistoryRouteCount: UInt64
    public let contextRouteCount: UInt64
    public let totalDisplayPointCount: UInt64
    public let routesOmittedByBudget: Bool
    public let historyPageHasMore: Bool

    public init(
        routes: [MobileRideMapHistoryContextRoute],
        sourceHistoryRouteCount: UInt64,
        contextRouteCount: UInt64,
        totalDisplayPointCount: UInt64,
        routesOmittedByBudget: Bool,
        historyPageHasMore: Bool
    ) {
        self.routes = routes
        self.sourceHistoryRouteCount = sourceHistoryRouteCount
        self.contextRouteCount = contextRouteCount
        self.totalDisplayPointCount = totalDisplayPointCount
        self.routesOmittedByBudget = routesOmittedByBudget
        self.historyPageHasMore = historyPageHasMore
    }
}

public enum MobileRideMapAssociationDto: Equatable, Hashable, Sendable {
    case associated
    case alreadyAssociated
    case candidateMissing
    case identityMismatch
    case timestampOutOfOrder
    case rideNotOpen
    case unknown
}

public enum MobileRideMapDecisionReason: Equatable, Hashable, Sendable {
    case rideNotRecording
    case duplicateLocation
    case timestampOutOfOrder
    case accuracyTooLow
    case unrealisticJump
}

public enum MobileRideMapDecisionDto: Equatable, Hashable, Sendable {
    /// The point passed admission but is still awaiting durable SQLite confirmation.
    case pending(point: MobileRideMapPointDto, segmentStarted: Bool)
    case accepted(point: MobileRideMapPointDto, segmentStarted: Bool)
    case rejected(reason: MobileRideMapDecisionReason)
    case ignored(reason: MobileRideMapDecisionReason)
    /// Durable persistence could not confirm this point. The point is not part of the durable ride.
    case storageError(message: String)
}

