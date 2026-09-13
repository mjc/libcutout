import CoreBluetooth

/// Native callbacks retain the identity of the attempt that installed their delegate.
final class CoreBluetoothConnectionAttempt: NSObject, CBPeripheralDelegate {
    let token: ConnectionAttemptToken
    let peripheral: CBPeripheral
    private weak var owner: CutoutSessionCore?

    init(token: ConnectionAttemptToken, peripheral: CBPeripheral, owner: CutoutSessionCore) {
        self.token = token
        self.peripheral = peripheral
        self.owner = owner
    }

    private func deliver(_ peripheral: CBPeripheral, _ body: (CutoutSessionCore) -> Void) {
        guard let owner, owner.acceptsConnectionCallback(peripheral, token: token) else { return }
        body(owner)
    }

    func peripheralIsReady(toSendWriteWithoutResponse peripheral: CBPeripheral) {
        deliver(peripheral) { $0.peripheralIsReady(toSendWriteWithoutResponse: peripheral) }
    }

    func peripheral(_ peripheral: CBPeripheral, didDiscoverServices error: Error?) {
        deliver(peripheral) { $0.peripheral(peripheral, didDiscoverServices: error) }
    }

    func peripheral(_ peripheral: CBPeripheral, didDiscoverCharacteristicsFor service: CBService, error: Error?) {
        deliver(peripheral) { $0.peripheral(peripheral, didDiscoverCharacteristicsFor: service, error: error) }
    }

    func peripheral(_ peripheral: CBPeripheral, didUpdateValueFor characteristic: CBCharacteristic, error: Error?) {
        deliver(peripheral) { $0.peripheral(peripheral, didUpdateValueFor: characteristic, error: error) }
    }

    func peripheral(_ peripheral: CBPeripheral, didUpdateNotificationStateFor characteristic: CBCharacteristic, error: Error?) {
        deliver(peripheral) { $0.peripheral(peripheral, didUpdateNotificationStateFor: characteristic, error: error) }
    }
}
