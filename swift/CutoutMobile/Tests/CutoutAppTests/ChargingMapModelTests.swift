import CutoutMobile
import CutoutMobileFFI
import MapKit
import XCTest

@testable import CutoutApp

@MainActor
final class ChargingMapModelTests: XCTestCase {
    func testPaginationDeduplicatesAndKeepsSelectionCurrent() async {
        let model = ChargingMapModel(
            queryPage: { _, _, cursor, _ in
                if cursor == nil {
                    return Self.page([Self.spot("one", revision: 1)], cursor: "next")
                }
                return Self.page([Self.spot("one", revision: 2), Self.spot("two")], cursor: nil)
            },
            readSpot: { _ in nil },
            writeVote: { _, _, _ in }
        )
        model.setViewport(bounds())
        await model.load()
        model.select(Self.spot("one", revision: 1))
        await model.loadMore()

        XCTAssertEqual(model.spots.map(\.id), ["one", "two"])
        XCTAssertEqual(model.selectedSpot?.localRevision, 2)
        XCTAssertFalse(model.hasMore)
    }

    func testSupersededViewportCannotReplaceNewResults() async {
        let gate = ChargingPageGate()
        let model = ChargingMapModel(
            queryPage: { bounds, _, _, _ in
                if bounds.minimumLatitudeDegrees == 1 { return await gate.wait() }
                return Self.page([Self.spot("new")], cursor: nil)
            },
            readSpot: { _ in nil },
            writeVote: { _, _, _ in }
        )
        model.setViewport(bounds(latitude: 1))
        let oldLoad = Task { await model.load() }
        await gate.waitUntilStarted()

        model.setViewport(bounds(latitude: 2))
        await model.load()
        await gate.release(Self.page([Self.spot("old")], cursor: nil))
        await oldLoad.value

        XCTAssertEqual(model.spots.map(\.id), ["new"])
    }

    func testVoteGateAndIdentityPreserveAuthoritativeCounts() async {
        let gate = ChargingVoteGate()
        let original = Self.spot("one")
        let refreshed = Self.spot("one", revision: 2, pendingVote: true, votesUp: 7)
        let model = ChargingMapModel(
            queryPage: { _, _, _, _ in Self.page([original], cursor: nil) },
            readSpot: { _ in refreshed },
            writeVote: { _, _, _ in await gate.waitForWrite() }
        )
        model.setViewport(bounds())
        await model.load()
        model.select(original)

        let vote = Task { await model.vote(.up) }
        await gate.waitUntilStarted()
        XCTAssertTrue(model.pendingVoteIDs.contains("one"))
        await model.vote(.down)
        model.select(Self.spot("two"))
        await gate.release()
        await vote.value

        XCTAssertEqual(model.selectedSpot?.id, "two")
        XCTAssertEqual(model.spots.first?.votesUp, 7)
        XCTAssertTrue(model.spots.first?.pendingVote == true)
        XCTAssertFalse(model.pendingVoteIDs.contains("one"))
    }

    func testNewerSpotRevisionWinsOverDelayedVoteReadback() async {
        let voteWrite = ChargingVoteGate()
        let oldReadbackGate = ChargingSpotReadGate()
        let original = Self.spot("one", revision: 1, votesUp: 4)
        let oldVoteReadback = Self.spot("one", revision: 2, votesUp: 5)
        let newerSpot = Self.spot("one", revision: 3, votesUp: 6)
        let model = ChargingMapModel(
            queryPage: { bounds, _, _, _ in
                Self.page([bounds.minimumLatitudeDegrees == 0 ? original : newerSpot], cursor: nil)
            },
            readSpot: { _ in await oldReadbackGate.wait() },
            writeVote: { _, _, _ in await voteWrite.waitForWrite() }
        )
        model.setViewport(bounds())
        await model.load()
        model.select(original)

        let vote = Task { await model.vote(.up) }
        await voteWrite.waitUntilStarted()
        await voteWrite.release()
        await oldReadbackGate.waitUntilStarted()

        model.setViewport(bounds(latitude: 1))
        await model.load()
        XCTAssertEqual(model.selectedSpot?.localRevision, 3)
        await oldReadbackGate.release(oldVoteReadback)
        await vote.value

        XCTAssertEqual(model.spots.first?.localRevision, 3)
        XCTAssertEqual(model.spots.first?.votesUp, 6)
        XCTAssertEqual(model.selectedSpot?.localRevision, 3)
        XCTAssertFalse(model.pendingVoteIDs.contains("one"))
    }

    func testVoteFailureClearsPendingState() async {
        let original = Self.spot("one")
        let model = ChargingMapModel(
            queryPage: { _, _, _, _ in Self.page([original], cursor: nil) },
            readSpot: { _ in nil },
            writeVote: { _, _, _ in throw MobileChargingError.StorageFailure }
        )
        model.setViewport(bounds())
        await model.load()
        model.select(original)
        await model.vote(.up)

        XCTAssertFalse(model.pendingVoteIDs.contains("one"))
        XCTAssertEqual(model.voteError, .StorageFailure)
    }

    func testNonvisibleVoteReadbackRemovesPageRowAndRetainsSelectedStatus() async {
        for status: MobileChargingStatusDto in [.hidden, .withdrawn, .removed] {
            let original = Self.spot("one", coordinate: .init(latitudeDegrees: 0.5, longitudeDegrees: 0.5))
            let tombstone = Self.spot("one", revision: 2, status: status)
            let model = ChargingMapModel(
                queryPage: { _, _, _, _ in Self.page([original], cursor: nil) },
                readSpot: { _ in tombstone },
                writeVote: { _, _, _ in }
            )
            model.setViewport(bounds())
            await model.load()
            model.select(original)
            await model.vote(.up)

            XCTAssertTrue(model.spots.isEmpty, "\(status) must not remain in page membership")
            XCTAssertEqual(model.selectedSpot, tombstone)
            XCTAssertTrue(model.pendingVoteIDs.isEmpty)
            XCTAssertNil(model.voteError)
        }
    }

    func testFilterChangeClearsResultsAndSelection() async {
        let model = ChargingMapModel(
            queryPage: { _, _, _, _ in Self.page([Self.spot("one")], cursor: nil) },
            readSpot: { _ in nil },
            writeVote: { _, _, _ in }
        )
        model.setViewport(bounds())
        await model.load()
        model.select(Self.spot("one"))
        model.setFilter(MobileChargingFilterDto(cost: .paid, connector: nil, voltageVolts: nil, venue: nil))

        XCTAssertTrue(model.spots.isEmpty)
        XCTAssertNil(model.selectedSpot)
        XCTAssertEqual(model.queryID, 2)
    }

