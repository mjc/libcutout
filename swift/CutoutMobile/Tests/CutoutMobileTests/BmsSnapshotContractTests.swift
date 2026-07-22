import XCTest
import CutoutMobileFFI
@testable import CutoutMobile

final class BmsSnapshotContractTests: XCTestCase {
    func testBmsAccessibilityCopyResolvesFromThePackageCatalog() {
        XCTAssertEqual(pevLocalizedText("bms.accessibility.group", Int64(7)), "Cell group 7")
        XCTAssertEqual(pevLocalizedText("bms.accessibility.group_named", Int64(7), "left pack"), "Cell group 7, left pack")
        XCTAssertEqual(pevLocalizedText("bms.accessibility.voltage", "4.071"), "4.071 volts")
        XCTAssertEqual(pevLocalizedText("bms.accessibility.voltage_unavailable"), "voltage unavailable")
        XCTAssertEqual(pevLocalizedText("bms.accessibility.balancing"), "balancing")
        XCTAssertEqual(pevLocalizedText("bms.accessibility.not_balancing"), "not balancing")
        XCTAssertEqual(pevLocalizedText("bms.accessibility.status.nominal"), "nominal")
        XCTAssertEqual(pevLocalizedText("bms.accessibility.status.warning"), "warning")
        XCTAssertEqual(pevLocalizedText("bms.accessibility.status.critical"), "critical")
        XCTAssertEqual(pevLocalizedText("bms.accessibility.status.unknown"), "status unknown")
        XCTAssertEqual(pevLocalizedText("bms.accessibility.show_details"), "Show available details for this cell group")
    }

    func testEnergyProgressUsesAndClampsTypedBatteryLevel() {
        let topology = BmsTopology(
            layoutLabel: "test pack",
            seriesGroupCount: nil,
            parallelCount: nil,
            packCount: 1,
            bmsCount: 1,
            confidence: .verified
        )

        XCTAssertNil(BmsSnapshot(topology: topology).energyProgress)
        XCTAssertEqual(
            BmsSnapshot(topology: topology, energyPercent: BatteryLevel(value: 72)).energyProgress,
            0.72
        )
        XCTAssertEqual(
            BmsSnapshot(topology: topology, energyPercent: BatteryLevel(value: 255)).energyProgress,
            1
        )
    }

