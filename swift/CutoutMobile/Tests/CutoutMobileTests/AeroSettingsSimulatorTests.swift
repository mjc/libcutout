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
}
