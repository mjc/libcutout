import XCTest
@testable import CutoutMobile

final class BmsSnapshotContractTests: XCTestCase {
    func testSnapshotPreservesSplitPackIdentityAndGroupMetadata() {
        let snapshot = BmsSnapshot(
            topology: BmsTopology(
                layoutLabel: "20S4P split pack",
                seriesGroupCount: 20,
                parallelCount: 4,
                packCount: 2,
                bmsCount: 2,
                confidence: .verified
            ),
            energyPercent: TelemetryReading.fixture(value: 72),
            voltage: TelemetryReading.fixture(value: 81_600),
            current: TelemetryReading.fixture(value: -12_400),
            cellDeltaMillivolts: TelemetryReading.fixture(value: 18),
            lowestGroupIndex: 17,
            highestTemperature: TelemetryReading.fixture(value: 37_800),
            highestTemperatureLabel: "right pack",
            balancingSummary: "idle - top groups only",
            balancingDetail: "3 groups bleeding: 03, 11, 19",
            faultSummary: "no active faults",
            faultDetail: "last: under-voltage warning - 3 days ago",
            groups: [
                BmsGroupSnapshot(
                    index: 17,
                    label: "right pack group 17",
                    voltage: TelemetryReading.fixture(value: 4_071),
                    temperature: TelemetryReading.fixture(value: 34_900),
                    resistanceMilliohms: 21,
                    isBalancing: true,
                    alertLevel: .warning,
                    detail: "drops first during acceleration"
                )
            ],
            faults: [
                BmsFault(code: "0x0040", label: "needs decoder", level: .critical)
            ],
            captureActionTitle: "record unsupported pack",
            captureActionState: "disabled for launch"
        )

        XCTAssertEqual(snapshot.topology.layoutLabel, "20S4P split pack")
        XCTAssertEqual(snapshot.topology.packCount, 2)
        XCTAssertEqual(snapshot.topology.bmsCount, 2)
        XCTAssertEqual(snapshot.topology.confidence, BmsTopologyConfidence.verified)
        XCTAssertEqual(snapshot.groups.map { $0.label }, ["right pack group 17"])
        XCTAssertEqual(snapshot.groups.map { $0.isBalancing }, [true])
        XCTAssertEqual(snapshot.groups.map { $0.alertLevel }, [BmsAlertLevel.warning])
        XCTAssertEqual(snapshot.captureActionState, "disabled for launch")
    }

    func testSnapshotKeepsUnknownTopologyExplicitWithoutInventedGroups() {
        let snapshot = BmsSnapshot(
            topology: BmsTopology(
                layoutLabel: "topology unverified",
                seriesGroupCount: nil,
                parallelCount: nil,
                packCount: 1,
                bmsCount: 1,
                confidence: .unverified
            ),
            voltage: TelemetryReading.fixture(value: 75_900),
            faultSummary: "BMS found, map unknown",
            faultDetail: "show raw-safe info until topology is confirmed",
            groups: [],
            faults: [BmsFault(code: "0x0040", label: "needs decoder", level: .critical)],
            captureActionTitle: "record unsupported pack",
            captureActionState: "disabled for launch"
        )

        XCTAssertEqual(snapshot.topology.confidence, BmsTopologyConfidence.unverified)
        XCTAssertNil(snapshot.topology.seriesGroupCount)
        XCTAssertTrue(snapshot.groups.isEmpty)
        XCTAssertEqual(snapshot.faults.map { $0.code }, ["0x0040"])
    }
}
