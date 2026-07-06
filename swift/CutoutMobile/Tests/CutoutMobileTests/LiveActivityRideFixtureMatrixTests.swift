import XCTest
@testable import CutoutMobile

final class LiveActivityRideFixtureMatrixTests: XCTestCase {
    func testFixtureMatrixCoversTheExpectedStateSet() {
        XCTAssertEqual(
            LiveActivityRideFixtureMatrix.v1.fixtures.map(\.kind),
            [.demo, .populated, .partial, .waitingForFirstTelemetry, .stale, .disconnected, .parked]
        )
    }

    func testFixtureMatrixProvidesTypedSnapshotsForNonLiveValidation() {
        let snapshots = LiveActivityRideFixtureMatrix.v1.fixtures.map(\.snapshot)

        XCTAssertEqual(snapshots.first?.connectionState, .fixture)
        XCTAssertEqual(snapshots.first?.visibleValues.allSatisfy { $0.source == .fixture }, true)
        XCTAssertEqual(snapshots.first(where: { $0.connectionState == .waitingForFirstTelemetry })?.speed.state, .unavailable)
        XCTAssertEqual(snapshots.first(where: { $0.connectionState == .stale })?.speed.state, .stale)
        XCTAssertEqual(snapshots.first(where: { $0.connectionState == .disconnected })?.sessionStatus.state, .unavailable)
        XCTAssertEqual(snapshots.first(where: { $0.pwm.state == .notApplicable })?.headroom.state, .notApplicable)
    }
}
