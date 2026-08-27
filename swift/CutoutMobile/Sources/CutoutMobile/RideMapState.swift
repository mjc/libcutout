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

/// Swift keeps only presentation DTOs and the canonical history handle. Rust owns all active
/// lifecycle, association, admission, locking, and live-route projection state. The FFI core
/// serializes every mutation, so this adapter is safe to call from the BLE and location queues.
public final class MobileRideMapState: @unchecked Sendable {
    private static let latestRoutePointLimit = 4_096
    private let core: MobileRideMapCore?
    private let database: RideDatabaseHandle?
    public private(set) var initializationError: MobileRideMapError?
    private let storageUnavailableError: MobileRideMapError?

#if DEBUG
    private static let debugDatabase: RideDatabaseHandle? = {
        let path = FileManager.default.temporaryDirectory
            .appendingPathComponent("cutout-map-test-\(UUID().uuidString).sqlite")
            .path
        return try? openRideDatabase(path: path)
    }()

    /// Creates a fresh durable map state for deterministic tests only.
    public convenience init() {
        guard let database = Self.debugDatabase else {
            self.init(storageUnavailable: "Rust ride database is unavailable")
            return
        }
        self.init(database: database)
        // A shared process database is required by the Rust service. Keep each DEBUG fixture
        // isolated at the map-core level by discarding any recovered active ride before use.
        if let core, core.currentSnapshot(atMs: monotonicMillisecondsNow()) != nil {
            _ = try? core.discard()
        }
    }
#endif

    init(database: RideDatabaseHandle) {
        let core = MobileRideMapCore.withDatabase(database: database)
        let initializationError = core.initializationError().map(Self.mapCoreError)
        self.core = initializationError == nil ? core : nil
        self.database = database
        self.initializationError = initializationError
        storageUnavailableError = initializationError
    }

    init(storageUnavailable message: String) {
        core = nil
        database = nil
        let error = MobileRideMapError.Storage(message)
        initializationError = error
        storageUnavailableError = error
    }

    private func requireCore() throws -> MobileRideMapCore {
        guard let core else {
            throw storageUnavailableError ?? .Storage("Rust ride database is unavailable")
        }
        return core
    }

    private func withCore<T>(_ operation: (MobileRideMapCore) throws -> T) throws -> T {
        do {
            return try operation(requireCore())
        } catch {
            throw map(error)
        }
    }

    public func currentSnapshot() -> MobileRideMapSnapshotDto? {
        core?.currentSnapshot(atMs: monotonicMillisecondsNow()).map(mapSnapshot)
    }

    public func currentSnapshot(atMs: UInt64) -> MobileRideMapSnapshotDto? {
        core?.currentSnapshot(atMs: atMs).map(mapSnapshot)
    }

    public func startGpsOnly(atMs: UInt64, lastConnectedVehicle: String?) throws -> MobileRideMapSnapshotDto {
        try withCore {
            mapSnapshot(try $0.startGpsOnly(atMs: atMs, lastConnectedVehicle: lastConnectedVehicle))
        }
    }

    public func ensureRecordingForVehicle(
        platformIdentifier: String,
        atMs: UInt64
    ) throws -> MobileRideMapSnapshotDto {
        try withCore {
            mapSnapshot(try $0.ensureRecordingForVehicle(
                platformIdentifier: platformIdentifier,
                atMs: atMs
            ))
        }
    }

    public func pause(atMs: UInt64) throws -> MobileRideMapSnapshotDto {
        try transition { try $0.pauseAt(atMs: atMs) }
    }

    public func resume(atMs: UInt64) throws -> MobileRideMapSnapshotDto {
        try transition { try $0.resumeAt(atMs: atMs) }
    }

    public func stop(atMs: UInt64) throws -> MobileRideMapSnapshotDto {
        try transition { try $0.stopAt(atMs: atMs) }
    }

    public func save() throws -> MobileRideMapSnapshotDto {
        try transition { try $0.save() }
    }

