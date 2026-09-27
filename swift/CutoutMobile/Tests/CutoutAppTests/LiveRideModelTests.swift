import CutoutMobileFFI
import Synchronization
import XCTest

@testable import CutoutApp
@testable import CutoutMobile

final class LiveRideModelTests: XCTestCase {
    @MainActor
    func testHeldProjectionCoalescesAcceptedOutcomesAndRejectsStaleResult() async throws {
        let state = MobileRideMapState()
        let started = try state.startGpsOnly(atMs: 1_000)
        let firstStarted = expectation(description: "first projection entered")
        let secondFinished = expectation(description: "coalesced projection finished")
        let query = HeldLiveRideQuery(
            state: state,
            firstStarted: firstStarted,
            secondFinished: secondFinished
        )
        defer { query.releaseFirst.signal() }
        let model = LiveRideModel(
            state: query,
            storageError: nil,
            availability: .ready,
            now: { 1_000 }
        )
        model.applyCommandSnapshot(started, resetPoints: true)

        model.applyDecision(snapshot: started, decision: .accepted(point: point(sequence: 1)))
        await fulfillment(of: [firstStarted], timeout: 3)
        model.applyDecision(snapshot: started, decision: .accepted(point: point(sequence: 2)))
        model.applyDecision(snapshot: started, decision: .accepted(point: point(sequence: 3)))
        XCTAssertEqual(query.stats.withLock { $0.calls }, 1)

        query.releaseFirst.signal()
        await fulfillment(of: [secondFinished], timeout: 3)
        let deadline = ContinuousClock.now.advanced(by: .seconds(2))
        while model.displayPoints.first?.sequence != 2 && ContinuousClock.now < deadline {
            try await Task.sleep(for: .milliseconds(10))
        }
        XCTAssertEqual(model.displayPoints.map(\.sequence), [2])
        XCTAssertEqual(query.stats.withLock { $0.calls }, 2)
        XCTAssertEqual(query.stats.withLock { $0.maxConcurrent }, 1)
        XCTAssertEqual(
            query.stats.withLock { $0.budgets },
            [MobileRideMapLimits.rustOwned.liveTailPointLimit, MobileRideMapLimits.rustOwned.liveTailPointLimit]
        )
        XCTAssertTrue(query.stats.withLock { $0.liveTokenCancelled })
        XCTAssertTrue(query.stats.withLock { $0.durableTokenCancelled })
    }

    @MainActor
    func testDurationUsesRustSnapshotAcrossPauseResumeAndStop() throws {
        let state = MobileRideMapState()
        let now = Mutex<UInt64>(1_000)
        let model = LiveRideModel(
            state: state,
            storageError: nil,
            availability: .ready,
            now: { now.withLock { $0 } }
        )

        let started = try state.startGpsOnly(atMs: now.withLock { $0 })
        model.applyCommandSnapshot(started, resetPoints: true)
        now.withLock { $0 = 4_000 }
        model.refreshDuration()
        XCTAssertEqual(model.snapshot?.summary.durationMilliseconds, 3_000)

        let paused = try state.pause(atMs: now.withLock { $0 })
        model.applyCommandSnapshot(paused, resetPoints: false)
        now.withLock { $0 = 9_000 }
        model.refreshDuration()
        XCTAssertEqual(model.snapshot?.summary.durationMilliseconds, 3_000)

        let resumed = try state.resume(atMs: 10_000)
        model.applyCommandSnapshot(resumed, resetPoints: false)
        now.withLock { $0 = 15_000 }
        model.refreshDuration()
        XCTAssertEqual(model.snapshot?.summary.durationMilliseconds, 8_000)

        let stopped = try state.stop(atMs: 16_000)
        model.applyCommandSnapshot(stopped, resetPoints: false)
        now.withLock { $0 = 20_000 }
        model.refreshDuration()
        XCTAssertEqual(model.snapshot?.summary.durationMilliseconds, 9_000)
    }

