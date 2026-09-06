import XCTest
import CutoutMobileFFI
@testable import CutoutMobile

final class RideMapStateTests: XCTestCase {
    func testMusicHistoryPolicyIsProjectedFromRustAndResetsForNewRide() throws {
        let state = MobileRideMapState()

        XCTAssertEqual(state.currentMusicHistoryPolicy(), .disabled)
        _ = try state.startGpsOnly(atMs: 1_000, lastConnectedVehicle: nil)
        try state.setMusicHistoryPolicy(.humanReadable)
        XCTAssertEqual(state.currentMusicHistoryPolicy(), .humanReadable)

        _ = try state.stop(atMs: 2_000)
        _ = try state.save()
        _ = try state.startGpsOnly(atMs: 3_000, lastConnectedVehicle: nil)
        XCTAssertEqual(state.currentMusicHistoryPolicy(), .disabled)
    }

    func testDeletingStoredMusicHistoryPreservesRide() throws {
        let state = MobileRideMapState()
        _ = try state.startGpsOnly(atMs: 1_000, lastConnectedVehicle: nil)
        try state.setMusicHistoryPolicy(.humanReadable)
        let snapshot = MobileMusicSnapshotDto(
            provider: .appleMusic,
            sessionId: "session",
            state: .playing,
            item: MobileMusicItemDto(
                identifier: "track-1",
                title: "Song",
                artist: "Artist"
            ),
            positionMilliseconds: nil,
            durationMilliseconds: nil,
            observedAtMs: 1_100,
            capabilities: MobileMusicCapabilitiesDto(
                previous: false,
                play: false,
                pause: true,
                next: true,
                openProvider: true
            )
        )
        XCTAssertEqual(
            try state.recordMusicEvent(
                snapshot: snapshot,
                kind: .play,
                monotonicAtMs: 1_100,
                wallClockAtMs: 1_700_000_000_000,
                clockUncertaintyMs: 5
            ),
            .recorded
        )

        let rideID = try state.stop(atMs: 2_000).rideID
        _ = try state.save()
        XCTAssertEqual(try state.storedMusicEvents(rideID: rideID).count, 1)

        try state.deleteMusicHistory(rideID: rideID)

        XCTAssertTrue(try state.storedMusicEvents(rideID: rideID).isEmpty)
        XCTAssertEqual(try state.storedHistoryRide(rideID: rideID)?.rideID, rideID)
    }

    func testHistoryContextOverviewBudgetIsBounded() {
        XCTAssertEqual(
            MobileRideMapHistoryContextBudget.overview,
            MobileRideMapHistoryContextBudget(
                historyPageLimit: 50,
                maxRoutes: 8,
                perRouteBudget: 512,
                totalPointBudget: 4_096
            )
        )
        XCTAssertLessThanOrEqual(
            MobileRideMapHistoryContextBudget.overview.perRouteBudget
                * MobileRideMapHistoryContextBudget.overview.maxRoutes,
            MobileRideMapHistoryContextBudget.overview.totalPointBudget
        )
    }

    func testHistoryContextProjectionStoresOnlyBoundedRouteProjections() {
        let route = MobileRideMapHistoryContextRoute(
            rideID: "ride-1",
            projection: MobileRideMapRouteProjection(
                points: [],
                segments: [],
                sourcePointCount: 2_000,
                sourceSegmentCount: 2,
                candidatePointCount: 512,
                candidateSegmentCount: 1,
                displayedSegmentCount: 1,
                backgroundGapCount: 0,
                canonicalStartSequence: 0,
                canonicalEndSequence: 1_999,
                canonicalStartVisible: false,
                canonicalEndVisible: false
            )
        )
        let context = MobileRideMapHistoryContextProjection(
            routes: [route],
            sourceHistoryRouteCount: 8,
            contextRouteCount: 1,
            totalDisplayPointCount: 512,
            routesOmittedByBudget: true,
            historyPageHasMore: true
        )

        XCTAssertEqual(context.routes.map(\.rideID), ["ride-1"])
        XCTAssertEqual(context.totalDisplayPointCount, 512)
        XCTAssertTrue(context.routesOmittedByBudget)
        XCTAssertTrue(context.historyPageHasMore)
    }