    func testClearingMapSelectionPreservesResultsAndFilter() async {
        let original = Self.spot("one")
        let filter = MobileChargingFilterDto(cost: .free, connector: nil, voltageVolts: nil, venue: nil)
        let model = ChargingMapModel(
            filter: filter,
            queryPage: { _, _, _, _ in Self.page([original], cursor: nil) },
            readSpot: { _ in nil },
            writeVote: { _, _, _ in }
        )
        model.setViewport(bounds())
        await model.load()
        let queryID = model.queryID
        model.select(id: original.id)
        XCTAssertEqual(model.selectedSpot, original)
        model.select(id: nil)

        XCTAssertNil(model.selectedSpot)
        XCTAssertEqual(model.spots, [original])
        XCTAssertEqual(model.filter, filter)
        XCTAssertEqual(model.queryID, queryID)
    }

    func testViewportFailureKeepsVisibleSpots() async {
        let model = ChargingMapModel(
            queryPage: { requestBounds, _, _, _ in
                if requestBounds.minimumLatitudeDegrees == 2 { throw MobileChargingError.StorageFailure }
                return Self.page([Self.spot("visible")], cursor: nil)
            },
            readSpot: { _ in nil },
            writeVote: { _, _, _ in }
        )
        model.setViewport(bounds(latitude: 1))
        await model.load()
        model.setViewport(bounds(latitude: 2))
        await model.load()

        XCTAssertEqual(model.spots.map(\.id), ["visible"])
        XCTAssertEqual(model.error, .StorageFailure)
    }

    func testResultLimitStopsAtFiveHundred() async {
        let values = (0..<501).map { Self.spot("spot-\($0)") }
        let model = ChargingMapModel(
            queryPage: { _, _, _, _ in Self.page(values, cursor: "more") },
            readSpot: { _ in nil },
            writeVote: { _, _, _ in }
        )
        model.setViewport(bounds())
        await model.load()

        XCTAssertEqual(model.spots.count, 500)
        XCTAssertTrue(model.limitReached)
        XCTAssertFalse(model.hasMore)
    }

    func testQueryStartedBeforeVoteCannotOverwriteVoteReadback() async {
        let pageGate = ChargingPageGate()
        let voteGate = ChargingVoteGate()
        let oldPageSpot = Self.spot("one", revision: 1, votesUp: 4)
        let voteReadback = Self.spot("one", revision: 2, pendingVote: true, votesUp: 5)
        let model = ChargingMapModel(
            queryPage: { _, _, _, _ in await pageGate.wait() },
            readSpot: { _ in voteReadback },
            writeVote: { _, _, _ in await voteGate.waitForWrite() }
        )
        model.setViewport(bounds())
        let query = Task { await model.load() }
        await pageGate.waitUntilStarted()
        model.select(oldPageSpot)
        let vote = Task { await model.vote(.up) }
        await voteGate.waitUntilStarted()
        await voteGate.release()
        await vote.value
        await pageGate.release(Self.page([oldPageSpot], cursor: nil))
        await query.value

        XCTAssertEqual(model.spots.first?.votesUp, 5)
        XCTAssertEqual(model.selectedSpot?.votesUp, 5)
    }

    func testCancelledViewLoadClearsLoadingWithoutStorageError() async {
        let gate = ChargingPageGate()
        let model = ChargingMapModel(
            queryPage: { _, _, _, _ in await gate.wait() },
            readSpot: { _ in nil },
            writeVote: { _, _, _ in }
        )
        model.setViewport(bounds())
        let load = Task { await model.load() }
        await gate.waitUntilStarted()
        load.cancel()
        await gate.release(Self.page([Self.spot("cancelled")], cursor: nil))
        await load.value
        XCTAssertFalse(model.isLoading)
        XCTAssertNil(model.error)
        XCTAssertTrue(model.spots.isEmpty)
    }

    func testCancelledViewLoadDropsLateStorageFailure() async {
        let gate = ChargingPageGate()
        let model = ChargingMapModel(
            queryPage: { _, _, _, _ in
                _ = await gate.wait()
                throw MobileChargingError.StorageFailure
            },
            readSpot: { _ in nil },
            writeVote: { _, _, _ in }
        )
        model.setViewport(bounds())
        let load = Task { await model.load() }
        await gate.waitUntilStarted()
        load.cancel()
        await gate.release(Self.page([], cursor: nil))
        await load.value
        XCTAssertFalse(model.isLoading)
        XCTAssertNil(model.error)
    }

    func testOldPinControlCannotVoteOnNewSelection() async {
        let model = ChargingMapModel(
            queryPage: { _, _, _, _ in Self.page([], cursor: nil) },
            readSpot: { _ in nil },
            writeVote: { _, _, _ in throw MobileChargingError.Conflict }
        )
        model.select(Self.spot("new"))
        await model.vote(.up, forSpotID: "old")
        XCTAssertNil(model.voteError)
        XCTAssertTrue(model.pendingVoteIDs.isEmpty)
        XCTAssertEqual(model.selectedSpot?.id, "new")
    }

