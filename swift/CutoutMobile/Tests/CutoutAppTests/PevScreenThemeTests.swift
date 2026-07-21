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

    @MainActor
    func testBmsAlertIndicatorAppearsOnlyWhenColorCannotCarrySeverity() {
        XCTAssertNil(BmsAlertIndicator.systemImageName(
            for: .critical,
            differentiateWithoutColor: false
        ))
        XCTAssertEqual(BmsAlertIndicator.systemImageName(
            for: .critical,
            differentiateWithoutColor: true
        ), "exclamationmark.triangle.fill")
        XCTAssertEqual(BmsAlertIndicator.systemImageName(
            for: .warning,
            differentiateWithoutColor: true
        ), "exclamationmark.triangle")
        XCTAssertEqual(BmsAlertIndicator.systemImageName(
            for: .unknown,
            differentiateWithoutColor: true
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
}
