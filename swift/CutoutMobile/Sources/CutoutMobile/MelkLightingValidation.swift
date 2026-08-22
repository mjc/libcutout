import CutoutMobileFFI
import Foundation

/// Failure while matching an observed standalone MELK controller to its typed profile.
public enum MelkLightingValidationError: Error, Equatable, Sendable {
    case missingService
    case missingWriteCharacteristic
    case missingNotificationCharacteristic
    case profileRejected
}

/// One Rust-owned MELK write ready for the existing CoreBluetooth operation sink.
public struct MelkLightingWritePlan: Equatable, Sendable {
    public let operation: CoreBluetoothPlannedOperation
    public let confirmationChannel: BluetoothUuid
    public let minimumIntervalMilliseconds: UInt16?

    public init(
        operation: CoreBluetoothPlannedOperation,
        confirmationChannel: BluetoothUuid,
        minimumIntervalMilliseconds: UInt16?
    ) {
        self.operation = operation
        self.confirmationChannel = confirmationChannel
        self.minimumIntervalMilliseconds = minimumIntervalMilliseconds
    }
}

/// Smallest reusable iPhone/CoreBluetooth seam for validating `MELK-OC21`.
///
/// Rust selects the profile and emits command bytes. This type only validates the observed GATT
/// roles and adapts those typed writes to the existing CoreBluetooth operation sink.
public struct MelkLightingValidationHarness: Sendable {
    public static let service = BluetoothUuid.bluetooth16(0xfff0)
    public static let write = BluetoothUuid.bluetooth16(0xfff3)
    public static let notify = BluetoothUuid.bluetooth16(0xfff4)

    private let profile: MobileMelkLightingProfile

    public let subscription: CoreBluetoothPlannedOperation

    public init(
        name: String,
        inventory: CoreBluetoothGattInventory
    ) throws {
        guard let serviceInventory = inventory.services.first(where: { $0.uuid == Self.service }) else {
            throw MelkLightingValidationError.missingService
        }
        guard let writeCharacteristic = serviceInventory.characteristics.first(where: {
            $0.uuid == Self.write && $0.properties.contains(.writeWithoutResponse)
        }) else {
            throw MelkLightingValidationError.missingWriteCharacteristic
        }
        guard let notifyCharacteristic = serviceInventory.characteristics.first(where: {
            $0.uuid == Self.notify && ($0.properties.contains(.notify) || $0.properties.contains(.indicate))
        }) else {
            throw MelkLightingValidationError.missingNotificationCharacteristic
        }

        do {
            profile = try MobileMelkLightingProfile(
                name: name,
                service: Self.service.bytes,
                write: writeCharacteristic.uuid.bytes,
                notify: notifyCharacteristic.uuid.bytes
            )
        } catch {
            throw MelkLightingValidationError.profileRejected
        }
        subscription = .subscribe(channel: notifyCharacteristic.uuid)
    }

    public func setPower(_ on: Bool) -> MelkLightingWritePlan {
        plan(profile.setPower(on: on))
    }

    public func setSolidColor(red: UInt8, green: UInt8, blue: UInt8) -> MelkLightingWritePlan {
        plan(profile.setSolidColor(red: red, green: green, blue: blue))
    }

    public func setBrightness(_ percentage: UInt8) throws -> MelkLightingWritePlan {
        plan(try profile.setBrightness(percentage: percentage))
    }

    private func plan(_ write: MobileMelkLightingWriteDto) -> MelkLightingWritePlan {
        precondition(write.mode == .withoutResponse)
        guard let channel = BluetoothUuid(write.characteristic),
              let confirmationChannel = BluetoothUuid(write.confirmationCharacteristic)
        else {
            preconditionFailure("Rust MELK writes contain fixed-width UUIDs")
        }
        return MelkLightingWritePlan(
            operation: .writeWithoutResponse(channel: channel, bytes: write.payload),
            confirmationChannel: confirmationChannel,
            minimumIntervalMilliseconds: write.minimumIntervalMs
        )
    }
}