    func testNearbyFallbackWaitsForRealLocationAndReturnsNearestTargetOnce() async {
        let gate = ChargingNearbyGate()
        let calls = ChargingNearbyCallCounter()
        let filter = MobileChargingFilterDto(cost: .paid, connector: nil, voltageVolts: nil, venue: nil)
        let model = ChargingMapModel(
            filter: filter,
            queryPage: { _, _, _, _ in Self.page([], cursor: nil) },
            readSpot: { _ in nil },
            nearbySpots: { metro, origin, filter, limit in
                _ = await calls.record(metro: metro, origin: origin, filter: filter, limit: limit)
                return await gate.wait()
            },
            writeVote: { _, _, _ in }
        )
        model.setViewport(bounds())
        await model.load()
        let beforeLocation = await calls.values()
        XCTAssertTrue(beforeLocation.isEmpty)

        let origin = MobileMapCoordinateDto(latitudeDegrees: 39.74, longitudeDegrees: -104.99)
        model.setUserLocation(origin)
        await gate.waitUntilStarted()
        let observed = await calls.values()
        XCTAssertEqual(observed.count, 1)
        XCTAssertEqual(observed[0].origin, origin)
        XCTAssertEqual(observed[0].filter, filter)
        XCTAssertEqual(observed[0].limit, 1)
        let expectedLatitudeSpan = 100_000.0 / 6_371_000.0 * 180.0 / Double.pi
        XCTAssertEqual(
            observed[0].metro.maximumLatitudeDegrees - observed[0].metro.minimumLatitudeDegrees,
            expectedLatitudeSpan,
            accuracy: 0.01
        )
        let expectedLongitudeSpan = expectedLatitudeSpan / cos(39.74 * Double.pi / 180.0)
        let longitudeSpan =
            observed[0].metro.minimumLongitudeDegrees
                > observed[0].metro.maximumLongitudeDegrees
            ? 360 - observed[0].metro.minimumLongitudeDegrees
                + observed[0].metro.maximumLongitudeDegrees
            : observed[0].metro.maximumLongitudeDegrees
                - observed[0].metro.minimumLongitudeDegrees
        XCTAssertEqual(longitudeSpan, expectedLongitudeSpan, accuracy: 0.02)
        XCTAssertGreaterThan(observed[0].metro.minimumLongitudeDegrees, -106)
        XCTAssertLessThan(observed[0].metro.maximumLongitudeDegrees, -104)
        XCTAssertEqual(model.userLocation, origin)

        let target = MobileMapCoordinateDto(latitudeDegrees: 39.75, longitudeDegrees: -104.98)
        await gate.release([Self.spot("nearby", coordinate: target)])
        await waitForTarget(on: model)
        XCTAssertEqual(model.discoveryTarget, target)
        model.setUserLocation(origin)
        await Task.yield()
        let afterSettled = await calls.values()
        XCTAssertEqual(afterSettled.count, 1)
        model.cancelAutomaticDiscovery()
        XCTAssertNil(model.discoveryTarget)
    }

    func testNearbyBoundsPreserveAntimeridianCrossing() async {
        let calls = ChargingNearbyCallCounter()
        let origin = MobileMapCoordinateDto(latitudeDegrees: 0, longitudeDegrees: 179.9)
        let model = ChargingMapModel(
            queryPage: { _, _, _, _ in Self.page([], cursor: nil) },
            readSpot: { _ in nil },
            nearbySpots: { metro, coordinate, filter, limit in
                _ = await calls.record(metro: metro, origin: coordinate, filter: filter, limit: limit)
                return []
            },
            writeVote: { _, _, _ in }
        )
        model.setUserLocation(origin)
        model.setViewport(bounds())
        await model.load()
        await calls.waitForCall()

        let recorded = await calls.values()
        XCTAssertEqual(recorded.count, 1)
        XCTAssertGreaterThan(recorded[0].metro.minimumLongitudeDegrees, recorded[0].metro.maximumLongitudeDegrees)
        XCTAssertGreaterThan(recorded[0].metro.minimumLongitudeDegrees, 179)
        XCTAssertLessThan(recorded[0].metro.maximumLongitudeDegrees, -179)
    }

    func testNearbyFallbackDoesNotRunWhenInitialViewportHasPins() async {
        let calls = ChargingNearbyCallCounter()
        let model = ChargingMapModel(
            queryPage: { _, _, _, _ in Self.page([Self.spot("local")], cursor: nil) },
            readSpot: { _ in nil },
            nearbySpots: { metro, origin, filter, limit in
                _ = await calls.record(metro: metro, origin: origin, filter: filter, limit: limit)
                return []
            },
            writeVote: { _, _, _ in }
        )
        model.setUserLocation(MobileMapCoordinateDto(latitudeDegrees: 39, longitudeDegrees: -105))
        model.setViewport(bounds())
        await model.load()

        let recorded = await calls.values()
        XCTAssertTrue(recorded.isEmpty)
        XCTAssertNil(model.discoveryTarget)
    }

    func testEmptyNearbyResultSettlesWithoutRetrying() async {
        let calls = ChargingNearbyCallCounter()
        let model = ChargingMapModel(
            queryPage: { _, _, _, _ in Self.page([], cursor: nil) },
            readSpot: { _ in nil },
            nearbySpots: { metro, origin, filter, limit in
                _ = await calls.record(metro: metro, origin: origin, filter: filter, limit: limit)
                return []
            },
            writeVote: { _, _, _ in }
        )
        model.setUserLocation(MobileMapCoordinateDto(latitudeDegrees: 39, longitudeDegrees: -105))
        model.setViewport(bounds())
        await model.load()
        await calls.waitForCall()
        await waitForNearbyToFinish(model)
        model.setUserLocation(MobileMapCoordinateDto(latitudeDegrees: 39.1, longitudeDegrees: -105))
        await Task.yield()

        let recorded = await calls.values()
        XCTAssertEqual(recorded.count, 1)
        XCTAssertNil(model.discoveryTarget)
    }

    func testDownloadedCommunityPinsRearmEmptyInitialDiscovery() async {
        let calls = ChargingNearbyCallCounter()
        let blockedLoads = ChargingBlockedOperationsReadSignal()
        let pages = ChargingTestPageSequence([
            Self.page([], cursor: nil),
            Self.page([], cursor: nil),
        ])
        let origin = MobileMapCoordinateDto(latitudeDegrees: 39.74, longitudeDegrees: -104.99)
        let distant = MobileMapCoordinateDto(latitudeDegrees: 39.80, longitudeDegrees: -105.04)
        let model = ChargingMapModel(
            queryPage: { _, _, _, _ in await pages.next() },
            readSpot: { _ in nil },
            nearbySpots: { metro, coordinate, filter, limit in
                let call = await calls.record(
                    metro: metro, origin: coordinate, filter: filter, limit: limit
                )
                return call == 1 ? [] : [Self.spot("remote", revision: 2, coordinate: distant)]
            },
            writeVote: { _, _, _ in },
            queryBlockedOperations: { await blockedLoads.read() },
            communitySync: Self.downloadedCommunitySync()
        )
        model.setUserLocation(origin)
        model.setViewport(bounds())
        await model.load()
        await calls.waitForCall()
        await waitForNearbyToFinish(model)
        model.communitySyncRequested()
        await blockedLoads.waitForCallCount(1)
        await waitForTarget(on: model)

        XCTAssertEqual(model.discoveryTarget, distant)
        let nearbyCalls = await calls.values()
        XCTAssertEqual(nearbyCalls.count, 2)
    }

