import XCTest
@testable import CutoutMobile

final class LiveActivityRideSnapshotTests: XCTestCase {
    func testPopulatedLiveSnapshotMapsVisibleFieldsFromTypedRideState() {
        let snapshot = LiveActivityRideSnapshot(
            identity: .model(.aero),
            rideState: liveRideState(
                speed: 12_070,
                telemetry: TelemetrySnapshot(
                    at: MonotonicMilliseconds(1_000),
                    speed: Speed(value: 12_070),
                    operatingState: .riding,
                    voltage: Voltage(value: 118_400),
                    batteryCurrent: BatteryCurrent(value: 2_000),
                    controllerTemperature: Temperature(value: 34_000),
                    pwm: DutyCycle(permille: 540),
                    distance: Distance(value: 12_552_883),
                    batteryLevelReported: BatteryLevel(value: 68)
                )
            ),
            now: MonotonicMilliseconds(1_500),
            staleAfter: MonotonicMilliseconds(2_000)
        )

        XCTAssertEqual(snapshot.identity.displayLabel, "Aero connected")
        XCTAssertEqual(snapshot.connectionState, .connected)
        XCTAssertEqual(snapshot.speed, .available(label: "Speed", value: "27.0", unit: "mph", source: .liveTelemetry))
        XCTAssertEqual(snapshot.battery, .available(label: "Battery", value: "68", unit: "%", source: .liveTelemetry))
        XCTAssertEqual(snapshot.packVoltage, .available(label: "Voltage", value: "118.4", unit: "V", source: .liveTelemetry))
        XCTAssertEqual(snapshot.pwm, .available(label: "PWM", value: "54", unit: "%", source: .liveTelemetry))
        XCTAssertEqual(snapshot.distance, .available(label: "Distance", value: "7.8", unit: "mi", source: .liveTelemetry))
        XCTAssertEqual(snapshot.headroom.value, "Headroom good")
        XCTAssertEqual(snapshot.temperature, .available(label: "Temp", value: "34", unit: "C", source: .liveTelemetry))
    }

    func testPartialLiveSnapshotMarksUnownedMockupFieldsDeferred() {
        let snapshot = LiveActivityRideSnapshot(
            identity: .model(.aero),
            rideState: liveRideState(
                speed: nil,
                telemetry: TelemetrySnapshot(
                    at: MonotonicMilliseconds(1_000),
                    operatingState: .standing,
                    batteryLevelEstimated: BatteryLevel(value: 42)
                )
            ),
            now: MonotonicMilliseconds(1_200),
            staleAfter: MonotonicMilliseconds(2_000)
        )

        XCTAssertEqual(snapshot.connectionState, .connected)
        XCTAssertEqual(snapshot.speed.state, .unavailable)
        XCTAssertEqual(snapshot.battery.source, .derivedTelemetry)
        XCTAssertEqual(snapshot.mode.state, .deferred)
        XCTAssertEqual(snapshot.duration.state, .deferred)
        XCTAssertEqual(snapshot.beeps.state, .deferred)
    }

    func testConnectionStateDistinguishesDisconnectedWaitingAndStale() {
        let disconnected = LiveActivityRideSnapshot(
            identity: .model(.aero),
            rideState: EucRideScreenState(phase: .scanning, displayState: RideDisplayState()),
            now: MonotonicMilliseconds(3_000),
            staleAfter: MonotonicMilliseconds(1_000)
        )
        let waiting = LiveActivityRideSnapshot(
            identity: .model(.aero),
            rideState: EucRideScreenState(phase: .live, displayState: RideDisplayState()),
            now: MonotonicMilliseconds(3_000),
            staleAfter: MonotonicMilliseconds(1_000)
        )
        let stale = LiveActivityRideSnapshot(
            identity: .model(.aero),
            rideState: liveRideState(
                speed: 2_000,
                telemetry: TelemetrySnapshot(at: MonotonicMilliseconds(1_000), speed: Speed(value: 2_000))
            ),
            now: MonotonicMilliseconds(3_000),
            staleAfter: MonotonicMilliseconds(1_000)
        )

        XCTAssertEqual(disconnected.connectionState, .disconnected)
        XCTAssertEqual(waiting.connectionState, .waitingForFirstTelemetry)
        XCTAssertEqual(stale.connectionState, .stale)
        XCTAssertEqual(stale.speed.state, .stale)
    }

    func testParkedSnapshotMarksPwmHeadroomNotApplicable() {
        let snapshot = LiveActivityRideSnapshot(
            identity: .model(.aero),
            rideState: liveRideState(
                speed: 0,
                telemetry: TelemetrySnapshot(
                    at: MonotonicMilliseconds(1_000),
                    speed: Speed(value: 0),
                    operatingState: .parked,
                    pwm: DutyCycle(permille: 120)
                )
            ),
            now: MonotonicMilliseconds(1_100),
            staleAfter: MonotonicMilliseconds(2_000)
        )

        XCTAssertEqual(snapshot.connectionState, .connected)
        XCTAssertEqual(snapshot.pwm.state, .notApplicable)
        XCTAssertEqual(snapshot.headroom.state, .notApplicable)
    }

    func testFixtureSnapshotIsExplicitDemoData() {
        let snapshot = LiveActivityRideSnapshot.fixture(
            identity: .fixture(label: "Lynx-S"),
            speed: .available(label: "Speed", value: "27", unit: "mph", source: .fixture),
            battery: .available(label: "Battery", value: "68", unit: "%", source: .fixture),
            packVoltage: .available(label: "Voltage", value: "118.4", unit: "V", source: .fixture),
            pwm: .available(label: "PWM", value: "54", unit: "%", source: .fixture),
            mode: .available(label: "Mode", value: "Sport", unit: nil, source: .fixture),
            duration: .available(label: "Duration", value: "18:42", unit: nil, source: .fixture),
            distance: .available(label: "Distance", value: "7.8", unit: "mi", source: .fixture),
            headroom: .available(label: "Headroom", value: "Headroom good", unit: nil, source: .fixture),
            beeps: .available(label: "Beeps", value: "Beeps armed", unit: nil, source: .fixture),
            temperature: .available(label: "Temp", value: "34", unit: "C", source: .fixture)
        )

        XCTAssertEqual(snapshot.connectionState, .fixture)
        XCTAssertEqual(snapshot.identity.displayLabel, "Lynx-S demo")
        XCTAssertTrue(snapshot.visibleValues.allSatisfy { $0.source == .fixture })
    }
}

private func liveRideState(speed: Int32?, telemetry: TelemetrySnapshot) -> EucRideScreenState {
    EucRideScreenState(
        phase: .live,
        displayState: RideDisplayState(
            speed: SpeedReadout(millimetersPerSecond: speed),
            telemetry: telemetry
        )
    )
}
