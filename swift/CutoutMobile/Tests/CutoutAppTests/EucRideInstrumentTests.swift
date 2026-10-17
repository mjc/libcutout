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