    func testSyncRefreshUpdatesSelectedSpotFromFreshLocalRecord() async {
        let blockedLoads = ChargingBlockedOperationsReadSignal()
        let updated = Self.spot("selected", revision: 2)
        let pages = ChargingTestPageSequence([
            Self.page([Self.spot("selected", revision: 1)], cursor: nil),
            Self.page([], cursor: nil),
        ])
        let model = ChargingMapModel(
            queryPage: { _, _, _, _ in await pages.next() },
            readSpot: { _ in updated },
            nearbySpots: { _, _, _, _ in [] },
            writeVote: { _, _, _ in },
            queryBlockedOperations: { await blockedLoads.read() },
            communitySync: Self.downloadedCommunitySync()
        )
        model.setViewport(bounds())
        await model.load()
        model.select(Self.spot("selected", revision: 1))

        model.communitySyncRequested()
        await blockedLoads.waitForCallCount(1)

        XCTAssertEqual(model.selectedSpot?.id, "selected")
        XCTAssertEqual(model.selectedSpot?.localRevision, 2)
        XCTAssertTrue(model.spots.isEmpty)
    }

    func testSyncRefreshKeepsRemoteTombstoneInSelectedDetailAndRemovesMapPin() async {
        let pages = ChargingTestPageSequence([
            Self.page([Self.spot("selected", revision: 1)], cursor: nil),
            Self.page([], cursor: nil),
        ])
        let readGate = ChargingSpotReadGate()
        let blockedLoads = ChargingBlockedOperationsReadSignal()
        let removed = Self.spot("selected", revision: 2, status: .removed)
        let model = ChargingMapModel(
            queryPage: { _, _, _, _ in await pages.next() },
            readSpot: { _ in await readGate.wait() },
            writeVote: { _, _, _ in },
            queryBlockedOperations: { await blockedLoads.read() },
            communitySync: Self.downloadedCommunitySync()
        )
        model.setViewport(bounds())
        await model.load()
        model.select(Self.spot("selected", revision: 1))

        model.communitySyncRequested()
        await readGate.waitUntilStarted()
        await readGate.release(removed)
        await blockedLoads.waitForCallCount(1)

        XCTAssertEqual(model.selectedSpot?.id, "selected")
        XCTAssertEqual(model.selectedSpot?.status, .removed)
        XCTAssertTrue(model.spots.isEmpty)
    }

    func testLateLocationUsesCacheUpdatedDuringInitialWait() async {
        let calls = ChargingNearbyCallCounter()
        let blockedLoads = ChargingBlockedOperationsReadSignal()
        let pages = ChargingTestPageSequence([
            Self.page([], cursor: nil),
            Self.page([], cursor: nil),
        ])
        let distant = MobileMapCoordinateDto(latitudeDegrees: 39.80, longitudeDegrees: -105.04)
        let model = ChargingMapModel(
            queryPage: { _, _, _, _ in await pages.next() },
            readSpot: { _ in nil },
            nearbySpots: { metro, origin, filter, limit in
                _ = await calls.record(metro: metro, origin: origin, filter: filter, limit: limit)
                return [Self.spot("remote", coordinate: distant)]
            },
            writeVote: { _, _, _ in },
            queryBlockedOperations: { await blockedLoads.read() },
            communitySync: Self.downloadedCommunitySync()
        )
        model.setViewport(bounds())
        await model.load()
        let initialCalls = await calls.values()
        XCTAssertTrue(initialCalls.isEmpty)

        model.communitySyncRequested()
        await blockedLoads.waitForCallCount(1)
        model.setUserLocation(MobileMapCoordinateDto(latitudeDegrees: 39.74, longitudeDegrees: -104.99))
        await waitForTarget(on: model)

        XCTAssertEqual(model.discoveryTarget, distant)
        let nearbyCalls = await calls.values()
        XCTAssertEqual(nearbyCalls.count, 1)
    }

    func testUserCancelledEmptyDiscoveryIsNotRearmedByDownload() async {
        let calls = ChargingNearbyCallCounter()
        let blockedLoads = ChargingBlockedOperationsReadSignal()
        let pages = ChargingTestPageSequence([
            Self.page([], cursor: nil),
            Self.page([], cursor: nil),
        ])
        let model = ChargingMapModel(
            queryPage: { _, _, _, _ in await pages.next() },
            readSpot: { _ in nil },
            nearbySpots: { metro, origin, filter, limit in
                _ = await calls.record(metro: metro, origin: origin, filter: filter, limit: limit)
                return []
            },
            writeVote: { _, _, _ in },
            queryBlockedOperations: { await blockedLoads.read() },
            communitySync: Self.downloadedCommunitySync()
        )
        model.setUserLocation(MobileMapCoordinateDto(latitudeDegrees: 39.74, longitudeDegrees: -104.99))
        model.setViewport(bounds())
        await model.load()
        await calls.waitForCall()
        await waitForNearbyToFinish(model)
        model.cancelAutomaticDiscovery()

        model.communitySyncRequested()
        await blockedLoads.waitForCallCount(1)

        let nearbyCalls = await calls.values()
        XCTAssertEqual(nearbyCalls.count, 1)
        XCTAssertNil(model.discoveryTarget)
    }

    func testForegroundSyncOutlivesCancelledBackgroundRun() async {
        let gate = ChargingSyncRunGate()
        let queries = ChargingPageQueryCounter()
        let sync = ChargingCommunitySync(
            credentialStore: ChargingModelCredentialStore(
                initial: .ready(.init(identityID: "identity", token: "token"))
            ),
            registerIdentity: { .init(identityID: "identity", token: "token") },
            synchronize: { _ in try await gate.run() },
            sleep: { _ in },
            enabled: { true }
        )
        let model = ChargingMapModel(
            queryPage: { _, _, _, _ in
                await queries.record()
                return Self.page([], cursor: nil)
            },
            readSpot: { _ in nil },
            writeVote: { _, _, _ in },
            communitySync: sync
        )
        model.setViewport(bounds())
        await model.load()

        model.communitySyncRequested()
        await gate.waitForCallCount(1)
        model.applicationDidEnterBackground()
        model.applicationDidBecomeActive()
        await gate.waitForCallCount(2)

        await gate.release(1, counts: .init(pendingCount: 0, downloaded: 1, uploaded: 0, outcome: .synced))
        await Task.yield()
        await Task.yield()
        let callsAfterCancelledRun = await gate.callCount()
        XCTAssertEqual(callsAfterCancelledRun, 2)

        model.communitySyncRequested()
        await gate.release(2, counts: .init(pendingCount: 0, downloaded: 2, uploaded: 0, outcome: .synced))
        await gate.waitForCallCount(3)
        for _ in 0..<100 where model.communitySyncDownloaded != 3 {
            await Task.yield()
        }
        for _ in 0..<100 {
            let count = await queries.count()
            if count == 3 { break }
            await Task.yield()
        }

        let finalCallCount = await gate.callCount()
        XCTAssertEqual(finalCallCount, 3)
        XCTAssertEqual(model.communitySyncDownloaded, 3)
        let queryCount = await queries.count()
        XCTAssertEqual(queryCount, 3)
    }

