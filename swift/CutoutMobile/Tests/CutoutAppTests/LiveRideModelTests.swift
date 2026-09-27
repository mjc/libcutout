import CutoutMobile
import XCTest

@testable import CutoutApp

final class LiveRideModelTests: XCTestCase {
    @MainActor
    func testDurationUsesRustSnapshotAcrossPauseResumeAndStop() throws {
        let state = MobileRideMapState()
        var now: UInt64 = 1_000
        let model = LiveRideModel(
            state: state,
            storageError: nil,
            availability: .ready,
            now: { now }
        )

        let started = try state.startGpsOnly(atMs: now)
        model.applyCommandSnapshot(started, resetPoints: true)
        now = 4_000
        model.refreshDuration()
        XCTAssertEqual(model.snapshot?.summary.durationMilliseconds, 3_000)

        let paused = try state.pause(atMs: now)
        model.applyCommandSnapshot(paused, resetPoints: false)
        now = 9_000
        model.refreshDuration()
        XCTAssertEqual(model.snapshot?.summary.durationMilliseconds, 3_000)

        let resumed = try state.resume(atMs: 10_000)
        model.applyCommandSnapshot(resumed, resetPoints: false)
        now = 15_000
        model.refreshDuration()
        XCTAssertEqual(model.snapshot?.summary.durationMilliseconds, 8_000)

        let stopped = try state.stop(atMs: 16_000)
        model.applyCommandSnapshot(stopped, resetPoints: false)
        now = 20_000
        model.refreshDuration()
        XCTAssertEqual(model.snapshot?.summary.durationMilliseconds, 9_000)
    }
}
