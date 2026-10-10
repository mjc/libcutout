import CutoutMobileFFI
import Foundation
import XCTest

@testable import CutoutApp
@testable import CutoutMobile

#if os(macOS)
    import AppKit
    import SwiftUI
#endif

final class RideHistoryModelTests: XCTestCase {
    private static func historySummary(_ rideID: String) -> MobileRideMapHistorySummaryDto {
        historySummary(rideID, pointCount: 0)
    }

    private static func historySummary(_ rideID: String, pointCount: UInt64) -> MobileRideMapHistorySummaryDto {
        MobileRideMapHistorySummaryDto(
            rideID: rideID,
            state: .saved,
            summary: MobileRideMapSummaryDto(
                pointCount: pointCount,
                distanceMeters: 0,
                durationMilliseconds: 0
            ),
            segmentCount: 0,
            createdAtMilliseconds: 0,
            candidateVehicle: nil,
            associatedVehicle: nil,
            associatedVehicleName: nil,
            telemetryState: .associatedNoTelemetry
        )
    }

    private final class InMemoryHistoryQuery: RideHistoryQuerying, @unchecked Sendable {
        final class PageGate: @unchecked Sendable {
            let started = XCTestExpectation(description: "history page started")
            let finished = XCTestExpectation(description: "history page finished")
            private let releaseSignal = DispatchSemaphore(value: 0)

            func waitForRelease() {
                started.fulfill()
                releaseSignal.wait()
                finished.fulfill()
            }

            func release() { releaseSignal.signal() }
        }

        let summaries: [MobileRideMapHistorySummaryDto]
        private let lock = NSLock()
        private var nextPageGate: PageGate?
        private var historyFilters = [MobileRideHistoryFilterDto]()

        var historyFilterSnapshots: [MobileRideHistoryFilterDto] {
            lock.lock()
            defer { lock.unlock() }
            return historyFilters
        }

        init(count: Int) {
            summaries = (0..<count).map { RideHistoryModelTests.historySummary("ride-\($0)", pointCount: 1) }
        }

        func gateNextPage() -> PageGate {
            let gate = PageGate()
            lock.lock()
            nextPageGate = gate
            lock.unlock()
            return gate
        }

        func storedHistoryPage(
            cursor: MobileRideCursorDto?,
            limit: UInt32,
            filter: MobileRideHistoryFilterDto?
        ) throws -> MobileRideMapHistoryPageDto {
            // The fake uses the cursor timestamp as an array offset; Rust owns real cursor semantics.
            let start = min(Int(cursor?.createdAtMilliseconds ?? 0), summaries.count)
            let end = min(start + Int(limit), summaries.count)
            let page = MobileRideMapHistoryPageDto(
                summaries: Array(summaries[start..<end]),
                nextCursor: end < summaries.count
                    ? MobileRideCursorDto(
                        createdAtMilliseconds: UInt64(end),
                        rideId: MobileRideIdDto(bytes: Data(repeating: 0, count: 16))
                    ) : nil
            )
            lock.lock()
            if let filter {
                historyFilters.append(filter)
            }
            let gate = nextPageGate
            nextPageGate = nil
            lock.unlock()
            gate?.waitForRelease()
            return page
        }

        func storedHistoryRide(rideID: String) throws -> MobileRideMapHistorySummaryDto? {
            summaries.first { $0.rideID == rideID }
        }

        func storedHistoryRide(
            rideID: String,
            filter: MobileRideHistoryFilterDto
        ) throws -> MobileRideMapHistorySummaryDto? {
            guard let summary = summaries.first(where: { $0.rideID == rideID }) else { return nil }
            guard filter.createdAfterMilliseconds.map({ summary.createdAtMilliseconds >= $0 }) ?? true,
                filter.vehicleIdentity == nil,
                filter.searchText.map({ summary.rideID.localizedCaseInsensitiveContains($0) }) ?? true
            else { return nil }
            return summary
        }

        func storedHistoryVehicleOptions() throws -> [MobileRideMapHistoryVehicleOptionDto] { [] }

        func storedMusicHistoryAsync(rideID: String) async throws -> MobileMusicHistoryDto {
            MobileMusicHistoryDto(status: .unavailable, events: [])
        }

        func projectStoredPoints(
            rideID: String,
            budget: UInt32,
            viewport: MobileGeoBoundsDto?,
            privacy: MobileRideMapRoutePrivacyPolicy,
            cancellation: MobileRideMapProjectionCancellation?
        ) throws -> MobileRideMapRouteProjection {
            guard let index = summaries.firstIndex(where: { $0.rideID == rideID }) else {
                throw MobileRideMapError.rideNotFound
            }
            return MobileRideMapRouteProjection(
                points: [
                    MobileRideMapRouteDisplayPoint(
                        sequence: UInt64(index), segmentId: 1,
                        latitudeDegrees: 39.7 + Double(index) * 0.001, longitudeDegrees: -104.9,
                        privacyClass: .precise
                    )
                ],
                segments: [],
                sourcePointCount: 1,
                sourceSegmentCount: 1,
                candidatePointCount: 1,
                candidateSegmentCount: 0,
                displayedSegmentCount: 0,
                backgroundGapCount: 0,
                presence: .visible
            )
        }
    }

    @MainActor
    private static func waitUntil(
        _ description: String,
        file: StaticString = #filePath,
        line: UInt = #line,
        condition: @escaping @MainActor () async -> Bool
    ) async {
        let deadline = ContinuousClock.now + .seconds(10)
        while ContinuousClock.now < deadline {
            if await condition() { return }
            await Task.yield()
        }
        XCTFail("timed out waiting for \(description)", file: file, line: line)
    }

    private static func settle(
        _ state: MobileRideMapState,
        _ decision: MobileRideMapDecisionDto,
        file: StaticString = #filePath,
        line: UInt = #line
    ) async -> MobileRideMapDecisionDto {
        guard case .pending = decision else { return decision }
        let deadline = ContinuousClock.now + .seconds(10)
        while ContinuousClock.now < deadline {
            if let terminal = state.pollLocationWrites().first {
                return terminal
            }
            if Task.isCancelled { break }
            try? await Task.sleep(for: .milliseconds(1))
        }
        XCTFail("timed out waiting for durable ride-map location outcome", file: file, line: line)
        return decision
    }

    @MainActor
    private static func saveHistoryRide(
        in state: MobileRideMapState,
        startingAt startMs: UInt64 = 100,
        latitude: Double = 39.7000
    ) async throws -> String {
        _ = try state.startGpsOnly(atMs: startMs)
        _ = await settle(
            state,
            try state.ingestLocation(
                monotonicMs: startMs,
                wallClockUnixMs: 1_700_000_000_000 + startMs,
                latitudeDegrees: latitude,
                longitudeDegrees: -104.9000,
                horizontalAccuracyMeters: 5
            ))
        _ = await settle(
            state,
            try state.ingestLocation(
                monotonicMs: startMs + 1_000,
                wallClockUnixMs: 1_700_000_001_000 + startMs,
                latitudeDegrees: latitude + 0.0001,
                longitudeDegrees: -104.9000,
                horizontalAccuracyMeters: 5
            ))
        _ = try state.stop(atMs: startMs + 1_000)
        return try state.save().rideID
    }

    @MainActor
    func testStorageFailureDoesNotLookLikeAnEmptyHistory() async {
        let model = RideHistoryModel(
            stateProvider: { nil },
            storageErrorProvider: { "Rust ride database is unavailable" }
        )

        model.reload()
        await Task.yield()

        XCTAssertEqual(model.error, .storageError("Rust ride database is unavailable"))
        XCTAssertFalse(model.isLoading)
    }

