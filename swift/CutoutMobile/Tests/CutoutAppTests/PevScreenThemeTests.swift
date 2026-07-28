import XCTest
@testable import CutoutApp
import CutoutMobile
import CutoutMobileFFI

@MainActor
final class PevScreenThemeTests: XCTestCase {
    func testBmsScreenPresentationUsesTheAppCatalog() {
        XCTAssertEqual(localizedAppText("bms.screen.subtitle"), "CutOut · BMS")
        XCTAssertEqual(localizedAppText("bms.unknown.temperature_sensors"), "sensors")
    }

    func testBmsResistanceUsesTypedUnavailableValue() {
        XCTAssertEqual(BmsGroupSnapshot(index: 0).resistanceMetricValue, .unavailable)
        XCTAssertEqual(
            BmsGroupSnapshot(index: 0, resistance: Resistance(value: 21)).resistanceMetricValue,
            .available(display: "21", accessibility: "21")
        )
    }

    func testVescRideSnapshotUsesTypedDashboardMetricValues() {
        let unavailable = VescRideSnapshot(
            title: "VESC",
            vehicleKind: .float,
            subProtocol: .generic,
            controllerState: .unknown
        )
        XCTAssertEqual(unavailable.batteryVoltageMetricValue, .unavailable)

        let voltage = Voltage(value: 81_600)
        let available = VescRideSnapshot(
            title: "VESC",
            vehicleKind: .float,
            subProtocol: .generic,
            controllerState: .unknown,
            batteryVoltage: voltage
        )
        let text = RideUnits.voltageText(millivolts: voltage.value)
        XCTAssertEqual(
            available.batteryVoltageMetricValue,
            .available(display: text, accessibility: text)
        )

        let current = PhaseCurrent(value: 71_000)
        let angle = Angle(value: -1_800)
        let temperature = Temperature(value: 54_000)
        let metrics = VescRideSnapshot(
            title: "VESC",
            vehicleKind: .float,
            subProtocol: .generic,
            controllerState: .unknown,
            motorCurrent: current,
            boardAngle: angle,
            controllerTemperature: temperature
        )
        let currentText = RideUnits.currentText(milliamps: current.value)
        let angleText = RideUnits.angleText(millidegrees: angle.value)
        let temperatureText = RideUnits.temperatureText(
            millicelsius: temperature.value,
            fractionDigits: 1
        )
        XCTAssertEqual(
            metrics.motorCurrentMetricValue,
            .available(display: currentText, accessibility: currentText)
        )
        XCTAssertEqual(
            metrics.boardAngleMetricValue,
            .available(display: angleText, accessibility: angleText)
        )
        XCTAssertEqual(
            metrics.controllerTemperatureMetricValue,
            .available(display: temperatureText, accessibility: temperatureText)
        )
    }

    func testEucRideAppPresentationUsesTheAppCatalog() {
        XCTAssertEqual(PevScreenCatalog.live.screen(id: .eucRide)?.title, "EUC ride")
        XCTAssertEqual(localizedAppText("euc.ride.connecting"), "Connecting")
        XCTAssertEqual(localizedAppText("euc.ride.untitled"), "EUC")
        XCTAssertEqual(localizedAppText("euc.metric.gps_speed"), "GPS speed")
        XCTAssertEqual(localizedAppText("euc.speed.caption"), "speed")
    }

    func testEucGpsTileKeepsUnavailableSeparateFromValidZero() {
        let unavailable = PhoneLocationReadback(
            snapshot: MobilePhoneLocationSnapshotDto(latestSample: nil, gpsSpeed: nil)
        )
        XCTAssertEqual(eucGpsSpeedTile(from: unavailable, at: 0).metricValue, .unavailable)
        XCTAssertEqual(eucGpsSpeedTile(from: unavailable, at: 0).unit, "")

        let zero = PhoneLocationReadback(
            snapshot: MobilePhoneLocationSnapshotDto(
                latestSample: nil,
                gpsSpeed: SpeedReading(
                    value: Speed(value: 0),
                    source: .reported,
                    quality: .known,
                    verification: .unverified
                )
            )
        )
        let tile = eucGpsSpeedTile(from: zero, at: 0)
        let value = RideUnits.speedText(millimetersPerSecond: 0)
        XCTAssertEqual(tile.metricValue, .available(display: value, accessibility: value))
        XCTAssertEqual(tile.unit, RideUnits.speedUnit)
    }

