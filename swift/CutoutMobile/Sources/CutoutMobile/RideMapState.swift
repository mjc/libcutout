import CutoutMobileFFI
import Foundation

/// Errors surfaced by the map presentation adapter.
public enum MobileRideMapError: Error, Equatable, Hashable {
    case AlreadyRecording
    case NoActiveRide
    case InvalidTransition
    case Storage(String)
}

public enum MobileRideMapStateDto: Equatable, Hashable {
    case recording
    case paused
    case stopped
    case saved
    case discarded
}

public enum MobileRideMapTelemetryStateDto: Equatable, Hashable {
    case gpsOnly
    case associatedNoTelemetry
    case associatedFresh
    case associatedStale
}

public struct MobileRideMapSummaryDto: Equatable, Hashable {
    public var pointCount: UInt64
    public var distanceMeters: Double
    public var durationMilliseconds: UInt64
}

public struct MobileRideMapPointDto: Equatable, Hashable {
    public var sequence: UInt64
    public var segmentId: UInt64
    public var latitudeDegrees: Double
    public var longitudeDegrees: Double
    public var wallClockUnixMs: UInt64
    public var monotonicMs: UInt64
    public var horizontalAccuracyMeters: Double
    public var telemetryState: MobileRideMapTelemetryStateDto
}

public struct MobileRideMapPointBatchDto: Equatable, Hashable {
    public var points: [MobileRideMapPointDto]
    public var nextCursor: UInt64
    public var hasMore: Bool
}

public struct MobileRideMapSnapshotDto: Equatable, Hashable {
    public var rideId: String
    public var state: MobileRideMapStateDto
    public var summary: MobileRideMapSummaryDto
    public var associatedVehicle: String?
}

public struct MobileRideMapHistorySummaryDto: Equatable, Hashable {
    public var rideId: String
    public var state: MobileRideMapStateDto
    public var summary: MobileRideMapSummaryDto
    public var segmentCount: UInt64
    public var candidateVehicle: String?
    public var associatedVehicle: String?
}

public enum MobileRideMapAssociationDto: Equatable, Hashable {
    case associated
    case alreadyAssociated
    case candidateMissing
    case identityMismatch
    case timestampOutOfOrder
    case rideNotOpen
}

public enum MobileRideMapDecisionDto: Equatable, Hashable {
    case accepted(point: MobileRideMapPointDto, segmentStarted: Bool)
    case rejected(reason: String)
    case ignored(reason: String)
}

/// Swift-facing map state backed by main's shared Rust SQLite service.
///
/// This adapter owns only the transient active-ride projection. All durable ride and route
/// writes go through `RideDatabaseHandle`; it intentionally has no SQLite implementation.
public final class MobileRideMapState {
    private let database: RideDatabaseHandle?
    private var activeRideID: MobileRideIdDto?
    private var activeState: MobileRideMapStateDto?
    private var activeCreatedAt: UInt64 = 0
    private var candidateVehicle: String?
    private var activeVehicle: String?
    private var activePoints = [MobileRideMapPointDto]()
    private var lastMonotonic: UInt64 = 0

    public init() {
        database = nil
    }

    init(database: RideDatabaseHandle) {
        self.database = database
        restoreActiveRide()
    }

    private func restoreActiveRide() {
        guard let database, let page = try? database.listRides(cursor: nil, limit: 50) else { return }
        guard let ride = page.rides.first(where: { $0.state == .active || $0.state == .paused || $0.state == .stopped }) else {
            return
        }
        activeRideID = ride.id
        activeCreatedAt = ride.createdAtMilliseconds
        activeState = mapState(ride.state)
        if let points = try? database.routePoints(rideId: ride.id, cursor: nil, limit: 4_096) {
            activePoints = points.points.enumerated().map { index, point in
                mapPoint(point.location, sequence: point.sequence == 0 ? UInt64(index) : point.sequence)
            }
            lastMonotonic = activePoints.last?.monotonicMs ?? 0
        }
    }

    public func currentSnapshot() -> MobileRideMapSnapshotDto? {
        guard let id = activeRideID, let state = activeState else { return nil }
        return snapshot(id: id.value, state: state)
    }