    @MainActor
    func testRecentFilterUsesInjectedCurrentTime() async throws {
        let fixedNow = Date(timeIntervalSince1970: 1_790_942_400)
        let query = GatedRideHistoryQuery(
            base: MobileRideMapState(),
            failAfterRelease: false
        )
        let model = RideHistoryModel(
            stateProvider: { query },
            dateProvider: { fixedNow }
        )

        model.reload()
        await Self.waitUntil("history query with fixed current time") {
            !model.isLoading
        }

        let filters = query.historyPageFiltersSnapshot
        XCTAssertEqual(filters.count, 1)
        let filter = try XCTUnwrap(filters[0])
        let nowMilliseconds = UInt64(fixedNow.timeIntervalSince1970 * 1_000)
        let expectedCutoff = nowMilliseconds - MobileRideMapLimits.rustOwned.historyRecentWindowMilliseconds
        XCTAssertEqual(filter.createdAfterMilliseconds, expectedCutoff)
        let september27Noon = UInt64(Date(timeIntervalSince1970: 1_790_510_400).timeIntervalSince1970 * 1_000)
        XCTAssertGreaterThanOrEqual(september27Noon, expectedCutoff)
        XCTAssertLessThanOrEqual(
            nowMilliseconds - september27Noon,
            MobileRideMapLimits.rustOwned.historyRecentWindowMilliseconds
        )
        XCTAssertNil(filter.vehicleIdentity)
        XCTAssertNil(filter.searchText)
        XCTAssertFalse(filter.includeShortRides)
    }

    @MainActor
    func testShortRideOverrideIsSessionScopedAndClearRestoresDefault() async throws {
        let query = InMemoryHistoryQuery(count: 2)
        let model = RideHistoryModel(stateProvider: { query })

        XCTAssertFalse(model.includeShortRides)
        model.reload()
        await Self.waitUntil("default history filter excludes short rides") {
            !model.isLoading && !query.historyFilterSnapshots.isEmpty
        }
        XCTAssertFalse(try XCTUnwrap(query.historyFilterSnapshots.last).includeShortRides)

        model.setDateFilter(.allTime)
        await Self.waitUntil("all-time history filter") { !model.isLoading }
        model.setIncludeShortRides(true)
        await Self.waitUntil("short-ride override reaches the Rust query") {
            !model.isLoading
                && query.historyFilterSnapshots.last?.includeShortRides == true
        }
        XCTAssertTrue(model.includeShortRides)
        XCTAssertTrue(
            RideMapHistoryContentView.hasActiveFilters(
                searchText: "",
                dateFilter: .last30Days,
                vehicleFilter: nil,
                includeShortRides: true
            )
        )

        model.reloadPreservingSelection()
        await Self.waitUntil("short-ride override survives History re-entry") {
            !model.isLoading
                && query.historyFilterSnapshots.last?.includeShortRides == true
        }
        XCTAssertTrue(model.includeShortRides)

        model.clearFilters()
        await Self.waitUntil("clear filters restores the default short-ride filter") {
            !model.isLoading
                && query.historyFilterSnapshots.last?.includeShortRides == false
        }
        XCTAssertFalse(model.includeShortRides)
        XCTAssertEqual(model.dateFilter, .last30Days)
        XCTAssertFalse(
            RideMapHistoryContentView.hasActiveFilters(
                searchText: "",
                dateFilter: .last30Days,
                vehicleFilter: nil,
                includeShortRides: false
            )
        )

        let nextLaunchModel = RideHistoryModel(stateProvider: { query })
        XCTAssertFalse(nextLaunchModel.includeShortRides)
    }

    #if os(macOS)
        @MainActor
        func testOpeningHistoryHidesShortRideUntilSessionOverride() async throws {
            let state = MobileRideMapState()
            let rideID = try await Self.saveHistoryRide(in: state)
            let model = CutoutAppModel(core: CutoutSessionCore(rideMapState: state))
            let presentation = RideMapPresentationState()
            let window = NSWindow(
                contentRect: NSRect(x: 0, y: 0, width: 480, height: 720),
                styleMask: [.titled], backing: .buffered, defer: false
            )
            window.isReleasedWhenClosed = false
            defer { window.close() }
            window.contentView = NSHostingView(rootView: RideMapRouteView(model: model, presentation: presentation))
            window.orderFront(nil)
            presentation.mode = .history

            await Self.waitUntil("initial Last 30 Days history load") {
                !model.rideHistory.isLoading && model.rideHistory.rides.isEmpty
            }
            XCTAssertEqual(model.rideHistory.dateFilter, .last30Days)
            XCTAssertFalse(model.rideHistory.includeShortRides)
            XCTAssertNil(model.rideHistory.error)

            model.rideHistory.setIncludeShortRides(true)
            await Self.waitUntil("short ride appears after session override") {
                !model.rideHistory.isLoading
                    && model.rideHistory.rides.contains { $0.rideID == rideID }
            }
        }
    #endif

    @MainActor
    func testDetailLoadGenerationRejectsDeletedOrReplacedSelection() {
        XCTAssertTrue(
            RideHistoryModel.shouldApplyHistoryDetailLoad(
                rideID: "ride-a",
                selectedRideID: "ride-a",
                loadGeneration: 3,
                currentGeneration: 3,
                isCancelled: false
            ))
        XCTAssertFalse(
            RideHistoryModel.shouldApplyHistoryDetailLoad(
                rideID: "ride-a",
                selectedRideID: "ride-a",
                loadGeneration: 3,
                currentGeneration: 4,
                isCancelled: false
            ))
        XCTAssertFalse(
            RideHistoryModel.shouldApplyHistoryDetailLoad(
                rideID: "ride-a",
                selectedRideID: "ride-b",
                loadGeneration: 3,
                currentGeneration: 3,
                isCancelled: false
            ))
    }

    @MainActor
    func testQueryGenerationRejectsLateReloadOrPageResults() {
        XCTAssertTrue(
            RideHistoryModel.shouldApplyHistoryQuery(
                generation: 7,
                currentGeneration: 7,
                isCancelled: false
            ))
        XCTAssertFalse(
            RideHistoryModel.shouldApplyHistoryQuery(
                generation: 7,
                currentGeneration: 8,
                isCancelled: false
            ))
        XCTAssertFalse(
            RideHistoryModel.shouldApplyHistoryQuery(
                generation: 7,
                currentGeneration: 7,
                isCancelled: true
            ))
    }

    @MainActor
    func testLateDetailViewportCannotRestoreInvalidatedProjection() {
        XCTAssertTrue(
            RideHistoryModel.shouldApplyHistoryDetailViewport(
                rideID: "ride-a",
                selectedRideID: "ride-a",
                expectedProjectionRideID: "ride-a",
                currentProjectionRideID: "ride-a",
                loadGeneration: 3,
                currentGeneration: 3,
                viewportLoadGeneration: 7,
                currentViewportLoadGeneration: 7,
                isCancelled: false
            ))
        XCTAssertFalse(
            RideHistoryModel.shouldApplyHistoryDetailViewport(
                rideID: "ride-a",
                selectedRideID: "ride-a",
                expectedProjectionRideID: "ride-a",
                currentProjectionRideID: nil,
                loadGeneration: 3,
                currentGeneration: 4,
                viewportLoadGeneration: 7,
                currentViewportLoadGeneration: 7,
                isCancelled: false
            ))
        XCTAssertFalse(
            RideHistoryModel.shouldApplyHistoryDetailViewport(
                rideID: "ride-a",
                selectedRideID: "ride-a",
                expectedProjectionRideID: "ride-a",
                currentProjectionRideID: "ride-b",
                loadGeneration: 3,
                currentGeneration: 3,
                viewportLoadGeneration: 7,
                currentViewportLoadGeneration: 7,
                isCancelled: false
            ))
        XCTAssertFalse(
            RideHistoryModel.shouldApplyHistoryDetailViewport(
                rideID: "ride-a",
                selectedRideID: "ride-a",
                expectedProjectionRideID: "ride-a",
                currentProjectionRideID: "ride-a",
                loadGeneration: 3,
                currentGeneration: 3,
                viewportLoadGeneration: 7,
                currentViewportLoadGeneration: 8,
                isCancelled: false
            ))
    }

