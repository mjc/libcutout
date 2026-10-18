import CutoutMobile
import CutoutMobileFFI
import SwiftUI
import XCTest

@testable import CutoutApp

#if os(macOS)
    import AppKit
#endif

@MainActor
final class EucRideInstrumentTests: XCTestCase {
    func testOrdinaryRideFixturesDoNotCreateSystemLiveActivities() {
        for fixture in [
            CutoutUITestSessionFixture.euc, .dynamicEuc, .staleEuc, .vesc, .unknownDevice, .bluetoothUnavailable,
        ] {
            XCTAssertTrue(fixture.makeLiveActivityManager() is CutoutUITestLiveActivityManager)
        }
        for fixture in [
            CutoutUITestSessionFixture.autoVescLiveActivity, .autoDynamicVescLiveActivity,
            .autoCriticalVescLiveActivity, .autoUnavailableVescLiveActivity, .autoStaleVescLiveActivity,
        ] {
            XCTAssertTrue(fixture.makeLiveActivityManager() is LiveActivityRideActivityKitManager)
        }
    }

    func testIsolatedLiveActivityPreservesLifecycleReceiptsWithoutSystemEffects() async throws {
        let manager = CutoutUITestSessionFixture.euc.makeLiveActivityManager()
        let snapshot = LiveActivityRideSnapshot(
            identity: .model(.aero),
            rideState: EucRideScreenState(phase: .live, displayState: RideDisplayState())
        )
        let identity = LiveActivityRideSessionIdentity(platformIdentifier: "fixture", sessionID: "ride")
        let started = try await manager.start(
            snapshot: snapshot, rideSessionIdentity: identity, staleAfterMilliseconds: 1_000,
            freshnessExpiresAt: .distantFuture
        )
        XCTAssertEqual(started, .started(activityID: "ui-test-ride"))
        let adopted = try await manager.start(
            snapshot: snapshot, rideSessionIdentity: identity, staleAfterMilliseconds: 1_000,
            freshnessExpiresAt: .distantFuture
        )
        XCTAssertEqual(adopted, .adopted(activityID: "ui-test-ride"))
        let updated = try await manager.update(
            snapshot: snapshot, staleAfterMilliseconds: 1_000, freshnessExpiresAt: .distantFuture
        )
        XCTAssertEqual(updated.activityID, "ui-test-ride")
        let ended = try await manager.end(reason: .disconnected)
        XCTAssertEqual(ended.activityIDs, ["ui-test-ride"])
        let endedAgain = try await manager.end(reason: .disconnected)
        XCTAssertTrue(endedAgain.activityIDs.isEmpty)
        do {
            _ = try await manager.update(
                snapshot: snapshot, staleAfterMilliseconds: 1_000, freshnessExpiresAt: .distantFuture
            )
            XCTFail("An ended fixture must not acknowledge another activity update")
        } catch {
            XCTAssertEqual(error as? LiveActivityRideLifecycleError, .activityUnavailable)
        }
    }

    func testHeadroomClearsWhenTelemetryAgesWithoutAnotherSample() throws {
        let state = EucRideScreenState(
            phase: .live,
            displayState: RideDisplayState(
                telemetry: TelemetrySnapshot(
                    at: MonotonicMilliseconds(1_000), operatingState: .riding, pwm: DutyCycle(permille: 680)
                )
            )
        )
        let fresh = view(state: state, now: 1_000)
        XCTAssertEqual(try XCTUnwrap(fresh.safetyBars.first?.progress), 0.32, accuracy: 0.001)
        XCTAssertEqual(fresh.safetyBars.first?.metricValue.displayText, "32%")
        XCTAssertTrue(view(state: state, now: 100_000).safetyBars.isEmpty)
        XCTAssertTrue(view(state: nil, now: 1_000).safetyBars.isEmpty)
    }

    func testDisconnectedTelemetryDoesNotFillTheGauge() {
        let state = EucRideScreenState(
            phase: .scanning,
            displayState: RideDisplayState(
                telemetry: TelemetrySnapshot(
                    at: MonotonicMilliseconds(1_000), operatingState: .riding, pwm: DutyCycle(permille: 680)
                )
            )
        )
        XCTAssertTrue(view(state: state, now: 1_000).safetyBars.isEmpty)
    }

    func testUnavailableGaugeNeverFillsAndZeroRemainsAvailable() {
        for value in [PevDashboardMetricValue.unavailable, .status(display: "N/A", accessibility: "Not applicable")] {
            XCTAssertNil(gauge(value: value, progress: 0.5).displayedProgress)
        }
        XCTAssertEqual(gauge(value: .available(display: "0%", accessibility: "0%"), progress: 0).displayedProgress, 0)
        XCTAssertEqual(
            gauge(value: .available(display: "100%", accessibility: "100%"), progress: 1).displayedProgress, 1)
    }

    #if os(macOS)
        func testBatteryStripKeepsItsSizeWhenTelemetryBecomesUnavailable() {
            func size(progress: Double?) -> CGSize {
                let host = NSHostingView(
                    rootView: EucInstrumentReading(
                        label: "Battery", value: .available(display: "78", accessibility: "78"),
                        unit: "%", pointSize: 52, identifier: "battery", showsProgress: true, progress: progress
                    ).environment(\.dynamicTypeSize, .large).frame(width: 180))
                return host.fittingSize
            }
            XCTAssertEqual(size(progress: 0.78), size(progress: nil))
        }
    #endif

    private func gauge(value: PevDashboardMetricValue, progress: Double) -> EucHeadroomGauge {
        EucHeadroomGauge(
            headroom: PevSafetyBar(
                id: .pwmHeadroom, label: "PWM headroom", metricValue: value, progress: progress, accent: .yellow)
        )
    }

    private func view(state: EucRideScreenState?, now: UInt64) -> EucRideScreenView {
        EucRideScreenView(
            rideState: state, rideTitle: "NOSFET Aero", now: MonotonicMilliseconds(now),
            captureStatusText: nil, connectionStatusText: "Connected",
            phoneLocationReadback: PhoneLocationReadback(
                snapshot: MobilePhoneLocationSnapshotDto(latestSample: nil, gpsSpeed: nil)
            )
        )
    }
}
