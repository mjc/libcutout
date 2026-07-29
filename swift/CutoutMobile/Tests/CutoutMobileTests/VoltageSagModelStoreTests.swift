import Foundation
import XCTest
import CutoutMobileFFI
@testable import CutoutMobile

final class VoltageSagModelStoreTests: XCTestCase {
    func testModelIsScopedToOneStableDeviceIdentity() throws {
        let suiteName = "VoltageSagModelStoreTests.\(UUID().uuidString)"
        let defaults = try XCTUnwrap(UserDefaults(suiteName: suiteName))
        defer { defaults.removePersistentDomain(forName: suiteName) }
        let store = VoltageSagModelStore(defaults: defaults)
        let model = MobileVoltageSagModelDto(
            schemaVersion: 1,
            effectiveResistanceMilliohms: 125,
            observations: 7,
            hardwareVerified: true
        )

        store.save(model, for: "wheel-a")

        let restored = try XCTUnwrap(store.load(for: "wheel-a"))
        XCTAssertEqual(restored.schemaVersion, 1)
        XCTAssertEqual(restored.effectiveResistanceMilliohms, 125)
        XCTAssertEqual(restored.observations, 7)
        XCTAssertTrue(restored.hardwareVerified)
        XCTAssertNil(store.load(for: "wheel-b"))
    }

    func testEmptyIdentityCannotRemovePrefixKey() throws {
        let suiteName = "VoltageSagModelStoreTests.\(UUID().uuidString)"
        let defaults = try XCTUnwrap(UserDefaults(suiteName: suiteName))
        defer { defaults.removePersistentDomain(forName: suiteName) }
        let store = VoltageSagModelStore(defaults: defaults)
        let prefixKey = "io.cutout.voltage-sag.v1."
        defaults.set("sentinel", forKey: prefixKey)

        store.remove(for: "")

        XCTAssertEqual(defaults.string(forKey: prefixKey), "sentinel")
    }

    func testSagVerificationUsesPublicVerificationState() throws {
        let state = ChargeEstimateState(MobileChargeEstimateStateDto(
            kind: .collectingSamples,
            estimate: nil,
            voltageSag: MobileVoltageSagEstimateDto(
                deltaMillivolts: -1_250,
                loadCurrent: BatteryCurrentReading(
                    value: BatteryCurrent(value: 10_000),
                    source: .reported,
                    quality: .known,
                    verification: .hardwareVerified
                ),
                effectiveResistanceMilliohms: 125,
                observations: 7,
                confidence: .medium,
                calculatedAt: MobileMonotonicMillisDto(milliseconds: 1_000),
                validUntil: MobileMonotonicMillisDto(milliseconds: 3_000)
            ),
            unavailableReason: nil,
            error: nil,
            resetReason: nil,
            samples: 0,
            observedFor: MobileDurationDto(milliseconds: 0)
        ))

        XCTAssertEqual(
            try XCTUnwrap(state.voltageSag).loadCurrentVerification,
            VerificationState.hardwareVerified
        )

        let presentation = state.dashboardPresentation
        XCTAssertEqual(
            presentation.metricValue,
            .status(display: "estimating", accessibility: "estimating")
        )
        guard case let .voltageSag(sag, estimateDetail) = presentation.detail else {
            return XCTFail("expected the source-owned voltage-sag detail")
        }
        XCTAssertEqual(sag.deltaMillivolts, -1_250)
        XCTAssertEqual(estimateDetail, "estimating charge time · 0 samples")

        XCTAssertEqual(TelemetrySnapshot().packVoltageDetail, .unavailable)
        XCTAssertEqual(
            TelemetrySnapshot(voltage: Voltage(value: 84_000)).packVoltageDetail,
            .sagUnavailable
        )
        guard case let .voltageSag(packSag) = TelemetrySnapshot(
            voltage: Voltage(value: 84_000),
            chargeEstimate: state
        ).packVoltageDetail else {
            return XCTFail("expected the source-owned pack voltage sag detail")
        }
        XCTAssertEqual(packSag, sag)
    }
}