    public func discard() throws -> MobileRideMapSnapshotDto {
        try transition { try $0.discard() }
    }

    public func observeVehicleConnection(platformIdentifier: String, atMs: UInt64) throws -> MobileRideMapAssociationDto {
        try withCore {
            map(try $0.observeVehicleConnection(platformIdentifier: platformIdentifier, atMs: atMs))
        }
    }

    public func observeTelemetry(atMs: UInt64) throws -> MobileRideMapTelemetryObservation {
        try withCore { map(try $0.observeTelemetry(atMs: atMs)) }
    }

    public func ingestLocation(
        monotonicMs: UInt64,
        wallClockUnixMs: UInt64,
        latitudeDegrees: Double,
        longitudeDegrees: Double,
        horizontalAccuracyMeters: Double
    ) throws -> MobileRideMapDecisionDto {
        return try withCore {
            map(try $0.ingestLocation(
                monotonicMs: monotonicMs,
                wallClockUnixMs: wallClockUnixMs,
                latitudeDegrees: latitudeDegrees,
                longitudeDegrees: longitudeDegrees,
                horizontalAccuracyMeters: horizontalAccuracyMeters
            ))
        }
    }

    /// Ingests the same validated phone sample used by the capture writer.
    public func ingestLocation(
        monotonicMs: UInt64,
        sample: MobilePhoneLocationSampleDto
    ) throws -> MobileRideMapDecisionDto {
        guard let horizontalAccuracyMeters = sample.horizontalAccuracyMeters else {
            throw MobileRideMapError.InvalidLocation
        }
        return try withCore {
            map(try $0.ingestLocation(
                monotonicMs: monotonicMs,
                wallClockUnixMs: sample.wallClockUnixMs,
                latitudeDegrees: sample.latitudeDegrees,
                longitudeDegrees: sample.longitudeDegrees,
                horizontalAccuracyMeters: horizontalAccuracyMeters
            ))
        }
    }

    /// Drains durable outcomes without waiting for the SQLite worker.
    ///
    /// A pending location is not capture-admitted until this method returns its accepted
    /// outcome. Callers should poll from a bounded scheduler and publish each terminal result.
    public func pollLocationWrites() -> [MobileRideMapDecisionDto] {
        core?.pollLocationWrites().map(map) ?? []
    }

    public func pointsAfter(afterCursor: UInt64?, limit: UInt32) throws -> MobileRideMapPointBatchDto? {
        try withCore { map(try $0.pointsAfter(afterCursor: afterCursor, limit: limit)) }
    }

    /// Returns the Rust recorder's bounded active-route tail for live recovery.
    ///
    /// Unlike `pointsAfter`, this never starts at sequence zero or scans durable history.
    public func latestRoutePoints() throws -> MobileRideMapPointBatchDto? {
        try withCore {
            var cursor: UInt64?
            var tail: [MobileRideMapPointDto] = []
            var hasMore = true
            while hasMore {
                let page = try $0.pointsAfter(afterCursor: cursor, limit: 500)
                tail.append(contentsOf: page.points.map(mapPoint))
                if tail.count > Self.latestRoutePointLimit {
                    tail.removeFirst(tail.count - Self.latestRoutePointLimit)
                }
                cursor = page.nextCursor
                hasMore = page.hasMore
            }
            return MobileRideMapPointBatchDto(points: tail, nextCursor: nil, hasMore: false)
        }
    }

    /// Projects the Rust-owned recorder tail for a bounded map display.
    ///
    /// This is a presentation projection over the active recorder tail. Durable history uses
    /// `projectStoredPoints`, so the application layer never performs route decimation itself.
    public func projectPoints(
        budget: UInt32,
        viewport: MobileGeoBoundsDto? = nil,
        privacy: MobileRideMapRoutePrivacyPolicy = .precise,
        cancellation: MobileLiveRideMapProjectionCancellation? = nil
    ) throws -> MobileRideMapRouteProjection {
        try withCore {
            let options = MobileRideMapRouteProjectionOptionsDto(
                viewport: viewport,
                budget: budget,
                privacy: Self.ffiPrivacyPolicy(privacy)
            )
            if let cancellation {
                return map(try $0.projectPointsCancellable(
                    options: options,
                    cancellation: cancellation.ffi
                ))
            }
            return map(try $0.projectPoints(options: options))
        }
    }