    public func startGpsOnly(atMs: UInt64, lastConnectedVehicle: String?) throws -> MobileRideMapSnapshotDto {
        guard activeState == nil || activeState == .saved || activeState == .discarded else {
            throw MobileRideMapError.AlreadyRecording
        }
        let id: MobileRideIdDto
        if let database {
            do {
                id = try database.createRide(source: .live, createdAtMilliseconds: atMs)
                _ = try database.transition(id: id, event: .start)
            } catch {
                throw map(error)
            }
        } else {
            id = MobileRideIdDto(value: UUID().uuidString)
        }
        activeRideID = id
        activeState = .recording
        activeCreatedAt = atMs
        candidateVehicle = lastConnectedVehicle
        activeVehicle = nil
        activePoints = []
        lastMonotonic = atMs
        return snapshot(id: id.value, state: .recording)
    }

    public func pause() throws -> MobileRideMapSnapshotDto { try transition(.pause, state: .paused) }
    public func resume() throws -> MobileRideMapSnapshotDto { try transition(.resume, state: .recording) }
    public func stop() throws -> MobileRideMapSnapshotDto { try transition(.stop, state: .stopped) }

    public func save() throws -> MobileRideMapSnapshotDto {
        let snapshot = try transition(.save, state: .saved)
        activeRideID = nil
        activeState = .saved
        return snapshot
    }

    public func discard() throws -> MobileRideMapSnapshotDto {
        let snapshot = try transition(.discard, state: .discarded)
        activeRideID = nil
        activeState = .discarded
        return snapshot
    }

    public func observeVehicleConnection(platformIdentifier: String, atMs: UInt64) throws -> MobileRideMapAssociationDto {
        guard activeRideID != nil, activeState == .recording || activeState == .paused else {
            return .rideNotOpen
        }
        guard atMs >= lastMonotonic else { return .timestampOutOfOrder }
        if let activeVehicle, activeVehicle == platformIdentifier { return .alreadyAssociated }
        if let candidateVehicle, candidateVehicle != platformIdentifier { return .identityMismatch }
        activeVehicle = platformIdentifier
        candidateVehicle = nil
        return .associated
    }

    public func ingestLocation(
        monotonicMs: UInt64,
        wallClockUnixMs: UInt64,
        latitudeDegrees: Double,
        longitudeDegrees: Double,
        horizontalAccuracyMeters: Double
    ) throws -> MobileRideMapDecisionDto {
        guard let id = activeRideID, let state = activeState else { throw MobileRideMapError.NoActiveRide }
        guard state == .recording else {
            return .ignored(reason: "ride is not recording")
        }
        guard monotonicMs >= lastMonotonic else { return .rejected(reason: "timestamp out of order") }
        let location = MobileRideLocationDto(
            latitudeDegrees: latitudeDegrees,
            longitudeDegrees: longitudeDegrees,
            monotonicMilliseconds: monotonicMs,
            wallClockUnixMilliseconds: wallClockUnixMs,
            horizontalAccuracyMillimetres: horizontalAccuracyMeters.isFinite && horizontalAccuracyMeters >= 0
                ? UInt32(min(horizontalAccuracyMeters * 1_000, Double(UInt32.max))) : nil,
            source: .live
        )
        if let database {
            do { _ = try database.appendLocation(id: id, location: location) }
            catch { throw map(error) }
        }
        let point = MobileRideMapPointDto(
            sequence: UInt64(activePoints.count), segmentId: 0,
            latitudeDegrees: latitudeDegrees, longitudeDegrees: longitudeDegrees,
            wallClockUnixMs: wallClockUnixMs, monotonicMs: monotonicMs,
            horizontalAccuracyMeters: horizontalAccuracyMeters,
            telemetryState: activeVehicle == nil ? .gpsOnly : .associatedNoTelemetry
        )
        activePoints.append(point)
        lastMonotonic = monotonicMs
        return .accepted(point: point, segmentStarted: activePoints.count == 1)
    }

    public func pointsAfter(afterCursor: UInt64, limit: UInt32) -> MobileRideMapPointBatchDto? {
        batch(from: activePoints, after: afterCursor, limit: limit)
    }

    public func storedSummaries(limit: UInt32) throws -> [MobileRideMapHistorySummaryDto] {
        guard let database else { return [] }
        do {
            let page = try database.listRides(cursor: nil, limit: limit)
            return page.rides.map { ride in
                MobileRideMapHistorySummaryDto(
                    rideId: ride.id.value, state: mapState(ride.state),
                    summary: MobileRideMapSummaryDto(
                        pointCount: ride.summary.pointCount,
                        distanceMeters: Double(ride.summary.distanceMillimetres) / 1_000,
                        durationMilliseconds: ride.updatedAtMilliseconds - ride.createdAtMilliseconds
                    ), segmentCount: ride.summary.pointCount > 0 ? 1 : 0,
                    candidateVehicle: nil, associatedVehicle: nil
                )
            }
        } catch { throw map(error) }
    }

