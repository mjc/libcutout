import Foundation

enum LiveValidatorInvocation {
    static func run(
        arguments: [String],
        environment: [String: String],
        observeConnection: (TimeInterval) -> Bool,
        record: (String) -> Void
    ) -> Int32 {
        let mutationFlags = [
            "CUTOUT_AERO_SETTINGS_TEST",
            "CUTOUT_AERO_ALLOW_UNRESTORABLE_WRITES",
            "CUTOUT_AERO_INCLUDE_HEADLIGHT",
            "CUTOUT_AERO_INCLUDE_AUDIBLE",
            "CUTOUT_AERO_INCLUDE_ALARM_MODES",
            "CUTOUT_AERO_INCLUDE_TRIP_RESET",
        ]
        guard !arguments.contains("--settings"),
            !mutationFlags.contains(where: { environment[$0] == "1" })
        else {
            record("settings_validation_disabled: live mutation requires a reviewed operation and restoration contract")
            return 1
        }
        guard arguments.count <= 1,
            let timeout = arguments.first.map(Double.init) ?? 45,
            timeout.isFinite, timeout > 0, timeout <= 600
        else {
            record("invalid_arguments: expected an optional timeout in seconds greater than 0 and at most 600")
            return 1
        }

        record("connection_validation=starting settings_validation=not_run discovery=active")
        let connected = observeConnection(timeout)
        record("connection_validation=\(connected ? "ok" : "failed") settings_validation=not_run")
        return connected ? 0 : 1
    }
}
