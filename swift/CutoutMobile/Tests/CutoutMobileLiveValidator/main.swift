import CutoutMobile
import Foundation

@main
struct CutoutMobileLiveValidator {
    static func main() {
        let arguments = Array(CommandLine.arguments.dropFirst())
        let settingsTest = arguments.contains("--settings")
        let timeout = TimeInterval(
            arguments.first(where: { Double($0) != nil }).flatMap(Double.init)
                ?? (settingsTest ? 180 : 45)
        )
        let validator = CutoutLiveValidator(timeout: timeout, settingsTest: settingsTest)
        validator.start()
        exit(validator.didValidate ? EXIT_SUCCESS : EXIT_FAILURE)
    }
}

private final class CutoutLiveValidator {
    private struct SettingPlan {
        let id: DeviceSettingID
        let name: String
    }

    private let timeout: TimeInterval
    private let settingsTest: Bool
    private let targetFilter: String?
    private let allowUnrestorableWrites: Bool
    private let includeAudibleControls: Bool
    private let includeAlarmModes: Bool
    private let includeTripReset: Bool
    private let startedAt = Date()
    private let core = CutoutSessionCore()
    private var records: [String] = []
    private var candidateRecordCount = 0
    private var candidateSamples: [String] = []
    private var didRequestProbe = false
    private var latestControls: DeviceControlsSnapshot
    private var hardFailures = 0
    private var unconfirmedWrites = 0
    private var skippedWrites = 0
    private var missingControls = 0
    private var validationFinished = false
    private(set) var didValidate = false

    private let expectedSettings: [SettingPlan] = [
        SettingPlan(id: .highBeam, name: "headlight/high_beam"),
        SettingPlan(id: .tiltbackSpeed, name: "tiltback_speed"),
        SettingPlan(id: .pwmTiltback, name: "pwm_tiltback"),
        SettingPlan(id: .pedalHardness, name: "pedal_hardness"),
        SettingPlan(id: .displayBrightness, name: "display_brightness"),
        SettingPlan(id: .beeperVolumePercent, name: "beeper_volume"),
        SettingPlan(id: .dynamicAssist, name: "dynamic_assist"),
        SettingPlan(id: .pedalDipCompensation, name: "pedal_dip"),
        SettingPlan(id: .lateralTiltLimit, name: "lateral_tilt_limit"),
        SettingPlan(id: .voltageCorrection, name: "voltage_correction"),
        SettingPlan(id: .highSpeedMode, name: "high_speed_mode"),
        SettingPlan(id: .lowBatteryMode, name: "low_battery_mode"),
        SettingPlan(id: .speedAlarmThreshold, name: "speed_alarm_threshold"),
        SettingPlan(id: .pedalAngle, name: "pedal_angle"),
        SettingPlan(id: .brakeOverpressureAlarm, name: "brake_alarm"),
    ]

    private let expectedActions: [(id: DeviceActionID, name: String)] = [
        (.horn, "horn"),
        (.resetTripMeter, "reset_trip_meter"),
    ]

    init(timeout: TimeInterval, settingsTest: Bool) {
        self.timeout = timeout
        self.settingsTest = settingsTest
        let environment = ProcessInfo.processInfo.environment
        targetFilter = settingsTest
            ? (environment["CUTOUT_AERO_TARGET"] ?? "NF2557")
            : environment["CUTOUT_AERO_TARGET"]
        allowUnrestorableWrites = environment["CUTOUT_AERO_ALLOW_UNRESTORABLE_WRITES"] == "1"
        includeAudibleControls = environment["CUTOUT_AERO_INCLUDE_AUDIBLE"] == "1"
        includeAlarmModes = environment["CUTOUT_AERO_INCLUDE_ALARM_MODES"] == "1"
        includeTripReset = environment["CUTOUT_AERO_INCLUDE_TRIP_RESET"] == "1"
        latestControls = core.deviceControlsSnapshot
        core.onRecord = { [weak self] record in
            self?.appendRecord(record)
        }
        core.onPhaseChange = { [weak self] phase in
            self?.appendDiagnostic("phase=\(phase)")
        }
        core.onScanStateChange = { [weak self] state in
            self?.probeFirstCandidate(from: state)
        }
        core.onDeviceControlsChange = { [weak self] controls in
            self?.latestControls = controls
        }
    }