    func testPendingSyncSurfacesDurableConflictAndRejectionStatuses() async {
        for reason in [MobileChargingBlockReasonDto.conflict, .invalid] {
            let blocked = MobileChargingSyncConflictDto(
                operationId: "operation",
                action: .edit,
                id: "spot",
                localRevision: 1,
                input: nil,
                remoteInput: nil,
                remoteStatus: nil,
                reason: reason,
                canRebase: false
            )
            let sync = ChargingCommunitySync(
                credentialStore: ChargingModelCredentialStore(
                    initial: .ready(.init(identityID: "identity", token: "token"))
                ),
                registerIdentity: { .init(identityID: "identity", token: "token") },
                synchronize: { _ in
                    .init(pendingCount: 1, downloaded: 0, uploaded: 0, outcome: .pending)
                },
                sleep: { _ in },
                enabled: { true }
            )
            let model = ChargingMapModel(
                queryPage: { _, _, _, _ in Self.page([], cursor: nil) },
                readSpot: { _ in nil },
                writeVote: { _, _, _ in },
                queryBlockedOperations: { [blocked] },
                communitySync: sync
            )

            model.communitySyncRequested()
            _ = await sync.run()
            await model.loadBlockedOperations()

            XCTAssertEqual(sync.status, .pending)
            XCTAssertEqual(
                model.communitySyncStatus,
                reason == .conflict ? .conflict : .rejected
            )
        }
    }

    func testOfflineSyncStillLoadsCachedBlockedDraftForReview() async {
        let draft = MobileChargingInputDto(
            coordinate: MobileMapCoordinateDto(latitudeDegrees: 39, longitudeDegrees: -105),
            accessNote: "Locally saved draft",
            cost: .free,
            name: nil,
            venue: nil,
            hours: nil,
            terms: nil,
            price: nil,
            supplies: []
        )
        let blocked = MobileChargingSyncConflictDto(
            operationId: "operation",
            action: .edit,
            id: "spot",
            localRevision: 2,
            input: draft,
            remoteInput: nil,
            remoteStatus: nil,
            reason: .conflict,
            canRebase: true
        )
        let sync = ChargingCommunitySync(
            credentialStore: ChargingModelCredentialStore(
                initial: .ready(.init(identityID: "identity", token: "token"))
            ),
            registerIdentity: { .init(identityID: "identity", token: "token") },
            synchronize: { _ in throw MobileChargingError.Network },
            sleep: { _ in },
            enabled: { true }
        )
        let model = ChargingMapModel(
            queryPage: { _, _, _, _ in Self.page([], cursor: nil) },
            readSpot: { _ in nil },
            writeVote: { _, _, _ in },
            queryBlockedOperations: { [blocked] },
            communitySync: sync
        )

        model.communitySyncRequested()
        _ = await sync.run()
        for _ in 0..<100 where model.blockedChargingOperations.isEmpty { await Task.yield() }

        XCTAssertEqual(sync.status, .offline)
        XCTAssertEqual(model.communitySyncStatus, .offline)
        XCTAssertEqual(model.blockedChargingOperations.first?.action, .edit)
        XCTAssertEqual(model.blockedChargingOperations.first?.input?.accessNote, "Locally saved draft")
    }

    func testManualPanCancellationDropsLateNearbyResult() async {
        let gate = ChargingNearbyGate()
        let model = ChargingMapModel(
            queryPage: { _, _, _, _ in Self.page([], cursor: nil) },
            readSpot: { _ in nil },
            nearbySpots: { _, _, _, _ in await gate.wait() },
            writeVote: { _, _, _ in }
        )
        model.setViewport(bounds())
        model.setUserLocation(MobileMapCoordinateDto(latitudeDegrees: 39, longitudeDegrees: -105))
        await model.load()
        await gate.waitUntilStarted()

        model.cancelAutomaticDiscovery()
        await gate.release([
            Self.spot("late", coordinate: MobileMapCoordinateDto(latitudeDegrees: 40, longitudeDegrees: -106))
        ])
        await Task.yield()
        await Task.yield()

        XCTAssertNil(model.discoveryTarget)
    }

    func testFilterChangeSuppressesAutomaticZoomAndPassesFilterToNearestQuery() async {
        let calls = ChargingNearbyCallCounter()
        let original = MobileChargingFilterDto(cost: .paid, connector: nil, voltageVolts: nil, venue: nil)
        let model = ChargingMapModel(
            filter: original,
            queryPage: { _, _, _, _ in Self.page([], cursor: nil) },
            readSpot: { _ in nil },
            nearbySpots: { metro, origin, filter, limit in
                _ = await calls.record(metro: metro, origin: origin, filter: filter, limit: limit)
                return []
            },
            writeVote: { _, _, _ in }
        )
        model.setViewport(bounds())
        await model.load()
        model.setFilter(MobileChargingFilterDto(cost: .free, connector: nil, voltageVolts: nil, venue: nil))
        model.setUserLocation(MobileMapCoordinateDto(latitudeDegrees: 39, longitudeDegrees: -105))
        await model.load()
        await Task.yield()

        let recorded = await calls.values()
        XCTAssertTrue(recorded.isEmpty)
        XCTAssertNil(model.discoveryTarget)
    }

    func testRefreshSavedSpotSelectsDirectReadWithoutChangingPageMembership() async {
        let writes = ChargingWriteCounter()
        let saved = Self.spot(
            "saved", coordinate: MobileMapCoordinateDto(latitudeDegrees: 4, longitudeDegrees: 5)
        )
        let model = ChargingMapModel(
            queryPage: { _, _, _, _ in Self.page([Self.spot("viewport")], cursor: nil) },
            readSpot: { id in id == saved.id ? saved : nil },
            writeVote: { _, _, _ in await writes.record() }
        )
        model.setViewport(bounds())

        await model.refreshSavedSpot(id: saved.id)

        XCTAssertEqual(model.spots.map(\.id), ["viewport"])
        XCTAssertEqual(model.selectedSpot?.id, "saved")
        XCTAssertEqual(model.selectedSpot?.input?.coordinate, saved.input?.coordinate)
        let writeCount = await writes.count()
        XCTAssertEqual(writeCount, 0)
        XCTAssertNil(model.error)
    }