    func testHistoryContextProjectionExcludesSelectedRideAndUsesRustBudget() async throws {
        let state = MobileRideMapState()
        let fixtureStartMs: UInt64 = 4_000_000_000_000
        _ = try state.startGpsOnly(atMs: fixtureStartMs, lastConnectedVehicle: nil)
        _ = await settle(state, try state.ingestLocation(
            monotonicMs: fixtureStartMs + 1,
            wallClockUnixMs: 1_700_000_000_101,
            latitudeDegrees: 39.7392,
            longitudeDegrees: -104.9903,
            horizontalAccuracyMeters: 4
        ))
        _ = try state.stop(atMs: fixtureStartMs + 100)
        _ = try state.save()

        _ = try state.startGpsOnly(atMs: fixtureStartMs + 200, lastConnectedVehicle: nil)
        _ = await settle(state, try state.ingestLocation(
            monotonicMs: fixtureStartMs + 201,
            wallClockUnixMs: 1_700_000_000_301,
            latitudeDegrees: 39.7393,
            longitudeDegrees: -104.9902,
            horizontalAccuracyMeters: 4
        ))
        let selectedRideID = try state.stop(atMs: fixtureStartMs + 300).rideID
        _ = try state.save()

        let projection = try state.projectStoredHistoryContext(
            filter: MobileRideHistoryFilterDto(
                createdAfterMilliseconds: nil,
                vehicleIdentity: nil,
                searchText: nil
            ),
            selectedRideID: selectedRideID,
            budget: MobileRideMapHistoryContextBudget(
                historyPageLimit: 50,
                maxRoutes: 1,
                perRouteBudget: 1,
                totalPointBudget: 1
            )
        )

        XCTAssertLessThanOrEqual(projection.routes.count, 1)
        XCTAssertFalse(projection.routes.contains { $0.rideID == selectedRideID })
        XCTAssertGreaterThanOrEqual(projection.contextRouteCount, UInt64(projection.routes.count))
        XCTAssertLessThanOrEqual(projection.totalDisplayPointCount, 1)
    }

    func testHistoricalVehicleDisplayNameAndFilterOptionsComeFromRustDeviceTable() throws {
        guard let database = RustPersistenceStore.shared else {
            throw XCTSkip("Rust ride database is unavailable in this test environment")
        }
        let platformIdentifier = "corebluetooth-history-\(UUID().uuidString)"

        let rideID = try database.createRide(source: .live, createdAtMilliseconds: 1_000)
        _ = try database.transition(id: rideID, event: .start, monotonicAtMilliseconds: 1_000)
        try database.saveDeviceName(
            platformIdentifier: platformIdentifier,
            displayName: "NF2557",
            updatedAtMilliseconds: 1_001
        )
        try database.updateRideMapMetadata(
            id: rideID,
            candidateVehicle: nil,
            associatedVehicle: platformIdentifier,
            associatedAtMilliseconds: nil,
            lastTelemetryAtMilliseconds: nil
        )
        _ = try database.transition(id: rideID, event: .stop, monotonicAtMilliseconds: 2_000)
        _ = try database.transition(id: rideID, event: .save, monotonicAtMilliseconds: 2_000)

        let page = try database.listRidesFiltered(
            cursor: nil,
            filter: MobileRideHistoryFilterDto(
                createdAfterMilliseconds: nil,
                vehicleIdentity: platformIdentifier,
                searchText: nil
            ),
            limit: 50
        )
        XCTAssertEqual(page.rides.first?.associatedVehicleName, "NF2557")
        let options = try database.listRideHistoryVehicleOptions()
        XCTAssertTrue(options.contains(
            MobileRideHistoryVehicleOptionDto(
                platformIdentifier: platformIdentifier,
                displayName: "NF2557"
            )
        ))
    }