    /// Projects a durable route through Rust-owned viewport, LOD, and privacy policy.
    ///
    /// The raw route remains a persistence concern; only bounded display points cross into the
    /// application layer.
    public func projectStoredPoints(
        rideID: String,
        budget: UInt32,
        viewport: MobileGeoBoundsDto? = nil,
        privacy: MobileRideMapRoutePrivacyPolicy = .precise,
        cancellation: MobileRideMapProjectionCancellation? = nil
    ) throws -> MobileRideMapRouteProjection {
        guard let database else {
            throw storageUnavailableError ?? .Storage("Rust ride database is unavailable")
        }
        do {
            let options = MobileRideMapRouteProjectionOptionsDto(
                viewport: viewport,
                budget: budget,
                privacy: Self.ffiPrivacyPolicy(privacy)
            )
            let projectionCancellation = cancellation ?? MobileRideMapProjectionCancellation()
            let projection = try database.projectRoutePointsCancellable(
                rideId: MobileRideIdDto(value: rideID),
                options: options,
                cancellation: projectionCancellation.ffi
            )
            return map(projection)
        } catch {
            throw map(error)
        }
    }

    public func storedSummaries(limit: UInt32) throws -> [MobileRideMapHistorySummaryDto] {
        try storedHistoryPage(cursor: nil, limit: limit).summaries
    }

    public func storedHistoryVehicleOptions() throws -> [MobileRideMapHistoryVehicleOptionDto] {
        guard let database else {
            throw storageUnavailableError ?? .Storage("Rust ride database is unavailable")
        }
        do {
            return try database.listRideHistoryVehicleOptions().map {
                MobileRideMapHistoryVehicleOptionDto(
                    platformIdentifier: $0.platformIdentifier,
                    displayName: $0.displayName
                )
            }
        } catch {
            throw map(error)
        }
    }

    public func storedHistoryRide(rideID: String) throws -> MobileRideMapHistorySummaryDto? {
        guard let database else {
            throw storageUnavailableError ?? .Storage("Rust ride database is unavailable")
        }
        do {
            let ride = try database.findRide(rideId: MobileRideIdDto(value: rideID))
            return ride.map(mapHistorySummary)
        } catch {
            throw map(error)
        }
    }

    public func storedHistoryPage(
        cursor: MobileRideCursorDto?,
        limit: UInt32,
        filter: MobileRideHistoryFilterDto? = nil
    ) throws -> MobileRideMapHistoryPageDto {
        guard let database else {
            throw storageUnavailableError ?? .Storage("Rust ride database is unavailable")
        }
        do {
            let filter = filter ?? MobileRideHistoryFilterDto(
                createdAfterMilliseconds: nil,
                vehicleIdentity: nil,
                searchText: nil
            )
            let page = try database.listRidesFiltered(cursor: cursor, filter: filter, limit: limit)
            let summaries = page.rides.map(mapHistorySummary)
            return MobileRideMapHistoryPageDto(
                summaries: summaries,
                nextCursor: page.nextCursor
            )
        } catch {
            throw map(error)
        }
    }