    @MainActor
    func testReloadPreservesTheSelectedRideWhenItStillMatches() {
        XCTAssertEqual(
            RideHistoryModel.preferredHistorySelection(
                requestedID: nil,
                currentID: "ride-2",
                summaries: ["ride-1", "ride-2"].map(Self.historySummary)
            ),
            "ride-2"
        )
        XCTAssertEqual(
            RideHistoryModel.preferredHistorySelection(
                requestedID: "ride-3",
                currentID: "ride-2",
                summaries: ["ride-1", "ride-3"].map(Self.historySummary)
            ),
            "ride-3"
        )
        XCTAssertEqual(
            RideHistoryModel.preferredHistorySelection(
                requestedID: nil,
                currentID: "ride-missing",
                summaries: ["ride-1", "ride-2"].map(Self.historySummary)
            ),
            "ride-1"
        )
        XCTAssertNil(
            RideHistoryModel.preferredHistorySelection(
                requestedID: "ride-missing",
                currentID: "ride-2",
                summaries: ["ride-1", "ride-2"].map(Self.historySummary)
            ))
        XCTAssertEqual(
            RideHistoryModel.selectionError(
                requestedID: "ride-missing",
                summaries: ["ride-1", "ride-2"].map(Self.historySummary)
            ),
            .rideNotFound
        )
        XCTAssertNil(
            RideHistoryModel.selectionError(
                requestedID: "ride-2",
                summaries: ["ride-1", "ride-2"].map(Self.historySummary)
            ))
        XCTAssertEqual(
            RideHistoryModel.selectionAction(
                requestedID: "ride-2",
                currentID: nil,
                summaries: ["ride-1", "ride-2"].map(Self.historySummary)
            ),
            .select("ride-2")
        )
        XCTAssertEqual(
            RideHistoryModel.selectionAction(
                requestedID: "ride-missing",
                currentID: nil,
                summaries: ["ride-1", "ride-2"].map(Self.historySummary)
            ),
            .load("ride-missing")
        )
        XCTAssertEqual(
            RideHistoryModel.selectionAction(
                requestedID: nil,
                currentID: nil,
                summaries: []
            ),
            .load(nil)
        )
    }

    @MainActor
    func testPageAppendDoesNotDuplicateExistingRides() {
        XCTAssertEqual(
            RideHistoryModel.appendingUniqueHistory(
                existing: ["ride-1", "ride-2"].map(Self.historySummary),
                incoming: ["ride-2", "ride-3", "ride-1", "ride-4"].map(Self.historySummary)
            ),
            ["ride-1", "ride-2", "ride-3", "ride-4"].map(Self.historySummary)
        )
    }

    @MainActor
    func testVehicleNamesUseRustDeviceOptionsAndSummaryNamesWhenDisconnected() {
        let summary = MobileRideMapHistorySummaryDto(
            rideID: "ride-1",
            state: .saved,
            summary: MobileRideMapSummaryDto(
                pointCount: 1,
                distanceMeters: 1,
                durationMilliseconds: 1_000
            ),
            segmentCount: 1,
            createdAtMilliseconds: 1,
            candidateVehicle: nil,
            associatedVehicle: "corebluetooth-old",
            associatedVehicleName: "NF2557",
            telemetryState: .associatedNoTelemetry
        )
        let options = [
            MobileRideMapHistoryVehicleOptionDto(
                platformIdentifier: "corebluetooth-old",
                displayName: "NF2557"
            ),
            MobileRideMapHistoryVehicleOptionDto(
                platformIdentifier: "corebluetooth-new",
                displayName: "NF2557"
            ),
        ]

        let names = RideHistoryModel.vehicleNames(options, summaries: [summary])

        XCTAssertEqual(names["corebluetooth-old"], "NF2557")
        XCTAssertEqual(names["corebluetooth-new"], "NF2557")
        XCTAssertEqual(summary.vehicleDisplayName, "NF2557")
        XCTAssertEqual(
            RideHistoryModel.mergeVehicleIdentities(
                existing: options.map(\.platformIdentifier),
                incoming: []
            ),
            ["corebluetooth-new", "corebluetooth-old"]
        )
    }

    @MainActor
    func testRoutePreviewActionLoadsTheLargestBoundedPreview() async throws {
        let state = MobileRideMapState()
        _ = try state.startGpsOnly(atMs: 100)
        for index in 0...4_096 {
            _ = await Self.settle(
                state,
                try state.ingestLocation(
                    monotonicMs: 100 + UInt64(index) * 1_000,
                    wallClockUnixMs: 1_700_000_000_100 + UInt64(index) * 1_000,
                    latitudeDegrees: 39.7000 + Double(index) * 0.00001,
                    longitudeDegrees: -104.9000,
                    horizontalAccuracyMeters: 5
                ))
        }
        _ = try state.stop(atMs: 4_096_100)
        let rideID = try state.save().rideID

        let model = RideHistoryModel(stateProvider: { state })
        model.setDateFilter(.allTime)
        model.reload(selecting: rideID)
        await Self.waitUntil("bounded history route preview") {
            model.selectedRideID == rideID
                && model.displayPoints.count == 512
                && !model.routeLoading
        }

        XCTAssertTrue(model.pointsTruncated)
        let initialCameraFitVersion = model.detailCameraFitVersion
        model.loadRoutePreview()
        await Self.waitUntil("largest bounded history route preview") {
            model.displayPoints.count == 4_097
                && !model.routeLoading
        }

        XCTAssertEqual(model.displayPoints.count, 4_097)
        XCTAssertFalse(model.pointsTruncated)
        XCTAssertNotEqual(model.detailCameraFitVersion, initialCameraFitVersion)
    }

    @MainActor
    func testSelectionDoesNotLoadSupplementaryMapContext() async throws {
        let state = MobileRideMapState()

        func saveRide(startingAt startMs: UInt64) async throws -> String {
            _ = try state.startGpsOnly(atMs: startMs)
            _ = await Self.settle(
                state,
                try state.ingestLocation(
                    monotonicMs: startMs,
                    wallClockUnixMs: 1_700_000_000_000 + startMs,
                    latitudeDegrees: 39.7000,
                    longitudeDegrees: -104.9000,
                    horizontalAccuracyMeters: 5
                ))
            _ = await Self.settle(
                state,
                try state.ingestLocation(
                    monotonicMs: startMs + 1_000,
                    wallClockUnixMs: 1_700_000_001_000 + startMs,
                    latitudeDegrees: 39.7001,
                    longitudeDegrees: -104.9000,
                    horizontalAccuracyMeters: 5
                ))
            _ = try state.stop(atMs: startMs + 1_000)
            return try state.save().rideID
        }

        _ = try await saveRide(startingAt: 100)
        let selectedRideID = try await saveRide(startingAt: 10_000)
        let query = GatedRideHistoryQuery(base: state, failAfterRelease: false)
        let model = RideHistoryModel(stateProvider: { query })
        model.setDateFilter(.allTime)
        model.reload(selecting: selectedRideID)

        await Self.waitUntil("selected history route") {
            model.selectedRideID == selectedRideID && !model.routeLoading
        }
        XCTAssertEqual(query.supplementaryContextQueryCountSnapshot, 0)
    }