    // CutoutMobileTests and CutoutAppTests are separate test modules, so this
    // small durable-write waiter intentionally stays local to each target.
    private func settle(
        _ state: MobileRideMapState,
        _ decision: MobileRideMapDecisionDto
    ) async -> MobileRideMapDecisionDto {
        guard case .pending = decision else { return decision }
        let clock = ContinuousClock()
        let deadline = clock.now.advanced(by: .seconds(10))
        while clock.now < deadline, !Task.isCancelled {
            let outcomes = state.pollLocationWrites()
            if outcomes.count > 1 {
                XCTFail("one location admission produced multiple terminal outcomes")
            }
            if let terminal = outcomes.first {
                return terminal
            }
            try? await Task.sleep(for: .milliseconds(1))
        }
        XCTFail("timed out waiting for durable ride-map location outcome")
        return decision
    }

    func testStorageUnavailableStateCannotCreateAnInMemoryRide() {
        let state = MobileRideMapState(storageUnavailable: "database unavailable")

        XCTAssertEqual(state.initializationError, .storageError("database unavailable"))
        XCTAssertNil(state.currentSnapshot())
        XCTAssertEqual(state.pollLocationWrites(), [.storageError(message: "database unavailable")])
        XCTAssertThrowsError(
            try state.startGpsOnly(atMs: 100, lastConnectedVehicle: nil)
        ) { error in
            XCTAssertEqual(error as? MobileRideMapError, .storageError("database unavailable"))
        }
        XCTAssertThrowsError(
            try state.ingestLocation(
                monotonicMs: 100,
                wallClockUnixMs: 1_700_000_000_100,
                latitudeDegrees: 39.7392,
                longitudeDegrees: -104.9903,
                horizontalAccuracyMeters: 4
            )
        ) { error in
            XCTAssertEqual(error as? MobileRideMapError, .storageError("database unavailable"))
        }
        XCTAssertThrowsError(
            try state.storedHistoryPage(cursor: nil, limit: 10)
        ) { error in
            XCTAssertEqual(error as? MobileRideMapError, .storageError("database unavailable"))
        }
        XCTAssertThrowsError(
            try state.storedHistoryRide(rideID: "missing")
        ) { error in
            XCTAssertEqual(error as? MobileRideMapError, .storageError("database unavailable"))
        }
        XCTAssertThrowsError(
            try state.storedPointsAfter(rideId: "missing", afterCursor: nil, limit: 10)
        ) { error in
            XCTAssertEqual(error as? MobileRideMapError, .storageError("database unavailable"))
        }
        XCTAssertThrowsError(
            try state.deleteMusicHistory(rideID: "missing")
        ) { error in
            XCTAssertEqual(error as? MobileRideMapError, .storageError("database unavailable"))
        }
    }

