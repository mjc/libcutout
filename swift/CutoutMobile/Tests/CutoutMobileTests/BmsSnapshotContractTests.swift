import XCTest
import CutoutMobileFFI
@testable import CutoutMobile

final class BmsSnapshotContractTests: XCTestCase {
    func testBmsCellMapPresentationResolvesFromThePackageCatalog() {
        XCTAssertEqual(pevLocalizedText("bms.cell_map.groups_visible", Int64(4)), "4 groups visible")
        XCTAssertEqual(pevLocalizedText("bms.cell_map.flagged", "17, 18"), "groups 17, 18 flagged")
        XCTAssertEqual(
            pevLocalizedText("bms.detail.group_from_average", Int64(17), Int64(18)),
            "group 17 · 18 mV from pack avg"
        )
        XCTAssertEqual(pevLocalizedText("bms.detail.history_unavailable"), "not enough history")
        XCTAssertEqual(pevLocalizedText("bms.detail.unnamed_group", Int64(17)), "group 17")
        XCTAssertEqual(pevLocalizedText("bms.topology.layout_verified"), "layout verified")
        XCTAssertEqual(
            pevLocalizedText("bms.cell_map.hint.resistance"),
            "tap a group for history, IR estimate, and BMS raw fields"
        )
        XCTAssertEqual(
            pevLocalizedText("bms.cell_map.overview_rule", Int64(40)),
            "40 groups need overview before exact cells"
        )
        XCTAssertEqual(pevLocalizedText("bms.no_data.title"), "No cell-level BMS data")
    }

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

    func testVerificationStatesResolveFromThePackageCatalog() {
        let cases: [(VerificationState, String, String)] = [
            (.unverified, "verification.unverified", "unverified"),
            (.inferred, "verification.inferred", "inferred"),
            (.sourceVerified, "verification.source_verified", "source verified"),
            (.hardwareVerified, "verification.hardware_verified", "hardware verified"),
            (.sourceAndHardwareVerified, "verification.source_and_hardware_verified", "source and hardware verified"),
        ]

        for (state, key, expected) in cases {
            XCTAssertEqual(pevLocalizedText(key), expected)
            XCTAssertEqual(state.displayText, pevLocalizedText(key))
        }
    }

    func testBmsNoDataRowsResolveFromThePackageCatalog() {
        let cases: [(BmsNoDataTextRow, String, String)] = [
            (.cellBalanceWarning, "bms.no_data.row.cell_balance_warning", "CutOut can’t see individual cell balance or weak groups."),
            (.bmsDiagnosticsWarning, "bms.no_data.row.diagnostics_warning", "BMS temperature, faults, or cutout reason stay unavailable."),
            (.cellVoltages, "bms.no_data.row.cell_voltages", "individual cell/group voltages"),
            (.weakGroups, "bms.no_data.row.weak_groups", "cell balance / weak parallel group"),
            (.bmsDiagnostics, "bms.no_data.row.diagnostics", "BMS temperature, faults, and cutout reason"),
        ]

        for (row, key, expected) in cases {
            XCTAssertEqual(pevLocalizedText(key), expected)
            XCTAssertEqual(row.text, pevLocalizedText(key))
        }
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
        XCTAssertEqual(BmsSnapshot(topology: topology).energyMetricValue, .unavailable)
        XCTAssertEqual(
            BmsSnapshot(topology: topology, energyPercent: BatteryLevel(value: 72)).energyProgress,
            0.72
        )
        XCTAssertEqual(
            BmsSnapshot(topology: topology, energyPercent: BatteryLevel(value: 72)).energyMetricValue,
            .available(display: "72%", accessibility: "72%")
        )

        XCTAssertEqual(
            BmsSnapshot(
                topology: topology,
                energyPercent: BatteryLevel(value: 72),
                energyPercentSource: .estimated
            ).energyPercentSource,
            .estimated
        )
        XCTAssertNil(
            BmsSnapshot(topology: topology, energyPercentSource: .estimated).energyPercentSource
        )
        XCTAssertEqual(
            BmsSnapshot(topology: topology, energyPercent: BatteryLevel(value: 255)).energyProgress,
            1
        )

        XCTAssertEqual(BmsSnapshot(topology: topology).voltageMetricValue, .unavailable)
        let voltageText = RideUnits.voltageText(millivolts: 81_600)
        XCTAssertEqual(
            BmsSnapshot(topology: topology, voltage: Voltage(value: 81_600)).voltageMetricValue,
            .available(display: voltageText, accessibility: voltageText)
        )
    }