    func testVescDebugPresentationUsesTheAppCatalog() {
        XCTAssertEqual(localizedAppText("vesc.debug.section"), "VESC debug")
        XCTAssertEqual(localizedAppText("vesc.debug.title"), "VESC Debug")
        XCTAssertEqual(localizedAppText("vesc.debug.metric.duty"), "duty")
        XCTAssertEqual(localizedAppText("vesc.debug.detail.motor_duty"), "motor duty cycle")
        XCTAssertEqual(localizedAppText("vesc.debug.metric.headroom"), "headroom")
        XCTAssertEqual(localizedAppText("vesc.debug.detail.remaining_duty"), "remaining duty")
        XCTAssertEqual(localizedAppText("vesc.debug.metric.board"), "board")
        XCTAssertEqual(localizedAppText("vesc.debug.detail.balance", "0.5"), "balance 0.5°")
        XCTAssertEqual(localizedAppText("vesc.debug.metric.controller"), "controller")
        XCTAssertEqual(localizedAppText("vesc.debug.detail.motor_temperature", "49.0"), "motor 49.0 °C")
        XCTAssertEqual(localizedAppText("vesc.debug.row.session"), "Session")
        XCTAssertEqual(localizedAppText("vesc.debug.row.protocol"), "Protocol")
        XCTAssertEqual(localizedAppText("vesc.debug.row.state"), "State")
        XCTAssertEqual(localizedAppText("vesc.debug.row.notifications"), "Notifications")
        XCTAssertEqual(localizedAppText("vesc.debug.row.pack_voltage"), "Pack voltage")
        XCTAssertEqual(localizedAppText("vesc.debug.row.battery_current"), "Battery current")
        XCTAssertEqual(localizedAppText("vesc.debug.row.motor_current"), "Motor current")
        XCTAssertEqual(localizedAppText("vesc.debug.row.footpad"), "Footpad")
        XCTAssertEqual(localizedAppText("vesc.debug.value.voltage", "54.3"), "54.3 V")
        XCTAssertEqual(localizedAppText("vesc.debug.value.current", "12.4"), "12.4 A")
    }

    func testVescDebugUsesTypedUnavailableMetricValues() {
        let snapshot = VescRideSnapshot(
            title: "VESC",
            vehicleKind: .float,
            subProtocol: .generic,
            controllerState: .unknown
        )

        let tiles = vescDebugTiles(snapshot)

        XCTAssertEqual(
            tiles.map(\.metricValue),
            [.unavailable, .unavailable, .unavailable, .unavailable]
        )
        XCTAssertEqual(
            tiles[2].detail,
            localizedAppText("vesc.debug.detail.balance", "unavailable")
        )
        XCTAssertEqual(
            tiles[3].detail,
            localizedAppText("vesc.debug.detail.motor_temperature", "unavailable")
        )
    }

    func testVescDebugRowsUseTypedUnavailableMetricValues() {
        let snapshot = VescRideSnapshot(
            title: "VESC",
            vehicleKind: .float,
            subProtocol: .generic,
            controllerState: .unknown
        )

        let rows = vescDebugRows(snapshot, phase: .live, notificationCount: 0)

        XCTAssertEqual(
            rows.suffix(4).map(\.metricValue),
            [.unavailable, .unavailable, .unavailable, .unavailable]
        )
        XCTAssertEqual(rows.suffix(4).map(\.accessibilityValueText), Array(repeating: "unavailable", count: 4))
    }

    func testLiveThermalValueUsesTypedUnavailableMetricValue() {
        XCTAssertEqual(liveThermalValue(telemetry: TelemetrySnapshot()), .unavailable)
    }