    @MainActor
    func testDetailViewportProjectionDoesNotReplaceHistoryProjection() async throws {
        let state = MobileRideMapState()
        _ = try state.startGpsOnly(atMs: 100)
        _ = await Self.settle(
            state,
            try state.ingestLocation(
                monotonicMs: 100,
                wallClockUnixMs: 1_700_000_000_100,
                latitudeDegrees: 39.7000,
                longitudeDegrees: -104.9000,
                horizontalAccuracyMeters: 5
            ))
        _ = await Self.settle(
            state,
            try state.ingestLocation(
                monotonicMs: 1_100,
                wallClockUnixMs: 1_700_000_001_100,
                latitudeDegrees: 39.7001,
                longitudeDegrees: -104.9000,
                horizontalAccuracyMeters: 5
            ))
        _ = try state.stop(atMs: 1_100)
        let rideID = try state.save().rideID

        let availability = RideHistoryAvailability()
        let model = RideHistoryModel(
            stateProvider: { availability.isAvailable ? state : nil },
            storageErrorProvider: {
                availability.isAvailable ? nil : "Rust ride database is unavailable"
            }
        )
        model.setDateFilter(.allTime)
        let initialHistoryProjectionVersion = model.projectionVersion
        let initialDetailProjectionVersion = model.detailProjectionVersion
        model.reload(selecting: rideID)
        await Self.waitUntil("history route selection") {
            model.selectedRideID == rideID && !model.displayPoints.isEmpty
        }
        XCTAssertEqual(model.selectedRideID, rideID)
        let historyPoints = model.displayPoints
        XCTAssertEqual(historyPoints.count, 2)
        XCTAssertEqual(model.detailRoutePresence, .visible)
        XCTAssertFalse(model.detailDisplayPoints.isEmpty)
        let selectedHistoryProjectionVersion = model.projectionVersion
        XCTAssertGreaterThan(selectedHistoryProjectionVersion, initialHistoryProjectionVersion)
        let selectedDetailProjectionVersion = model.detailProjectionVersion
        XCTAssertGreaterThan(selectedDetailProjectionVersion, initialDetailProjectionVersion)

        model.projectDetailViewport(
            MobileGeoBoundsDto(
                minimumLatitudeDegrees: 39.70009,
                maximumLatitudeDegrees: 39.70011,
                minimumLongitudeDegrees: -104.90001,
                maximumLongitudeDegrees: -104.89999
            ))
        XCTAssertFalse(model.detailRouteLoading)
        model.invalidateForMusicDeletion()
        XCTAssertFalse(model.detailRouteLoading)
        XCTAssertEqual(model.detailProjectionRideID, rideID)
        XCTAssertFalse(model.detailDisplayPoints.isEmpty)

        model.selectFromHistoryList(rideID)
        await Self.waitUntil("same-shape history route reprojection") {
            model.projectionVersion > selectedHistoryProjectionVersion
        }
        XCTAssertEqual(model.displayPoints, historyPoints)

        model.projectDetailViewport(
            MobileGeoBoundsDto(
                minimumLatitudeDegrees: 39.70009,
                maximumLatitudeDegrees: 39.70011,
                minimumLongitudeDegrees: -104.90001,
                maximumLongitudeDegrees: -104.89999
            ))
        await Self.waitUntil("detail viewport projection") {
            model.detailDisplayPoints.count == 1
        }
        XCTAssertEqual(model.detailDisplayPoints.count, 1)
        XCTAssertEqual(model.displayPoints, historyPoints)
        XCTAssertGreaterThan(model.detailProjectionVersion, selectedDetailProjectionVersion)

        model.projectDetailViewport(nil)
        await Self.waitUntil("nil history detail viewport error") {
            !model.detailRouteLoading && model.detailRouteError == .invalidRouteProjection
        }
        XCTAssertTrue(model.detailDisplayPoints.isEmpty)
        XCTAssertEqual(model.displayPoints, historyPoints)

        model.projectDetailViewport(
            MobileGeoBoundsDto(
                minimumLatitudeDegrees: 40,
                maximumLatitudeDegrees: 39,
                minimumLongitudeDegrees: -104.90001,
                maximumLongitudeDegrees: -104.89999
            ))
        await Self.waitUntil("invalid history detail viewport error") {
            model.detailRouteError != nil
        }
        XCTAssertNotNil(model.detailRouteError)
        XCTAssertNil(model.routeError)
        XCTAssertFalse(model.detailRouteLoading)

        model.projectDetailViewport(
            MobileGeoBoundsDto(
                minimumLatitudeDegrees: 40,
                maximumLatitudeDegrees: 41,
                minimumLongitudeDegrees: -104,
                maximumLongitudeDegrees: -103
            ))
        await Self.waitUntil("empty history detail viewport") {
            !model.detailRouteLoading
                && model.detailRouteError == nil
                && model.detailDisplayPoints.isEmpty
                && model.detailRoutePresence == .emptyViewport
        }
        XCTAssertEqual(model.detailRoutePresence, .emptyViewport)
        XCTAssertNil(model.detailRouteError)
        XCTAssertEqual(model.detailProjectionRideID, rideID)

        availability.isAvailable = false
        model.selectFromHistoryList(rideID)
        await Self.waitUntil("same-ride selection failure") {
            !model.routeLoading && model.routeError != nil
        }
        XCTAssertTrue(model.displayPoints.isEmpty)
        XCTAssertTrue(model.detailDisplayPoints.isEmpty)

        availability.isAvailable = true
        model.selectFromHistoryList(rideID)
        XCTAssertTrue(model.routeLoading)
        model.invalidateForMusicDeletion()
        XCTAssertFalse(model.routeLoading)
        XCTAssertFalse(model.detailRouteLoading)
    }

    @MainActor
    func testReloadFailureRetainsPreviouslySelectedRouteAndRetryRestoresIt() async throws {
        let state = MobileRideMapState()
        _ = try state.startGpsOnly(atMs: 100)
        _ = await Self.settle(
            state,
            try state.ingestLocation(
                monotonicMs: 100,
                wallClockUnixMs: 1_700_000_000_100,
                latitudeDegrees: 39.7000,
                longitudeDegrees: -104.9000,
                horizontalAccuracyMeters: 5
            ))
        _ = await Self.settle(
            state,
            try state.ingestLocation(
                monotonicMs: 1_100,
                wallClockUnixMs: 1_700_000_001_100,
                latitudeDegrees: 39.7001,
                longitudeDegrees: -104.9000,
                horizontalAccuracyMeters: 5
            ))
        _ = try state.stop(atMs: 1_100)
        let rideID = try state.save().rideID

        let availability = RideHistoryAvailability()
        let model = RideHistoryModel(
            stateProvider: { availability.isAvailable ? state : nil },
            storageErrorProvider: {
                availability.isAvailable ? nil : "Rust ride database is unavailable"
            }
        )
        model.setDateFilter(.allTime)
        model.reload(selecting: rideID)
        await Self.waitUntil("selected history route before reload failure") {
            model.selectedRideID == rideID
                && !model.displayPoints.isEmpty
                && !model.routeLoading
        }

        availability.isAvailable = false
        model.reload(selecting: rideID)
        await Task.yield()

        XCTAssertFalse(model.displayPoints.isEmpty)
        XCTAssertEqual(model.selectedRideID, rideID)
        XCTAssertEqual(model.detailProjectionRideID, rideID)
        XCTAssertEqual(model.routeError, .storageError("Rust ride database is unavailable"))
        XCTAssertEqual(model.detailRouteError, .storageError("Rust ride database is unavailable"))

        availability.isAvailable = true
        model.reload(selecting: rideID)
        XCTAssertTrue(model.isLoading)
        XCTAssertEqual(model.selectedRideID, rideID)
        await Self.waitUntil("same-ride history retry restores route") {
            !model.isLoading
                && !model.routeLoading
                && !model.detailRouteLoading
                && model.routeError == nil
                && model.detailRouteError == nil
                && model.detailProjectionRideID == rideID
        }

        XCTAssertEqual(model.selectedRideID, rideID)
        XCTAssertFalse(model.displayPoints.isEmpty)
        XCTAssertFalse(model.detailDisplayPoints.isEmpty)
        XCTAssertNil(model.error)
        XCTAssertNil(model.routeError)
        XCTAssertNil(model.detailRouteError)
    }