    func testCellMapSummaryMetricValuesKeepStatusDistinctFromAvailableData() {
        let topology = BmsTopology(
            layoutLabel: "test pack",
            seriesGroupCount: nil,
            parallelCount: nil,
            packCount: 1,
            bmsCount: 1,
            confidence: .verified
        )
        let unavailableSpread = BmsSnapshot(topology: topology)
        XCTAssertEqual(
            unavailableSpread.cellMapSpreadMetricValue,
            .status(
                display: unavailableSpread.cellMapSpreadSummary,
                accessibility: unavailableSpread.cellMapSpreadSummary
            )
        )

        let snapshot = BmsSnapshot(
            topology: topology,
            cellDelta: VoltageDelta(value: 18),
            lowestGroupIndex: 7,
            groups: [BmsGroupSnapshot(index: 7)]
        )
        XCTAssertEqual(
            snapshot.cellMapVisibilityMetricValue,
            .available(
                display: snapshot.cellMapVisibilitySummary,
                accessibility: snapshot.cellMapVisibilitySummary
            )
        )
        XCTAssertEqual(
            snapshot.cellMapSpreadMetricValue,
            .available(
                display: snapshot.cellMapSpreadSummary,
                accessibility: snapshot.cellMapSpreadSummary
            )
        )
        XCTAssertEqual(
            snapshot.cellMapFocusMetricValue,
            .status(
                display: snapshot.cellMapFocusSummary,
                accessibility: snapshot.cellMapFocusSummary
            )
        )
        let trend = pevLocalizedText("bms.detail.trend", snapshot.detailGroupTrend(for: 7))
        XCTAssertEqual(
            snapshot.detailGroupTrendMetricValue(for: 7),
            .status(display: trend, accessibility: trend)
        )
    }

    func testNoDataMetricsAreOwnedByTheBmsSnapshot() {
        let topology = BmsTopology(
            layoutLabel: "test pack",
            seriesGroupCount: nil,
            parallelCount: nil,
            packCount: 1,
            bmsCount: 1,
            confidence: .verified
        )
        let snapshot = BmsSnapshot(
            topology: topology,
            energyPercent: BatteryLevel(value: 72),
            captureActionTitle: "record unsupported pack"
        )

        let fallbackEstimate = RideUnits.decimalString(72, fractionDigits: 0)
        XCTAssertEqual(
            snapshot.noDataPackEstimateMetricValue(controllerEstimatePercent: nil),
            .available(display: fallbackEstimate, accessibility: fallbackEstimate)
        )
        let controllerEstimate = RideUnits.decimalString(64, fractionDigits: 0)
        XCTAssertEqual(
            snapshot.noDataPackEstimateMetricValue(controllerEstimatePercent: BatteryLevel(value: 64)),
            .available(display: controllerEstimate, accessibility: controllerEstimate)
        )
        XCTAssertEqual(
            BmsSnapshot(topology: topology).noDataPackEstimateMetricValue(controllerEstimatePercent: nil),
            .unavailable
        )
        XCTAssertEqual(
            snapshot.captureActionMetricValue,
            .available(display: "record unsupported pack", accessibility: "record unsupported pack")
        )
        XCTAssertEqual(BmsSnapshot(topology: topology).captureActionMetricValue, .unavailable)
    }

    func testSharedBmsMetricFormattersKeepUnavailableAndZeroDistinct() {
        XCTAssertEqual(BmsGroupSnapshot(index: 7).voltageMetricValue, .unavailable)
        XCTAssertEqual(
            BmsGroupSnapshot(index: 7, voltage: Voltage(value: 4_036)).voltageMetricValue,
            .available(display: "4.036", accessibility: "4.036")
        )
        XCTAssertEqual(
            bmsPackVoltageMetricValue(Voltage(value: 0)),
            .available(display: "0.0", accessibility: "0.0")
        )
        XCTAssertEqual(
            bmsBatteryCurrentMetricValue(BatteryCurrent(value: 0)),
            .available(display: "0", accessibility: "0")
        )
    }