    public func storedPointsAfter(rideId: String, afterCursor: UInt64, limit: UInt32) throws -> MobileRideMapPointBatchDto? {
        guard let database else { return nil }
        do {
            let page = try database.routePoints(
                rideId: MobileRideIdDto(value: rideId),
                cursor: afterCursor == 0 ? nil : MobileRoutePointCursorDto(sequence: afterCursor),
                limit: limit
            )
            let points = page.points.map { mapPoint($0.location, sequence: $0.sequence) }
            return MobileRideMapPointBatchDto(
                points: points,
                nextCursor: page.nextCursor?.sequence ?? 0,
                hasMore: page.nextCursor != nil
            )
        } catch { throw map(error) }
    }

    private func transition(_ event: MobileRideEventDto, state: MobileRideMapStateDto) throws -> MobileRideMapSnapshotDto {
        guard let id = activeRideID, activeState != nil else { throw MobileRideMapError.NoActiveRide }
        if let database { do { _ = try database.transition(id: id, event: event) } catch { throw map(error) } }
        activeState = state
        return snapshot(id: id.value, state: state)
    }

    private func snapshot(id: String, state: MobileRideMapStateDto) -> MobileRideMapSnapshotDto {
        MobileRideMapSnapshotDto(
            rideId: id, state: state,
            summary: summary(), associatedVehicle: activeVehicle
        )
    }

    private func summary() -> MobileRideMapSummaryDto {
        let distance = zip(activePoints, activePoints.dropFirst()).reduce(0.0) { partial, pair in
            partial + haversineMeters(pair.0.latitudeDegrees, pair.0.longitudeDegrees, pair.1.latitudeDegrees, pair.1.longitudeDegrees)
        }
        return MobileRideMapSummaryDto(
            pointCount: UInt64(activePoints.count), distanceMeters: distance,
            durationMilliseconds: lastMonotonic >= activeCreatedAt ? lastMonotonic - activeCreatedAt : 0
        )
    }

    private func batch(from points: [MobileRideMapPointDto], after cursor: UInt64, limit: UInt32) -> MobileRideMapPointBatchDto {
        let start = cursor == 0 ? 0 : points.firstIndex { $0.sequence > cursor } ?? points.count
        let end = min(points.count, start + Int(limit))
        return MobileRideMapPointBatchDto(points: Array(points[start..<end]), nextCursor: end == 0 ? 0 : points[end - 1].sequence, hasMore: end < points.count)
    }

    private func mapPoint(_ location: MobileRideLocationDto, sequence: UInt64) -> MobileRideMapPointDto {
        MobileRideMapPointDto(
            sequence: sequence, segmentId: 0,
            latitudeDegrees: location.latitudeDegrees, longitudeDegrees: location.longitudeDegrees,
            wallClockUnixMs: location.wallClockUnixMilliseconds, monotonicMs: location.monotonicMilliseconds,
            horizontalAccuracyMeters: Double(location.horizontalAccuracyMillimetres ?? 0) / 1_000,
            telemetryState: .gpsOnly
        )
    }

    private func mapState(_ state: MobileRideLifecycleStateDto) -> MobileRideMapStateDto {
        switch state {
        case .active: return .recording
        case .paused: return .paused
        case .stopped, .interrupted: return .stopped
        case .saved, .imported: return .saved
        case .discarded: return .discarded
        case .draft: return .stopped
        }
    }

    private func map(_ error: Error) -> MobileRideMapError {
        if let error = error as? MobileRideDatabaseError {
            switch error {
            case .NotFound: return .NoActiveRide
            case .InvalidTransition, .InvalidRideState: return .InvalidTransition
            default: return .Storage(String(describing: error))
            }
        }
        return .Storage(String(describing: error))
    }
}

private func haversineMeters(_ lat1: Double, _ lon1: Double, _ lat2: Double, _ lon2: Double) -> Double {
    let radius = 6_371_000.0
    let dLat = (lat2 - lat1) * .pi / 180
    let dLon = (lon2 - lon1) * .pi / 180
    let a = sin(dLat / 2) * sin(dLat / 2) + cos(lat1 * .pi / 180) * cos(lat2 * .pi / 180) * sin(dLon / 2) * sin(dLon / 2)
    return radius * 2 * atan2(sqrt(a), sqrt(max(0, 1 - a)))
}
