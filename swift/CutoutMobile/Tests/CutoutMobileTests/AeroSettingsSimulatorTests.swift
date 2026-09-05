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

    func testAllSupportedAeroSettingsUseTheSameTypedSimulatorBoundary() {
        let simulator = AeroSettingsSimulator()
        let commands: [MobileCommandDto] = [
            .setAeroTiltbackSpeed(MobileAeroSpeedSettingDto(kilometresPerHour: 42)),
            .setAeroPwmPercent(MobileAeroPwmPercentDto(percent: 64)),
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
        _ = simulator.tick(monotonicMs: MobileMonotonicMillisDto(milliseconds: 17))

        let readback = simulator.readback()
        XCTAssertEqual(readback.tiltbackSpeed?.kilometresPerHour, 42)
        XCTAssertEqual(readback.pwmPercent?.percent, 64)
        XCTAssertEqual(readback.alarmSpeed?.kilometresPerHour, 56)
        XCTAssertEqual(readback.angleAdjustment?.tenthsOfDegree, -12)
        XCTAssertEqual(readback.pedalMode, .hard)
        XCTAssertEqual(readback.highBeam, .on)
        XCTAssertEqual(readback.headlight, .on)
        XCTAssertEqual(readback.tripMeterResetCount, 1)
    }

    func testMotionGateRefusesAeroWriteWithoutChangingReadback() {
        let simulator = AeroSettingsSimulator()

        let outputs = simulator.issue(
            command: .setAeroPwmPercent(MobileAeroPwmPercentDto(percent: 71)),
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
            command: .setAeroPwmPercent(MobileAeroPwmPercentDto(percent: 71)),
            operatingState: .riding,
            speed: Speed(value: 501),
            monotonicMs: MobileMonotonicMillisDto(milliseconds: 10)
        )

        XCTAssertEqual(result.error?.kind, .commandRefused)
        XCTAssertEqual(result.error?.reason, .missingArm)
        XCTAssertFalse(result.outputs.contains { $0.kind == .write })
    }
}
