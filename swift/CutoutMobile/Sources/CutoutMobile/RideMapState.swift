import CutoutMobileFFI

public enum MobileRideMapAvailability: Equatable, Hashable, Sendable {
    case checking
    case ready
    case permissionRequired
    case denied
    case restricted
    case storageUnavailable
}

/// Errors surfaced by the map presentation adapter.
public enum MobileRideMapError: Error, Equatable, Hashable, Sendable {
    case AlreadyRecording
    case NoActiveRide
    case InvalidTransition
    case InvalidLocation
    case InvalidRouteProjection
    case Storage(String)
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
}

public struct MobileRideMapPointDto: Equatable, Hashable, Sendable {
    public var sequence: UInt64
    public var segmentId: UInt64
    public var latitudeDegrees: Double
    public var longitudeDegrees: Double
    public var wallClockUnixMs: UInt64
    public var monotonicMs: UInt64
    public var horizontalAccuracyMeters: Double
    public var telemetryState: MobileRideMapTelemetryStateDto
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

public struct MobileRideMapRouteDisplayPoint: Equatable, Hashable, Sendable {
    public var sequence: UInt64
    public var segmentId: UInt64
    public var latitudeDegrees: Double
    public var longitudeDegrees: Double
    public var privacyClass: MobileRideMapRoutePrivacyClass
}

public struct MobileRideMapRouteProjection: Equatable, Hashable, Sendable {
    public var points: [MobileRideMapRouteDisplayPoint]
    public var sourcePointCount: UInt64
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
    case storageError(message: String)
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
    /// Creates an in-memory map state for deterministic tests only.
    public init() {
        core = MobileRideMapCore()
        database = nil
        initializationError = nil
        storageUnavailableError = nil
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

    public func currentSnapshot() -> MobileRideMapSnapshotDto? {
        core?.currentSnapshot().map(mapSnapshot)
    }

    public func currentSnapshot(atMs: UInt64) -> MobileRideMapSnapshotDto? {
        core?.currentSnapshotAt(atMs: atMs).map(mapSnapshot)
    }

    public func startGpsOnly(atMs: UInt64, lastConnectedVehicle: String?) throws -> MobileRideMapSnapshotDto {
        do {
            let core = try requireCore()
            return mapSnapshot(try core.startGpsOnly(atMs: atMs, lastConnectedVehicle: lastConnectedVehicle))
        } catch {
            throw map(error)
        }
    }

    public func ensureRecordingForVehicle(
        platformIdentifier: String,
        atMs: UInt64
    ) throws -> MobileRideMapSnapshotDto {
        do {
            let core = try requireCore()
            return mapSnapshot(try core.ensureRecordingForVehicle(
                platformIdentifier: platformIdentifier,
                atMs: atMs
            ))
        } catch {
            throw map(error)
        }
    }

    public func pause(atMs: UInt64) throws -> MobileRideMapSnapshotDto {
        try transition { try requireCore().pauseAt(atMs: atMs) }
    }

    public func resume(atMs: UInt64) throws -> MobileRideMapSnapshotDto {
        try transition { try requireCore().resumeAt(atMs: atMs) }
    }

    public func stop(atMs: UInt64) throws -> MobileRideMapSnapshotDto {
        try transition { try requireCore().stopAt(atMs: atMs) }
    }

    public func save() throws -> MobileRideMapSnapshotDto {
        try transition { try requireCore().save() }
    }

    public func discard() throws -> MobileRideMapSnapshotDto {
        try transition { try requireCore().discard() }
    }

    public func observeVehicleConnection(platformIdentifier: String, atMs: UInt64) throws -> MobileRideMapAssociationDto {
        do {
            let core = try requireCore()
            return map(try core.observeVehicleConnection(platformIdentifier: platformIdentifier, atMs: atMs))
        } catch {
            throw map(error)
        }
    }

    public func observeTelemetry(atMs: UInt64) throws -> MobileRideMapTelemetryObservation {
        do {
            let core = try requireCore()
            return map(try core.observeTelemetry(atMs: atMs))
        } catch {
            throw map(error)
        }
    }

    public func ingestLocation(
        monotonicMs: UInt64,
        wallClockUnixMs: UInt64,
        latitudeDegrees: Double,
        longitudeDegrees: Double,
        horizontalAccuracyMeters: Double
    ) throws -> MobileRideMapDecisionDto {
        do {
            let core = try requireCore()
            return map(try core.ingestLocation(
                monotonicMs: monotonicMs,
                wallClockUnixMs: wallClockUnixMs,
                latitudeDegrees: latitudeDegrees,
                longitudeDegrees: longitudeDegrees,
                horizontalAccuracyMeters: horizontalAccuracyMeters
            ))
        } catch {
            throw map(error)
        }
    }

    /// Ingests the same validated phone sample used by the capture writer.
    public func ingestLocation(
        monotonicMs: UInt64,
        sample: MobilePhoneLocationSampleDto
    ) throws -> MobileRideMapDecisionDto {
        try ingestLocation(
            monotonicMs: monotonicMs,
            wallClockUnixMs: sample.wallClockUnixMs,
            latitudeDegrees: sample.latitudeDegrees,
            longitudeDegrees: sample.longitudeDegrees,
            horizontalAccuracyMeters: sample.horizontalAccuracyMeters
        )
    }

    /// Drains durable outcomes without waiting for the SQLite worker.
    ///
    /// A pending location is not capture-admitted until this method returns its accepted
    /// outcome. Callers should poll from a bounded scheduler and publish each terminal result.
    public func pollLocationWrites() -> [MobileRideMapDecisionDto] {
        core?.pollLocationWrites().map(map) ?? []
    }

    public func pointsAfter(afterCursor: UInt64?, limit: UInt32) throws -> MobileRideMapPointBatchDto? {
        do {
            let core = try requireCore()
            return map(try core.pointsAfter(afterCursor: afterCursor, limit: limit))
        } catch {
            throw map(error)
        }
    }

    /// Projects the Rust-owned recorder tail for a bounded map display.
    ///
    /// This is a presentation projection over the active recorder tail. It does not replace the
    /// paged SQLite history API; full-history viewport LOD remains a separate persistence task.
    public func projectPoints(
        budget: UInt32,
        viewport: MobileGeoBoundsDto? = nil,
        privacy: MobileRideMapRoutePrivacyPolicy = .precise
    ) throws -> MobileRideMapRouteProjection {
        do {
            let core = try requireCore()
            let mappedPrivacy: MobileRideMapRoutePrivacyPolicyDto
            switch privacy {
            case .precise:
                mappedPrivacy = .precise
            case let .grid(e7):
                mappedPrivacy = .grid(gridE7: e7)
            }
            return map(try core.projectPoints(options: MobileRideMapRouteProjectionOptionsDto(
                viewport: viewport,
                budget: budget,
                privacy: mappedPrivacy
            )))
        } catch {
            throw map(error)
        }
    }

    public func storedSummaries(limit: UInt32) throws -> [MobileRideMapHistorySummaryDto] {
        try storedHistoryPage(cursor: nil, limit: limit).summaries
    }

    public func storedHistoryRide(rideID: String) throws -> MobileRideMapHistorySummaryDto? {
        guard let database else { return nil }
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
            return MobileRideMapHistoryPageDto(summaries: [], nextCursor: nil)
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
                durationMilliseconds: ride.durationMilliseconds
            ),
            segmentCount: ride.segmentCount,
            createdAtMilliseconds: ride.createdAtMilliseconds,
            candidateVehicle: ride.candidateVehicle,
            associatedVehicle: ride.associatedVehicle
        )
    }

    public func storedPointsAfter(rideId: String, afterCursor: UInt64?, limit: UInt32) throws -> MobileRideMapPointBatchDto? {
        guard let database else { return nil }
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

    private func transition(_ operation: () throws -> MobileRideMapCoreSnapshotDto) throws -> MobileRideMapSnapshotDto {
        do {
            return mapSnapshot(try operation())
        } catch {
            throw map(error)
        }
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
            sourcePointCount: projection.sourcePointCount
        )
    }

    private func map(_ privacyClass: MobileRideMapRoutePrivacyClassDto) -> MobileRideMapRoutePrivacyClass {
        switch privacyClass {
        case .precise: return .precise
        case .gridRedacted: return .gridRedacted
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
            return .storageError(message: message)
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
            latitudeDegrees: point.location.latitudeDegrees,
            longitudeDegrees: point.location.longitudeDegrees,
            wallClockUnixMs: point.location.wallClockUnixMilliseconds,
            monotonicMs: point.location.monotonicMilliseconds,
            horizontalAccuracyMeters: Double(point.location.horizontalAccuracyMillimetres ?? 0) / 1_000,
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
        case let .Storage(message): return .Storage(message)
        }
    }
}