    func testBmsNoDataWarningUsesLocaleAwareAccessibilityList() {
        let card = BmsNoDataWarningCard(
            snapshot: BmsSnapshot(
                availability: .unavailable,
                topology: BmsTopology(
                    layoutLabel: "test",
                    seriesGroupCount: nil,
                    parallelCount: nil,
                    packCount: 0,
                    bmsCount: 0,
                    confidence: .unverified
                )
            )
        )

        XCTAssertEqual(
            card.accessibilityValueText,
            "CutOut can’t see individual cell balance or weak groups. and BMS temperature, faults, or cutout reason stay unavailable."
        )
    }

    func testVescDebugEnumValuesUseTheAppCatalog() {
        XCTAssertEqual(vescDebugProtocolText(.generic), "VESC")

        let snapshot = VescRideSnapshot(
            title: "VESC",
            vehicleKind: .float,
            subProtocol: .generic,
            controllerState: .unknown,
            operatingState: .riding
        )
        XCTAssertEqual(vescOperatingStateText(snapshot), "Riding")
    }

    func testVescRideSubtitleUsesTheSharedLocalizedOperatingState() {
        let snapshot = VescRideSnapshot(
            title: "VESC",
            vehicleKind: .float,
            subProtocol: .generic,
            controllerState: .unknown,
            operatingState: .charging
        )

        XCTAssertEqual(vescRideSubtitle(snapshot), "Charging")
    }

    func testVescFootpadPresentationUsesTheAppCatalog() {
        XCTAssertEqual(localizedAppText("vesc.footpad.left"), "LEFT / ADC1")
        XCTAssertEqual(localizedAppText("vesc.footpad.right"), "RIGHT / ADC2")
    }

    func testRideHeroReadoutSharesExplicitAvailabilitySemantics() {
        let unavailable = PevRideHeroReadout.unavailable(
            provenance: .vehicleTelemetry,
            freshness: .stale,
            severity: .caution
        )
        XCTAssertEqual(unavailable.displayValue, "Unavailable")
        XCTAssertEqual(unavailable.displayUnit, "")
        XCTAssertEqual(
            unavailable.accessibilityValue,
            "unavailable, vehicle telemetry, stale, caution"
        )
        XCTAssertFalse(unavailable.isAvailable)

        let available = PevRideHeroReadout.available(
            value: "19",
            unit: "mph",
            provenance: .vehicleTelemetry,
            freshness: .fresh,
            severity: .nominal
        )
        XCTAssertEqual(available.displayValue, "19")
        XCTAssertEqual(available.displayUnit, "mph")
        XCTAssertEqual(
            available.accessibilityValue,
            "19, mph, available, vehicle telemetry, fresh, nominal"
        )
        XCTAssertTrue(available.isAvailable)
    }

    func testRideHeroReadoutDoesNotInventEucSpeedAvailability() {
        let state = EucRideScreenState(
            phase: .live,
            displayState: RideDisplayState(
                telemetry: TelemetrySnapshot(at: MonotonicMilliseconds(1_000))
            )
        )

        let readout = PevRideHeroReadout.euc(
            state: state,
            now: MonotonicMilliseconds(1_500)
        )

        XCTAssertFalse(readout.isAvailable)
        XCTAssertEqual(
            readout.accessibilityValue,
            "unavailable, vehicle telemetry, fresh, caution"
        )
    }

    func testRideHeroReadoutDerivesVescFreshnessAndSeverityFromSnapshot() {
        let snapshot = VescRideSnapshot(
            title: "VESC",
            vehicleKind: .float,
            subProtocol: .refloat,
            controllerState: .unknown,
            warning: .pushbackSoon,
            boardSpeed: Speed(value: 19_000),
            lastUpdate: MonotonicMilliseconds(1_000)
        )

        let readout = PevRideHeroReadout.vesc(
            snapshot: snapshot,
            now: MonotonicMilliseconds(4_000)
        )

        XCTAssertTrue(readout.isAvailable)
        XCTAssertEqual(
            readout.accessibilityValue,
            "42.5, mph, available, vehicle telemetry, stale, caution"
        )
    }