    private func point(sequence: UInt64) -> MobileRideMapPointDto {
        MobileRideMapPointDto(
            sequence: sequence,
            segmentId: 1,
            startReason: .initial,
            latitudeDegrees: 40,
            longitudeDegrees: -105,
            wallClockUnixMs: 1_000,
            monotonicMs: 1_000,
            horizontalAccuracyMeters: 1,
            telemetryState: .gpsOnly
        )
    }
}

private final class HeldLiveRideQuery: LiveRideQuerying {
    struct Stats: Sendable {
        var calls = 0
        var active = 0
        var maxConcurrent = 0
        var budgets = [UInt32]()
        var liveTokenCancelled = false
        var durableTokenCancelled = false
    }

    let stats = Mutex(Stats())
    let releaseFirst = DispatchSemaphore(value: 0)
    private let state: MobileRideMapState
    private let firstStarted: XCTestExpectation
    private let secondFinished: XCTestExpectation

    init(
        state: MobileRideMapState,
        firstStarted: XCTestExpectation,
        secondFinished: XCTestExpectation
    ) {
        self.state = state
        self.firstStarted = firstStarted
        self.secondFinished = secondFinished
    }

    func currentSnapshot() -> MobileRideMapSnapshotDto? { state.currentSnapshot() }

    func currentSnapshot(atMs: UInt64) -> MobileRideMapSnapshotDto? {
        state.currentSnapshot(atMs: atMs)
    }

    func projectStoredPoints(
        rideID: String,
        budget: UInt32,
        viewport: MobileGeoBoundsDto?,
        privacy: MobileRideMapRoutePrivacyPolicy,
        cancellation: MobileRideMapProjectionCancellation?
    ) throws -> MobileRideMapRouteProjection {
        try state.projectStoredPoints(
            rideID: rideID,
            budget: budget,
            viewport: viewport,
            privacy: privacy,
            cancellation: cancellation
        )
    }

    func projectCurrentRoutePoints(
        budget: UInt32,
        rideID: String?,
        viewport: MobileGeoBoundsDto?,
        privacy: MobileRideMapRoutePrivacyPolicy,
        durableCancellation: MobileRideMapProjectionCancellation?,
        liveCancellation: MobileLiveRideMapProjectionCancellation?
    ) throws -> MobileRideMapRouteProjection {
        let call = stats.withLock { stats in
            stats.calls += 1
            stats.active += 1
            stats.maxConcurrent = max(stats.maxConcurrent, stats.active)
            stats.budgets.append(budget)
            return stats.calls
        }
        defer { stats.withLock { $0.active -= 1 } }
        if call == 1 {
            firstStarted.fulfill()
            releaseFirst.wait()
            let liveCancelled = cancellationObserved {
                try state.projectPoints(
                    budget: 1,
                    cancellation: liveCancellation
                )
            }
            let durableCancelled = cancellationObserved {
                try state.projectStoredPoints(
                    rideID: rideID ?? "",
                    budget: 1,
                    cancellation: durableCancellation
                )
            }
            stats.withLock {
                $0.liveTokenCancelled = liveCancelled
                $0.durableTokenCancelled = durableCancelled
            }
        } else {
            secondFinished.fulfill()
        }
        let sequence = UInt64(call)
        return MobileRideMapRouteProjection(
            points: [
                MobileRideMapRouteDisplayPoint(
                    sequence: sequence,
                    segmentId: 1,
                    latitudeDegrees: 40,
                    longitudeDegrees: -105,
                    privacyClass: .precise
                )
            ],
            segments: [],
            sourcePointCount: sequence,
            sourceSegmentCount: 1,
            candidatePointCount: 1,
            candidateSegmentCount: 0,
            displayedSegmentCount: 0,
            backgroundGapCount: 0,
            presence: .visible
        )
    }

    private func cancellationObserved(
        _ operation: () throws -> MobileRideMapRouteProjection
    ) -> Bool {
        do {
            _ = try operation()
            return false
        } catch {
            return error as? MobileRideMapError == .cancelled
        }
    }
}
