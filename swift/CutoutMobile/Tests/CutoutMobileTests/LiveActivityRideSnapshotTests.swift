import CutoutMobileFFI
import XCTest

@testable import CutoutMobile

final class LiveActivityRideSnapshotTests: XCTestCase {
    func testMetricGridOmitsMissingOptionalValuesAndFooterOwnedSafetyMetrics() {
        let snapshot = gridSnapshot()
        XCTAssertEqual(snapshot.metricGridValues.map(\.role), [.battery, .packVoltage, .pwm])
        XCTAssertEqual(snapshot.metricGridValues.map(\.value), [snapshot.battery, snapshot.packVoltage, snapshot.pwm])
        XCTAssertFalse(snapshot.metricGridValues.contains { $0.role.isRepeatedInSafetyFooter })
        XCTAssertEqual(snapshot.headroom.value, "Headroom good")
        XCTAssertEqual(snapshot.temperature.value, "24")
        XCTAssertEqual(snapshot.beeps.state, .deferred)
    }

    func testMetricGridKeepsReportedZeroAndStaleOptionalValuesWithoutSafetyDuplicates() {
        let mode = LiveActivityRideValue.available(label: "Mode", value: "Off", unit: nil, source: .liveTelemetry)
        let duration = LiveActivityRideValue.available(
            label: "Duration", value: "0:00", unit: nil, source: .appLifecycle)
        let distance = LiveActivityRideValue.available(
            label: "Distance", value: "0.0", unit: "mi", source: .appLifecycle)
        let charge = LiveActivityRideValue.stale(label: "Charge", value: "45 min", unit: nil, source: .derivedTelemetry)
        let snapshot = gridSnapshot(mode: mode, duration: duration, distance: distance, charge: charge)
        XCTAssertEqual(
            snapshot.metricGridValues.map(\.role),
            [.battery, .packVoltage, .pwm, .mode, .duration, .distance, .chargeEstimate])
        XCTAssertEqual(Array(snapshot.metricGridValues.suffix(4)).map(\.value), [mode, duration, distance, charge])
        XCTAssertEqual(Set(snapshot.metricGridValues.map(\.role)).count, snapshot.metricGridValues.count)
        XCTAssertFalse(snapshot.metricGridValues.contains { $0.role.isRepeatedInSafetyFooter })
    }

    func testMetricGridRetainsMissingPrimaryTelemetryWithoutEmptyOptionalRows() {
        let snapshot = gridSnapshot(
            mode: .notApplicable(label: "Mode"),
            duration: .unavailable(label: "Duration"),
            distance: .deferred(label: "Distance"),
            charge: .unavailable(label: "Charge"),
            battery: .unavailable(label: "Battery", unit: "%"))
        XCTAssertEqual(snapshot.metricGridValues.map(\.role), [.battery, .packVoltage, .pwm])
        XCTAssertEqual(snapshot.metricGridValues.first?.value.state, .unavailable)
        XCTAssertEqual(snapshot.metricGridValues.first?.value.accessibilityValue, "unavailable, %")
    }

    private func gridSnapshot(
        mode: LiveActivityRideValue = .deferred(label: "Mode"),
        duration: LiveActivityRideValue = .deferred(label: "Duration"),
        distance: LiveActivityRideValue = .unavailable(label: "Distance"),
        charge: LiveActivityRideValue = .unavailable(label: "Charge"),
        battery: LiveActivityRideValue = .available(label: "Battery", value: "81", unit: "%", source: .liveTelemetry)
    ) -> LiveActivityRideSnapshot {
        LiveActivityRideSnapshot(
            identity: .device("Falcon"), connectionState: .connected,
            sessionStatus: .available(label: "Status", value: "Parked", unit: nil, source: .liveTelemetry),
            speed: .available(label: "Speed", value: "0.0", unit: "mph", source: .liveTelemetry),
            battery: battery,
            packVoltage: .available(label: "Voltage", value: "96.4", unit: "V", source: .liveTelemetry),
            pwm: .available(label: "PWM", value: "0", unit: "%", source: .liveTelemetry),
            mode: mode, duration: duration, distance: distance,
            headroom: .available(label: "Headroom", value: "Headroom good", unit: nil, source: .derivedTelemetry),
            beeps: .deferred(label: "Beeps"),
            temperature: .available(label: "Temp", value: "24", unit: "°C", source: .liveTelemetry),
            chargeEstimate: charge, headroomSeverity: .nominal)
    }

