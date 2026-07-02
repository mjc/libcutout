import CutoutMobile
import Foundation

@main
struct CutoutMobileLiveValidator {
    static func main() {
        let timeoutSeconds = TimeInterval(
            CommandLine.arguments.dropFirst().first.flatMap(Double.init) ?? 45
        )
        let validator = LiveRideValidator(timeout: timeoutSeconds)
        validator.start()
        exit(validator.didValidate ? EXIT_SUCCESS : EXIT_FAILURE)
    }
}

private final class LiveRideValidator {
    private let timeout: TimeInterval
    private let startedAt = Date()
    private let core = LiveRideSessionCore()
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

    func start() {
        core.start()
        while !didValidate, Date().timeIntervalSince(startedAt) < timeout {
            RunLoop.current.run(mode: .default, before: Date(timeIntervalSinceNow: 0.1))
            didValidate = core.hasObservedSpeedSnapshot
        }
        if didValidate {
            records.append("validation=ok")
            core.disconnect()
            printRecords()
        } else {
            print("validation=timeout")
            records.append(contentsOf: core.timeoutDiagnosticRecords(timeoutSeconds: Int(timeout)))
            printRecords()
        }
    }

    private func printRecords() {
        records.forEach { print($0) }
    }
}