    @MainActor
    func testFilterFailureRetryDoesNotReinsertPreviouslySelectedRide() async throws {
        let state = MobileRideMapState()
        let previouslySelectedRideID = try await Self.saveHistoryRide(in: state, startingAt: 100)
        let matchingRideID = try await Self.saveHistoryRide(in: state, startingAt: 10_000)
        XCTAssertEqual(try state.storedHistoryRide(rideID: previouslySelectedRideID)?.state, .saved)
        XCTAssertEqual(try state.storedHistoryRide(rideID: matchingRideID)?.state, .saved)
        let query = GatedRideHistoryQuery(base: state, failAfterRelease: false)
        let availability = RideHistoryAvailability()
        let model = RideHistoryModel(
            stateProvider: { availability.isAvailable ? query : nil },
            storageErrorProvider: {
                availability.isAvailable ? nil : "Rust ride database is unavailable"
            }
        )

        model.setIncludeShortRides(true)
        await Self.waitUntil("unfiltered history before selection and filter change") {
            !model.isLoading
                && model.rides.contains { $0.rideID == previouslySelectedRideID }
                && model.rides.contains { $0.rideID == matchingRideID }
        }
        model.selectFromHistoryList(previouslySelectedRideID)
        await Self.waitUntil("previously selected route before filter change") {
            model.selectedRideID == previouslySelectedRideID
                && !model.displayPoints.isEmpty
                && !model.routeLoading
                && !model.detailRouteLoading
        }
        let retainedRoute = model.displayPoints

        query.armNextHistoryPage()
        model.searchText = matchingRideID
        let filteredQueryStarted = await query.waitUntilGatedHistoryPageStarts()
        XCTAssertTrue(filteredQueryStarted)
        XCTAssertEqual(query.gatedHistoryPageRideIDsSnapshot, [matchingRideID])
        let activeFilter = try XCTUnwrap(query.historyPageFiltersSnapshot.compactMap { $0 }.last)
        XCTAssertEqual(activeFilter.searchText, matchingRideID)

        availability.isAvailable = false
        model.reloadPreservingSelection()
        XCTAssertEqual(model.error, .storageError("Rust ride database is unavailable"))
        XCTAssertEqual(model.searchText, matchingRideID)
        XCTAssertEqual(model.selectedRideID, previouslySelectedRideID)
        XCTAssertEqual(model.displayPoints, retainedRoute)

        query.releaseGatedHistoryPage()
        let staleQueryFinished = await query.waitUntilGatedHistoryPageFinishes()
        XCTAssertTrue(staleQueryFinished)
        for _ in 0..<20 { await Task.yield() }
        XCTAssertEqual(model.error, .storageError("Rust ride database is unavailable"))

        availability.isAvailable = true
        model.reloadPreservingSelection()
        await Self.waitUntil("filtered retry excludes previous selection") {
            !model.isLoading
                && !model.routeLoading
                && !model.detailRouteLoading
                && model.rides.map(\.rideID) == [matchingRideID]
                && model.selectedRideID == matchingRideID
        }

        XCTAssertEqual(model.rides.map(\.rideID), [matchingRideID])
        XCTAssertFalse(model.rides.contains { $0.rideID == previouslySelectedRideID })
        XCTAssertEqual(model.searchText, matchingRideID)
        XCTAssertEqual(model.selectedRideID, matchingRideID)
        XCTAssertFalse(model.displayPoints.isEmpty)
        XCTAssertNil(model.error)
    }

    @MainActor
    func testSearchChangeRejectsLatePriorPageResult() async throws {
        let state = MobileRideMapState()
        _ = try state.startGpsOnly(atMs: 100)
        _ = await Self.settle(
            state,
            try state.ingestLocation(
                monotonicMs: 100,
                wallClockUnixMs: 1_700_000_000_100,
                latitudeDegrees: 39.7000,
                longitudeDegrees: -104.9000,
                horizontalAccuracyMeters: 5
            ))
        _ = await Self.settle(
            state,
            try state.ingestLocation(
                monotonicMs: 1_100,
                wallClockUnixMs: 1_700_000_001_100,
                latitudeDegrees: 39.7001,
                longitudeDegrees: -104.9000,
                horizontalAccuracyMeters: 5
            ))
        _ = try state.stop(atMs: 1_100)
        let rideID = try state.save().rideID
        let query = GatedRideHistoryQuery(base: state, failAfterRelease: false)
        let model = RideHistoryModel(stateProvider: { query })
        model.setIncludeShortRides(true)
        await Self.waitUntil("short ride is loaded before search race") {
            !model.isLoading && model.rides.contains { $0.rideID == rideID }
        }
        query.resetHistoryPageSnapshots()
        query.armNextHistoryPage()

        let unmatchedSearch = "no-match-\(UUID().uuidString)"
        model.searchText = unmatchedSearch
        let gatedPageStarted = await query.waitUntilGatedHistoryPageStarts()
        XCTAssertTrue(gatedPageStarted)
        XCTAssertFalse(query.gatedHistoryPageRideIDsSnapshot.contains(rideID))

        model.searchText = rideID
        let replacementQueryStarted = await query.waitUntilHistoryPageFilter(rideID)
        XCTAssertTrue(replacementQueryStarted)
        await Self.waitUntil("matching history query after replacing filtered page") {
            !model.isLoading && model.rides.contains(where: { $0.rideID == rideID })
        }
        let matchingRideIDs = model.rides.map(\.rideID)
        let selectedRideID = model.selectedRideID
        XCTAssertEqual(matchingRideIDs, [rideID])

        query.releaseGatedHistoryPage()
        let gatedPageFinished = await query.waitUntilGatedHistoryPageFinishes()
        XCTAssertTrue(gatedPageFinished)
        for _ in 0..<20 { await Task.yield() }

        XCTAssertEqual(model.rides.map(\.rideID), matchingRideIDs)
        XCTAssertEqual(model.selectedRideID, selectedRideID)
        XCTAssertFalse(model.isLoading)
        XCTAssertFalse(model.canLoadMore)
        XCTAssertNil(model.error)
    }