    func testChargeEstimateDisplayNamesPreserveCollectingFullAndBalancingStates() {
        let collecting = ChargeEstimateState(
            MobileChargeEstimateStateDto(
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

        for (kind, expected) in [(MobileChargeEstimateStateKindDto.full, "Full"), (.balancing, "Balancing")] {
            let state = ChargeEstimateState(
                MobileChargeEstimateStateDto(
                    kind: kind, estimate: nil, voltageSag: nil, unavailableReason: nil,
                    error: nil, resetReason: nil, samples: 0,
                    observedFor: MobileDurationDto(milliseconds: 0)
                ))
            XCTAssertEqual(state.displayValue, expected)
            XCTAssertEqual(state.displayDetail, expected)
        }
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
        let staleEstimate = ChargeEstimateState(
            MobileChargeEstimateStateDto(
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
        let state = ChargeEstimateState(
            MobileChargeEstimateStateDto(
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

    func testAccessibilityValuesSpeakAvailabilityAndFreshness() {
        XCTAssertEqual(
            LiveActivityRideConnectionState.waitingForFirstTelemetry.accessibilityValue,
            "waiting for telemetry"
        )
        XCTAssertEqual(
            LiveActivityRideValue.available(
                label: "Voltage",
                value: "84",
                unit: "volts",
                source: .liveTelemetry
            ).accessibilityValue,
            "84, volts, vehicle telemetry"
        )
        XCTAssertEqual(
            LiveActivityRideValue.stale(
                label: "Voltage",
                value: "83",
                unit: "volts",
                source: .liveTelemetry
            ).accessibilityValue,
            "83, volts, vehicle telemetry, stale"
        )
        XCTAssertEqual(
            LiveActivityRideValue.unavailable(label: "Voltage").accessibilityValue,
            "unavailable"
        )
        XCTAssertEqual(
            LiveActivityRideValue.notApplicable(label: "Headroom").accessibilityValue,
            "not applicable"
        )
        XCTAssertEqual(
            LiveActivityRideValue.deferred(label: "Battery").accessibilityValue,
            "waiting for data"
        )
    }

    func testUnavailableStatesKeepTheirTypedUnitInSpokenValues() {
        XCTAssertEqual(
            LiveActivityRideValue.unavailable(label: "Voltage", unit: "V").accessibilityValue,
            "unavailable, V"
        )
        XCTAssertEqual(
            LiveActivityRideValue.notApplicable(label: "PWM", unit: "%").accessibilityValue,
            "not applicable, %"
        )
        XCTAssertEqual(
            LiveActivityRideValue.deferred(label: "Battery", unit: "%").accessibilityValue,
            "waiting for data, %"
        )
    }

    func testSafetyMetricSpokenValuesIncludeTypedProvenance() {
        XCTAssertTrue(
            LiveActivityRideValue.available(
                label: "Speed",
                value: "27.0",
                unit: "mph",
                source: .liveTelemetry
            ).accessibilityValue.contains("vehicle telemetry")
        )
        XCTAssertTrue(
            LiveActivityRideValue.stale(
                label: "Headroom",
                value: "Reduce acceleration",
                unit: nil,
                source: .derivedTelemetry
            ).accessibilityValue.contains("derived telemetry")
        )
    }

    func testEveryLiveActivityValueHasOneTypedSpokenRepresentationAcrossConnectionStates() {
        let snapshots = [
            LiveActivityRideSnapshot(
                identity: .model(.aero),
                rideState: EucRideScreenState(
                    phase: .connecting(model: .aero),
                    displayState: RideDisplayState()
                ),
                now: MonotonicMilliseconds(3_000)
            ),
            LiveActivityRideSnapshot(
                identity: .unavailable,
                rideState: EucRideScreenState(
                    phase: .bluetoothUnavailable(rawState: 4),
                    displayState: RideDisplayState()
                ),
                now: MonotonicMilliseconds(3_000)
            ),
            LiveActivityRideSnapshot(
                identity: .model(.aero),
                rideState: liveRideState(
                    speed: 2_000,
                    telemetry: TelemetrySnapshot(
                        at: MonotonicMilliseconds(1_000),
                        speed: Speed(value: 2_000)
                    )
                ),
                now: MonotonicMilliseconds(3_000),
                staleAfter: MonotonicMilliseconds(1_000)
            ),
        ]

        for snapshot in snapshots {
            for value in snapshot.visibleValues {
                XCTAssertFalse(value.label.isEmpty)
                XCTAssertFalse(value.accessibilityValue.isEmpty)
            }
        }
    }

    func testProductionDeviceIdentityIncludesConnectionStateInAccessibilityValue() {
        let identity = LiveActivityRideIdentity.device("Little FOCer BT")

        XCTAssertEqual(identity.source, .productionDevice)
        XCTAssertEqual(
            identity.accessibilityValue(for: .connected),
            "Little FOCer BT, connected"
        )
    }

    func testUnavailableIdentitySpokenStateAvoidsDuplicateUnavailableWord() {
        XCTAssertEqual(
            LiveActivityRideIdentity.unavailable.accessibilityValue(for: .unavailable),
            "Device, unavailable"
        )
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
            staleAfter: MonotonicMilliseconds(2_000),
            rideDistance: Distance(value: 12_552_883)
        )

        XCTAssertEqual(snapshot.glyph, .electricUnicycle)
        XCTAssertEqual(snapshot.connectionState, .connected)
        XCTAssertEqual(
            snapshot.speed,
            .available(
                label: "Speed",
                value: "27.0",
                unit: "mph",
                normalizedProgress: RideUnits.speedValue(millimetersPerSecond: 12_070) / 50,
                source: .liveTelemetry
            )
        )
        XCTAssertEqual(
            snapshot.battery,
            .available(label: "Battery", value: "68", unit: "%", normalizedProgress: 0.68, source: .liveTelemetry)
        )
        XCTAssertEqual(
            snapshot.packVoltage, .available(label: "Voltage", value: "118.4", unit: "V", source: .liveTelemetry))
        XCTAssertEqual(
            snapshot.pwm,
            .available(label: "PWM", value: "54", unit: "%", normalizedProgress: 0.54, source: .liveTelemetry)
        )
        XCTAssertEqual(
            snapshot.distance, .available(label: "Distance", value: "7.8", unit: "mi", source: .appLifecycle))
        XCTAssertEqual(snapshot.headroom.value, "Headroom good")
        XCTAssertEqual(snapshot.headroomSeverity, .nominal)
        XCTAssertEqual(snapshot.temperature, .available(label: "Temp", value: "34", unit: "°C", source: .liveTelemetry))
        XCTAssertEqual(
            snapshot.chargeEstimate,
            .unavailable(label: "Charge", accessibilityDetail: "usable pack capacity unavailable")
        )
    }

    func testLowHeadroomUsesTypedReduceAccelerationSeverity() {
        let snapshot = LiveActivityRideSnapshot(
            identity: .model(.aero),
            rideState: liveRideState(
                speed: 12_000,
                telemetry: TelemetrySnapshot(
                    at: MonotonicMilliseconds(1_000),
                    speed: Speed(value: 12_000),
                    operatingState: .riding,
                    pwm: DutyCycle(permille: 800)
                )
            ),
            now: MonotonicMilliseconds(1_100)
        )

        XCTAssertEqual(snapshot.headroomSeverity, .reduceAcceleration)
        XCTAssertFalse(snapshot.showsSecondarySafetyMetrics)
        XCTAssertEqual(snapshot.compactTrailingValue, snapshot.headroom)
        XCTAssertEqual(
            snapshot.minimalAccessibilitySummary,
            "Aero, connected, Speed, 26.8, mph, vehicle telemetry, Headroom, Reduce acceleration, derived telemetry"
        )
    }

    func testNominalCompactPresentationKeepsBatteryAndOmitsRedundantHeadroom() {
        let snapshot = LiveActivityRideSnapshot(
            identity: .model(.aero),
            rideState: liveRideState(
                speed: 12_000,
                telemetry: TelemetrySnapshot(
                    at: MonotonicMilliseconds(1_000),
                    speed: Speed(value: 12_000),
                    operatingState: .riding,
                    pwm: DutyCycle(permille: 540),
                    batteryLevelReported: BatteryLevel(value: 68)
                )
            ),
            now: MonotonicMilliseconds(1_100)
        )

        XCTAssertEqual(snapshot.compactTrailingValue, snapshot.battery)
        XCTAssertTrue(snapshot.showsSecondarySafetyMetrics)
        XCTAssertEqual(
            snapshot.minimalAccessibilitySummary,
            "Aero, connected, Speed, 26.8, mph, vehicle telemetry"
        )
    }

    func testExpandedMetricAccessibilityAvoidsFooterDuplicatesAndPrioritizesCriticalHeadroom() {
        let footerDuplicates = Set(PevLiveActivityMetricRole.allCases.filter(\.isRepeatedInSafetyFooter))

        XCTAssertEqual(footerDuplicates, [.headroom, .temperature])
        XCTAssertEqual(
            PevLiveActivityMetricRole.headroom.accessibilitySortPriority(for: .reduceAcceleration),
            2
        )
        XCTAssertEqual(PevLiveActivityMetricRole.headroom.accessibilitySortPriority(for: .nominal), 0)
        XCTAssertEqual(PevLiveActivityMetricRole.battery.accessibilitySortPriority(for: .reduceAcceleration), 0)
    }

    func testDistanceValueConvertsRecordingDistanceToKilometresWhenSpeedUnitIsMetric() {
        XCTAssertEqual(
            LiveActivityRideSnapshot.distanceValue(rideDistance: Distance(value: 1_000_000), speedUnit: "km/h"),
            .available(label: "Distance", value: "1.0", unit: "km", source: .appLifecycle))
    }

    func testDistanceValueTreatsKmhAsMetric() {
        XCTAssertEqual(
            LiveActivityRideSnapshot.distanceValue(rideDistance: Distance(value: 1_000_000), speedUnit: "kmh"),
            .available(label: "Distance", value: "1.0", unit: "km", source: .appLifecycle))
    }

    func testWheelOdometerDoesNotPopulateRecordingDistanceOrDuration() {
        let ride = liveRideState(
            speed: 0,
            telemetry: TelemetrySnapshot(
                at: MonotonicMilliseconds(1_000), speed: Speed(value: 0),
                distance: Distance(value: 1_842_000_000)))
        let snapshot = LiveActivityRideSnapshot(identity: .device("Falcon"), rideState: ride)
        XCTAssertEqual(snapshot.distance.state, .unavailable)
        XCTAssertEqual(snapshot.duration.state, .deferred)
        XCTAssertFalse(snapshot.metricGridValues.contains { $0.role == .distance || $0.role == .duration })
    }

    func testRecordingStatisticsOverrideWheelOdometerAndPreserveActualZero() {
        let ride = liveRideState(
            speed: 0,
            telemetry: TelemetrySnapshot(
                at: MonotonicMilliseconds(1_000), speed: Speed(value: 0),
                distance: Distance(value: 1_842_000_000)))
        let snapshot = LiveActivityRideSnapshot(
            identity: .device("Falcon"), rideState: ride,
            rideDuration: MonotonicMilliseconds(65_000), rideDistance: Distance(value: 0))
        XCTAssertEqual(
            snapshot.duration, .available(label: "Duration", value: "1:05", unit: nil, source: .appLifecycle))
        XCTAssertEqual(
            snapshot.distance, .available(label: "Distance", value: "0.0", unit: "mi", source: .appLifecycle))
        XCTAssertTrue(snapshot.metricGridValues.contains { $0.role == .duration && $0.value == snapshot.duration })
        XCTAssertTrue(snapshot.metricGridValues.contains { $0.role == .distance && $0.value == snapshot.distance })
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

        XCTAssertEqual(snapshot.glyph, .floatwheelAtom)
        XCTAssertEqual(
            snapshot.packVoltage, .available(label: "Voltage", value: "61.8", unit: "V", source: .liveTelemetry))
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

    func testSystemStaleStateOverridesAConnectedSnapshotPresentation() {
        let snapshot = LiveActivityRideSnapshot(
            identity: .model(.aero),
            rideState: liveRideState(
                speed: 2_000,
                telemetry: TelemetrySnapshot(
                    at: MonotonicMilliseconds(1_000),
                    speed: Speed(value: 2_000)
                )
            ),
            now: MonotonicMilliseconds(1_100)
        )

        let presented = snapshot.presented(isStale: true)

        XCTAssertEqual(snapshot.connectionState, .connected)
        XCTAssertEqual(presented.connectionState, .stale)
        XCTAssertEqual(presented.speed.state, .stale)
        XCTAssertTrue(presented.minimalAccessibilitySummary.contains("Aero, stale"))
        XCTAssertTrue(presented.minimalAccessibilitySummary.contains("vehicle telemetry, stale"))
    }

    func testMinimalAccessibilitySummaryNamesNormalAlertStaleAndUnavailableStates() {
        let normal = LiveActivityRideSnapshot(
            identity: .model(.aero),
            rideState: liveRideState(
                speed: 2_000,
                telemetry: TelemetrySnapshot(
                    at: MonotonicMilliseconds(1_000),
                    speed: Speed(value: 2_000),
                    operatingState: .riding
                )
            ),
            now: MonotonicMilliseconds(1_100)
        )
        let alert = LiveActivityRideSnapshot(
            identity: .model(.aero),
            rideState: liveRideState(
                speed: 12_000,
                telemetry: TelemetrySnapshot(
                    at: MonotonicMilliseconds(1_000),
                    speed: Speed(value: 12_000),
                    operatingState: .riding,
                    pwm: DutyCycle(permille: 800)
                )
            ),
            now: MonotonicMilliseconds(1_100)
        )
        let stale = LiveActivityRideSnapshot(
            identity: .model(.aero),
            rideState: liveRideState(
                speed: 2_000,
                telemetry: TelemetrySnapshot(
                    at: MonotonicMilliseconds(1_000),
                    speed: Speed(value: 2_000)
                )
            ),
            now: MonotonicMilliseconds(3_000),
            staleAfter: MonotonicMilliseconds(1_000)
        )
        let unavailable = LiveActivityRideSnapshot(
            identity: .unavailable,
            rideState: EucRideScreenState(
                phase: .bluetoothUnavailable(rawState: 4),
                displayState: RideDisplayState()
            ),
            now: MonotonicMilliseconds(1_100)
        )

        XCTAssertEqual(normal.connectionState, .connected)
        XCTAssertEqual(
            normal.minimalAccessibilitySummary,
            "Aero, connected, Speed, 4.5, mph, vehicle telemetry"
        )
        XCTAssertEqual(alert.connectionState, .connected)
        XCTAssertEqual(
            alert.minimalAccessibilitySummary,
            "Aero, connected, Speed, 26.8, mph, vehicle telemetry, Headroom, Reduce acceleration, derived telemetry"
        )
        XCTAssertEqual(stale.connectionState, .stale)
        XCTAssertEqual(
            stale.minimalAccessibilitySummary,
            "Aero, stale, Speed, 4.5, mph, vehicle telemetry, stale"
        )
        XCTAssertEqual(unavailable.connectionState, .unavailable)
        XCTAssertFalse(unavailable.showsSecondarySafetyMetrics)
        XCTAssertEqual(
            unavailable.minimalAccessibilitySummary,
            "Device, unavailable, Speed, unavailable, mph"
        )
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

    func testParkedSnapshotKeepsPwmHeadroomAvailable() {
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
        XCTAssertEqual(snapshot.pwm.state, .available)
        XCTAssertEqual(snapshot.headroom.state, .available)
        XCTAssertEqual(snapshot.headroomSeverity, .nominal)
    }

    func testPercentProgressComesFromAvailableAndStaleValues() {
        let values: [(LiveActivityRideValue, Double?)] = [
            (
                .available(label: "Battery", value: "68", unit: "%", normalizedProgress: 0.68, source: .liveTelemetry),
                0.68
            ),
            (.stale(label: "PWM", value: "42", unit: "%", normalizedProgress: 0.42, source: .liveTelemetry), 0.42),
            (
                .available(label: "Battery", value: "120", unit: "%", normalizedProgress: 1.2, source: .liveTelemetry),
                1.0
            ),
            (.available(label: "PWM", value: "-8", unit: "%", normalizedProgress: -0.08, source: .liveTelemetry), 0.0),
            (
                .available(label: "Voltage", value: "68", unit: "V", normalizedProgress: 0.68, source: .liveTelemetry),
                nil
            ),
            (.unavailable(label: "Battery", unit: "%"), nil),
            (.deferred(label: "PWM", unit: "%"), nil),
        ]

        XCTAssertEqual(values.map { $0.0.progressValue }, values.map(\.1))
    }

    func testPwmPresentationUsesDutyAndAStrictCriticalThreshold() {
        let values: [(LiveActivityRideValue, LiveActivityRidePwmSeverity)] = [
            (
                .available(label: "PWM", value: "80", unit: "%", normalizedProgress: 0.80, source: .liveTelemetry),
                .nominal
            ),
            (
                .available(
                    label: "PWM", value: "not parsed", unit: "%", normalizedProgress: 0.801, source: .liveTelemetry),
                .critical
            ),
            (
                .available(label: "PWM", value: "120", unit: "%", normalizedProgress: 1.2, source: .liveTelemetry),
                .critical
            ),
            (.unavailable(label: "PWM", unit: "%"), .unavailable),
        ]

        XCTAssertEqual(values.map { LiveActivityRidePwmSeverity(pwm: $0.0) }, values.map(\.1))
        XCTAssertEqual(LiveActivityRidePwmSeverity.criticalThreshold, 0.80)
    }

    func testVescCompactIslandPwmBarUsesDutyAndRespectsReduceMotion() {
        let snapshot = LiveActivityRideSnapshot(
            identity: .device("VESC BLE UART"),
            glyph: .floatwheelAtom,
            rideState: liveRideState(
                speed: 12_000,
                telemetry: TelemetrySnapshot(
                    at: MonotonicMilliseconds(1_000),
                    speed: Speed(value: 12_000),
                    operatingState: .riding,
                    pwm: DutyCycle(permille: 810)
                )
            ),
            now: MonotonicMilliseconds(1_100)
        )

        XCTAssertTrue(snapshot.showsCompactPwmBar)
        XCTAssertEqual(snapshot.pwmSeverity, .critical)
        XCTAssertEqual(snapshot.pwm.progressValue, 0.81)
        XCTAssertEqual(snapshot.pwmSeverity.accessibilityDescription, "critical PWM")
        XCTAssertTrue(PevLiveActivityCompactPwmBar.shouldPulse(severity: .critical, reduceMotion: false))
        XCTAssertFalse(PevLiveActivityCompactPwmBar.shouldPulse(severity: .critical, reduceMotion: true))
    }

    func testCompactIslandPwmBarRequiresVescAndTypedPwmProgress() {
        let telemetryWithPwm = TelemetrySnapshot(
            at: MonotonicMilliseconds(1_000),
            speed: Speed(value: 12_000),
            operatingState: .riding,
            pwm: DutyCycle(permille: 500)
        )
        let vescWithoutPwm = LiveActivityRideSnapshot(
            identity: .device("VESC BLE UART"),
            glyph: .floatwheelAtom,
            rideState: liveRideState(
                speed: 12_000,
                telemetry: TelemetrySnapshot(
                    at: MonotonicMilliseconds(1_000),
                    speed: Speed(value: 12_000),
                    operatingState: .riding
                )
            ),
            now: MonotonicMilliseconds(1_100)
        )
        let eucWithPwm = LiveActivityRideSnapshot(
            identity: .device("Aero"),
            glyph: .electricUnicycle,
            rideState: liveRideState(speed: 12_000, telemetry: telemetryWithPwm),
            now: MonotonicMilliseconds(1_100)
        )

        XCTAssertEqual(vescWithoutPwm.pwm.state, .unavailable)
        XCTAssertFalse(vescWithoutPwm.showsCompactPwmBar)
        XCTAssertNotNil(eucWithPwm.pwm.progressValue)
        XCTAssertFalse(eucWithPwm.showsCompactPwmBar)
    }

    func testSpeedGaugeProgressComesFromTypedNumericState() {
        let values: [(LiveActivityRideValue, Double?)] = [
            (
                .available(
                    label: "Speed", value: "not parsed", unit: "mph", normalizedProgress: 0.5, source: .liveTelemetry),
                0.5
            ),
            (
                .stale(
                    label: "Speed", value: "not parsed", unit: "mph", normalizedProgress: 0.25, source: .liveTelemetry),
                0.25
            ),
            (
                .available(
                    label: "Speed", value: "not parsed", unit: "mph", normalizedProgress: 2, source: .liveTelemetry),
                1.0
            ),
            (
                .available(
                    label: "Speed", value: "not parsed", unit: "mph", normalizedProgress: -1, source: .liveTelemetry),
                0.0
            ),
            (.unavailable(label: "Speed", unit: "mph"), nil),
            (.available(label: "Speed", value: "--", unit: "mph", source: .liveTelemetry), nil),
        ]

        XCTAssertEqual(values.map { $0.0.speedGaugeProgressValue }, values.map(\.1))
    }

    func testSpeedGaugeMakesMissingUnitExplicitlyUnavailable() {
        let speed = LiveActivityRideValue.available(
            label: "Speed",
            value: "27",
            unit: nil,
            source: .liveTelemetry
        )

        XCTAssertEqual(
            LiveActivityRideValue.speedPresentation(for: speed),
            .unavailable(label: "Speed")
        )
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

    func testSharedLiveActivityAccessibilityLabelsResolveFromTheCatalog() {
        XCTAssertEqual(LiveActivityRideIdentity.accessibilityLabel, "Device")
        XCTAssertEqual(LiveActivityRideSnapshot.activityAccessibilityLabel, "CutOut ride")
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
