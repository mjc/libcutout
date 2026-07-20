import XCTest
@testable import CutoutMobile

final class LiveActivityRideSnapshotTests: XCTestCase {
    func testChargeEstimateDisplayNamesPreserveCollectingAndNearFullStates() {
        let collecting = ChargeEstimateState(MobileChargeEstimateStateDto(
            kind: .collectingSamples,
            estimate: nil,
            voltageSag: nil,
            unavailableReason: nil,
            error: nil,
            resetReason: nil,
            samples: 2,
            observedFor: MobileDurationDto(milliseconds: 15_000)
        ))
        XCTAssertEqual(collecting.displayValue, "estimating")
        XCTAssertEqual(collecting.displayDetail, "estimating charge time · 2 samples")

        let nearFull = ChargeEstimateState(MobileChargeEstimateStateDto(
            kind: .unavailable,
            estimate: nil,
            voltageSag: nil,
            unavailableReason: .fullOrNearFull,
            error: nil,
            resetReason: nil,
            samples: 0,
            observedFor: MobileDurationDto(milliseconds: 0)
        ))
        XCTAssertEqual(nearFull.displayValue, "near full")
        XCTAssertEqual(nearFull.displayDetail, "near full")
    }

    func testChargeEstimateAccessibilityCarriesKindAndConfidence() {
        let value = LiveActivityRideValue(
            label: "Charge",
            value: "45 min",
            unit: nil,
            accessibilityDetail: "at present current, medium confidence",
            state: .available,
            source: .derivedTelemetry
        )

        XCTAssertEqual(
            value.accessibilityText,
            "45 min, at present current, medium confidence"
        )
    }

    func testStaleChargeEstimateIsHiddenWhileDisconnected() {
        let staleEstimate = ChargeEstimateState(MobileChargeEstimateStateDto(
            kind: .stale,
            estimate: nil,
            voltageSag: nil,
            unavailableReason: nil,
            error: nil,
            resetReason: nil,
            samples: 3,
            observedFor: MobileDurationDto(milliseconds: 30_000)
        ))
        let rideState = liveRideState(
            speed: nil,
            telemetry: TelemetrySnapshot(chargeEstimate: staleEstimate)
        )

        let disconnected = LiveActivityRideSnapshot.chargeEstimateValue(
            rideState: rideState,
            connectionState: .disconnected
        )
        XCTAssertEqual(disconnected.state, .unavailable)
        XCTAssertEqual(disconnected.value, "--")

        let connected = LiveActivityRideSnapshot.chargeEstimateValue(
            rideState: rideState,
            connectionState: .connected
        )
        XCTAssertEqual(connected.state, .stale)
        XCTAssertEqual(connected.value, "stale")
    }

    func testChargeEstimatePublicSurfaceWrapsGeneratedEnums() throws {
        let estimate = MobileChargeTimeEstimateDto(
            lower: MobileDurationDto(milliseconds: 1_000),
            expected: MobileDurationDto(milliseconds: 2_000),
            upper: MobileDurationDto(milliseconds: 3_000),
            kind: .atPresentCurrent,
            confidence: .medium,
            currentRate: MobileCurrentRateSummaryDto(
                meanMilliamps: 2_000,
                minimumMilliamps: 1_900,
                maximumMilliamps: 2_100,
                variabilityPermille: 100
            ),
            batteryLevel: BatteryLevelReading(
                value: BatteryLevel(value: 65),
                source: .reported,
                quality: .known,
                verification: .hardwareVerified
            ),
            batteryLevelBasis: .profileEstimated,
            batteryProfileId: 42,
            capacitySource: .hardwareMeasured,
            voltageSag: nil,
            calculatedAt: MobileMonotonicMillisDto(milliseconds: 4_000),
            validUntil: MobileMonotonicMillisDto(milliseconds: 5_000)
        )
        let state = ChargeEstimateState(MobileChargeEstimateStateDto(
            kind: .available,
            estimate: estimate,
            voltageSag: nil,
            unavailableReason: nil,
            error: .arithmeticOverflow,
            resetReason: .profileChanged,
            samples: 5,
            observedFor: MobileDurationDto(milliseconds: 30_000)
        ))

        let wrappedEstimate = try XCTUnwrap(state.estimate)
        let basis: BatteryLevelBasis = wrappedEstimate.batteryLevelBasis
        let capacitySource: ChargeCapacitySource = wrappedEstimate.capacitySource
        let error: ChargeEstimateError? = state.error
        let resetReason: ChargeEstimateResetReason? = state.resetReason

        XCTAssertEqual(basis, .profileEstimated)
        XCTAssertEqual(capacitySource, .hardwareMeasured)
        XCTAssertEqual(error, .arithmeticOverflow)
        XCTAssertEqual(resetReason, .profileChanged)
    }