    func testMapStateReportsTypedAdmissionReasons() throws {
        let state = MobileRideMapState()

        XCTAssertThrowsError(
            try state.ingestLocation(
                monotonicMs: 1,
                wallClockUnixMs: 1_700_000_000_001,
                latitudeDegrees: 39.7392,
                longitudeDegrees: -104.9903,
                horizontalAccuracyMeters: 4
            )
        ) { error in
            XCTAssertEqual(error as? MobileRideMapError, .noActiveRide)
        }

        _ = try state.startGpsOnly(atMs: 100, lastConnectedVehicle: nil)
        _ = try state.ingestLocation(
            monotonicMs: 100,
            wallClockUnixMs: 1_700_000_000_100,
            latitudeDegrees: 39.7392,
            longitudeDegrees: -104.9903,
            horizontalAccuracyMeters: 4
        )
        XCTAssertEqual(
            try state.ingestLocation(
                monotonicMs: 100,
                wallClockUnixMs: 1_700_000_000_100,
                latitudeDegrees: 39.7392,
                longitudeDegrees: -104.9903,
                horizontalAccuracyMeters: 4
            ),
            .ignored(reason: .duplicateLocation)
        )
        XCTAssertEqual(
            try state.ingestLocation(
                monotonicMs: 99,
                wallClockUnixMs: 1_700_000_000_099,
                latitudeDegrees: 39.7393,
                longitudeDegrees: -104.9902,
                horizontalAccuracyMeters: 4
            ),
            .rejected(reason: .timestampOutOfOrder)
        )
        XCTAssertEqual(
            try state.ingestLocation(
                monotonicMs: 102,
                wallClockUnixMs: 1_700_000_000_102,
                latitudeDegrees: 39.7393,
                longitudeDegrees: -104.9902,
                horizontalAccuracyMeters: 100_001
            ),
            .rejected(reason: .accuracyTooLow)
        )
    }

    func testMapStateKeepsLifecycleAndRouteProjectionTyped() async throws {
        let state = MobileRideMapState()

        _ = try state.startGpsOnly(atMs: 100, lastConnectedVehicle: "pev-1")
        XCTAssertEqual(
            state.currentSnapshot(atMs: 1_100)?.summary.durationMilliseconds,
            1_000
        )
        let decision = await settle(state, try state.ingestLocation(
            monotonicMs: 100,
            wallClockUnixMs: 1_700_000_000_100,
            latitudeDegrees: 39.7392,
            longitudeDegrees: -104.9903,
            horizontalAccuracyMeters: 4
        ))
        XCTAssertEqual(try state.observeVehicleConnection(platformIdentifier: "pev-1", atMs: 200), .associated)

        guard case let .accepted(point, _) = decision else {
            return XCTFail("expected the location to be admitted")
        }
        let secondDecision = await settle(state, try state.ingestLocation(
            monotonicMs: 2_000,
            wallClockUnixMs: 1_700_000_002_000,
            latitudeDegrees: 39.7393,
            longitudeDegrees: -104.9902,
            horizontalAccuracyMeters: 4
        ))
        XCTAssertEqual(
            secondDecision,
            .accepted(
                point: MobileRideMapPointDto(
                    sequence: 1,
                    segmentId: 0,
                    startReason: .initial,
                    latitudeDegrees: 39.7393,
                    longitudeDegrees: -104.9902,
                    wallClockUnixMs: 1_700_000_002_000,
                    monotonicMs: 2_000,
                    horizontalAccuracyMeters: 4,
                    telemetryState: .associatedNoTelemetry
                ),
                segmentStarted: false
            )
        )
        let stopped = try state.stop(atMs: 2_000)
        XCTAssertEqual(stopped.state, .stopped)
        XCTAssertEqual(point.sequence, 0)
        XCTAssertEqual(stopped.summary.pointCount, 2)
        let firstBatch = try state.pointsAfter(afterCursor: nil, limit: 1)
        XCTAssertEqual(firstBatch.points.count, 1)
        XCTAssertEqual(firstBatch.nextCursor, 0)
        XCTAssertTrue(firstBatch.hasMore)
        XCTAssertEqual(firstBatch.points.first?.startReason, .initial)
        XCTAssertEqual(
            try state.pointsAfter(afterCursor: firstBatch.nextCursor, limit: 10).points.map(\.sequence),
            [1]
        )
        XCTAssertEqual(
            try state.pointsAfter(
                afterCursor: firstBatch.nextCursor,
                limit: 10
            ).points.first?.startReason,
            .initial
        )
        XCTAssertEqual(try state.save().state, .saved)
    }