    func testPowerFlowDetailUsesPlainStateWords() {
        XCTAssertEqual(powerFlowDetail(.discharge, fallback: "fallback"), "discharging")
        XCTAssertEqual(powerFlowDetail(.zero, fallback: "fallback"), "idle")
        XCTAssertEqual(powerFlowDetail(.charging, fallback: "fallback"), "charging input")
        XCTAssertEqual(powerFlowDetail(.regeneration, fallback: "fallback"), "regen")
        XCTAssertEqual(powerFlowDetail(.negativeUnknown, fallback: "fallback"), "regen/discharge unverified")
        XCTAssertEqual(powerFlowDetail(nil, fallback: "fallback"), "fallback")
    }

    func testPowerFlowLabelsResolveFromTheAppCatalog() {
        XCTAssertEqual(localizedAppText("telemetry.power_flow.discharge"), "discharging")
        XCTAssertEqual(localizedAppText("telemetry.power_flow.zero"), "idle")
        XCTAssertEqual(localizedAppText("telemetry.power_flow.charging"), "charging input")
        XCTAssertEqual(localizedAppText("telemetry.power_flow.regeneration"), "regen")
        XCTAssertEqual(localizedAppText("telemetry.power_flow.negative_unknown"), "regen/discharge unverified")
    }

    func testSharedRideTilePresentationUsesTheAppCatalog() {
        let telemetry = TelemetrySnapshot(
            voltage: Voltage(value: 54_300),
            batteryCurrent: BatteryCurrent(value: 12_400),
            powerFlow: .discharge,
            controllerTemperature: Temperature(value: 54_000),
            motorTemperature: Temperature(value: 49_000)
        )

        let expectedPower = RideUnits.powerText(milliwatts: 673_320, fractionDigits: 2)
        XCTAssertEqual(livePowerTile(from: telemetry).label, "power")
        XCTAssertEqual(
            livePowerTile(from: telemetry).metricValue,
            .available(display: expectedPower, accessibility: expectedPower)
        )
        XCTAssertEqual(livePowerTile(from: telemetry).detail, "discharging")
        XCTAssertEqual(liveThermalDetail(telemetry: telemetry), "ESC 54 °C · motor 49 °C")
        XCTAssertEqual(localizedAppText("ride.metric.power"), "power")
        XCTAssertEqual(
            localizedAppText("ride.thermal.controller_motor", "54", "°C", "49", "°C"),
            "ESC 54 °C · motor 49 °C"
        )
        XCTAssertEqual(localizedAppText("ride.value.unavailable"), "Unavailable")
        XCTAssertEqual(percentageString(fromPercent: 62), "62%")
        XCTAssertEqual(percentageString(fromPermille: 625), "62%")
    }

    func testChargeEstimateMetricValueKeepsEveryTypedStateExplicit() {
        let display = "state value"

        XCTAssertEqual(
            chargeEstimateMetricValue(kind: .available, display: display),
            .available(display: display, accessibility: display)
        )
        for kind in [
            ChargeEstimateStateKind.collectingSamples,
            .stale,
            .unavailable,
            .failed,
        ] {
            XCTAssertEqual(
                chargeEstimateMetricValue(kind: kind, display: display),
                .status(display: display, accessibility: display)
            )
        }
    }

    func testHighlightedBmsGroupAccessibilityUsesTheAppCatalog() {
        XCTAssertEqual(
            bmsGroupAccessibilityValue("Group 4, 3.8 volts", isHighlighted: true),
            "Group 4, 3.8 volts, highlighted"
        )
        XCTAssertEqual(
            bmsGroupAccessibilityValue("Group 4, 3.8 volts", isHighlighted: false),
            "Group 4, 3.8 volts"
        )
    }

