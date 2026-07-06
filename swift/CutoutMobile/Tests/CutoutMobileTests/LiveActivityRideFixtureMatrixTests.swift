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

    func testFixtureValuesDoNotEmbedTheirUnits() {
        let values = LiveActivityRideFixtureMatrix.v1.fixtures.flatMap(\.snapshot.visibleValues)

        let duplicatedUnits = values.compactMap { value -> String? in
            guard
                let unit = value.unit,
                unit.isEmpty == false,
                value.state == .available || value.state == .stale
            else { return nil }

            return value.value.hasSuffix(unit) ? "\(value.label)=\(value.value) \(unit)" : nil
        }

        XCTAssertEqual(duplicatedUnits, [])
    }
}
