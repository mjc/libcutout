import XCTest
import CutoutMobileFFI
@testable import CutoutMobile

final class MelkLightingValidationTests: XCTestCase {
    private let service = BluetoothUuid.bluetooth16(0xfff0)
    private let write = BluetoothUuid.bluetooth16(0xfff3)
    private let notify = BluetoothUuid.bluetooth16(0xfff4)

    func testMELKScanPolicyRoutesStandaloneAccessoryWithoutAnEUCModelHint() {
        let advertisement = CoreBluetoothAdvertisement(
            peripheralIdentifier: CoreBluetoothPeripheralIdentifier("melk-1"),
            localName: "MELK-OC21  6A",
            advertisedServiceUuids: [service]
        )
        let coordinator = CoreBluetoothCentralCoordinator(
            scanPolicy: .melk,
            writeLimit: TransportWriteLimitBytes(23)
        )

        XCTAssertEqual(coordinator.scanPolicy.serviceUuids, [service])
        XCTAssertEqual(
            coordinator.handleDiscovered(advertisement),
            .connect(peripheralIdentifier: CoreBluetoothPeripheralIdentifier("melk-1"))
        )
    }

    func testMELKCommandEvidenceNeverTreatsAWriteAsConfirmedByDefault() {
        var evidence = MelkLightingCommandEvidence()
        XCTAssertEqual(evidence.status, .idle)

        evidence.requested()
        XCTAssertEqual(evidence.status, .requested)
        evidence.unconfirmed()
        XCTAssertEqual(evidence.status, .unconfirmed)
        evidence.requested()
        evidence.confirmed()
        XCTAssertEqual(evidence.status, .confirmed)
    }

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

    func testRememberedMELKTargetAcceptsOnlyTheSamePlatformIdentity() {
        let target = MelkLightingTargetPolicy(preferredPlatformIdentifier: "A1B2C3D4-E5F6-4789-ABCD-0123456789AB")

        XCTAssertTrue(target.accepts(CoreBluetoothPeripheralIdentifier("a1b2c3d4-e5f6-4789-abcd-0123456789ab")))
        XCTAssertFalse(target.accepts(CoreBluetoothPeripheralIdentifier("B1B2C3D4-E5F6-4789-ABCD-0123456789AB")))
        XCTAssertFalse(target.isInvalid)
    }

    func testFirstPairingTargetAcceptsAnyPlatformIdentity() {
        let target = MelkLightingTargetPolicy(preferredPlatformIdentifier: nil)

        XCTAssertTrue(target.accepts(CoreBluetoothPeripheralIdentifier("first-melk")))
        XCTAssertFalse(target.isInvalid)
    }

    func testMalformedRememberedTargetFailsClosed() {
        let target = MelkLightingTargetPolicy(preferredPlatformIdentifier: "legacy-melk")

        XCTAssertTrue(target.isInvalid)
        XCTAssertFalse(target.accepts(CoreBluetoothPeripheralIdentifier("legacy-melk")))
    }
}