    /// Projects bounded surrounding history routes through Rust-owned filtering, LOD, and
    /// privacy policy. No raw historical points are accumulated by Swift.
    public func projectStoredHistoryContext(
        filter: MobileRideHistoryFilterDto,
        selectedRideID: String?,
        budget: MobileRideMapHistoryContextBudget = .overview,
        viewport: MobileGeoBoundsDto? = nil,
        privacy: MobileRideMapRoutePrivacyPolicy = .precise
    ) throws -> MobileRideMapHistoryContextProjection {
        guard let database else {
            throw storageUnavailableError ?? .Storage("Rust ride database is unavailable")
        }
        do {
            let ffiBudget = MobileRideHistoryContextBudgetDto(
                historyPageLimit: budget.historyPageLimit,
                maxRoutes: budget.maxRoutes,
                perRouteBudget: budget.perRouteBudget,
                totalPointBudget: budget.totalPointBudget
            )
            let options = MobileRideHistoryContextOptionsDto(
                filter: filter,
                selectedRideId: selectedRideID.map(MobileRideIdDto.init(value:)),
                budget: ffiBudget,
                viewport: viewport,
                privacy: Self.ffiPrivacyPolicy(privacy)
            )
            return map(try database.projectHistoryContext(options: options))
        } catch {
            throw map(error)
        }
    }

    private func mapHistorySummary(_ ride: MobileRideRecordDto) -> MobileRideMapHistorySummaryDto {
        MobileRideMapHistorySummaryDto(
            rideId: ride.id.value,
            state: mapState(ride.state),
            summary: MobileRideMapSummaryDto(
                pointCount: ride.summary.pointCount,
                distanceMeters: Double(ride.summary.distanceMillimetres) / 1_000,
                durationMilliseconds: ride.durationMilliseconds,
                averageSpeedMillimetresPerSecond: ride.summary.averageSpeedMillimetresPerSecond
            ),
            segmentCount: ride.segmentCount,
            createdAtMilliseconds: ride.createdAtMilliseconds,
            candidateVehicle: ride.candidateVehicle,
            associatedVehicle: ride.associatedVehicle,
            candidateVehicleName: ride.candidateVehicleName,
            associatedVehicleName: ride.associatedVehicleName,
            telemetryState: MobileRideMapHistorySummaryDto.telemetryState(
                associatedVehicle: ride.associatedVehicle,
                lastTelemetryAtMilliseconds: ride.lastTelemetryAtMilliseconds
            )
        )
    }

    public func storedPointsAfter(rideId: String, afterCursor: UInt64?, limit: UInt32) throws -> MobileRideMapPointBatchDto {
        guard let database else {
            throw storageUnavailableError ?? .Storage("Rust ride database is unavailable")
        }
        do {
            let page = try database.routePoints(
                rideId: MobileRideIdDto(value: rideId),
                cursor: afterCursor.map(MobileRoutePointCursorDto.init(sequence:)),
                limit: limit
            )
            return MobileRideMapPointBatchDto(
                points: page.points.map(mapStoredPoint),
                nextCursor: page.nextCursor?.sequence,
                hasMore: page.nextCursor != nil
            )
        } catch {
            throw map(error)
        }
    }

    private func transition(_ operation: (MobileRideMapCore) throws -> MobileRideMapCoreSnapshotDto) throws -> MobileRideMapSnapshotDto {
        try withCore { mapSnapshot(try operation($0)) }
    }

    private func mapSnapshot(_ snapshot: MobileRideMapCoreSnapshotDto) -> MobileRideMapSnapshotDto {
        MobileRideMapSnapshotDto(
            rideId: snapshot.rideId,
            state: mapState(snapshot.state),
            summary: MobileRideMapSummaryDto(
                pointCount: snapshot.summary.pointCount,
                distanceMeters: snapshot.summary.distanceMeters,
                durationMilliseconds: snapshot.summary.durationMilliseconds
            ),
            segmentCount: snapshot.segmentCount,
            associatedVehicle: snapshot.associatedVehicle
        )
    }

    private func map(_ batch: MobileRideMapCorePointBatchDto) -> MobileRideMapPointBatchDto {
        MobileRideMapPointBatchDto(
            points: batch.points.map(mapPoint),
            nextCursor: batch.nextCursor,
            hasMore: batch.hasMore
        )
    }

