import Foundation

@main
struct MobileFfiSmoke {
    static func main() throws {
        let aero = AeroReadOnlySession()
        let aeroLink = MobileSessionInputDto(
            kind: .linkUp,
            monotonicMs: 1,
            maxWriteLen: 185,
            command: nil
        )
        let aeroResult = aero.ingestChecked(input: aeroLink)
        precondition(aeroResult.error == nil)
        precondition(aeroResult.outputs.contains { output in
            output.kind == .subscribe && !output.channel.isEmpty
        })
        precondition(aero.diagnostics().malformedFrames == 0)
        _ = aero.currentSnapshot()

        let falcon = try FalconReadOnlySession()
        let horn = MobileSessionInputDto(
            kind: .command,
            monotonicMs: 2,
            maxWriteLen: nil,
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
    }
}