    @MainActor
    func testVescDashboardPresentationUsesTheAppCatalog() {
        let snapshot = VescRideSnapshot(
            title: "VESC",
            vehicleKind: .float,
            subProtocol: .refloat,
            controllerState: .unknown,
            batteryVoltage: Voltage(value: 54_300),
            batteryLevelReported: BatteryLevel(value: 71),
            batteryCurrent: BatteryCurrent(value: 12_400),
            powerFlow: .discharge,
            motorCurrent: PhaseCurrent(value: 71_000),
            boardAngle: Angle(value: -1_800),
            balanceAngle: Angle(value: 500),
            controllerTemperature: Temperature(value: 54_000),
            motorTemperature: Temperature(value: 49_000)
        )
        let view = VescRideScreenView(
            liveSnapshot: snapshot,
            phase: .live,
            now: MonotonicMilliseconds(1_000),
            captureStatusText: nil,
            connectionStatusText: nil
        )

        XCTAssertEqual(view.dashboardTiles.map(\.label), [
            "voltage", "motor current", "board angle", "controller",
        ])
        XCTAssertEqual(view.dashboardTiles.map(\.detail), [
            "battery 71% reported · current 12.4 A",
            "discharging",
            "nose down · balance 0.5°",
            "motor 49.0 °C",
        ])
        XCTAssertEqual(localizedAppText("vesc.metric.battery_voltage"), "voltage")
        XCTAssertEqual(
            localizedAppText("vesc.battery_detail.reported_current", "71%", "12.4"),
            "battery 71% reported · current 12.4 A"
        )
        XCTAssertEqual(
            localizedAppText("vesc.battery_detail.reported_unavailable", "71%"),
            "battery 71% reported · current unavailable"
        )
        XCTAssertEqual(
            localizedAppText("vesc.battery_detail.estimated_current", "71%", "12.4"),
            "battery 71% estimated · current 12.4 A"
        )
        XCTAssertEqual(
            localizedAppText("vesc.battery_detail.estimated_unavailable", "71%"),
            "battery 71% estimated · current unavailable"
        )
        XCTAssertEqual(
            localizedAppText("vesc.battery_detail.unavailable_current", "12.4"),
            "battery level unavailable · current 12.4 A"
        )
        XCTAssertEqual(
            localizedAppText("vesc.battery_detail.unavailable_unavailable"),
            "battery level unavailable · current unavailable"
        )
        XCTAssertEqual(
            localizedAppText("vesc.board_angle.nose_down_with_balance", "0.5"),
            "nose down · balance 0.5°"
        )
        XCTAssertEqual(localizedAppText("vesc.board_angle.nose_up"), "nose up")
        XCTAssertEqual(localizedAppText("vesc.board_angle.level"), "level")
    }

    func testVescDashboardUsesTypedUnavailableMetricValues() {
        let snapshot = VescRideSnapshot(
            title: "VESC",
            vehicleKind: .float,
            subProtocol: .generic,
            controllerState: .unknown
        )
        let view = VescRideScreenView(
            liveSnapshot: snapshot,
            phase: .live,
            now: MonotonicMilliseconds(1_000),
            captureStatusText: nil,
            connectionStatusText: nil
        )

        XCTAssertEqual(
            view.dashboardTiles.map(\.metricValue),
            [.unavailable, .unavailable, .unavailable, .unavailable]
        )
    }

    @MainActor
    func testBmsAlertIndicatorAlwaysShowsNonNominalSeverity() {
        XCTAssertEqual(BmsAlertIndicator.systemImageName(
            for: .critical,
            differentiateWithoutColor: false
        ), "exclamationmark.triangle")
        XCTAssertEqual(BmsAlertIndicator.systemImageName(
            for: .critical,
            differentiateWithoutColor: true
        ), "exclamationmark.triangle.fill")
        XCTAssertEqual(BmsAlertIndicator.systemImageName(
            for: .warning,
            differentiateWithoutColor: false
        ), "exclamationmark.triangle")
        XCTAssertEqual(BmsAlertIndicator.systemImageName(
            for: .unknown,
            differentiateWithoutColor: false
        ), "questionmark.circle")
        XCTAssertNil(BmsAlertIndicator.systemImageName(
            for: .nominal,
            differentiateWithoutColor: true
        ))
    }

    @MainActor
    func testBmsChipGlassRespectsReduceTransparency() {
        XCTAssertTrue(BmsChip.usesGlassEffect(reduceTransparency: false))
        XCTAssertFalse(BmsChip.usesGlassEffect(reduceTransparency: true))
    }

