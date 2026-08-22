import XCTest
import CutoutMobileFFI
@testable import CutoutMobile

final class MelkLightingValidationTests: XCTestCase {
    private let service = BluetoothUuid.bluetooth16(0xfff0)
    private let write = BluetoothUuid.bluetooth16(0xfff3)
    private let notify = BluetoothUuid.bluetooth16(0xfff4)

    func testObservedMELKInventoryPlansTypedWriteAndNotificationSubscription() throws {
        let harness = try MelkLightingValidationHarness(
            name: "MELK-OC21  6A",
            inventory: CoreBluetoothGattInventory(services: [
                CoreBluetoothGattService(
                    uuid: service,
                    characteristics: [
                        CoreBluetoothGattCharacteristic(
                            uuid: write,
                            properties: [.writeWithoutResponse]
                        ),
                        CoreBluetoothGattCharacteristic(
                            uuid: notify,
                            properties: [.notify]
                        ),
                    ]
                ),
            ])
        )

        let plan = harness.setPower(true)
        XCTAssertEqual(plan.operation, .writeWithoutResponse(
            channel: write,
            bytes: Data([0x7e, 0x00, 0x04, 0x01, 0, 0, 0, 0, 0xef])
        ))
        XCTAssertEqual(plan.confirmationChannel, notify)
        XCTAssertEqual(harness.subscription, .subscribe(channel: notify))
    }

    func testMELKValidationRequiresTheObservedIdentityAndCharacteristicRoles() {
        XCTAssertThrowsError(
            try MelkLightingValidationHarness(
                name: "Govee_H607C_D635",
                inventory: CoreBluetoothGattInventory(services: [] as [CoreBluetoothGattService])
            )
        )
    }
}