    private func map(_ projection: MobileRideMapRouteProjectionDto) -> MobileRideMapRouteProjection {
        MobileRideMapRouteProjection(
            points: projection.points.map { point in
                MobileRideMapRouteDisplayPoint(
                    sequence: point.sequence,
                    segmentId: point.segmentId,
                    latitudeDegrees: point.latitudeDegrees,
                    longitudeDegrees: point.longitudeDegrees,
                    privacyClass: map(point.privacyClass)
                )
            },
            segments: projection.segments.map { segment in
                MobileRideMapSegmentDisplayMetadata(
                    segmentId: segment.segmentId,
                    startReason: map(segment.startReason),
                    visiblePointCount: segment.visiblePointCount,
                    canonicalPointCount: segment.canonicalPointCount,
                    firstVisibleSequence: segment.firstVisibleSequence,
                    lastVisibleSequence: segment.lastVisibleSequence
                )
            },
            sourcePointCount: projection.sourcePointCount,
            sourceSegmentCount: projection.sourceSegmentCount,
            candidatePointCount: projection.candidatePointCount,
            candidateSegmentCount: projection.candidateSegmentCount,
            displayedSegmentCount: projection.displayedSegmentCount,
            backgroundGapCount: projection.backgroundGapCount,
            canonicalStartSequence: projection.canonicalStartSequence,
            canonicalEndSequence: projection.canonicalEndSequence,
            canonicalStartVisible: projection.canonicalStartVisible,
            canonicalEndVisible: projection.canonicalEndVisible
        )
    }

    private static func ffiPrivacyPolicy(
        _ privacy: MobileRideMapRoutePrivacyPolicy
    ) -> MobileRideMapRoutePrivacyPolicyDto {
        switch privacy {
        case .precise:
            return .precise
        case let .grid(e7):
            return .grid(gridE7: e7)
        }
    }

    private func map(
        _ projection: MobileRideHistoryContextProjectionDto
    ) -> MobileRideMapHistoryContextProjection {
        MobileRideMapHistoryContextProjection(
            routes: projection.routes.map {
                MobileRideMapHistoryContextRoute(
                    rideID: $0.rideId.value,
                    projection: map($0.projection)
                )
            },
            sourceHistoryRouteCount: projection.sourceHistoryRouteCount,
            contextRouteCount: projection.contextRouteCount,
            totalDisplayPointCount: projection.totalDisplayPointCount,
            routesOmittedByBudget: projection.routesOmittedByBudget,
            historyPageHasMore: projection.historyPageHasMore
        )
    }

    private func map(_ privacyClass: MobileRideMapRoutePrivacyClassDto) -> MobileRideMapRoutePrivacyClass {
        switch privacyClass {
        case .precise: return .precise
        case .gridRedacted: return .gridRedacted
        }
    }

    private func map(
        _ reason: MobileRideSegmentStartReasonDto
    ) -> MobileRideMapSegmentStartReason {
        switch reason {
        case .initial: return .initial
        case .resume: return .resume
        case .backgroundGap: return .backgroundGap
        case .importBoundary: return .importBoundary
        }
    }

    private func map(_ decision: MobileRideMapCoreDecisionDto) -> MobileRideMapDecisionDto {
        switch decision {
        case let .pending(point, segmentStarted):
            return .pending(point: mapPoint(point), segmentStarted: segmentStarted)
        case let .accepted(point, segmentStarted):
            return .accepted(point: mapPoint(point), segmentStarted: segmentStarted)
        case let .rejected(reason):
            return .rejected(reason: map(reason))
        case let .ignored(reason):
            return .ignored(reason: map(reason))
        case let .storageError(message):
            return .storageError(message: message, retryable: false)
        }
    }

    private func map(_ reason: CutoutMobileFFI.MobileRideMapDecisionReasonDto) -> MobileRideMapDecisionReason {
        switch reason {
        case .rideNotRecording: return .rideNotRecording
        case .duplicateLocation: return .duplicateLocation
        case .timestampOutOfOrder: return .timestampOutOfOrder
        case .accuracyTooLow: return .accuracyTooLow
        case .unrealisticJump: return .unrealisticJump
        }
    }

