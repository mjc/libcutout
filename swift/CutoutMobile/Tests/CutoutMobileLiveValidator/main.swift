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
    private var candidateRecordCount = 0
    private var didRequestPairing = false
    private(set) var didValidate = false

    init(timeout: TimeInterval) {
        self.timeout = timeout
        core.onRecord = { [weak self] record in
            self?.appendRecord(record)
        }
        core.onPhaseChange = { [weak self] phase in
            self?.appendDiagnostic("phase=\(phase)")
        }
        core.onScanStateChange = { [weak self] state in
            self?.pairFirstSupportedCandidate(from: state)
        }
    }

    func start() {
        core.start()
        while !didValidate, Date().timeIntervalSince(startedAt) < timeout {
            RunLoop.current.run(mode: .default, before: Date(timeIntervalSinceNow: 0.1))
            didValidate = rideState.isLiveValidationReady && hasConfirmedAeroIdentity
        }
        if didValidate {
            appendDiagnostic("validation=ok")
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

    private func pairFirstSupportedCandidate(from state: DevicePickerScanState) {
        guard !didRequestPairing else {
            return
        }

        guard let row = state.rows.first(where: {
            $0.isSupported
                && $0.connectionRoute == .electricUnicycle
                && $0.electricUnicycleModel != nil
        }) else {
            return
        }

        didRequestPairing = true
        let didPair = core.pair(platformIdentifier: row.id)
        appendDiagnostic("auto_pair=\(didPair) id=\(row.id) title=\(row.title)")
    }

    private func appendRecord(_ record: String) {
        guard !record.hasPrefix("candidate=") else {
            candidateRecordCount += 1
            return
        }

        appendDiagnostic(record)
    }

    private func appendDiagnostic(_ record: String) {
        guard records.count < 2_048 else {
            return
        }
        records.append(record)
    }

    private func printRecords() {
        print("candidate_records_seen=\(candidateRecordCount)")
        records.forEach { print($0) }
    }
}