    func start() {
        if settingsTest {
            appendDiagnostic("settings_test=enabled target=\(targetFilter ?? "any")")
            appendDiagnostic("settings_unrestorable_writes=\(allowUnrestorableWrites)")
            appendDiagnostic("settings_audible_controls=\(includeAudibleControls)")
            appendDiagnostic("settings_alarm_modes=\(includeAlarmModes)")
            appendDiagnostic("settings_trip_reset=\(includeTripReset)")
        }

        core.start()
        while !didValidate, Date().timeIntervalSince(startedAt) < timeout {
            runLoop(for: 0.1)
            guard rideState.isLiveValidationReady && hasConfirmedAeroIdentity else {
                continue
            }

            if settingsTest {
                didValidate = runSettingsValidation()
                validationFinished = true
            } else {
                didValidate = true
            }
        }

        if didValidate {
            appendDiagnostic("validation=ok")
            core.disconnect()
            printRecords()
        } else if settingsTest && validationFinished {
            print("validation=failed")
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

    private func probeFirstCandidate(from state: DevicePickerScanState) {
        guard !didRequestProbe else { return }

        let row: DevicePickerRow?
        if let targetFilter {
            row = state.rows.first(where: {
                $0.id == targetFilter || $0.title.localizedCaseInsensitiveContains(targetFilter)
            })
            if row == nil {
                let visible = state.rows.map { "\($0.title)[\($0.id)]" }.joined(separator: ";")
                appendDiagnostic("target_not_found=\(targetFilter) candidates=\(visible)")
            }
        } else {
            row = state.rows.first(where: { $0.isProbeRecommended })
        }

        guard let row else { return }
        didRequestProbe = true
        let didProbe = core.probe(platformIdentifier: row.id)
        appendDiagnostic("auto_probe=\(didProbe) id=\(row.id) title=\(row.title)")
    }

    private func runSettingsValidation() -> Bool {
        guard let token = core.connectionSnapshot.token else {
            appendDiagnostic("settings_validation=connection_token_missing")
            return false
        }

        do {
            try core.setDeviceControlsValidation(token: token, authorized: true)
        } catch {
            appendDiagnostic("settings_validation=authorization_failed error=\(error)")
            return false
        }

        guard wait(until: { self.latestControls.validationAuthorized }, for: 3) else {
            appendDiagnostic("settings_validation=authorization_timeout")
            return false
        }

        appendDiagnostic(
            "controls descriptors=\(latestControls.settingDescriptors.count) actions=\(latestControls.actionDescriptors.count)"
        )
        for plan in expectedSettings { testSetting(plan, token: token) }
        for action in expectedActions { testAction(id: action.id, name: action.name, token: token) }

        appendDiagnostic(
            "settings_summary hard_failures=\(hardFailures) unconfirmed=\(unconfirmedWrites) "
                + "skipped=\(skippedWrites) missing=\(missingControls)"
        )
        return hardFailures == 0 && missingControls == 0
    }

    private func testSetting(_ plan: SettingPlan, token: ConnectionAttemptToken) {
        if plan.id == .beeperVolumePercent && !includeAudibleControls {
            skippedWrites += 1
            appendDiagnostic("setting=\(plan.name) result=skipped_audible_opt_in_required")
            return
        }
        if (plan.id == .highSpeedMode || plan.id == .lowBatteryMode) && !includeAlarmModes {
            skippedWrites += 1
            appendDiagnostic("setting=\(plan.name) result=skipped_alarm_mode_opt_in_required")
            return
        }

        guard let descriptor = latestControls.descriptor(for: plan.id) else {
            missingControls += 1
            appendDiagnostic("setting=\(plan.name) result=missing_descriptor")
            return
        }
        guard descriptor.access != .readOnly else {
            missingControls += 1
            appendDiagnostic("setting=\(plan.name) result=read_only")
            return
        }

        if plan.id == .speedAlarmThreshold {
            testInvalidSpeedAlarmValueIfApplicable(descriptor: descriptor, id: plan.id, token: token)
        }

        guard let current = latestControls.setting(for: plan.id)?.current else {
            if case .boolean = descriptor.control {
                testUnknownBoolean(plan, token: token)
                return
            }
            guard allowUnrestorableWrites,
                  let value = probeValue(id: plan.id, current: nil, descriptor: descriptor) else {
                skippedWrites += 1
                appendDiagnostic("setting=\(plan.name) result=skipped_current_unknown")
                return
            }
            submitSetting(value, id: plan.id, name: plan.name, token: token, restore: nil)
            return
        }

        switch current {
        case .boolean:
            testBooleanSetting(plan, current: current, token: token)
        case .number, .choice:
            guard let value = probeValue(id: plan.id, current: current, descriptor: descriptor) else {
                skippedWrites += 1
                appendDiagnostic("setting=\(plan.name) result=skipped_no_alternate")
                return
            }
            submitSetting(value, id: plan.id, name: plan.name, token: token, restore: current)
        case .disabled:
            skippedWrites += 1
            appendDiagnostic("setting=\(plan.name) result=skipped_disabled")
        }
    }

    private func testUnknownBoolean(_ plan: SettingPlan, token: ConnectionAttemptToken) {
        for value in [false, true, false] {
            submitSetting(
                .boolean(value: value),
                id: plan.id,
                name: plan.name,
                token: token,
                restore: nil
            )
        }
    }

    private func testInvalidSpeedAlarmValueIfApplicable(
        descriptor: DeviceSettingDescriptor,
        id: DeviceSettingID,
        token: ConnectionAttemptToken
    ) {
        guard case let .number(_, _, step, _, _, _) = descriptor.control, step > 1 else { return }
        do {
            try core.submitDeviceSetting(
                token: token,
                id: id,
                value: .number(value: 348)
            )
            hardFailures += 1
            appendDiagnostic("setting=speed_alarm_threshold invalid_348=result_accepted")
        } catch DeviceSettingSubmissionError.InvalidValue {
            appendDiagnostic("setting=speed_alarm_threshold invalid_348=result_rejected")
        } catch {
            hardFailures += 1
            appendDiagnostic("setting=speed_alarm_threshold invalid_348=result_wrong_error error=\(error)")
        }
    }

    private func testBooleanSetting(
        _ plan: SettingPlan,
        current: DeviceSettingValue,
        token: ConnectionAttemptToken
    ) {
        guard case let .boolean(value: currentValue) = current else { return }
        submitSetting(
            .boolean(value: !currentValue),
            id: plan.id,
            name: plan.name,
            token: token,
            restore: current
        )
    }

    private func submitSetting(
        _ value: DeviceSettingValue,
        id: DeviceSettingID,
        name: String,
        token: ConnectionAttemptToken,
        restore: DeviceSettingValue?
    ) {
        do {
            try core.submitDeviceSetting(token: token, id: id, value: value)
        } catch {
            hardFailures += 1
            appendDiagnostic("setting=\(name) requested=\(value) result=submit_error error=\(error)")
            return
        }

        let result = waitForSettingResult(id: id, value: value)
        appendDiagnostic("setting=\(name) requested=\(value) result=\(result)")

        guard let restore else { return }
        do {
            try core.submitDeviceSetting(token: token, id: id, value: restore)
            let restoreResult = waitForSettingResult(id: id, value: restore)
            appendDiagnostic("setting=\(name) restore=\(restore) result=\(restoreResult)")
        } catch {
            hardFailures += 1
            appendDiagnostic("setting=\(name) result=restore_submit_error error=\(error)")
        }
    }

    private func testAction(id: DeviceActionID, name: String, token: ConnectionAttemptToken) {
        if id == .horn && !includeAudibleControls {
            skippedWrites += 1
            appendDiagnostic("action=\(name) result=skipped_audible_opt_in_required")
            return
        }
        if id == .resetTripMeter && !includeTripReset {
            skippedWrites += 1
            appendDiagnostic("action=\(name) result=skipped_trip_reset_opt_in_required")
            return
        }

        guard latestControls.actionDescriptors.contains(where: { $0.id == id }) else {
            missingControls += 1
            appendDiagnostic("action=\(name) result=missing_descriptor")
            return
        }

        do {
            try core.submitDeviceAction(token: token, id: id)
        } catch {
            hardFailures += 1
            appendDiagnostic("action=\(name) result=submit_error error=\(error)")
            return
        }

        guard wait(until: {
            guard let action = self.latestControls.actions.first(where: { $0.id == id }) else {
                return false
            }
            return action.status != .idle
        }, for: 3),
            let action = latestControls.actions.first(where: { $0.id == id }) else {
            hardFailures += 1
            appendDiagnostic("action=\(name) result=no_lifecycle_update")
            return
        }

        appendDiagnostic("action=\(name) status=\(action.status) refusal=\(String(describing: action.refusal))")
        if action.status == .failed || action.status == .refused { hardFailures += 1 }
    }

    private func waitForSettingResult(id: DeviceSettingID, value: DeviceSettingValue) -> String {
        guard wait(until: {
            guard let setting = self.latestControls.settings.first(where: { $0.id == id }) else {
                return false
            }
            return setting.requested == value && setting.status != .idle
        }, for: 6) else {
            hardFailures += 1
            return "no_lifecycle_update"
        }

        guard let setting = latestControls.settings.first(where: { $0.id == id }) else {
            hardFailures += 1
            return "missing_snapshot"
        }
        switch setting.status {
        case .confirmed:
            return "confirmed current=\(String(describing: setting.current))"
        case .sentWithoutConfirmation:
            unconfirmedWrites += 1
            return "sent_without_confirmation"
        case .timedOut:
            hardFailures += 1
            return "timed_out"
        case .failed:
            hardFailures += 1
            return "failed"
        case .refused:
            hardFailures += 1
            return "refused=\(String(describing: setting.refusal))"
        case .idle, .waitingForConfirmation:
            hardFailures += 1
            return "pending_timeout"
        }
    }

    private func probeValue(
        id: DeviceSettingID,
        current: DeviceSettingValue,
        descriptor: DeviceSettingDescriptor
    ) -> DeviceSettingValue? {
        switch current {
        case .number(value: let value):
            guard case let .number(minimum, maximum, step, _, _, _) = descriptor.control,
                  step > 0 else { return nil }
            return validNumericProbes(for: id, current: value, minimum: minimum, maximum: maximum, step: step)
                .first.map { .number(value: $0) }
        case .choice(id: let choice):
            guard case let .choices(choices) = descriptor.control,
                  let alternate = choices.first(where: { $0.writable && $0.id != choice }) else {
                return nil
            }
            return .choice(id: alternate.id)
        case .boolean, .disabled:
            return nil
        }
    }

    private func probeValue(
        id: DeviceSettingID,
        current: DeviceSettingValue?,
        descriptor: DeviceSettingDescriptor
    ) -> DeviceSettingValue? {
        switch descriptor.control {
        case .boolean:
            return .boolean(value: false)
        case let .number(minimum, maximum, step, _, _, _):
            guard minimum <= maximum, step > 0 else { return nil }
            let currentValue: Int32?
            if case let .number(value: value) = current { currentValue = value } else { currentValue = nil }
            return validNumericProbes(
                for: id,
                current: currentValue,
                minimum: minimum,
                maximum: maximum,
                step: step
            ).first.map { .number(value: $0) }
        case let .choices(choices):
            return choices.first(where: { $0.writable }).map { .choice(id: $0.id) }
        case .readOnly:
            return nil
        }
    }

    private func validNumericProbes(
        for id: DeviceSettingID,
        current: Int32?,
        minimum: Int32,
        maximum: Int32,
        step: Int32
    ) -> [Int32] {
        let preferred: Int32? = switch id {
        case .tiltbackSpeed: 500
        case .pwmTiltback: 80
        case .pedalHardness: 50
        case .displayBrightness: 50
        case .beeperVolumePercent: 50
        case .dynamicAssist: 50
        case .pedalDipCompensation: 20
        case .lateralTiltLimit: 55
        case .voltageCorrection: 0
        case .speedAlarmThreshold: 400
        case .pedalAngle: 0
        case .brakeOverpressureAlarm: 80
        default: nil
        }
        let candidates = [preferred, current.map { $0 + step }, current.map { $0 - step }].compactMap { $0 }
        return candidates.filter { candidate in
            candidate >= minimum
                && candidate <= maximum
                && (candidate - minimum).isMultiple(of: step)
                && candidate != current
        }
    }

    private func wait(until condition: () -> Bool, for seconds: TimeInterval) -> Bool {
        let deadline = Date().addingTimeInterval(seconds)
        while Date() < deadline {
            if condition() { return true }
            runLoop(for: 0.1)
        }
        return condition()
    }

    private func runLoop(for seconds: TimeInterval) {
        RunLoop.current.run(mode: .default, before: Date(timeIntervalSinceNow: seconds))
    }

    private func appendRecord(_ record: String) {
        guard !record.hasPrefix("candidate=") else {
            candidateRecordCount += 1
            if candidateSamples.count < 16 { candidateSamples.append(record) }
            return
        }
        appendDiagnostic(record)
    }

    private func appendDiagnostic(_ record: String) {
        guard records.count < 2_048 else { return }
        records.append(record)
    }

    private func printRecords() {
        print("candidate_records_seen=\(candidateRecordCount)")
        candidateSamples.forEach { print($0) }
        records.forEach { print($0) }
    }
}