    func testMapStateProjectsBoundedLiveRouteAndRejectsInvalidBudget() async throws {
        let state = MobileRideMapState()
        _ = try state.startGpsOnly(atMs: 1_000, lastConnectedVehicle: nil)
        for (monotonicMs, latitudeDegrees) in [
            (1_001, 40.0),
            (2_001, 40.0001),
            (3_001, 40.0002),
            (4_001, 40.0003),
        ] {
            _ = await settle(state, try state.ingestLocation(
                monotonicMs: UInt64(monotonicMs),
                wallClockUnixMs: 1_700_000_000_000 + UInt64(monotonicMs),
                latitudeDegrees: latitudeDegrees,
                longitudeDegrees: -105.0,
                horizontalAccuracyMeters: 3
            ))
        }

        let projection = try state.projectPoints(
            budget: 2,
            viewport: MobileGeoBoundsDto(
                minimumLatitudeDegrees: 40.0,
                maximumLatitudeDegrees: 40.0002,
                minimumLongitudeDegrees: -105.0,
                maximumLongitudeDegrees: -105.0
            ),
            privacy: .grid(e7: 1_000)
        )
        XCTAssertEqual(projection.sourcePointCount, 4)
        XCTAssertEqual(projection.sourceSegmentCount, 1)
        XCTAssertEqual(projection.candidatePointCount, 3)
        XCTAssertEqual(projection.candidateSegmentCount, 1)
        XCTAssertEqual(projection.displayedSegmentCount, 1)
        XCTAssertTrue(projection.pointsOmittedByBudget)
        XCTAssertFalse(projection.segmentsOmittedByBudget)
        XCTAssertEqual(projection.points.map(\.sequence), [0, 2])
        XCTAssertEqual(projection.points.map(\.privacyClass), [.gridRedacted, .gridRedacted])
        XCTAssertEqual(projection.points.map(\.latitudeDegrees), [40.0, 40.0002])
        XCTAssertEqual(projection.backgroundGapCount, 0)

        XCTAssertThrowsError(try state.projectPoints(budget: 0)) { error in
            XCTAssertEqual(error as? MobileRideMapError, .invalidRouteProjection)
        }
    }

    func testMapStateProjectsCanonicalBackgroundGapCount() async throws {
        let state = MobileRideMapState()
        _ = try state.startGpsOnly(atMs: 1_000, lastConnectedVehicle: nil)

        for (monotonicMs, latitudeDegrees) in [
            (1_001, 40.0),
            (40_001, 40.0001),
        ] {
            _ = await settle(state, try state.ingestLocation(
                monotonicMs: UInt64(monotonicMs),
                wallClockUnixMs: 1_700_000_000_000 + UInt64(monotonicMs),
                latitudeDegrees: latitudeDegrees,
                longitudeDegrees: -105.0,
                horizontalAccuracyMeters: 3
            ))
        }

        let projection = try state.projectPoints(budget: 8)
        XCTAssertEqual(projection.backgroundGapCount, 1)
        XCTAssertEqual(projection.sourceSegmentCount, 2)
        XCTAssertEqual(projection.segments.map(\.startReason), [.initial, .backgroundGap])
    }

    func testLiveProjectionCancellationIsTypedAndLeavesCompatibilityPathUsable() async throws {
        let state = MobileRideMapState()
        _ = try state.startGpsOnly(atMs: 1_000, lastConnectedVehicle: nil)
        _ = await settle(state, try state.ingestLocation(
            monotonicMs: 1_001,
            wallClockUnixMs: 1_700_000_001_001,
            latitudeDegrees: 40,
            longitudeDegrees: -105,
            horizontalAccuracyMeters: 3
        ))

        let cancellation = MobileLiveRideMapProjectionCancellation()
        cancellation.cancel()
        XCTAssertThrowsError(
            try state.projectPoints(budget: 1, cancellation: cancellation)
        ) { error in
            XCTAssertEqual(error as? MobileRideMapError, .cancelled)
        }
        XCTAssertEqual(try state.projectPoints(budget: 1).points.count, 1)
        _ = try state.discard()
    }

