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
    case AlreadyRecording
    case NoActiveRide
    case InvalidTransition
    case InvalidLocation
    case InvalidRouteProjection
    case RideNotFound
    case cancelled
    case Storage(String)
}

/// Swift-owned handle for cancelling one Rust durable route projection.
public final class MobileRideMapProjectionCancellation: @unchecked Sendable {
    fileprivate let ffi: MobileRouteProjectionCancellation

    public init(timeoutMilliseconds: UInt64 = 2_000) {
        ffi = MobileRouteProjectionCancellation.withTimeoutMilliseconds(
            timeoutMilliseconds: timeoutMilliseconds
        )
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
    case recording
    case paused
    case stopped
    case saved
    case discarded
}

public enum MobileRideMapTelemetryStateDto: Equatable, Hashable, Sendable {
    case gpsOnly
    case associatedNoTelemetry
    case associatedFresh
    case associatedStale
}

public enum MobileRideMapTelemetryObservation: Equatable, Hashable, Sendable {
    case observed
    case alreadyObserved
    case notAssociated
    case timestampOutOfOrder
    case rideNotOpen
}

public struct MobileRideMapSummaryDto: Equatable, Hashable, Sendable {
    public var pointCount: UInt64
    public var distanceMeters: Double
    public var durationMilliseconds: UInt64
    public var averageSpeedMillimetresPerSecond: UInt64?

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
    public var sequence: UInt64
    public var segmentId: UInt64
    public var startReason: MobileRideMapSegmentStartReason
    public var latitudeDegrees: Double
    public var longitudeDegrees: Double
    public var wallClockUnixMs: UInt64
    public var monotonicMs: UInt64
    public var horizontalAccuracyMeters: Double?
    public var telemetryState: MobileRideMapTelemetryStateDto

    public init(
        sequence: UInt64,
        segmentId: UInt64,
        startReason: MobileRideMapSegmentStartReason = .unknown,
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
    public var points: [MobileRideMapPointDto]
    public var nextCursor: UInt64?
    public var hasMore: Bool
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
            return "Route start point"
        case .resume:
            return "Route resumed"
        case .backgroundGap:
            return "Background location gap"
        case .importBoundary:
            return "Imported route segment"
        case .unknown:
            return "Route point"
        }
    }

    /// Accessibility text for a singleton retained by the bounded display projection.
    ///
    /// The wording intentionally describes the displayed point rather than claiming that the
    /// canonical source segment contains one point or that this point is a route endpoint.
    public var retainedSingletonAccessibilityLabel: String {
        switch self {
        case .initial:
            return "Displayed route point; initial segment has one retained display point"
        case .resume:
            return "Displayed route point; resumed segment has one retained display point"
        case .backgroundGap:
            return "Displayed route point; background location gap segment has one retained display point"
        case .importBoundary:
            return "Displayed route point; imported route segment has one retained display point"
        case .unknown:
            return "Displayed route point; segment reason unavailable and one display point retained"
        }
    }
}

public struct MobileRideMapRouteDisplayPoint: Equatable, Hashable, Sendable {
    public var sequence: UInt64
    public var segmentId: UInt64
    public var latitudeDegrees: Double
    public var longitudeDegrees: Double
    public var privacyClass: MobileRideMapRoutePrivacyClass

    public init(
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

    public init(_ point: MobileRideMapPointDto) {
        self.init(
            sequence: point.sequence,
            segmentId: point.segmentId,
            latitudeDegrees: point.latitudeDegrees,
            longitudeDegrees: point.longitudeDegrees,
            privacyClass: .precise
        )
    }
}

/// Bounded metadata for one segment represented by a route projection.
public struct MobileRideMapSegmentDisplayMetadata: Equatable, Hashable, Sendable, Identifiable {
    public var segmentId: UInt64
    public var startReason: MobileRideMapSegmentStartReason
    /// Number of points retained for this segment in the bounded display projection.
    /// This is not the cardinality of the canonical source segment.
    public var visiblePointCount: UInt64
    /// Number of points in the canonical source segment, when the projection has durable data.
    /// Live-tail projections may leave this unknown after older points are evicted.
    public var canonicalPointCount: UInt64?
    public var firstVisibleSequence: UInt64?
    public var lastVisibleSequence: UInt64?

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
            return "\(startReason.accessibilityLabel); segment contains one recorded point"
        }
        return "\(startReason.accessibilityLabel); one display point represents \(canonicalPointCount) recorded points"
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

