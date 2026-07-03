import CutoutMobile
import Foundation

@main
struct CutoutMobileLiveValidator {
    static func main() {
        let timeoutSeconds = TimeInterval(
            CommandLine.arguments.dropFirst().first.flatMap(Double.init) ?? 45
        )
        let validator = CutoutLiveValidator(timeout: timeoutSeconds)
        validator.start()
        exit(validator.didValidate ? EXIT_SUCCESS : EXIT_FAILURE)
    }
}

private final class CutoutLiveValidator {
    private let timeout: TimeInterval
    private let startedAt = Date()
    private let core = CutoutSessionCore()
    private var records: [String] = []
    private(set) var didValidate = false

    init(timeout: TimeInterval) {
        self.timeout = timeout
        core.onRecord = { [weak self] record in
            self?.records.append(record)
        }
        core.onPhaseChange = { [weak self] phase in
            self?.records.append("phase=\(phase)")
        }
    }

    deinit {}

    func start() {
        core.start()
        while !didValidate, Date().timeIntervalSince(startedAt) < timeout {
            RunLoop.current.run(mode: .default, before: Date(timeIntervalSinceNow: 0.1))
            didValidate = rideState.isLiveValidationReady && hasConfirmedAeroIdentity
        }
        if didValidate {
            records.append("validation=ok")
            core.disconnect()
            printRecords()
        } else {
            print("validation=timeout")
            print("missing_fields=\(missingFieldText)")
            printRecords()
        }
    }

    private var rideState: EucRideScreenState {
        EucRideScreenState(phase: core.phase, displayState: core.displayState)
    }

    private var missingFieldText: String {
        var fields = rideState.liveValidationMissingFields.map(\.rawValue)
        if !hasConfirmedAeroIdentity {
            fields.append("protocolIdentity")
        }
        return fields.isEmpty ? "none" : fields.joined(separator: ",")
    }

    private var hasConfirmedAeroIdentity: Bool {
        core.protocolIdentityCandidate?.support.electricUnicycleModel == .aero
    }

    private func printRecords() {
        records.forEach { print($0) }
    }
}
