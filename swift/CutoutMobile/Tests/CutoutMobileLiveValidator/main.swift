import CutoutMobile
import Foundation

@main
struct CutoutMobileLiveValidator {
    @MainActor
    static func main() {
        let environment = ProcessInfo.processInfo.environment
        let status = LiveValidatorInvocation.run(
            arguments: Array(CommandLine.arguments.dropFirst()),
            environment: environment,
            observeConnection: { timeout in
                CutoutLiveValidator(timeout: timeout, targetFilter: environment["CUTOUT_AERO_TARGET"])
                    .start()
            },
            record: writeRecord
        )
        exit(status)
    }
}

private func writeRecord(_ record: String) {
    // Emit evidence as it arrives, including when a run is interrupted.
    FileHandle.standardOutput.write(Data((record + "\n").utf8))
}

// Connection smoke check only. Protocol discovery can still transmit probes;
// this is neither passive capture nor settings acceptance (LIBCU-505/836).
@MainActor
private final class CutoutLiveValidator {
    private let timeout: TimeInterval
    private let targetFilter: String?
    private let core = CutoutSessionCore()
    private var candidateRecordCount = 0
    private var didRequestProbe = false

    init(timeout: TimeInterval, targetFilter: String?) {
        self.timeout = timeout
        self.targetFilter = targetFilter
        core.onRecord = { [weak self] record in self?.appendRecord(record) }
        core.onPhaseChange = { phase in writeRecord("phase=\(phase)") }
        core.onScanStateChange = { [weak self] state in self?.probeFirstCandidate(from: state) }
    }

    func start() -> Bool {
        let startedAt = ProcessInfo.processInfo.systemUptime
        core.start()
        defer {
            core.disconnect()
            writeRecord("candidate_records=\(candidateRecordCount)")
        }
        while ProcessInfo.processInfo.systemUptime - startedAt < timeout {
            RunLoop.current.run(until: Date(timeIntervalSinceNow: 0.1))
            if rideState.isLiveValidationReady && hasConfirmedAeroIdentity {
                return true
            }
        }
        writeRecord("connection_timeout missing_fields=\(missingFieldText)")
        return false
    }

    private var rideState: EucRideScreenState {
        EucRideScreenState(phase: core.phase, displayState: core.displayState)
    }

    private var missingFieldText: String {
        var fields = rideState.liveValidationMissingFields.map(\.rawValue)
        if !hasConfirmedAeroIdentity { fields.append("protocolIdentity") }
        return fields.isEmpty ? "none" : fields.joined(separator: ",")
    }

    private var hasConfirmedAeroIdentity: Bool {
        core.protocolIdentityCandidate?.support.electricUnicycleModel == .aero
    }

    private func probeFirstCandidate(from state: DevicePickerScanState) {
        guard !didRequestProbe else { return }
        let row: DevicePickerRow?
        if let targetFilter {
            row = state.rows.first(where: {
                $0.id == targetFilter || $0.title.localizedCaseInsensitiveContains(targetFilter)
            })
        } else {
            row = state.rows.first(where: { $0.isProbeRecommended })
        }
        guard let row else { return }
        didRequestProbe = true
        let didProbe = core.probe(platformIdentifier: row.id)
        writeRecord("auto_probe=\(didProbe) id=\(row.id) title=\(row.title)")
    }

    private func appendRecord(_ record: String) {
        if record.hasPrefix("candidate=") {
            candidateRecordCount += 1
            guard candidateRecordCount <= 16 else { return }
        }
        writeRecord(record)
    }
}