    @MainActor
    func testBmsNoDataRidingRuleExposesClampedProgressToAccessibility() {
        XCTAssertEqual(
            BmsNoDataRidingRuleCard(
                metricValue: bmsNoDataRidingRuleMetricValue("record unsupported pack"),
                progress: 0.62
            ).progressAccessibilityValue,
            "62 percent"
        )
        XCTAssertEqual(
            BmsNoDataRidingRuleCard(
                metricValue: bmsNoDataRidingRuleMetricValue("record unsupported pack"),
                progress: 1.8
            ).progressAccessibilityValue,
            "100 percent"
        )
        XCTAssertEqual(
            BmsNoDataRidingRuleCard(
                metricValue: bmsNoDataRidingRuleMetricValue("record unsupported pack"),
                progress: -0.4
            ).progressAccessibilityValue,
            "0 percent"
        )
    }

    @MainActor
    func testBmsNoDataMetricSpeaksTypedUnavailableValue() {
        let metric = BmsNoDataMetric(
            metricValue: .unavailable,
            unit: "V",
            label: "Pack voltage"
        )

        XCTAssertEqual(metric.accessibilityValueText, "unavailable")
    }

    func testBmsNoDataPackEstimateSpeaksTypedUnavailableValue() {
        let metricValue = bmsNoDataPackEstimateMetricValue(nil)
        let detail = "Controller telemetry is unavailable."
        let card = BmsNoDataPackEstimateCard(
            metricValue: metricValue,
            detail: detail,
            confidenceTitle: "Unknown",
            confidenceDetail: "Telemetry unavailable"
        )

        XCTAssertEqual(metricValue, .unavailable)
        XCTAssertEqual(
            card.accessibilityValueText,
            localizedAppText(
                "bms.no_data.pack_estimate_accessibility_value",
                "unavailable",
                detail
            )
        )
    }

    func testBmsNoDataRidingRuleSpeaksTypedUnavailableTitle() {
        let metricValue = bmsNoDataRidingRuleMetricValue(nil)
        let card = BmsNoDataRidingRuleCard(metricValue: metricValue, progress: 0)

        XCTAssertEqual(metricValue, .unavailable)
        XCTAssertEqual(card.titleAccessibilityText, "unavailable")
    }

    func testBmsVoltageUsesTypedUnavailableMetricValue() {
        let voltage: Voltage? = nil
        XCTAssertEqual(bmsVoltageMetricValue(voltage), .unavailable)
    }

    func testBmsVoltagePreservesAvailableValueForVisualAndSpokenPresentation() {
        XCTAssertEqual(
            bmsVoltageMetricValue(Voltage(value: 4_036)),
            .available(display: "4.036", accessibility: "4.036")
        )
    }

    func testBmsNoDataTelemetryMetricValuesKeepZeroDistinctFromUnavailable() {
        XCTAssertEqual(bmsPackVoltageMetricValue(nil), .unavailable)
        XCTAssertEqual(
            bmsPackVoltageMetricValue(Voltage(value: 0)),
            .available(display: "0.0", accessibility: "0.0")
        )
        XCTAssertEqual(bmsVoltageSagMetricValue(nil), .unavailable)
        XCTAssertEqual(
            bmsVoltageSagMetricValue(VoltageDelta(value: 0)),
            .available(display: "0.0", accessibility: "0.0")
        )
        XCTAssertEqual(bmsBatteryCurrentMetricValue(nil), .unavailable)
        XCTAssertEqual(
            bmsBatteryCurrentMetricValue(BatteryCurrent(value: 0)),
            .available(display: "0", accessibility: "0")
        )
    }

    func testBmsDetailTemperatureKeepsAvailabilityAndFormattingTyped() {
        XCTAssertEqual(bmsTemperatureMetricValue(nil), .unavailable)
        XCTAssertEqual(
            bmsTemperatureMetricValue(Temperature(value: 25_500)),
            .available(display: "25.5", accessibility: "25.5")
        )
    }
}