    func testRefreshSavedSpotKeepsEditedSpotOutOfActiveFilterResults() async {
        let coordinate = MobileMapCoordinateDto(latitudeDegrees: 0.5, longitudeDegrees: 0.5)
        let saved = Self.spot(
            "saved", revision: 2, coordinate: coordinate,
            cost: .paid
        )
        let model = ChargingMapModel(
            queryPage: { _, _, _, _ in Self.page([], cursor: nil) },
            readSpot: { _ in saved },
            writeVote: { _, _, _ in }
        )
        model.setViewport(bounds())
        model.setFilter(MobileChargingFilterDto(cost: .free, connector: nil, voltageVolts: nil, venue: nil))

        await model.refreshSavedSpot(id: saved.id)

        XCTAssertTrue(model.spots.isEmpty)
        XCTAssertEqual(model.selectedSpot?.id, saved.id)
        XCTAssertEqual(model.selectedSpot?.input?.cost, .paid)
    }

    func testAcceptingCommunityWithdrawalRestoresSpotAndActions() async throws {
        let coordinate = MobileMapCoordinateDto(latitudeDegrees: 0.5, longitudeDegrees: 0.5)
        let original = Self.spot("owned", revision: 1, coordinate: coordinate, owned: true)
        let restored = Self.spot("owned", revision: 8, coordinate: coordinate, owned: true)
        let blocked = MobileChargingSyncConflictDto(
            operationId: "withdrawal", action: .withdraw, id: "owned", localRevision: 2,
            input: nil, remoteInput: restored.input, remoteStatus: .published,
            reason: .conflict, canRebase: false
        )
        let model = ChargingMapModel(
            queryPage: { _, _, _, _ in Self.page([restored], cursor: nil) },
            readSpot: { _ in restored },
            writeVote: { _, _, _ in },
            writeWithdrawal: { _, _ in },
            resolveOperation: { _, _, _ in }
        )
        model.setViewport(bounds())
        await model.load()
        let firstWithdrawal = await model.withdraw(original)
        XCTAssertTrue(firstWithdrawal)

        try await model.resolveChargingOperation(blocked, resolution: .acceptServer)

        XCTAssertEqual(model.spots.map(\.id), ["owned"])
        XCTAssertEqual(model.spots.first?.localRevision, 8)
        let secondWithdrawal = await model.withdraw(restored)
        XCTAssertTrue(secondWithdrawal)
    }

    func testConflictResolutionRefreshesLocalStateWhileSyncIsOffline() async throws {
        let coordinate = MobileMapCoordinateDto(latitudeDegrees: 0.5, longitudeDegrees: 0.5)
        let draft = Self.spot("spot", revision: 1, coordinate: coordinate, cost: .free)
        let accepted = Self.spot("spot", revision: 2, coordinate: coordinate, cost: .paid)
        let pages = ChargingTestPageSequence([
            Self.page([draft], cursor: nil), Self.page([accepted], cursor: nil),
        ])
        let blocked = MobileChargingSyncConflictDto(
            operationId: "edit", action: .edit, id: "spot", localRevision: 1,
            input: draft.input, remoteInput: accepted.input, remoteStatus: .published,
            reason: .conflict, canRebase: false
        )
        let sync = ChargingCommunitySync(
            credentialStore: ChargingModelCredentialStore(
                initial: .ready(.init(identityID: "identity", token: "token"))
            ),
            registerIdentity: { .init(identityID: "identity", token: "token") },
            synchronize: { _ in throw MobileChargingError.Network },
            sleep: { _ in }, enabled: { true }
        )
        let model = ChargingMapModel(
            queryPage: { _, _, _, _ in await pages.next() },
            readSpot: { _ in accepted }, writeVote: { _, _, _ in },
            queryBlockedOperations: { [] }, resolveOperation: { _, _, _ in }, communitySync: sync
        )
        model.setViewport(bounds())
        await model.load()
        model.select(draft)

        try await model.resolveChargingOperation(blocked, resolution: .acceptServer)
        for _ in 0..<100 where sync.status != .offline { await Task.yield() }

        XCTAssertEqual(sync.status, .offline)
        XCTAssertEqual(model.spots.first?.localRevision, 2)
        XCTAssertEqual(model.spots.first?.input?.cost, .paid)
        XCTAssertEqual(model.selectedSpot?.localRevision, 2)
        XCTAssertEqual(model.selectedSpot?.input?.cost, .paid)
    }

    func testDiscoveryTargetCanBeConsumedOnlyOnce() async {
        let coordinate = MobileMapCoordinateDto(latitudeDegrees: 39, longitudeDegrees: -105)
        let model = ChargingMapModel(
            queryPage: { _, _, _, _ in Self.page([], cursor: nil) },
            readSpot: { _ in nil },
            nearbySpots: { _, _, _, _ in [Self.spot("nearby", coordinate: coordinate)] },
            writeVote: { _, _, _ in }
        )
        model.setViewport(bounds())
        model.setUserLocation(coordinate)
        await model.load()
        await waitForTarget(on: model)

        XCTAssertEqual(model.consumeDiscoveryTarget(), coordinate)
        XCTAssertNil(model.consumeDiscoveryTarget())
    }

    func testRefreshSavedSpotStopsOnViewportLoadFailure() async {
        let reads = ChargingReadCounter()
        let writes = ChargingWriteCounter()
        let model = ChargingMapModel(
            queryPage: { _, _, _, _ in throw MobileChargingError.StorageFailure },
            readSpot: { _ in
                await reads.record()
                return nil
            },
            writeVote: { _, _, _ in await writes.record() }
        )
        model.setViewport(bounds())

        await model.refreshSavedSpot(id: "saved")

        XCTAssertEqual(model.error, .StorageFailure)
        let readCount = await reads.count()
        let writeCount = await writes.count()
        XCTAssertEqual(readCount, 0)
        XCTAssertEqual(writeCount, 0)
    }

    func testRefreshSavedSpotReadFailureReportsErrorWithoutWriting() async {
        let writes = ChargingWriteCounter()
        let model = ChargingMapModel(
            queryPage: { _, _, _, _ in Self.page([], cursor: nil) },
            readSpot: { _ in throw MobileChargingError.Unavailable },
            writeVote: { _, _, _ in await writes.record() }
        )
        model.setViewport(bounds())

        await model.refreshSavedSpot(id: "saved")

        XCTAssertEqual(model.error, .Unavailable)
        let writeCount = await writes.count()
        XCTAssertEqual(writeCount, 0)
    }