    func testProductionDeviceIdentityUsesConnectedDisplayLabel() {
        let identity = LiveActivityRideIdentity.device("Little FOCer BT")

        XCTAssertEqual(identity.source, .productionDevice)
        XCTAssertEqual(identity.displayLabel, "Little FOCer BT connected")
    }

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
        XCTAssertEqual(snapshot.glyph, .electricUnicycle)
        XCTAssertEqual(snapshot.connectionState, .connected)
        XCTAssertEqual(snapshot.speed, .available(label: "Speed", value: "27.0", unit: "mph", source: .liveTelemetry))
        XCTAssertEqual(snapshot.battery, .available(label: "Battery", value: "68", unit: "%", source: .liveTelemetry))
        XCTAssertEqual(snapshot.packVoltage, .available(label: "Voltage", value: "118.4", unit: "V", source: .liveTelemetry))
        XCTAssertEqual(snapshot.pwm, .available(label: "PWM", value: "54", unit: "%", source: .liveTelemetry))
        XCTAssertEqual(snapshot.distance, .available(label: "Distance", value: "7.8", unit: "mi", source: .liveTelemetry))
        XCTAssertEqual(snapshot.headroom.value, "Headroom good")
        XCTAssertEqual(snapshot.temperature, .available(label: "Temp", value: "34", unit: "°C", source: .liveTelemetry))
        XCTAssertEqual(
            snapshot.chargeEstimate,
            .unavailable(label: "Charge", accessibilityDetail: "usable pack capacity unavailable")
        )
    }

    func testDistanceValueConvertsToKilometresWhenSpeedUnitIsMetric() {
        let telemetry = TelemetrySnapshot(
            at: MonotonicMilliseconds(1_000),
            distance: Distance(value: 1_000_000)
        )

        XCTAssertEqual(
            LiveActivityRideSnapshot.distanceValue(
                telemetry: telemetry,
                speedUnit: "km/h",
                connectionState: .connected
            ),
            .available(label: "Distance", value: "1.0", unit: "km", source: .liveTelemetry)
        )
    }

    func testDistanceValueTreatsKmhAsMetric() {
        let telemetry = TelemetrySnapshot(
            at: MonotonicMilliseconds(1_000),
            distance: Distance(value: 1_000_000)
        )

        XCTAssertEqual(
            LiveActivityRideSnapshot.distanceValue(
                telemetry: telemetry,
                speedUnit: "kmh",
                connectionState: .connected
            ),
            .available(label: "Distance", value: "1.0", unit: "km", source: .liveTelemetry)
        )
    }

    func testPartialLiveSnapshotMarksUnownedFieldsDeferred() {
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
        XCTAssertEqual(
            snapshot.chargeEstimate,
            .unavailable(label: "Charge", accessibilityDetail: "usable pack capacity unavailable")
        )
        XCTAssertEqual(snapshot.mode.state, .deferred)
        XCTAssertEqual(snapshot.duration.state, .deferred)
        XCTAssertEqual(snapshot.beeps.state, .deferred)
    }

    func testVescRefloatLiveActivitySnapshotUsesFloatwheelAtomGlyph() {
        let snapshot = LiveActivityRideSnapshot(
            identity: .device("VESC BLE UART"),
            glyph: .floatwheelAtom,
            rideState: liveRideState(
                speed: 0,
                telemetry: TelemetrySnapshot(
                    at: MonotonicMilliseconds(1_000),
                    speed: Speed(value: 0),
                    voltage: Voltage(value: 61_800),
                    controllerTemperature: Temperature(value: 27_000)
                )
            ),
            now: MonotonicMilliseconds(1_200),
            staleAfter: MonotonicMilliseconds(2_000)
        )

        XCTAssertEqual(snapshot.identity.displayLabel, "VESC BLE UART connected")
        XCTAssertEqual(snapshot.glyph, .floatwheelAtom)
        XCTAssertEqual(snapshot.packVoltage, .available(label: "Voltage", value: "61.8", unit: "V", source: .liveTelemetry))
        XCTAssertEqual(snapshot.temperature, .available(label: "Temp", value: "27", unit: "°C", source: .liveTelemetry))
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

    func testConnectingSnapshotWaitsForFirstTelemetry() {
        let snapshot = LiveActivityRideSnapshot(
            identity: .model(.aero),
            rideState: EucRideScreenState(
                phase: .connecting(model: .aero),
                displayState: RideDisplayState()
            ),
            now: MonotonicMilliseconds(3_000),
            staleAfter: MonotonicMilliseconds(1_000)
        )

        XCTAssertEqual(snapshot.connectionState, .waitingForFirstTelemetry)
        XCTAssertEqual(snapshot.speed.state, .unavailable)
    }

    func testWaitingSnapshotDoesNotShowRetainedTelemetryAsCurrent() {
        let snapshot = LiveActivityRideSnapshot(
            identity: .model(.aero),
            rideState: EucRideScreenState(
                phase: .connecting(model: .aero),
                displayState: RideDisplayState(
                    speed: SpeedReadout(millimetersPerSecond: 12_070),
                    telemetry: TelemetrySnapshot(
                        at: MonotonicMilliseconds(1_000),
                        speed: Speed(value: 12_070),
                        voltage: Voltage(value: 118_400),
                        batteryLevelReported: BatteryLevel(value: 68)
                    )
                )
            ),
            now: MonotonicMilliseconds(1_500),
            staleAfter: MonotonicMilliseconds(2_000)
        )

        XCTAssertEqual(snapshot.connectionState, .waitingForFirstTelemetry)
        XCTAssertEqual(snapshot.speed, .unavailable(label: "Speed", unit: "mph"))
        XCTAssertEqual(snapshot.battery, .unavailable(label: "Battery", unit: "%"))
        XCTAssertEqual(snapshot.packVoltage, .unavailable(label: "Voltage", unit: "V"))
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

    func testPercentProgressComesFromAvailableAndStaleValues() {
        let values: [(LiveActivityRideValue, Double?)] = [
            (.available(label: "Battery", value: "68", unit: "%", source: .liveTelemetry), 0.68),
            (.stale(label: "PWM", value: "42", unit: "%", source: .liveTelemetry), 0.42),
            (.available(label: "Battery", value: "120", unit: "%", source: .liveTelemetry), 1.0),
            (.available(label: "PWM", value: "-8", unit: "%", source: .liveTelemetry), 0.0),
            (.available(label: "Voltage", value: "68", unit: "V", source: .liveTelemetry), nil),
            (.unavailable(label: "Battery", unit: "%"), nil),
            (.deferred(label: "PWM", unit: "%"), nil),
        ]

        XCTAssertEqual(values.map { $0.0.progressValue }, values.map(\.1))
    }

    func testNumericFractionComesFromAvailableAndStaleValues() {
        let values: [(LiveActivityRideValue, Double?)] = [
            (.available(label: "Speed", value: "25.0", unit: "mph", source: .liveTelemetry), 0.5),
            (.stale(label: "Speed", value: "12.5", unit: "mph", source: .liveTelemetry), 0.25),
            (.available(label: "Speed", value: "123.4", unit: "mph", source: .liveTelemetry), 1.0),
            (.available(label: "Speed", value: "-1.0", unit: "mph", source: .liveTelemetry), 0.0),
            (.unavailable(label: "Speed", unit: "mph"), nil),
            (.available(label: "Speed", value: "--", unit: "mph", source: .liveTelemetry), nil),
        ]

        XCTAssertEqual(values.map { $0.0.fraction(of: 50) }, values.map(\.1))
        XCTAssertNil(LiveActivityRideValue.available(label: "Speed", value: "25.0", unit: "mph", source: .liveTelemetry).fraction(of: 0))
    }

    func testDisplayValueNormalizesUnavailableAndNotApplicableValues() {
        let values: [(LiveActivityRideValue, String)] = [
            (.available(label: "Speed", value: "25.0", unit: "mph", source: .liveTelemetry), "25.0"),
            (.stale(label: "Speed", value: "12.5", unit: "mph", source: .liveTelemetry), "12.5"),
            (.unavailable(label: "Speed", unit: "mph"), "--"),
            (.deferred(label: "Duration"), "--"),
            (.notApplicable(label: "PWM", unit: "%"), "n/a"),
        ]

        XCTAssertEqual(values.map { $0.0.displayValue }, values.map(\.1))
    }

    func testNotApplicablePreservesUnitWhenProvided() {
        XCTAssertEqual(LiveActivityRideValue.notApplicable(label: "PWM", unit: "%").unit, "%")
    }

    func testNotApplicableAccessibilityUsesFullWords() {
        XCTAssertEqual(
            LiveActivityRideValue.notApplicable(label: "PWM", unit: "%").accessibilityText,
            "Not applicable, %"
        )
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
