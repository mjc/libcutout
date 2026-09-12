import CutoutMobileFFI
import XCTest

final class AeroSettingsSimulatorTests: XCTestCase {
    func testTypedAeroWriteAndReadbackCrossTheMobileBoundary() {
        let simulator = AeroSettingsSimulator()

        let outputs = simulator.issue(
            command: .setAeroTiltbackSpeed(MobileAeroSpeedSettingDto(kilometresPerHour: 42)),
            operatingState: .parked,
            speed: nil,
            monotonicMs: MobileMonotonicMillisDto(milliseconds: 10)
        )

        XCTAssertTrue(outputs.contains { $0.kind == .write && $0.bytes.starts(with: Array("LdAp".utf8)) })
        XCTAssertEqual(simulator.readback().tiltbackSpeed?.kilometresPerHour, 42)
    }

    func testAllTypedAeroSettingsUseTheRustSimulatorBoundary() {
        let simulator = AeroSettingsSimulator()
        let commands: [MobileCommandDto] = [
            .setAeroTiltbackSpeed(MobileAeroSpeedSettingDto(kilometresPerHour: 42)),
            .setAeroPwmPercent(MobileAeroPwmPercentDto(percent: 64)),
            .setAeroDisplayBacklight(MobileAeroDisplayBacklightDto(percent: 80)),
            .setAeroBeeperVolume(MobileAeroBeeperVolumeDto(percent: 40)),
            .setAeroDynamicAssist(MobileAeroDynamicAssistDto(percent: 35)),
            .setAeroPedalDipCompensation(MobileAeroPedalDipCompensationDto(percent: 25)),
            .setAeroLateralTiltLimit(MobileAeroLateralTiltLimitDto(degrees: 55)),
            .setAeroVoltageCorrection(MobileAeroVoltageCorrectionDto(tenthsOfPercent: -5)),
            .setAeroMaxChargeVoltageRaw(MobileAeroMaxChargeVoltageRawDto(raw: 46)),
            .setAeroWheelUnits(.imperial),
            .setAeroHighSpeedMode(MobileAeroToggleDto(enabled: true)),
            .setAeroLowBatteryMode(MobileAeroToggleDto(enabled: false)),
            .setAeroTransportMode(MobileAeroToggleDto(enabled: true)),
            .setAeroGyroCalibration,
            .setAeroRidingMode(.hard),
            .setAeroBrakeOverpressureAlarm(MobileAeroBrakeOverpressureAlarmDto(percent: 110)),
            .setAeroPedalHardness(MobileAeroPedalHardnessDto(percent: 75)),
            .setAeroAlarmSpeed(MobileAeroSpeedSettingDto(kilometresPerHour: 56)),
            .setAeroAngleAdjustment(MobileAeroAngleAdjustmentDto(tenthsOfDegree: -12)),
            .setPedalMode(.hard),
            .setAeroHighBeam(.on),
            .setLights(.on),
            .resetTripMeter,
        ]

        for (index, command) in commands.enumerated() {
            let outputs = simulator.issue(
                command: command,
                operatingState: .parked,
                speed: nil,
                monotonicMs: MobileMonotonicMillisDto(milliseconds: UInt64(10 + index))
            )
            XCTAssertTrue(outputs.contains { $0.kind == .write }, "missing write for \(command)")
        }
        _ = simulator.tick(monotonicMs: MobileMonotonicMillisDto(milliseconds: UInt64(10 + commands.count)))

        let readback = simulator.readback()
        XCTAssertEqual(readback.tiltbackSpeed?.kilometresPerHour, 42)
        XCTAssertEqual(readback.pwmPercent?.percent, 64)
        XCTAssertEqual(readback.displayBacklight?.percent, 80)
        XCTAssertEqual(readback.beeperVolume?.percent, 40)
        XCTAssertEqual(readback.dynamicAssist?.percent, 35)
        XCTAssertEqual(readback.pedalDipCompensation?.percent, 25)
        XCTAssertEqual(readback.lateralTiltLimit?.degrees, 55)
        XCTAssertEqual(readback.voltageCorrection?.tenthsOfPercent, -5)
        XCTAssertEqual(readback.maxChargeVoltageRaw?.raw, 46)
        XCTAssertEqual(readback.wheelUnits, .imperial)
        XCTAssertEqual(readback.highSpeedMode?.enabled, true)
        XCTAssertEqual(readback.lowBatteryMode?.enabled, false)
        XCTAssertEqual(readback.transportMode?.enabled, true)
        XCTAssertEqual(readback.gyroCalibrationState, .waiting)
        XCTAssertEqual(readback.brakeOverpressureAlarm?.percent, 110)
        XCTAssertEqual(readback.pedalHardness?.percent, 75)
        XCTAssertEqual(readback.alarmSpeed?.kilometresPerHour, 56)
        XCTAssertEqual(readback.angleAdjustment?.tenthsOfDegree, -12)
        XCTAssertEqual(readback.pedalMode, .hard)
        XCTAssertEqual(readback.highBeam, .on)
        XCTAssertEqual(readback.tripMeterResetCount, 1)
    }

    func testSingleFrameHeadlightUpdatesTypedSimulatorSnapshot() {
        let simulator = AeroSettingsSimulator()

        _ = simulator.issue(
            command: .setLights(.on),
            operatingState: .parked,
            speed: nil,
            monotonicMs: MobileMonotonicMillisDto(milliseconds: 10)
        )

        XCTAssertEqual(simulator.readback().headlight, .on)
    }

    func testHighBeamSequenceUpdatesTypedSimulatorSnapshot() {
        let simulator = AeroSettingsSimulator()

        _ = simulator.issue(
            command: .setAeroHighBeam(.on),
            operatingState: .parked,
            speed: nil,
            monotonicMs: MobileMonotonicMillisDto(milliseconds: 10)
        )
        _ = simulator.tick(monotonicMs: MobileMonotonicMillisDto(milliseconds: 10))

        XCTAssertEqual(simulator.readback().highBeam, .on)
    }

    func testMotionGateRefusesAeroWriteWithoutChangingReadback() {
        let simulator = AeroSettingsSimulator()

        let outputs = simulator.issue(
            command: .setAeroPwmPercent(MobileAeroPwmPercentDto(percent: 60)),
            operatingState: .riding,
            speed: Speed(value: 501),
            monotonicMs: MobileMonotonicMillisDto(milliseconds: 10)
        )

        XCTAssertFalse(outputs.contains { $0.kind == .write })
        XCTAssertEqual(simulator.readback().pwmPercent?.percent, 60)
    }

    func testMotionGatePreservesTypedRefusalReason() {
        let simulator = AeroSettingsSimulator()

        let result = simulator.issueChecked(
            command: .setAeroPwmPercent(MobileAeroPwmPercentDto(percent: 60)),
            operatingState: .riding,
            speed: Speed(value: 501),
            monotonicMs: MobileMonotonicMillisDto(milliseconds: 10)
        )

        XCTAssertEqual(result.error?.kind, .commandRefused)
        XCTAssertEqual(result.error?.reason, .missingArm)
        XCTAssertFalse(result.outputs.contains { $0.kind == .write })
    }
}