    @MainActor
    func testReloadRejectsLateCursorPageResult() async throws {
        let state = MobileRideMapState()
        let pageLimit = Int(MobileRideMapLimits.rustOwned.historyPageLimit)
        for index in 0...pageLimit {
            let startMs = UInt64(index + 1) * 10_000
            _ = try state.startGpsOnly(atMs: startMs)
            _ = await Self.settle(
                state,
                try state.ingestLocation(
                    monotonicMs: startMs,
                    wallClockUnixMs: 1_700_000_000_000 + startMs,
                    latitudeDegrees: 39.7000,
                    longitudeDegrees: -104.9000,
                    horizontalAccuracyMeters: 5
                ))
            _ = await Self.settle(
                state,
                try state.ingestLocation(
                    monotonicMs: startMs + 1_000,
                    wallClockUnixMs: 1_700_000_001_000 + startMs,
                    latitudeDegrees: 39.7001,
                    longitudeDegrees: -104.9000,
                    horizontalAccuracyMeters: 5
                ))
            _ = try state.stop(atMs: startMs + 1_000)
            _ = try state.save()
        }

        let query = GatedRideHistoryQuery(base: state, failAfterRelease: false)
        let model = RideHistoryModel(stateProvider: { query })
        model.setDateFilter(.allTime)
        await Self.waitUntil("all-time history load before short-ride override") {
            !model.isLoading
        }
        query.resetHistoryPageSnapshots()
        model.setIncludeShortRides(true)
        await Self.waitUntil("first Rust history page") {
            !model.isLoading && model.canLoadMore
        }
        let firstPageRideIDs = model.rides.map(\.rideID)
        XCTAssertEqual(firstPageRideIDs.count, pageLimit)
        XCTAssertEqual(query.historyPageCursorPresenceSnapshot, [false])

        query.armNextHistoryPage()
        model.loadMore()
        let cursorPageStarted = await query.waitUntilGatedHistoryPageStarts()
        XCTAssertTrue(cursorPageStarted)
        XCTAssertEqual(query.historyPageCursorPresenceSnapshot, [false, true])
        XCTAssertTrue(Set(firstPageRideIDs).isDisjoint(with: query.gatedHistoryPageRideIDsSnapshot))

        model.reload()
        await Self.waitUntil("reloaded first Rust history page") {
            !model.isLoading && model.rides.map(\.rideID) == firstPageRideIDs
        }
        XCTAssertTrue(model.canLoadMore)

        query.releaseGatedHistoryPage()
        let stalePageFinished = await query.waitUntilGatedHistoryPageFinishes()
        XCTAssertTrue(stalePageFinished)
        for _ in 0..<20 { await Task.yield() }

        XCTAssertEqual(model.rides.map(\.rideID), firstPageRideIDs)
        XCTAssertTrue(model.canLoadMore)
        XCTAssertNil(model.error)
        XCTAssertEqual(query.historyPageCursorPresenceSnapshot, [false, true, false])
    }

    @MainActor
    func testReloadPreservesSelectedRideBeyondFirstFilteredPage() async throws {
        let state = MobileRideMapState()
        let pageLimit = Int(MobileRideMapLimits.rustOwned.historyPageLimit)
        var rideIDs = [String]()
        for index in 0...pageLimit {
            let startMs = UInt64(index + 1) * 10_000
            rideIDs.append(try await Self.saveHistoryRide(in: state, startingAt: startMs))
        }
        let selectedRideID = try XCTUnwrap(rideIDs.first)
        let selectedRide = try XCTUnwrap(try state.storedHistoryRide(rideID: selectedRideID))
        XCTAssertEqual(selectedRide.state, .saved)
        let formatter = DateFormatter()
        formatter.locale = Locale(identifier: "en_US_POSIX")
        formatter.timeZone = TimeZone(secondsFromGMT: 0)
        formatter.dateFormat = "yyyy-MM-dd"
        let searchTerm = formatter.string(
            from: Date(timeIntervalSince1970: Double(selectedRide.createdAtMilliseconds) / 1_000)
        )
        let matchingFilter = MobileRideHistoryFilterDto(
            createdAfterMilliseconds: nil,
            vehicleIdentity: nil,
            searchText: searchTerm,
            includeShortRides: true
        )
        let firstFilteredPage = try state.storedHistoryPage(
            cursor: nil,
            limit: UInt32(pageLimit),
            filter: matchingFilter
        )
        XCTAssertEqual(firstFilteredPage.summaries.count, pageLimit)
        XCTAssertFalse(firstFilteredPage.summaries.contains { $0.rideID == selectedRideID })
        let nextCursor = try XCTUnwrap(firstFilteredPage.nextCursor)
        let secondFilteredPage = try state.storedHistoryPage(
            cursor: nextCursor,
            limit: UInt32(pageLimit),
            filter: matchingFilter
        )
        XCTAssertTrue(secondFilteredPage.summaries.contains { $0.rideID == selectedRideID })

        let query = GatedRideHistoryQuery(base: state, failAfterRelease: false)
        let model = RideHistoryModel(stateProvider: { query })
        model.setIncludeShortRides(true)
        await Self.waitUntil("first history page before explicit saved-ride entry") {
            !model.isLoading && model.rides.count == pageLimit && model.canLoadMore
        }
        model.reload(selecting: selectedRideID)
        await Self.waitUntil("explicitly selected older saved ride") {
            !model.isLoading && model.selectedRideID == selectedRideID
                && !model.routeLoading && !model.detailRouteLoading
        }

        model.searchText = searchTerm
        await Self.waitUntil("matching search reload completes") {
            query.historyPageFiltersSnapshot.compactMap { $0 }.last?.searchText == searchTerm
                && !model.isLoading
                && !model.routeLoading
                && !model.detailRouteLoading
        }

        XCTAssertEqual(model.searchText, searchTerm)
        XCTAssertEqual(model.rides.count, pageLimit + 1)
        XCTAssertTrue(model.rides.contains { $0.rideID == selectedRideID })
        XCTAssertEqual(
            model.selectedRideID,
            selectedRideID,
            "A selected ride that matches the active Rust filter should remain in the displayed page even when it falls beyond page one"
        )
    }

    @MainActor
    func testStorageUnavailableReloadRejectsLateInitialPage() async throws {
        let query = InMemoryHistoryQuery(count: 1)
        let rideID = try XCTUnwrap(query.summaries.first?.rideID)
        let availability = RideHistoryAvailability()
        let gate = query.gateNextPage()
        defer { gate.release() }
        let model = RideHistoryModel(
            stateProvider: { query },
            storageErrorProvider: {
                availability.isAvailable ? nil : "Rust ride database is unavailable"
            }
        )

        model.setDateFilter(.allTime)
        await fulfillment(of: [gate.started], timeout: 5)

        availability.isAvailable = false
        model.reload()
        let storageError = MobileRideMapError.storageError("Rust ride database is unavailable")
        XCTAssertEqual(model.error, storageError)
        XCTAssertFalse(model.isLoading)

        gate.release()
        await fulfillment(of: [gate.finished], timeout: 5)
        for _ in 0..<20 { await Task.yield() }

        XCTAssertTrue(model.rides.isEmpty)
        XCTAssertNil(model.selectedRideID)
        XCTAssertFalse(model.canLoadMore)
        XCTAssertFalse(model.isLoading)
        XCTAssertEqual(model.error, storageError)
        XCTAssertEqual(model.routeError, storageError)
        XCTAssertEqual(model.detailRouteError, storageError)

        availability.isAvailable = true
        model.reload()
        await Self.waitUntil("history retry after unavailable storage") {
            !model.isLoading && model.selectedRideID == rideID
        }
        XCTAssertNil(model.error)
    }

    @MainActor
    func testStorageUnavailableReloadRejectsLateCursorPage() async throws {
        let pageLimit = Int(MobileRideMapLimits.rustOwned.historyPageLimit)
        let query = InMemoryHistoryQuery(count: pageLimit + 1)
        let availability = RideHistoryAvailability()
        let model = RideHistoryModel(
            stateProvider: { query },
            storageErrorProvider: {
                availability.isAvailable ? nil : "Rust ride database is unavailable"
            }
        )
        model.setDateFilter(.allTime)
        await Self.waitUntil("first page before unavailable storage") {
            !model.isLoading && model.canLoadMore && !model.routeLoading
        }
        let firstPageRideIDs = model.rides.map(\.rideID)
        let selectedRideID = model.selectedRideID
        let routePoints = model.displayPoints
        XCTAssertFalse(routePoints.isEmpty)

        let gate = query.gateNextPage()
        defer { gate.release() }
        model.loadMore()
        await fulfillment(of: [gate.started], timeout: 5)

        availability.isAvailable = false
        model.reload()
        gate.release()
        await fulfillment(of: [gate.finished], timeout: 5)
        for _ in 0..<20 { await Task.yield() }

        let storageError = MobileRideMapError.storageError("Rust ride database is unavailable")
        XCTAssertEqual(model.rides.map(\.rideID), firstPageRideIDs)
        XCTAssertEqual(model.selectedRideID, selectedRideID)
        XCTAssertEqual(model.displayPoints, routePoints)
        XCTAssertTrue(model.canLoadMore)
        XCTAssertFalse(model.isLoading)
        XCTAssertEqual(model.error, storageError)
        XCTAssertEqual(model.routeError, storageError)
        XCTAssertEqual(model.detailRouteError, storageError)

        availability.isAvailable = true
        model.loadMore()
        await Self.waitUntil("cursor retry after unavailable storage") {
            model.rides.count == pageLimit + 1 && !model.canLoadMore
        }
        XCTAssertNil(model.error)
    }