    func testGroupAccessibilityDescribesVoltageAlertAndBalancingState() {
        let group = BmsGroupSnapshot(
            index: 7,
            label: "left pack",
            voltage: Voltage(value: 4_071),
            isBalancing: true,
            alertLevel: .warning,
            detail: "sagging under load"
        )

        XCTAssertEqual(group.accessibilityLabel, "Cell group 7, left pack")
        XCTAssertEqual(
            group.accessibilityValue,
            "4.071 volts, warning, balancing, and sagging under load"
        )
        XCTAssertEqual(group.detailSelectionAccessibilityHint, "Show available details for this cell group")

        let unavailable = BmsGroupSnapshot(index: 8, alertLevel: .unknown)
        XCTAssertEqual(unavailable.accessibilityLabel, "Cell group 8")
        XCTAssertEqual(unavailable.accessibilityValue, "voltage unavailable and status unknown")
    }

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
            pageSelector: 3,
            pageKind: "temperature",
            pageVerification: .hardwareVerified,
            energyPercent: BatteryLevel(value: 72),
            voltage: Voltage(value: 81_600),
            current: BatteryCurrent(value: -12_400),
            bmsPackCurrent0: BatteryCurrent(value: -8_100),
            bmsPackCurrent1: BatteryCurrent(value: -4_300),
            cellDelta: VoltageDelta(value: 18),
            lowestGroupIndex: 17,
            highestTemperature: Temperature(value: 37_800),
            temperatureReadings: [Temperature(value: 37_800), Temperature(value: 35_200)],
            highestTemperatureLabel: "right pack",
            balancingSummary: "idle - top groups only",
            balancingDetail: "3 groups bleeding: 03, 11, 19",
            faultSummary: "no active faults",
            faultDetail: "last: under-voltage warning - 3 days ago",
            groups: [
                BmsGroupSnapshot(
                    index: 17,
                    label: "right pack group 17",
                    voltage: Voltage(value: 4_071),
                    temperature: Temperature(value: 34_900),
                    resistance: Resistance(value: 21),
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
        XCTAssertEqual(snapshot.energyPercent, BatteryLevel(value: 72))
        XCTAssertEqual(snapshot.voltage, Voltage(value: 81_600))
        XCTAssertEqual(snapshot.current, BatteryCurrent(value: -12_400))
        XCTAssertEqual(snapshot.cellDelta, VoltageDelta(value: 18))
        XCTAssertEqual(snapshot.groups.map { $0.label }, ["right pack group 17"])
        XCTAssertEqual(snapshot.groups.map { $0.isBalancing }, [true])
        XCTAssertEqual(snapshot.groups.map { $0.resistance }, [Resistance(value: 21)])
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
            voltage: Voltage(value: 75_900),
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

    func testUnavailableSnapshotDoesNotCarryPackDetails() {
        let snapshot = BmsSnapshot(
            availability: .unsupported,
            topology: BmsTopology(
                layoutLabel: "unsupported pack",
                seriesGroupCount: 20,
                parallelCount: 4,
                packCount: 2,
                bmsCount: 2,
                confidence: .verified
            ),
            energyPercent: BatteryLevel(value: 72),
            voltage: Voltage(value: 81_600),
            current: BatteryCurrent(value: -12_400),
            bmsPackCurrent0: BatteryCurrent(value: -8_100),
            bmsPackCurrent1: BatteryCurrent(value: -4_300),
            cellDelta: VoltageDelta(value: 18),
            lowestGroupIndex: 17,
            highestTemperature: Temperature(value: 37_800),
            highestTemperatureLabel: "right pack",
            balancingSummary: "idle",
            balancingDetail: "3 groups bleeding",
            faultSummary: "no active faults",
            faultDetail: "last warning",
            groups: [
                BmsGroupSnapshot(index: 17, voltage: Voltage(value: 4_071), alertLevel: .warning)
            ],
            faults: [
                BmsFault(code: "0x0040", label: "needs decoder", level: .critical)
            ],
            captureActionTitle: "record pack",
            captureActionState: "disabled"
        )

        XCTAssertEqual(snapshot.availability, .unsupported)
        XCTAssertEqual(snapshot.topology.layoutLabel, "unsupported pack")
        XCTAssertNil(snapshot.energyPercent)
        XCTAssertNil(snapshot.pageSelector)
        XCTAssertNil(snapshot.pageKind)
        XCTAssertNil(snapshot.pageVerification)
        XCTAssertNil(snapshot.voltage)
        XCTAssertNil(snapshot.current)
        XCTAssertNil(snapshot.cellDelta)
        XCTAssertNil(snapshot.lowestGroupIndex)
        XCTAssertNil(snapshot.highestTemperature)
        XCTAssertTrue(snapshot.temperatureReadings.isEmpty)
        XCTAssertNil(snapshot.highestTemperatureLabel)
        XCTAssertNil(snapshot.balancingSummary)
        XCTAssertNil(snapshot.balancingDetail)
        XCTAssertNil(snapshot.faultSummary)
        XCTAssertNil(snapshot.faultDetail)
        XCTAssertTrue(snapshot.groups.isEmpty)
        XCTAssertTrue(snapshot.faults.isEmpty)
        XCTAssertNil(snapshot.captureActionTitle)
        XCTAssertNil(snapshot.captureActionState)
    }

    func testSnapshotExposesReadbackRowsForLivePackData() {
        let snapshot = BmsSnapshot(
            topology: BmsTopology(
                layoutLabel: "20S4P split pack",
                seriesGroupCount: 20,
                parallelCount: 4,
                packCount: 2,
                bmsCount: 2,
                confidence: .verified
            ),
            pageSelector: 3,
            pageKind: "temperature",
            pageVerification: .hardwareVerified,
            energyPercent: BatteryLevel(value: 72),
            voltage: Voltage(value: 81_600),
            current: BatteryCurrent(value: -12_400),
            bmsPackCurrent0: BatteryCurrent(value: -8_100),
            bmsPackCurrent1: BatteryCurrent(value: -4_300),
            cellDelta: VoltageDelta(value: 18),
            lowestGroupIndex: 17,
            highestTemperature: Temperature(value: 37_800),
            temperatureReadings: [Temperature(value: 37_800), Temperature(value: 35_200)],
            groups: [
                BmsGroupSnapshot(index: 17, voltage: Voltage(value: 4_071), alertLevel: .warning),
                BmsGroupSnapshot(index: 18, voltage: Voltage(value: 4_089), alertLevel: .nominal)
            ]
        )

        XCTAssertTrue(snapshot.shouldRenderReadback)
        XCTAssertEqual(
            snapshot.readbackRows,
            [
                SessionDebugRow(label: "availability", value: "available"),
                SessionDebugRow(label: "page", value: "temperature #3", role: .transportMetadata),
                SessionDebugRow(label: "page verification", value: "hardware verified", role: .transportMetadata),
                SessionDebugRow(label: "charge", value: "72%"),
                SessionDebugRow(label: "voltage", value: "81.6"),
                SessionDebugRow(label: "current", value: "-12.4"),
                SessionDebugRow(label: "bms current 0", value: "-8.1"),
                SessionDebugRow(label: "bms current 1", value: "-4.3"),
                SessionDebugRow(label: "high group", value: "4.089"),
                SessionDebugRow(label: "low group", value: "4.071"),
                SessionDebugRow(label: "delta", value: "18"),
                SessionDebugRow(label: "lowest group", value: "17"),
                SessionDebugRow(label: "temperature", value: "37.8"),
                SessionDebugRow(label: "temperature sensors", value: "2"),
                SessionDebugRow(label: "topology", value: "20S4P split pack"),
            ]
        )
    }

    func testSnapshotExposesOverviewHelpersForLivePackData() {
        let snapshot = BmsSnapshot(
            topology: BmsTopology(
                layoutLabel: "20S4P split pack",
                seriesGroupCount: 20,
                parallelCount: 4,
                packCount: 2,
                bmsCount: 2,
                confidence: .verified
            ),
            cellDelta: VoltageDelta(value: 18),
            lowestGroupIndex: 17,
            groups: [
                BmsGroupSnapshot(index: 17, voltage: Voltage(value: 4_071), alertLevel: .warning),
                BmsGroupSnapshot(index: 18, voltage: Voltage(value: 4_089), alertLevel: .nominal)
            ]
        )

        XCTAssertEqual(snapshot.averageGroupVoltage, Voltage(value: 4_080))
        XCTAssertEqual(snapshot.lowestGroupLabel, "group 17")
    }

    func testSnapshotExposesCellMapHelpersForFlaggedGroups() {
        let snapshot = BmsSnapshot(
            topology: BmsTopology(
                layoutLabel: "large EUC pack",
                seriesGroupCount: 40,
                parallelCount: 4,
                packCount: 1,
                bmsCount: 1,
                confidence: .verified
            ),
            cellDelta: VoltageDelta(value: 18),
            lowestGroupIndex: 17,
            highestTemperatureLabel: "group 31",
            groups: [
                BmsGroupSnapshot(index: 17, voltage: Voltage(value: 4_071), alertLevel: .warning),
                BmsGroupSnapshot(index: 18, voltage: Voltage(value: 4_068), alertLevel: .warning),
                BmsGroupSnapshot(
                    index: 19,
                    voltage: Voltage(value: 4_066),
                    alertLevel: .critical,
                    detail: "sagging under load"
                ),
                BmsGroupSnapshot(index: 31, voltage: Voltage(value: 4_089), alertLevel: .warning)
            ]
        )

        XCTAssertEqual(snapshot.cellMapVisibilitySummary, "4 groups visible")
        XCTAssertEqual(snapshot.cellMapSpreadSummary, "18 mV spread")
        XCTAssertEqual(snapshot.cellMapFocusSummary, "groups 17, 18, 19, 31 flagged")
        XCTAssertEqual(snapshot.cellMapFocusDetail, "sagging under load")
    }

    func testSnapshotExposesDetailHelpersForSelectedGroup() {
        let snapshot = BmsSnapshot(
            topology: BmsTopology(
                layoutLabel: "large EUC pack",
                seriesGroupCount: 40,
                parallelCount: 4,
                packCount: 1,
                bmsCount: 1,
                confidence: .verified
            ),
            cellDelta: VoltageDelta(value: 18),
            lowestGroupIndex: 17,
            highestTemperatureLabel: "group 31",
            groups: [
                BmsGroupSnapshot(
                    index: 17,
                    label: "front pack group 17",
                    voltage: Voltage(value: 4_071),
                    temperature: Temperature(value: 34_900),
                    resistance: Resistance(value: 21),
                    alertLevel: .warning,
                    detail: "sagging under load"
                ),
                BmsGroupSnapshot(index: 18, voltage: Voltage(value: 4_089), alertLevel: .nominal)
            ]
        )

        XCTAssertEqual(snapshot.detailGroupStatus(for: 17), "lowest group · 18 mV below pack avg")
        XCTAssertEqual(snapshot.detailGroupTrend(for: 17), "sagging under load")
        XCTAssertEqual(snapshot.detailGroupTrendDetail(for: 17), "18 mV spread")
    }

    func testSnapshotExposesUnknownTopologyHelpers() {
        let snapshot = BmsSnapshot(
            topology: BmsTopology(
                layoutLabel: "topology unverified",
                seriesGroupCount: nil,
                parallelCount: nil,
                packCount: 1,
                bmsCount: 1,
                confidence: .unverified
            ),
            voltage: Voltage(value: 75_900),
            faultSummary: "BMS found, map unknown",
            faultDetail: "show raw-safe info until topology is confirmed",
            faults: [
                BmsFault(code: "0x0040", label: "needs decoder", level: .critical)
            ],
            captureActionTitle: "record unsupported pack",
            captureActionState: "disabled for launch"
        )

        XCTAssertEqual(snapshot.unknownTopologyVoltageDetail, "topology unverified")
        XCTAssertEqual(snapshot.unknownTopologyCellCountValue, "?")
        XCTAssertEqual(snapshot.unknownTopologyCellCountDetail, "layout unverified")
        XCTAssertNil(snapshot.unknownTopologyTemperatureSensorCount)
        XCTAssertEqual(snapshot.unknownTopologyTemperatureDetail, "sensor names unavailable")
        XCTAssertEqual(snapshot.unknownTopologyCaptureDetail, "show raw-safe info until topology is confirmed")
    }

    func testUnknownTopologyTemperatureSensorCountUsesOnlyReportedTemperatures() {
        let snapshot = BmsSnapshot(
            topology: BmsTopology(
                layoutLabel: "topology unverified",
                seriesGroupCount: nil,
                parallelCount: nil,
                packCount: 1,
                bmsCount: 1,
                confidence: .unverified
            ),
            groups: [
                BmsGroupSnapshot(index: 0, voltage: Voltage(value: 4_000), temperature: Temperature(value: 30_000)),
                BmsGroupSnapshot(index: 1, voltage: Voltage(value: 4_000)),
                BmsGroupSnapshot(index: 2, voltage: Voltage(value: 4_000), temperature: Temperature(value: 31_000))
            ]
        )

        XCTAssertEqual(snapshot.unknownTopologyTemperatureSensorCount, 2)
    }

    func testSnapshotExposesScrollableCellMapHelpers() {
        let snapshot = BmsSnapshot(
            topology: BmsTopology(
                layoutLabel: "large EUC pack",
                seriesGroupCount: 40,
                parallelCount: 4,
                packCount: 1,
                bmsCount: 1,
                confidence: .verified
            ),
            groups: [
                BmsGroupSnapshot(index: 17, voltage: Voltage(value: 4_071), temperature: Temperature(value: 34_900), resistance: Resistance(value: 21), alertLevel: .warning),
                BmsGroupSnapshot(index: 18, voltage: Voltage(value: 4_089), alertLevel: .nominal)
            ] + (19...56).map { index in
                BmsGroupSnapshot(index: index, voltage: Voltage(value: 4_080), alertLevel: .nominal)
            }
        )

        XCTAssertEqual(snapshot.inlineCellMapModes, [.balanceView, .temperatures, .faults])
        XCTAssertEqual(snapshot.scrollableCellMapModes, [.overview, .strip, .rawTable, .temperatures])
        XCTAssertEqual(snapshot.inlineCellMapModes.map(\.title), ["balance view", "temps", "faults"])
        XCTAssertEqual(snapshot.scrollableCellMapModes.map(\.title), ["overview", "strip", "raw table", "temps"])
        XCTAssertEqual(snapshot.cellMapInteractionHint, "tap a group for history, IR estimate, and BMS raw fields")
        XCTAssertEqual(snapshot.scrollableCellMapRule, "40 groups need overview before exact cells")
        XCTAssertEqual(snapshot.scrollableCellMapFocusHint, "show flagged groups before the raw table")
    }

    func testSnapshotExposesNoDataHelpers() {
        let snapshot = BmsSnapshot(
            availability: .unsupported,
            topology: BmsTopology(
                layoutLabel: "non-smart BMS",
                seriesGroupCount: nil,
                parallelCount: nil,
                packCount: 1,
                bmsCount: 0,
                confidence: .inferred
            ),
            energyPercent: BatteryLevel(value: 71),
            voltage: Voltage(value: 117_600),
            current: BatteryCurrent(value: 38_000),
            captureActionTitle: "Trust sag, alarms, and headroom more than percent.",
            captureActionState: "limited data"
        )

        XCTAssertEqual(snapshot.noDataWarningTitle, "No cell-level BMS data")
        XCTAssertEqual(snapshot.noDataWarningLines.map(\.id), [
            .cellBalanceWarning,
            .bmsDiagnosticsWarning,
        ])
        XCTAssertEqual(snapshot.noDataWarningLines.map(\.text), [
            "CutOut can’t see individual cell balance or weak groups.",
            "BMS temperature, faults, or cutout reason stay unavailable.",
        ])
        XCTAssertEqual(snapshot.noDataUnknownRows.map(\.id), [
            .cellVoltages,
            .weakGroups,
            .bmsDiagnostics,
        ])
        XCTAssertEqual(snapshot.noDataUnknownRows.map(\.text), [
            "individual cell/group voltages",
            "cell balance / weak parallel group",
            "BMS temperature, faults, and cutout reason",
        ])
    }
}
