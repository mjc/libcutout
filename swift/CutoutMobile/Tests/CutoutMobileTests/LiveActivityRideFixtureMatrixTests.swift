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

    func testFixtureMatrixUsesProductionRideLabelsAndStatusValues() {
        let snapshots = LiveActivityRideFixtureMatrix.v1.fixtures.map(\.snapshot)

        XCTAssertEqual(snapshots.map(\.packVoltage.label).allSatisfy { $0 == "Voltage" }, true)
        XCTAssertEqual(snapshots.map(\.temperature.label).allSatisfy { $0 == "Temp" }, true)
        XCTAssertEqual(snapshots.first?.headroom.value, "Headroom good")
        XCTAssertEqual(snapshots.first?.headroom.unit, nil)
        XCTAssertEqual(snapshots.first?.beeps.value, "Beeps armed")
    }

    func testFixtureMatrixMatchesProductionUnavailableAndNotApplicableSemantics() {
        let fixtures = Dictionary(uniqueKeysWithValues: LiveActivityRideFixtureMatrix.v1.fixtures.map { ($0.kind, $0.snapshot) })

        XCTAssertEqual(fixtures[.waitingForFirstTelemetry]?.distance.state, .unavailable)
        XCTAssertEqual(fixtures[.disconnected]?.distance.state, .unavailable)

        let parked = fixtures[.parked]
        XCTAssertEqual(parked?.speed, .available(label: "Speed", value: "0.0", unit: "mph", source: .fixture))
        XCTAssertEqual(parked?.duration.state, .deferred)
        XCTAssertEqual(parked?.distance.state, .unavailable)
        XCTAssertEqual(parked?.pwm.state, .notApplicable)
        XCTAssertEqual(parked?.headroom.state, .notApplicable)

        let notApplicableUnits = LiveActivityRideFixtureMatrix.v1.fixtures
            .flatMap(\.snapshot.visibleValues)
            .filter { $0.state == .notApplicable }
            .compactMap(\.unit)
        XCTAssertEqual(notApplicableUnits, [])
    }
}
