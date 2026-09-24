import CoreBluetooth
import Foundation

/// Owns native write back-pressure and receipt ordering for one BLE connection boundary.
final class CutoutSessionBluetoothWriteAdapter {
    private let pendingWrites = CoreBluetoothWriteQueue(capacity: 64)
    private var nextWriteID: UInt64 = 0
    private let makeCaptureReceipt: (
        BluetoothUuid,
        Data,
        UInt64
    ) -> (CoreBluetoothWriteDisposition) -> Bool
    private let recordWrite: (BluetoothUuid, Data) -> Void

    init(
        makeCaptureReceipt: @escaping (
            BluetoothUuid,
            Data,
            UInt64
        ) -> (CoreBluetoothWriteDisposition) -> Bool,
        recordWrite: @escaping (BluetoothUuid, Data) -> Void
    ) {
        self.makeCaptureReceipt = makeCaptureReceipt
        self.recordWrite = recordWrite
    }

    func submit(
        channel: BluetoothUuid,
        bytes: Data,
        peripheral: CBPeripheral,
        characteristic: CBCharacteristic,
        isCurrent: @escaping () -> Bool,
        onReceipt: @escaping (CoreBluetoothWriteDisposition) -> Void
    ) -> CoreBluetoothWriteDisposition {
        let (writeID, overflow) = nextWriteID.addingReportingOverflow(1)
        guard !overflow else {
            onReceipt(.rejected)
            return .rejected
        }
        nextWriteID = writeID

        let captureReceipt = makeCaptureReceipt(channel, bytes, writeID)
        guard captureReceipt(.queued) else {
            onReceipt(.rejected)
            return .rejected
        }
        return pendingWrites.submit(
            canSend: {
                isCurrent() && peripheral.canSendWriteWithoutResponse
            },
            isCurrent: {
                isCurrent()
            },
            write: { [recordWrite] in
                peripheral.writeValue(bytes, for: characteristic, type: .withoutResponse)
                recordWrite(channel, bytes)
            },
            onReceipt: { disposition in
                if disposition != .queued {
                    _ = captureReceipt(disposition)
                }
                onReceipt(disposition)
            }
        )
    }

    func flush(peripheral: CBPeripheral, isCurrent: @escaping () -> Bool) {
        pendingWrites.flush {
            isCurrent() && peripheral.canSendWriteWithoutResponse
        }
    }

    func clear() {
        pendingWrites.clear()
    }
}