    private func map(_ association: MobileRideMapCoreAssociationDto) -> MobileRideMapAssociationDto {
        switch association {
        case .associated: return .associated
        case .alreadyAssociated: return .alreadyAssociated
        case .candidateMissing: return .candidateMissing
        case .identityMismatch: return .identityMismatch
        case .timestampOutOfOrder: return .timestampOutOfOrder
        case .rideNotOpen: return .rideNotOpen
        case .unknown: return .unknown
        }
    }

    private func map(_ observation: CutoutMobileFFI.MobileRideMapTelemetryObservationDto) -> MobileRideMapTelemetryObservation {
        switch observation {
        case .observed: return .observed
        case .alreadyObserved: return .alreadyObserved
        case .notAssociated: return .notAssociated
        case .timestampOutOfOrder: return .timestampOutOfOrder
        case .rideNotOpen: return .rideNotOpen
        case .unknown: return .unknown
        }
    }

    private func mapPoint(_ point: MobileRideMapCorePointDto) -> MobileRideMapPointDto {
        MobileRideMapPointDto(
            sequence: point.sequence,
            segmentId: point.segmentId,
            startReason: map(point.startReason),
            latitudeDegrees: point.latitudeDegrees,
            longitudeDegrees: point.longitudeDegrees,
            wallClockUnixMs: point.wallClockUnixMs,
            monotonicMs: point.monotonicMs,
            horizontalAccuracyMeters: point.horizontalAccuracyMeters,
            telemetryState: map(point.telemetryState)
        )
    }

    private func mapStoredPoint(_ point: MobileRoutePointDto) -> MobileRideMapPointDto {
        MobileRideMapPointDto(
            sequence: point.sequence,
            segmentId: point.segmentId,
            startReason: map(point.startReason),
            latitudeDegrees: point.location.latitudeDegrees,
            longitudeDegrees: point.location.longitudeDegrees,
            wallClockUnixMs: point.location.wallClockUnixMilliseconds,
            monotonicMs: point.location.monotonicMilliseconds,
            horizontalAccuracyMeters: point.location.horizontalAccuracyMillimetres.map {
                Double($0) / 1_000
            },
            telemetryState: map(point.telemetryState)
        )
    }

    private func map(_ telemetry: MobileRideMapCoreTelemetryStateDto) -> MobileRideMapTelemetryStateDto {
        switch telemetry {
        case .gpsOnly: return .gpsOnly
        case .associatedNoTelemetry: return .associatedNoTelemetry
        case .associatedFresh: return .associatedFresh
        case .associatedStale: return .associatedStale
        case .unknown: return .unknown
        }
    }

    private func mapState(_ state: MobileRideLifecycleStateDto) -> MobileRideMapStateDto {
        switch state {
        case .active: return .recording
        case .paused: return .paused
        case .stopped, .interrupted: return .stopped
        case .saved, .imported: return .saved
        case .draft: return .stopped
        case .discarded: return .discarded
        }
    }

    private func map(_ error: Error) -> MobileRideMapError {
        if let error = error as? MobileRideMapError {
            return error
        }
        if let error = error as? MobileRideMapCoreErrorDto {
            return Self.mapCoreError(error)
        }
        if let error = error as? MobileRideDatabaseError {
            switch error {
            case .NotFound: return .NoActiveRide
            case .InvalidTransition, .InvalidRideState: return .InvalidTransition
            case .Cancelled: return .cancelled
            default: return .Storage(String(describing: error))
            }
        }
        return .Storage(String(describing: error))
    }

    private static func mapCoreError(_ error: MobileRideMapCoreErrorDto) -> MobileRideMapError {
        switch error {
        case .AlreadyRecording: return .AlreadyRecording
        case .NoActiveRide: return .NoActiveRide
        case .InvalidTransition: return .InvalidTransition
        case .InvalidLocation: return .InvalidLocation
        case .InvalidRouteProjection: return .InvalidRouteProjection
        case .Cancelled: return .cancelled
        case let .Storage(message): return .Storage(message)
        }
    }
}
