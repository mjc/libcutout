import XCTest
@testable import CutoutApp
import CutoutMobile

final class PevScreenThemeTests: XCTestCase {
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

    func testUnavailableDashboardTilesCarryTypedAvailability() {
        let source = PevDashboardTile(
            kind: .packVoltage,
            label: "pack",
            value: "84.2",
            unit: "V",
            detail: "fresh",
            accent: .cyan
        )

        XCTAssertEqual(unavailableDashboardTiles(from: [source]).first?.metricValue, .unavailable)
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
                title: "record unsupported pack",
                progress: 0.62
            ).progressAccessibilityValue,
            "62 percent"
        )
        XCTAssertEqual(
            BmsNoDataRidingRuleCard(
                title: "record unsupported pack",
                progress: 1.8
            ).progressAccessibilityValue,
            "100 percent"
        )
        XCTAssertEqual(
            BmsNoDataRidingRuleCard(
                title: "record unsupported pack",
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
}