    func testStoredProjectionHonorsRustCancellationToken() async throws {
        let state = MobileRideMapState()
        _ = try state.startGpsOnly(atMs: 1_000, lastConnectedVehicle: nil)
        _ = await settle(state, try state.ingestLocation(
            monotonicMs: 1_001,
            wallClockUnixMs: 1_700_000_001_001,
            latitudeDegrees: 40,
            longitudeDegrees: -105,
            horizontalAccuracyMeters: 3
        ))
        let rideID = try state.stop(atMs: 2_000).rideID
        _ = try state.save()

        let storedPage = try state.storedPointsAfter(rideId: rideID, afterCursor: nil, limit: 10)
        XCTAssertEqual(storedPage.points.first?.startReason, .initial)

        let cancellation = MobileRideMapProjectionCancellation()
        cancellation.cancel()
        XCTAssertThrowsError(
            try state.projectStoredPoints(
                rideID: rideID,
                budget: 1,
                cancellation: cancellation
            )
        ) { error in
            XCTAssertEqual(error as? MobileRideMapError, .cancelled)
        }
    }

    func testStoredProjectionUsesRustBoundedRouteContract() async throws {
        let state = MobileRideMapState()
        _ = try state.startGpsOnly(atMs: 1_000, lastConnectedVehicle: nil)

        for (offset, latitudeDegrees) in [
            (1_001, 40.0),
            (2_001, 40.0001),
            (3_001, 40.0002),
            (4_001, 40.0003),
        ] {
            _ = await settle(state, try state.ingestLocation(
                monotonicMs: UInt64(offset),
                wallClockUnixMs: 1_700_000_000_000 + UInt64(offset),
                latitudeDegrees: latitudeDegrees,
                longitudeDegrees: -105.0,
                horizontalAccuracyMeters: 3
            ))
        }

        let rideID = try state.stop(atMs: 4_001).rideID
        _ = try state.save()

        let projection = try state.projectStoredPoints(rideID: rideID, budget: 2)

        XCTAssertEqual(projection.sourcePointCount, 4)
        XCTAssertLessThanOrEqual(projection.points.count, 2)
        XCTAssertEqual(projection.points.first?.sequence, 0)
        XCTAssertEqual(projection.points.last?.sequence, 3)
        XCTAssertEqual(projection.canonicalStartSequence, 0)
        XCTAssertEqual(projection.canonicalEndSequence, 3)
        XCTAssertTrue(projection.pointsOmittedByBudget)
    }

    func testMapStatePointsAfterReturnsTheCompleteRustPagedSequence() async throws {
        let state = MobileRideMapState()
        _ = try state.startGpsOnly(atMs: 1_000, lastConnectedVehicle: nil)
        for offset in 0 ..< 4_097 {
            let monotonicMs = UInt64(1_001 + offset)
            _ = await settle(state, try state.ingestLocation(
                monotonicMs: monotonicMs,
                wallClockUnixMs: 1_700_000_000_000 + monotonicMs,
                latitudeDegrees: 40.0,
                longitudeDegrees: -105.0,
                horizontalAccuracyMeters: 3
            ))
        }

        var points: [MobileRideMapPointDto] = []
        var cursor: UInt64?
        var hasMore = true
        for _ in 0 ..< 9 {
            let page = try state.pointsAfter(afterCursor: cursor, limit: 500)
            points.append(contentsOf: page.points)
            guard page.hasMore else {
                hasMore = false
                break
            }
            guard let nextCursor = page.nextCursor, nextCursor > (cursor ?? 0) else {
                XCTFail("a paged response with more data must advance its cursor")
                hasMore = false
                break
            }
            cursor = nextCursor
        }

        XCTAssertFalse(hasMore, "the expected 4,097 points must fit in nine 500-point pages")
        XCTAssertEqual(points.map(\.sequence), Array(0 ... 4_096))
        XCTAssertEqual(points.first?.startReason, .initial)
    }

}
