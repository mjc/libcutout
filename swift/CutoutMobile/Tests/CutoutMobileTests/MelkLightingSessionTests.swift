import CoreBluetooth
import CutoutMobileFFI
@testable import CutoutMobile
import XCTest

final class MelkLightingSessionTests: XCTestCase {
    func testMELKGattUuidPreservesTheBluetoothBaseUuidAcrossTheFFIBoundary() throws {
        let coreBluetoothUuid = CBUUID(data: Data([0xff, 0xf3]))
        let mobileUuid = try XCTUnwrap(MobileBluetoothUuid(coreBluetoothUuid: coreBluetoothUuid))

        XCTAssertEqual(mobileUuid.mostSignificantBits, 0x0000_fff3_0000_1000)
        XCTAssertEqual(mobileUuid.leastSignificantBits, 0x8000_0080_5f9b_34fb)
        XCTAssertEqual(mobileUuid.coreBluetoothUuid, coreBluetoothUuid)
    }
}