    func testGroupAccessibilityDescribesVoltageAlertAndBalancingState() {
        let group = BmsGroupSnapshot(
            index: 7,
            label: "left pack",
            voltage: Voltage(value: 4_071),
            temperature: Temperature(value: 34_900),
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
        XCTAssertEqual(
            group.voltageMetricValue,
            .available(display: "4.071", accessibility: "4.071")
        )
        XCTAssertEqual(
            group.temperatureMetricValue,
            .available(display: "34.9", accessibility: "34.9")
        )

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
        XCTAssertEqual(
            snapshot.balancingMetricValue,
            .status(display: "idle - top groups only", accessibility: "idle - top groups only")
        )
        XCTAssertEqual(snapshot.balancingMetricDetail, "3 groups bleeding: 03, 11, 19")
        XCTAssertEqual(
            snapshot.faultMetricValue,
            .status(display: "no active faults", accessibility: "no active faults")
        )
        XCTAssertEqual(snapshot.faultMetricDetail, "last: under-voltage warning - 3 days ago")
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
        XCTAssertEqual(
            snapshot.readbackRows.first(where: { $0.id == "voltage" })?.metricValue,
            .unavailable
        )
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
            snapshot.readbackRows.map(\.id),
            [
                "availability", "page", "page-verification", "charge", "voltage", "current",
                "bms-current-0", "bms-current-1", "high-group", "low-group", "delta",
                "lowest-group", "temperature", "temperature-sensors", "topology",
            ]
        )
        XCTAssertEqual(
            snapshot.readbackRows,
            [
                SessionDebugRow(
                    id: "availability",
                    label: "availability",
                    metricValue: .status(display: "available", accessibility: "available")
                ),
                SessionDebugRow(
                    id: "page",
                    label: "page",
                    metricValue: .status(
                        display: "temperature #3",
                        accessibility: "temperature #3"
                    ),
                    role: .transportMetadata
                ),
                SessionDebugRow(
                    id: "page-verification",
                    label: "page verification",
                    metricValue: .status(display: "hardware verified", accessibility: "hardware verified"),
                    role: .transportMetadata
                ),
                availableDebugRow(id: "charge", label: "charge", value: "72%"),
                availableDebugRow(id: "voltage", label: "voltage", value: "81.6"),
                availableDebugRow(id: "current", label: "current", value: "-12.4"),
                availableDebugRow(id: "bms-current-0", label: "bms current 0", value: "-8.1"),
                availableDebugRow(id: "bms-current-1", label: "bms current 1", value: "-4.3"),
                availableDebugRow(id: "high-group", label: "high group", value: "4.089"),
                availableDebugRow(id: "low-group", label: "low group", value: "4.071"),
                availableDebugRow(id: "delta", label: "delta", value: "18"),
                availableDebugRow(id: "lowest-group", label: "lowest group", value: "17"),
                availableDebugRow(id: "temperature", label: "temperature", value: "37.8"),
                availableDebugRow(id: "temperature-sensors", label: "temperature sensors", value: "2"),
                SessionDebugRow(
                    id: "topology",
                    label: "topology",
                    metricValue: .status(
                        display: "20S4P split pack",
                        accessibility: "20S4P split pack"
                    )
                ),
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

        let overview = snapshot.overviewPresentation
        XCTAssertEqual(overview.averageGroupVoltage, Voltage(value: 4_080))
        XCTAssertEqual(overview.lowestGroupVoltage, Voltage(value: 4_071))
        XCTAssertEqual(
            overview.averageGroupMetricValue,
            .available(display: "4.080", accessibility: "4.080")
        )
        XCTAssertEqual(
            overview.lowestGroupMetricValue,
            .available(display: "4.071", accessibility: "4.071")
        )
        XCTAssertEqual(overview.lowestGroupLabel, "group 17")
        XCTAssertNil(overview.highestTemperature)
        XCTAssertNil(overview.highestTemperatureMetricValue)
        XCTAssertEqual(overview.highestTemperatureLabel, "")
        XCTAssertNil(overview.energyHero)
        XCTAssertFalse(overview.shouldShowBalancingSummary)
        XCTAssertFalse(overview.shouldShowFaultSummary)

        let zeroReadings = BmsSnapshot(
            topology: snapshot.topology,
            lowestGroupIndex: 17,
            highestTemperature: Temperature(value: 0),
            groups: [BmsGroupSnapshot(index: 17, voltage: Voltage(value: 0))]
        ).overviewPresentation
        XCTAssertNil(zeroReadings.averageGroupVoltage)
        XCTAssertNil(zeroReadings.lowestGroupVoltage)
        XCTAssertNil(zeroReadings.highestTemperature)

        let temperatureReadings = BmsSnapshot(
            topology: snapshot.topology,
            highestTemperature: Temperature(value: 37_800),
            temperatureReadings: [Temperature(value: 37_800)],
            highestTemperatureLabel: "right pack"
        ).overviewPresentation
        XCTAssertEqual(temperatureReadings.highestTemperature, Temperature(value: 37_800))
        XCTAssertEqual(
            temperatureReadings.highestTemperatureMetricValue,
            .available(display: "37.8", accessibility: "37.8")
        )
        XCTAssertEqual(temperatureReadings.highestTemperatureLabel, "right pack")

        let cardStates = BmsSnapshot(
            topology: snapshot.topology,
            energyPercent: BatteryLevel(value: 0),
            balancingSummary: "idle",
            faultSummary: "no active faults"
        ).overviewPresentation
        XCTAssertEqual(
            cardStates.energyHero,
            BmsEnergyHeroPresentation(
                metricValue: .available(display: "0", accessibility: "0%"),
                unit: RideUnits.percentUnit,
                accessibilityUnit: "",
                detail: "20S4P split pack",
                progress: 0
            )
        )
        XCTAssertTrue(cardStates.shouldShowBalancingSummary)
        XCTAssertTrue(cardStates.shouldShowFaultSummary)
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
        XCTAssertEqual(snapshot.scrollableCellMapFocusDetail, "sagging under load")

        let noTrend = BmsSnapshot(
            topology: snapshot.topology,
            cellDelta: VoltageDelta(value: 12),
            groups: [BmsGroupSnapshot(index: 0, voltage: Voltage(value: 4_071))]
        )
        XCTAssertEqual(noTrend.scrollableCellMapFocusDetail, "12 mV spread")
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
        XCTAssertEqual(
            snapshot.unknownTopologyVoltageMetricValue,
            .available(display: "75.900", accessibility: "75.900")
        )
        XCTAssertEqual(snapshot.unknownTopologyCellCountMetricValue, .unavailable)
        XCTAssertEqual(snapshot.unknownTopologyCellCountDetail, "layout unverified")
        XCTAssertEqual(snapshot.unknownTopologyTemperatureSensorCountMetricValue, .unavailable)
        XCTAssertEqual(snapshot.unknownTopologyTemperatureDetail, "sensor names unavailable")
        XCTAssertEqual(
            snapshot.unknownTopologySummaryMetricValue,
            .status(display: "BMS found, map unknown", accessibility: "BMS found, map unknown")
        )
        XCTAssertEqual(
            snapshot.unknownTopologyFaultMetricValue,
            .available(display: "0x0040", accessibility: "0x0040")
        )
        XCTAssertEqual(snapshot.unknownTopologyFaultDetail, "needs decoder")
        XCTAssertEqual(snapshot.unknownTopologyCaptureDetail, "show raw-safe info until topology is confirmed")

        let capture = snapshot.unknownTopologyCapturePresentation
        XCTAssertEqual(capture.title, "record unsupported pack")
        XCTAssertEqual(capture.detail, "show raw-safe info until topology is confirmed")
        XCTAssertEqual(capture.state, "disabled for launch")

        let unavailableCapture = BmsSnapshot(topology: snapshot.topology).unknownTopologyCapturePresentation
        XCTAssertEqual(unavailableCapture.title, "Unavailable")
        XCTAssertEqual(unavailableCapture.detail, "topology unverified")
        XCTAssertEqual(unavailableCapture.state, "")
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

        XCTAssertEqual(
            snapshot.unknownTopologyTemperatureSensorCountMetricValue,
            .available(display: "2", accessibility: "2")
        )
    }

    func testUnknownTopologyCellCountKeepsReportedValueAvailable() {
        let snapshot = BmsSnapshot(
            topology: BmsTopology(
                layoutLabel: "topology inferred",
                seriesGroupCount: 20,
                parallelCount: nil,
                packCount: 1,
                bmsCount: 1,
                confidence: .inferred
            )
        )

        XCTAssertEqual(
            snapshot.unknownTopologyCellCountMetricValue,
            .available(display: "20", accessibility: "20")
        )
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

private func availableDebugRow(id: String, label: String, value: String) -> SessionDebugRow {
    SessionDebugRow(
        id: id,
        label: label,
        metricValue: .available(display: value, accessibility: value)
    )
}
