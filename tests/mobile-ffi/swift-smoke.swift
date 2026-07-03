import Foundation

@main
struct MobileFfiSmoke {
    static func main() throws {
        let aero = AeroReadOnlySession()
        let aeroLink = MobileSessionInputDto(
            kind: .linkUp,
            monotonicMs: MobileMonotonicMillisDto(milliseconds: 1),
            maxWriteLen: MobileTransportWriteLimitDto(bytes: 185),
            channel: Data(),
            bytes: Data(),
            command: nil
        )
        let aeroResult = aero.ingestChecked(input: aeroLink)
        precondition(aeroResult.error == nil)
        let aeroChannel = aeroResult.outputs.first { output in
            output.kind == MobileSessionOutputKindDto.subscribe && !output.channel.isEmpty
        }?.channel
        precondition(aeroChannel != nil)
        let aeroNotification = MobileSessionInputDto(
            kind: .notification,
            monotonicMs: MobileMonotonicMillisDto(milliseconds: 2),
            maxWriteLen: nil,
            channel: aeroChannel!,
            bytes: hexBytes("""
                dc5a5c532a7c000000000000ab41001700000cff
                000000000226021ca8f607801afa000080c80000
                808080808080022880803080800e310e310e2f0e
                2f0e300e2a0e320e2e0e300e310e300e2d0e2f0e
                310e2e9e05e3ad
            """),
            command: nil
        )
        precondition(aero.ingestChecked(input: aeroNotification).error == nil)
        precondition(aero.currentSnapshot().voltage?.value.value == 108_760)
        precondition(aero.diagnostics().malformedFrames.count == 0)

        let falcon = try FalconReadOnlySession()
        let horn = MobileSessionInputDto(
            kind: .command,
            monotonicMs: MobileMonotonicMillisDto(milliseconds: 2),
            maxWriteLen: nil,
            channel: Data(),
            bytes: Data(),
            command: .soundHorn
        )
        let hornResult = falcon.ingestChecked(input: horn)
        precondition(hornResult.error?.kind == .commandRefused)
        precondition(hornResult.error?.command == .soundHorn)

        do {
            _ = try FalconReadOnlySession.withProfile(profile: .unsupported)
            preconditionFailure("unsupported Falcon profile should throw")
        } catch MobileSessionConstructorError.UnsupportedFalconProfile {
        } catch {
            preconditionFailure("unexpected constructor error: \(error)")
        }

        let capture = MobilePevcapCaptureBuilder(
            wallClockStartUnixMs: MobileWallClockUnixMillisDto(milliseconds: 1_700_000_000_000),
            platformId: "ios-corebluetooth",
            writeLimit: MobileTransportWriteLimitDto(bytes: 185)
        )
        capture.addAnnotation(annotation: "capture_label=powered_on_stationary")
        capture.addAnnotation(annotation: "capture_privacy=redacted")
        capture.addAnnotation(annotation: "capture_distribution=redistributable")
        capture.addAnnotation(annotation: "capture_evidence=hardware_tested")
        let ffe0 = hexBytes("0000ffe000001000800000805f9b34fb")
        let ffe1 = hexBytes("0000ffe100001000800000805f9b34fb")
        try capture.addAdvertisedService(service: ffe0)
        try capture.addGattFingerprint(fingerprint: MobileGattFingerprintDto(
            service: ffe0,
            characteristic: ffe1,
            roles: [.read, .writeWithoutResponse, .notify],
            verification: .hardwareVerified
        ))
        capture.setResolvedIdentity(identity: MobileResolvedIdentityDto(
            protocolFamily: .begodeGotway,
            model: MobileVerifiedStringDto(value: "Begode Falcon", verification: .inferred),
            firmware: MobileVerifiedStringDto(value: "GW2015004", verification: .hardwareVerified)
        ))
        capture.recordLinkUp(
            monotonicMs: MobileMonotonicMillisDto(milliseconds: 1),
            maxWriteLen: MobileTransportWriteLimitDto(bytes: 185)
        )
        try capture.recordNotification(
            monotonicMs: MobileMonotonicMillisDto(milliseconds: 2),
            characteristic: Data(repeating: 0x11, count: 16),
            service: Data(repeating: 0x22, count: 16),
            bytes: Data([0xde, 0xad, 0xbe, 0xef])
        )
        let exported = try capture.export(encoding: .jsonl)
        let exportedText = String(data: exported, encoding: .utf8)!
        precondition(exportedText.contains("capture_label=powered_on_stationary"))
        precondition(exportedText.contains(#""protocol_family":"BegodeGotway""#))
        precondition(exportedText.contains(#""model":{"value":"Begode Falcon","verification":"Inferred"}"#))
        precondition(exportedText.contains(#""roles":["Read","WriteWithoutResponse","Notify"]"#))
        precondition(exportedText.contains(#""bytes":[222,173,190,239]"#))
    }
}

func hexBytes(_ text: String) -> Data {
    let digits = text.filter { !$0.isWhitespace }
    precondition(digits.count % 2 == 0)
    var bytes: [UInt8] = []
    var index = digits.startIndex
    while index < digits.endIndex {
        let next = digits.index(index, offsetBy: 2)
        bytes.append(UInt8(digits[index..<next], radix: 16)!)
        index = next
    }
    return Data(bytes)
}
