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
        do {
            _ = try falcon.soundHorn(at: MonotonicMilliseconds(3))
            preconditionFailure("Falcon read-only facade must refuse soundHorn")
        } catch CutoutSessionError.commandRefused(let command, _) {
            precondition(command == .soundHorn)
        }
    }
}

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