    @MainActor
    func testHistoryReentryPreservesSelectedRideOutsideFirstPage() async throws {
        let pageLimit = Int(MobileRideMapLimits.rustOwned.historyPageLimit)
        let query = InMemoryHistoryQuery(count: pageLimit + 1)
        let model = RideHistoryModel(stateProvider: { query })
        model.setDateFilter(.allTime)
        await Self.waitUntil("first page before history reentry") {
            !model.isLoading && model.canLoadMore
        }
        let firstPageRideIDs = model.rides.map(\.rideID)
        model.loadMore()
        await Self.waitUntil("second page before history reentry") {
            model.rides.count == pageLimit + 1
        }
        let selectedRideID = try XCTUnwrap(model.rides.last?.rideID)
        XCTAssertFalse(firstPageRideIDs.contains(selectedRideID))
        model.selectFromHistoryList(selectedRideID)
        await Self.waitUntil("paginated ride selection") {
            model.selectedRideID == selectedRideID && !model.routeLoading && !model.detailRouteLoading
        }
        let routePoints = model.displayPoints
        XCTAssertFalse(routePoints.isEmpty)

        model.reloadPreservingSelection()
        await Self.waitUntil("same paginated selection after history reentry") {
            !model.isLoading && !model.routeLoading && !model.detailRouteLoading
        }

        XCTAssertEqual(model.selectedRideID, selectedRideID)
        XCTAssertEqual(model.detailProjectionRideID, selectedRideID)
        XCTAssertTrue(model.rides.contains { $0.rideID == selectedRideID })
        XCTAssertEqual(model.displayPoints, routePoints)
        XCTAssertEqual(model.dateFilter, .allTime)
        XCTAssertTrue(model.canLoadMore)
        XCTAssertNil(model.error)
    }

    @MainActor
    func testSelectionReplacementRejectsLateRouteProjection() async throws {
        let state = MobileRideMapState()

        func saveRide(startingAt startMs: UInt64, latitude: Double) async throws -> String {
            _ = try state.startGpsOnly(atMs: startMs)
            _ = await Self.settle(
                state,
                try state.ingestLocation(
                    monotonicMs: startMs,
                    wallClockUnixMs: 1_700_000_000_000 + startMs,
                    latitudeDegrees: latitude,
                    longitudeDegrees: -104.9000,
                    horizontalAccuracyMeters: 5
                ))
            _ = await Self.settle(
                state,
                try state.ingestLocation(
                    monotonicMs: startMs + 1_000,
                    wallClockUnixMs: 1_700_000_001_000 + startMs,
                    latitudeDegrees: latitude + 0.0001,
                    longitudeDegrees: -104.9000,
                    horizontalAccuracyMeters: 5
                ))
            _ = try state.stop(atMs: startMs + 1_000)
            return try state.save().rideID
        }

        let firstRideID = try await saveRide(startingAt: 100, latitude: 39.7000)
        let secondRideID = try await saveRide(startingAt: 10_000, latitude: 39.7100)
        let query = GatedRideHistoryQuery(base: state, failAfterRelease: false)
        let model = RideHistoryModel(stateProvider: { query })
        model.setDateFilter(.allTime)
        await Self.waitUntil("all-time history before selection race") {
            !model.isLoading
        }
        model.setIncludeShortRides(true)
        await Self.waitUntil("short-ride history before selection race") {
            !model.isLoading
        }
        model.reload(selecting: secondRideID)
        await Self.waitUntil("initial selected history route") {
            model.selectedRideID == secondRideID
                && model.detailProjectionRideID == secondRideID
                && !model.isLoading
                && !model.routeLoading
                && !model.detailRouteLoading
        }
        let currentPoints = model.displayPoints
        XCTAssertFalse(currentPoints.isEmpty)

        query.armNextProjection()
        model.selectFromHistoryList(firstRideID)
        let staleProjectionStarted = await query.waitUntilGatedProjectionStarts()
        XCTAssertTrue(staleProjectionStarted)

        model.selectFromHistoryList(secondRideID)
        await Self.waitUntil("replacement history route projection") {
            model.selectedRideID == secondRideID
                && model.detailProjectionRideID == secondRideID
                && !model.routeLoading
                && !model.detailRouteLoading
        }
        let replacementPoints = model.displayPoints
        XCTAssertEqual(replacementPoints, currentPoints)

        query.releaseGatedProjection()
        let staleProjectionFinished = await query.waitUntilGatedProjectionFinishes()
        XCTAssertTrue(staleProjectionFinished)
        for _ in 0..<20 { await Task.yield() }

        XCTAssertEqual(model.selectedRideID, secondRideID)
        XCTAssertEqual(model.detailProjectionRideID, secondRideID)
        XCTAssertEqual(model.displayPoints, currentPoints)
        XCTAssertEqual(model.detailDisplayPoints, replacementPoints)
        XCTAssertFalse(model.routeLoading)
        XCTAssertFalse(model.detailRouteLoading)
        XCTAssertNil(model.routeError)
        XCTAssertNil(model.detailRouteError)
    }

    @MainActor
    func testSameRideViewportReplacementRejectsLateFailureAndLoadingState() async throws {
        let state = MobileRideMapState()
        let rideID = try await Self.saveHistoryRide(in: state)

        let query = GatedRideHistoryQuery(base: state, failAfterRelease: true)
        let model = RideHistoryModel(stateProvider: { query })
        model.setDateFilter(.allTime)
        await Self.waitUntil("all-time history page before short-ride override") {
            !model.isLoading
        }
        model.setIncludeShortRides(true)
        await Self.waitUntil("short-ride history page before explicit selection") {
            !model.isLoading
        }
        model.reload(selecting: rideID)
        await Self.waitUntil("same-ride history detail") {
            model.selectedRideID == rideID
                && model.detailProjectionRideID == rideID
                && !model.isLoading
                && !model.routeLoading
                && !model.detailRouteLoading
        }

        query.armNextProjection()
        model.projectDetailViewport(
            MobileGeoBoundsDto(
                minimumLatitudeDegrees: 39.6999,
                maximumLatitudeDegrees: 39.70001,
                minimumLongitudeDegrees: -104.9001,
                maximumLongitudeDegrees: -104.8999
            ))
        let staleProjectionStarted = await query.waitUntilGatedProjectionStarts()
        XCTAssertTrue(staleProjectionStarted)

        model.projectDetailViewport(
            MobileGeoBoundsDto(
                minimumLatitudeDegrees: 39.70009,
                maximumLatitudeDegrees: 39.70011,
                minimumLongitudeDegrees: -104.9001,
                maximumLongitudeDegrees: -104.8999
            ))
        await Self.waitUntil("replacement same-ride viewport projection") {
            !model.detailRouteLoading && model.detailDisplayPoints.count == 1
        }
        let replacementPoints = model.detailDisplayPoints
        let replacementVersion = model.detailProjectionVersion
        XCTAssertNil(model.detailRouteError)

        query.releaseGatedProjection()
        let staleProjectionFinished = await query.waitUntilGatedProjectionFinishes()
        XCTAssertTrue(staleProjectionFinished)
        for _ in 0..<20 { await Task.yield() }

        XCTAssertEqual(model.selectedRideID, rideID)
        XCTAssertEqual(model.detailProjectionRideID, rideID)
        XCTAssertEqual(model.detailDisplayPoints, replacementPoints)
        XCTAssertEqual(model.detailProjectionVersion, replacementVersion)
        XCTAssertFalse(model.detailRouteLoading)
        XCTAssertNil(model.detailRouteError)
    }

