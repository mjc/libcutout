import CutoutMobile
import Foundation

@main
struct CutoutMobilePackageSmoke {
    static func main() throws {
        let aero = AeroSession()
        let linkActions = try aero.linkUp(
            at: MonotonicMilliseconds(1),
            writeLimit: TransportWriteLimitBytes(185)
        )
        precondition(linkActions.contains { $0.kind == .subscribe })

        let telemetry = try aero.ingestNotification(
            Data(hex: """
                dc5a5c532a7c000000000000ab41001700000cff
                000000000226021ca8f607801afa000080c80000
                808080808080022880803080800e310e310e2f0e
                2f0e300e2a0e320e2e0e300e310e300e2d0e2f0e
                310e2e9e05e3ad
            """),
            channel: linkActions.firstSubscribeChannel!,
            at: MonotonicMilliseconds(2)
        )
        precondition(telemetry.voltageMillivolts == 108_760)
        precondition(aero.diagnostics.malformedFrames == 0)

        let falcon = try FalconSession()
        let falconLinkActions = try falcon.linkUp(
            at: MonotonicMilliseconds(10),
            writeLimit: TransportWriteLimitBytes(23)
        )
        precondition(falconLinkActions.contains { $0.kind == .subscribe })
        let falconChannel = falconLinkActions.firstSubscribeChannel!
        for (offset, chunk) in falconRidingChunks.enumerated() {
            _ = try falcon.ingestNotification(
                Data(chunk),
                channel: falconChannel,
                at: MonotonicMilliseconds(UInt64(11 + offset))
            )
        }
        precondition(falcon.currentSnapshot.voltageMillivolts != nil)

        do {
            _ = try falcon.soundHorn(at: MonotonicMilliseconds(3))
            preconditionFailure("Falcon read-only facade must refuse soundHorn")
        } catch CutoutSessionError.commandRefused(let command, _) {
            precondition(command == .soundHorn)
        }

        let advertisement = CoreBluetoothAdvertisement(
            peripheralIdentifier: CoreBluetoothPeripheralIdentifier("falcon-001"),
            localName: "Begode Falcon",
            advertisedServiceUuids: [BluetoothUuid.bluetooth16(0xffe0)]
        )
        precondition(advertisement.modelHint == .falcon)

        let writeAction = SessionAction(
            kind: .write,
            channel: BluetoothUuid.bluetooth16(0xffe1).bytes,
            bytes: Data([0x01, 0x02, 0x03, 0x04, 0x05])
        )
        let writes = CoreBluetoothTransportPlanner(
            writeLimit: TransportWriteLimitBytes(2)
        ).plan(action: writeAction)
        precondition(writes == [
            .writeWithoutResponse(channel: BluetoothUuid.bluetooth16(0xffe1), bytes: Data([0x01, 0x02])),
            .writeWithoutResponse(channel: BluetoothUuid.bluetooth16(0xffe1), bytes: Data([0x03, 0x04])),
            .writeWithoutResponse(channel: BluetoothUuid.bluetooth16(0xffe1), bytes: Data([0x05])),
        ])
    }
}

private let falconRidingChunks: [[UInt8]] = [
    [0, 0, 0, 0, 0, 0, 3, 2, 90, 90, 90, 90, 85, 170, 0, 17, 118, 110, 73, 1],
    [28, 21, 0, 45, 0, 1, 0, 0, 0, 18, 4, 24, 90, 90, 90, 90, 85, 170, 0, 28],
    [0, 147, 0, 22, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 7, 24, 90, 90, 90, 90],
    [71, 87, 49, 54, 50, 49, 48, 48, 51],
    [85, 170, 25, 153, 0, 0, 0, 63, 0, 1, 255, 136, 244, 151, 0, 136, 0, 1, 0, 24],
    [90, 90, 90, 90, 85, 170, 0, 75, 255, 253, 3, 215, 0, 0, 0, 0, 19, 136, 0, 0],
    [0, 0, 1, 3, 90, 90, 90, 90, 85, 170, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
]

private extension Array where Element == SessionAction {
    var firstSubscribeChannel: Data? {
        first { $0.kind == .subscribe }?.channel
    }
}

private extension Data {
    init(hex text: String) {
        let digits = text.filter { !$0.isWhitespace }
        precondition(digits.count.isMultiple(of: 2))
        self = stride(from: 0, to: digits.count, by: 2).reduce(into: Data()) { bytes, offset in
            let start = digits.index(digits.startIndex, offsetBy: offset)
            let end = digits.index(start, offsetBy: 2)
            bytes.append(UInt8(digits[start..<end], radix: 16)!)
        }
    }
}