    func testNewerViewportQueryWinsOverLateSavedSpotReadback() async {
        let gate = ChargingSpotReadGate()
        let saved = Self.spot("saved", revision: 1)
        let model = ChargingMapModel(
            queryPage: { requestedBounds, _, _, _ in
                if requestedBounds.minimumLatitudeDegrees == 2 {
                    return Self.page([Self.spot("newer")], cursor: nil)
                }
                return Self.page([], cursor: nil)
            },
            readSpot: { _ in await gate.wait() },
            writeVote: { _, _, _ in }
        )
        model.setViewport(bounds(latitude: 1))
        let refresh = Task { await model.refreshSavedSpot(id: saved.id) }
        await gate.waitUntilStarted()

        model.setViewport(bounds(latitude: 2))
        await model.load()
        await gate.release(saved)
        await refresh.value

        XCTAssertEqual(model.spots.map(\.id), ["newer"])
        XCTAssertNil(model.selectedSpot)
    }

    func testNewerVoteReadbackWinsOverLateSavedSpotRead() async {
        let gate = ChargingSpotReadGate()
        let original = Self.spot("saved", revision: 1, votesUp: 4)
        let voted = Self.spot("saved", revision: 2, pendingVote: true, votesUp: 5)
        let reads = ChargingSequencedSpotRead(gate: gate, laterRead: voted)
        let model = ChargingMapModel(
            queryPage: { _, _, _, _ in Self.page([original], cursor: nil) },
            readSpot: { _ in await reads.read() },
            writeVote: { _, _, _ in }
        )
        model.setViewport(bounds())
        await model.load()
        model.select(original)
        let refresh = Task { await model.refreshSavedSpot(id: original.id) }
        await gate.waitUntilStarted()

        await model.vote(.up)
        await gate.release(original)
        await refresh.value

        XCTAssertEqual(model.selectedSpot?.localRevision, 2)
        XCTAssertEqual(model.spots.first?.localRevision, 2)
        XCTAssertEqual(model.spots.first?.votesUp, 5)
    }

    private func waitForTarget(on model: ChargingMapModel) async {
        for _ in 0..<100 where model.discoveryTarget == nil { await Task.yield() }
    }

    private static func downloadedCommunitySync() -> ChargingCommunitySync {
        ChargingCommunitySync(
            credentialStore: ChargingModelCredentialStore(),
            registerIdentity: { .init(identityID: "test", token: "test") },
            synchronize: { _ in
                ChargingCommunitySyncCounts(
                    pendingCount: 0, downloaded: 1, uploaded: 0, outcome: .synced
                )
            },
            sleep: { _ in },
            enabled: { true }
        )
    }

    private func waitForNearbyToFinish(_ model: ChargingMapModel) async {
        for _ in 0..<100 { await Task.yield() }
    }

    func testWithdrawalUsesRevisionAndPreventsStalePageResurrection() async {
        let original = Self.spot("owned", revision: 7, owned: true)
        let writes = ChargingWriteCounter()
        let model = ChargingMapModel(
            queryPage: { _, _, _, _ in Self.page([original], cursor: nil) },
            readSpot: { _ in original },
            writeVote: { _, _, _ in },
            writeWithdrawal: { id, revision in
                XCTAssertEqual(id, "owned")
                XCTAssertEqual(revision, 7)
                await writes.record()
            }
        )
        model.setViewport(bounds())
        await model.load()
        model.select(original)

        let deleted = await model.withdraw(original)
        XCTAssertTrue(deleted)
        XCTAssertTrue(model.spots.isEmpty)
        XCTAssertNil(model.selectedSpot)
        await model.load()
        XCTAssertTrue(model.spots.isEmpty)
        let repeated = await model.withdraw(original)
        XCTAssertFalse(repeated)
        let writeCount = await writes.count()
        XCTAssertEqual(writeCount, 1)
    }

    func testPageStartedBeforeWithdrawalCannotRestoreSpot() async {
        let gate = ChargingPageGate()
        let original = Self.spot("owned", revision: 7, owned: true)
        let model = ChargingMapModel(
            queryPage: { _, _, _, _ in await gate.wait() },
            readSpot: { _ in original },
            writeVote: { _, _, _ in },
            writeWithdrawal: { _, _ in }
        )
        model.setViewport(bounds())
        let load = Task { await model.load() }
        await gate.waitUntilStarted()

        let deleted = await model.withdraw(original)
        XCTAssertTrue(deleted)
        await gate.release(Self.page([original], cursor: nil))
        await load.value

        XCTAssertTrue(model.spots.isEmpty)
    }

    func testWithdrawalRejectsUnownedSpotAndKeepsFailedOwnedSpot() async {
        let original = Self.spot("owned", owned: true)
        let model = ChargingMapModel(
            queryPage: { _, _, _, _ in Self.page([original], cursor: nil) },
            readSpot: { _ in original }, writeVote: { _, _, _ in },
            writeWithdrawal: { _, _ in throw MobileChargingError.Conflict }
        )
        model.setViewport(bounds())
        await model.load()
        let unowned = await model.withdraw(Self.spot("other"))
        XCTAssertFalse(unowned)
        let failed = await model.withdraw(original)
        XCTAssertFalse(failed)
        XCTAssertEqual(model.spots, [original])
        XCTAssertEqual(model.error, .Conflict)
        XCTAssertTrue(model.pendingWithdrawalIDs.isEmpty)
    }

    private func bounds(latitude: Double = 0) -> MobileGeoBoundsDto {
        MobileGeoBoundsDto(
            minimumLatitudeDegrees: latitude, maximumLatitudeDegrees: latitude + 1,
            minimumLongitudeDegrees: 0, maximumLongitudeDegrees: 1
        )
    }

    private nonisolated static func spot(
        _ id: String,
        revision: UInt64 = 1,
        pendingVote: Bool = false,
        votesUp: UInt64 = 4,
        coordinate: MobileMapCoordinateDto? = nil,
        owned: Bool = false,
        status: MobileChargingStatusDto = .published,
        cost: MobileChargingCostDto = .free
    ) -> MobileChargingSpotDto {
        MobileChargingSpotDto(
            id: id,
            input: coordinate.map { coordinate in
                MobileChargingInputDto(
                    coordinate: coordinate, accessNote: "", cost: cost,
                    name: nil, venue: nil, hours: nil, terms: nil, price: nil, supplies: []
                )
            }, owned: owned, status: status,
            localRevision: revision, remoteId: nil, remoteRevision: revision,
            changeSequence: revision, verifiedAtMs: nil, vote: nil, voteRevision: 0,
            votesUp: votesUp, votesDown: 1, pendingOperations: 0, pendingVote: pendingVote
        )
    }

    private nonisolated static func page(_ spots: [MobileChargingSpotDto], cursor: String?) -> MobileChargingPageDto {
        MobileChargingPageDto(
            spots: spots,
            nextCursor: cursor.map { MobileMapPointCursorDto(id: $0) }
        )
    }
}