    public init(
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
    public var points: [MobileRideMapRouteDisplayPoint]
    public var segments: [MobileRideMapSegmentDisplayMetadata]
    public var sourcePointCount: UInt64
    public var sourceSegmentCount: UInt64
    public var candidatePointCount: UInt64
    public var candidateSegmentCount: UInt64
    public var displayedSegmentCount: UInt64
    /// Number of canonical BackgroundGap segments in the complete route.
    public var backgroundGapCount: UInt64
    public var canonicalStartSequence: UInt64?
    public var canonicalEndSequence: UInt64?
    public var canonicalStartVisible: Bool
    public var canonicalEndVisible: Bool

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

    public init(
        canonicalStartSequence: UInt64? = nil,
        canonicalEndSequence: UInt64? = nil,
        canonicalStartVisible: Bool = false,
        canonicalEndVisible: Bool = false
    ) {
        self.canonicalStartSequence = canonicalStartSequence
        self.canonicalEndSequence = canonicalEndSequence
        self.canonicalStartVisible = canonicalStartVisible
        self.canonicalEndVisible = canonicalEndVisible
    }

    public static let empty = Self()
}

public struct MobileRideMapSnapshotDto: Equatable, Hashable, Sendable {
    public var rideId: String
    public var state: MobileRideMapStateDto
    public var summary: MobileRideMapSummaryDto
    public var segmentCount: UInt64
    public var associatedVehicle: String?
}

public struct MobileRideMapHistorySummaryDto: Equatable, Hashable, Sendable {
    public var rideId: String
    public var state: MobileRideMapStateDto
    public var summary: MobileRideMapSummaryDto
    public var segmentCount: UInt64
    public var createdAtMilliseconds: UInt64
    public var candidateVehicle: String?
    public var associatedVehicle: String?
    public var candidateVehicleName: String?
    public var associatedVehicleName: String?
    /// Rust-derived telemetry provenance for the latest durable route evidence.
    public var telemetryState: MobileRideMapTelemetryStateDto

    public init(
        rideId: String,
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
        self.rideId = rideId
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
        associatedVehicleName ?? candidateVehicleName
    }

    /// Derives the historical telemetry label from Rust's persisted association metadata.
    /// A timestamp proves that telemetry was observed; freshness is evaluated while recording.
    public static func telemetryState(
        associatedVehicle: String?,
        lastTelemetryAtMilliseconds: UInt64?
    ) -> MobileRideMapTelemetryStateDto {
        guard associatedVehicle != nil else { return .gpsOnly }
        return lastTelemetryAtMilliseconds == nil ? .associatedNoTelemetry : .associatedFresh
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
    public var summaries: [MobileRideMapHistorySummaryDto]
    public var nextCursor: MobileRideCursorDto?
}

public enum MobileRideMapAssociationDto: Equatable, Hashable, Sendable {
    case associated
    case alreadyAssociated
    case candidateMissing
    case identityMismatch
    case timestampOutOfOrder
    case rideNotOpen
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
    /// The point could not be queued or persisted. It is not part of the ride.
    ///
    /// `retryable` is true only after Rust reconciles a dropped response and proves that the
    /// write was not committed. The caller may explicitly re-admit that sample; this adapter
    /// never retries it automatically.
    case storageError(message: String, retryable: Bool)
}

/// Swift keeps only presentation DTOs and the canonical history handle. Rust owns all active
/// lifecycle, association, admission, locking, and live-route projection state. The FFI core
/// serializes every mutation, so this adapter is safe to call from the BLE and location queues.
public final class MobileRideMapState: @unchecked Sendable {
    private let core: MobileRideMapCore?
    private let database: RideDatabaseHandle?
    public private(set) var initializationError: MobileRideMapError?
    private let storageUnavailableError: MobileRideMapError?

#if DEBUG
    private static let debugDatabase: RideDatabaseHandle? = {
        if let shared = RustPersistenceStore.shared {
            return shared
        }
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
        if let core, core.currentSnapshot() != nil {
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
        core?.currentSnapshot().map(mapSnapshot)
    }

    public func currentSnapshot(atMs: UInt64) -> MobileRideMapSnapshotDto? {
        core?.currentSnapshotAt(atMs: atMs).map(mapSnapshot)
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
        try withCore {
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
        try withCore {
            map(try $0.ingestLocationSample(monotonicMs: monotonicMs, sample: sample))
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
        try withCore { map(try $0.latestRoutePoints()) }
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
            let mappedPrivacy: MobileRideMapRoutePrivacyPolicyDto
            switch privacy {
            case .precise:
                mappedPrivacy = .precise
            case let .grid(e7):
                mappedPrivacy = .grid(gridE7: e7)
            }
            let options = MobileRideMapRouteProjectionOptionsDto(
                viewport: viewport,
                budget: budget,
                privacy: mappedPrivacy
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
        let mappedPrivacy: MobileRideMapRoutePrivacyPolicyDto
        switch privacy {
        case .precise:
            mappedPrivacy = .precise
        case let .grid(e7):
            mappedPrivacy = .grid(gridE7: e7)
        }
        do {
            let options = MobileRideMapRouteProjectionOptionsDto(
                viewport: viewport,
                budget: budget,
                privacy: mappedPrivacy
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
        case let .storageError(message, retryable):
            return .storageError(message: message, retryable: retryable)
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
        }
    }

    private func map(_ observation: CutoutMobileFFI.MobileRideMapTelemetryObservationDto) -> MobileRideMapTelemetryObservation {
        switch observation {
        case .observed: return .observed
        case .alreadyObserved: return .alreadyObserved
        case .notAssociated: return .notAssociated
        case .timestampOutOfOrder: return .timestampOutOfOrder
        case .rideNotOpen: return .rideNotOpen
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