    @MainActor
    func testSameRideSelectionReplacementRejectsLateFailureAndLoadingState() async throws {
        let state = MobileRideMapState()
        let rideID = try await Self.saveHistoryRide(in: state)

        let query = GatedRideHistoryQuery(base: state, failAfterRelease: true)
        let model = RideHistoryModel(stateProvider: { query })
        model.setDateFilter(.allTime)
        await Self.waitUntil("all-time history page before short-ride override") {
            !model.isLoading
        }
        model.setIncludeShortRides(true)
        await Self.waitUntil("short-ride history page before explicit selection") {
            !model.isLoading
        }
        model.reload(selecting: rideID)
        await Self.waitUntil("initial same-ride selection projection") {
            model.selectedRideID == rideID
                && model.detailProjectionRideID == rideID
                && !model.isLoading
                && !model.routeLoading
                && !model.detailRouteLoading
        }

        query.armNextProjection()
        model.selectFromHistoryList(rideID)
        let staleProjectionStarted = await query.waitUntilGatedProjectionStarts()
        XCTAssertTrue(staleProjectionStarted)

        model.selectFromHistoryList(rideID)
        await Self.waitUntil("replacement same-ride selection projection") {
            !model.routeLoading && !model.detailRouteLoading
        }
        let replacementRoutePoints = model.displayPoints
        let replacementDetailPoints = model.detailDisplayPoints
        let replacementVersion = model.projectionVersion
        let replacementDetailVersion = model.detailProjectionVersion
        XCTAssertFalse(replacementRoutePoints.isEmpty)
        XCTAssertFalse(replacementDetailPoints.isEmpty)

        query.releaseGatedProjection()
        let staleProjectionFinished = await query.waitUntilGatedProjectionFinishes()
        XCTAssertTrue(staleProjectionFinished)
        for _ in 0..<20 { await Task.yield() }

        XCTAssertEqual(model.selectedRideID, rideID)
        XCTAssertEqual(model.detailProjectionRideID, rideID)
        XCTAssertEqual(model.displayPoints, replacementRoutePoints)
        XCTAssertEqual(model.detailDisplayPoints, replacementDetailPoints)
        XCTAssertEqual(model.projectionVersion, replacementVersion)
        XCTAssertEqual(model.detailProjectionVersion, replacementDetailVersion)
        XCTAssertFalse(model.routeLoading)
        XCTAssertFalse(model.detailRouteLoading)
        XCTAssertNil(model.routeError)
        XCTAssertNil(model.detailRouteError)
    }

    @MainActor
    func testSameRideReloadReplacementRejectsLateDetailFailure() async throws {
        let state = MobileRideMapState()
        let rideID = try await Self.saveHistoryRide(in: state)

        let query = GatedRideHistoryQuery(base: state, failAfterRelease: true)
        let model = RideHistoryModel(stateProvider: { query })
        model.setDateFilter(.allTime)
        await Self.waitUntil("initial same-ride history load") {
            !model.isLoading
        }
        model.setIncludeShortRides(true)
        await Self.waitUntil("short-ride history page before explicit selection") {
            !model.isLoading
        }
        model.reload(selecting: rideID)
        await Self.waitUntil("initial same-ride history selection") {
            model.selectedRideID == rideID
                && !model.isLoading
                && !model.routeLoading
                && !model.detailRouteLoading
        }
        let originalPoints = model.displayPoints

        query.armNextProjection()
        model.reload(selecting: rideID)
        let staleProjectionStarted = await query.waitUntilGatedProjectionStarts()
        XCTAssertTrue(staleProjectionStarted)

        model.reload(selecting: rideID)
        await Self.waitUntil("replacement same-ride history reload") {
            !model.isLoading
                && !model.routeLoading
                && !model.detailRouteLoading
        }
        let replacementRoutePoints = model.displayPoints
        let replacementDetailPoints = model.detailDisplayPoints
        let replacementVersion = model.projectionVersion
        let replacementDetailVersion = model.detailProjectionVersion
        XCTAssertEqual(replacementRoutePoints, originalPoints)
        XCTAssertFalse(replacementDetailPoints.isEmpty)

        query.releaseGatedProjection()
        let staleProjectionFinished = await query.waitUntilGatedProjectionFinishes()
        XCTAssertTrue(staleProjectionFinished)
        for _ in 0..<20 { await Task.yield() }

        XCTAssertEqual(model.selectedRideID, rideID)
        XCTAssertEqual(model.detailProjectionRideID, rideID)
        XCTAssertEqual(model.displayPoints, replacementRoutePoints)
        XCTAssertEqual(model.detailDisplayPoints, replacementDetailPoints)
        XCTAssertEqual(model.projectionVersion, replacementVersion)
        XCTAssertEqual(model.detailProjectionVersion, replacementDetailVersion)
        XCTAssertFalse(model.isLoading)
        XCTAssertFalse(model.routeLoading)
        XCTAssertFalse(model.detailRouteLoading)
        XCTAssertNil(model.error)
        XCTAssertNil(model.routeError)
        XCTAssertNil(model.detailRouteError)
    }

    @MainActor
    func testModelDeallocatesWhilePageQueryIsInFlight() async {
        let query = GatedRideHistoryQuery(
            base: MobileRideMapState(),
            failAfterRelease: false
        )
        query.armNextHistoryPage()
        weak var weakModel: RideHistoryModel?
        var model: RideHistoryModel? = RideHistoryModel(stateProvider: { query })

        model?.reload()
        let pageStarted = await query.waitUntilGatedHistoryPageStarts()
        XCTAssertTrue(pageStarted)

        weakModel = model
        model = nil
        XCTAssertNil(weakModel)

        query.releaseGatedHistoryPage()
        let pageFinished = await query.waitUntilGatedHistoryPageFinishes()
        XCTAssertTrue(pageFinished)
    }

    @MainActor
    func testDetailViewportPreservesSourceBudgetOmission() {
        XCTAssertTrue(
            RideHistoryModel.detailPointsAreTruncated(
                sourcePointsOmittedByBudget: true,
                viewportPointsOmittedByBudget: false
            ))
        XCTAssertTrue(
            RideHistoryModel.detailPointsAreTruncated(
                sourcePointsOmittedByBudget: false,
                viewportPointsOmittedByBudget: true
            ))
        XCTAssertFalse(
            RideHistoryModel.detailPointsAreTruncated(
                sourcePointsOmittedByBudget: false,
                viewportPointsOmittedByBudget: false
            ))
    }

    @MainActor
    func testDetailViewportPreservesSourceSegmentBudgetOmission() {
        XCTAssertTrue(
            RideHistoryModel.detailSegmentsAreOmitted(
                sourceSegmentsOmittedByBudget: true,
                viewportSegmentsOmittedByBudget: false
            ))
        XCTAssertTrue(
            RideHistoryModel.detailSegmentsAreOmitted(
                sourceSegmentsOmittedByBudget: false,
                viewportSegmentsOmittedByBudget: true
            ))
        XCTAssertFalse(
            RideHistoryModel.detailSegmentsAreOmitted(
                sourceSegmentsOmittedByBudget: false,
                viewportSegmentsOmittedByBudget: false
            ))
    }
}