private actor ChargingTestPageSequence {
    private var pages: [MobileChargingPageDto]

    init(_ pages: [MobileChargingPageDto]) {
        self.pages = pages
    }

    func next() -> MobileChargingPageDto {
        pages.isEmpty ? MobileChargingPageDto(spots: [], nextCursor: nil) : pages.removeFirst()
    }
}

private actor ChargingBlockedOperationsReadSignal {
    private var calls = 0
    private var waiters = [(Int, CheckedContinuation<Void, Never>)]()

    func read() -> [MobileChargingSyncConflictDto] {
        calls += 1
        waiters.filter { $0.0 <= calls }.forEach { $0.1.resume() }
        waiters.removeAll { $0.0 <= calls }
        return []
    }

    func waitForCallCount(_ target: Int) async {
        if calls >= target { return }
        await withCheckedContinuation { waiters.append((target, $0)) }
    }
}

private actor ChargingSyncRunGate {
    private var count = 0
    private var waiters = [Int: CheckedContinuation<ChargingCommunitySyncCounts, Error>]()
    private var countWaiters = [(Int, CheckedContinuation<Void, Never>)]()

    func run() async throws -> ChargingCommunitySyncCounts {
        count += 1
        let current = count
        countWaiters.filter { $0.0 <= current }.forEach { $0.1.resume() }
        countWaiters.removeAll { $0.0 <= current }
        if current > 2 {
            return .init(pendingCount: 0, downloaded: 3, uploaded: 0, outcome: .synced)
        }
        return try await withCheckedThrowingContinuation { waiters[current] = $0 }
    }

    func waitForCallCount(_ target: Int) async {
        if count >= target { return }
        await withCheckedContinuation { countWaiters.append((target, $0)) }
    }

    func release(_ index: Int, counts: ChargingCommunitySyncCounts) {
        waiters.removeValue(forKey: index)?.resume(returning: counts)
    }

    func callCount() -> Int { count }
}

private actor ChargingPageQueryCounter {
    private var queries = 0

    func record() { queries += 1 }
    func count() -> Int { queries }
}

private actor ChargingModelCredentialStore: ChargingCredentialStore {
    private var record: ChargingCredentialRecord?

    init(initial: ChargingCredentialRecord? = nil) {
        record = initial
    }

    func load() async throws -> ChargingCredentialRecord? { record }

    func beginRegistration() async throws -> Bool {
        guard record == nil else { return false }
        record = .registrationStarted
        return true
    }

    func save(_ credential: ChargingCommunityCredential) async throws {
        guard record == .registrationStarted else {
            throw ChargingCredentialStoreError.unavailable
        }
        record = .ready(credential)
    }
}

private actor ChargingNearbyGate {
    private var continuation: CheckedContinuation<[MobileChargingSpotDto], Never>?
    private var started: CheckedContinuation<Void, Never>?

    func wait() async -> [MobileChargingSpotDto] {
        await withCheckedContinuation { continuation in
            self.continuation = continuation
            started?.resume()
            started = nil
        }
    }

    func waitUntilStarted() async {
        if continuation != nil { return }
        await withCheckedContinuation { started = $0 }
    }

    func release(_ spots: [MobileChargingSpotDto]) {
        continuation?.resume(returning: spots)
        continuation = nil
    }
}

private actor ChargingNearbyCallCounter {
    struct Call {
        let metro: MobileGeoBoundsDto
        let origin: MobileMapCoordinateDto
        let filter: MobileChargingFilterDto
        let limit: UInt32
    }

    private var recorded = [Call]()
    private var waiters = [CheckedContinuation<Void, Never>]()

    func record(
        metro: MobileGeoBoundsDto, origin: MobileMapCoordinateDto, filter: MobileChargingFilterDto, limit: UInt32
    ) -> Int {
        recorded.append(Call(metro: metro, origin: origin, filter: filter, limit: limit))
        waiters.forEach { $0.resume() }
        waiters.removeAll()
        return recorded.count
    }

    func values() -> [Call] { recorded }

    func waitForCall() async {
        if !recorded.isEmpty { return }
        await withCheckedContinuation { waiters.append($0) }
    }
}

private actor ChargingWriteCounter {
    private var writes = 0

    func record() { writes += 1 }
    func count() -> Int { writes }
}

private actor ChargingReadCounter {
    private var reads = 0

    func record() { reads += 1 }
    func count() -> Int { reads }
}

private actor ChargingSpotReadGate {
    private var continuation: CheckedContinuation<MobileChargingSpotDto?, Never>?
    private var started: CheckedContinuation<Void, Never>?

    func wait() async -> MobileChargingSpotDto? {
        await withCheckedContinuation { continuation in
            self.continuation = continuation
            started?.resume()
            started = nil
        }
    }

    func waitUntilStarted() async {
        if continuation != nil { return }
        await withCheckedContinuation { started = $0 }
    }

    func release(_ spot: MobileChargingSpotDto) {
        continuation?.resume(returning: spot)
        continuation = nil
    }
}

private actor ChargingSequencedSpotRead {
    private let gate: ChargingSpotReadGate
    private let laterRead: MobileChargingSpotDto
    private var reads = 0

    init(gate: ChargingSpotReadGate, laterRead: MobileChargingSpotDto) {
        self.gate = gate
        self.laterRead = laterRead
    }

    func read() async -> MobileChargingSpotDto? {
        reads += 1
        if reads == 1 { return await gate.wait() }
        return laterRead
    }
}

private actor ChargingVoteGate {
    private var continuation: CheckedContinuation<Void, Never>?
    private var started: CheckedContinuation<Void, Never>?

    func waitForWrite() async {
        await withCheckedContinuation { continuation in
            self.continuation = continuation
            started?.resume()
            started = nil
        }
    }

    func waitUntilStarted() async {
        if continuation != nil { return }
        await withCheckedContinuation { started = $0 }
    }

    func release() {
        continuation?.resume()
        continuation = nil
    }
}

private actor ChargingPageGate {
    private var continuation: CheckedContinuation<MobileChargingPageDto, Never>?
    private var started: CheckedContinuation<Void, Never>?

    func wait() async -> MobileChargingPageDto {
        await withCheckedContinuation { continuation in
            self.continuation = continuation
            started?.resume()
            started = nil
        }
    }

    func waitUntilStarted() async {
        if continuation != nil { return }
        await withCheckedContinuation { started = $0 }
    }

    func release(_ page: MobileChargingPageDto) {
        continuation?.resume(returning: page)
        continuation = nil
    }
}
