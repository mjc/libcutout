import CutoutMobile
import CutoutMobileFFI
import XCTest
@testable import CutoutApp

final class DeviceControlsPresentationTests: XCTestCase {
    func testPWMDutyKeepsWireValueAndOffDistinctFromUnknown() {
        let control = DeviceSettingControl.number(minimum: 10, maximum: 90, step: 1, precision: 0, unit: .pwmDutyPercent, canDisable: true)
        XCTAssertEqual(DeviceControlPresentation.value(.number(value: 80), control: control), "80%")
        XCTAssertEqual(DeviceControlPresentation.value(.number(value: 20), control: control), "20%")
        XCTAssertEqual(DeviceControlPresentation.value(.disabled, control: control), "Off")
        XCTAssertNotEqual(DeviceControlPresentation.value(nil, control: control), "Off")
        XCTAssertTrue(DeviceControlPresentation.options(control).isEmpty, "Numeric controls must not allocate menu entries")
        XCTAssertEqual(DeviceControlPresentation.value(.boolean(value: true), control: .boolean), "On")
    }

    func testSubmissionFeedbackShowsPendingUnconfirmedAndFailures() {
        for raw: Int32 in [0, 46, 70] {
            XCTAssertEqual(DeviceControlPresentation.value(.number(value: raw), control: .readOnly), String(raw))
        }
        XCTAssertEqual(localizedAppText("settings.charge_limit_diagnostic.label"), "Charge limit (raw)")
        XCTAssertEqual(localizedAppText("settings.high_beam.label"), "Headlight")
        XCTAssertNil(DeviceControlPresentation.status(.idle))
        XCTAssertNil(DeviceControlPresentation.actionStatus(.idle))
        XCTAssertEqual(DeviceControlPresentation.status(.failed), "Failed")
        XCTAssertEqual(DeviceControlPresentation.status(.sentWithoutConfirmation), "Sent; no confirmation")
        XCTAssertEqual(DeviceControlPresentation.refusal(.busy), "The wheel is processing another command.")
    }

    func testSpeedOptionsUseRideUnitsWithoutChangingCanonicalValues() {
        let control = DeviceSettingControl.number(minimum: 10, maximum: 600, step: 10, precision: 1, unit: .kilometresPerHour, canDisable: false)
        let value = DeviceSettingValue.number(value: 320)
        let ride = SpeedReadout(millimetersPerSecond: 8_889)
        XCTAssertEqual(DeviceControlPresentation.value(value, control: control), "\(ride.displayValue) \(ride.displayUnit)")
        XCTAssertEqual(DeviceControlPresentation.steppedValue(draft: value, current: nil, control: control, increasing: true), .number(value: 330))
        XCTAssertEqual(DeviceControlPresentation.steppedValue(draft: nil, current: .number(value: 319), control: control, increasing: false), .number(value: 310))
        // A fractional reported value is still readable even though it cannot be selected for a write.
        XCTAssertNotEqual(DeviceControlPresentation.value(.number(value: 116), control: control), DeviceControlPresentation.value(nil, control: control))
    }

    func testReadOnlyChoicesRemainReadableButCannotBeSelected() {
        let control = DeviceSettingControl.choices(choices: [
            .init(id: 1, labelKey: "settings.choice.off", writable: false),
            .init(id: 2, labelKey: "settings.choice.pwm_tiltback", writable: false),
            .init(id: 3, labelKey: "settings.choice.both_alarm_stages", writable: true),
        ])
        XCTAssertEqual(DeviceControlPresentation.value(.choice(id: 1), control: control), "Off")
        XCTAssertEqual(DeviceControlPresentation.options(control), [.choice(id: 3)])
    }

    func testAeroEditableCatalogAppearsOnceInOrdinaryGroupsInRustOrder() throws {
        let owner = CutoutSessionStateHandle()
        let token = try XCTUnwrap(owner.beginConnectionAttempt(platformIdentifier: "Aero", nowMs: 0).token)
        _ = owner.connectionLinkEstablished(token: token)
        var frame = Data(repeating: 0, count: 42)
        frame.replaceSubrange(0..<4, with: [0xdc, 0x5a, 0x5c, 38])
        frame.replaceSubrange(28..<30, with: [0xa7, 0xf8])
        _ = owner.observeConnectionNotification(token: token, bytes: frame)
        _ = owner.resolveDeviceSession(token: token, identificationComplete: false, nowMs: 1)
        let descriptors = owner.settingsDescriptors().descriptors
        let expected: [(DeviceSettingGroup, [DeviceSettingID])] = [
            (.interface, [.highBeam, .displayBrightness, .displayUnits, .beeperVolumePercent]),
            (.limits, [.tiltbackSpeed, .pwmTiltback, .lateralTiltLimit, .speedAlarmThreshold, .brakeOverpressureAlarm]),
            (.riding, [.pedalHardness, .dynamicAssist, .pedalDipCompensation, .pedalAngle, .ridingPreset]),
            (.modes, [.voltageCorrection, .highSpeedMode, .lowBatteryMode, .transportMode]),
        ]
        for (group, ids) in expected {
            XCTAssertEqual(DeviceControlPresentation.settings(Array(descriptors.reversed()), in: group).map(\.id), ids)
        }
        let visible = DeviceControlPresentation.groups.flatMap { DeviceControlPresentation.settings(descriptors, in: $0) }
        XCTAssertEqual(visible.count, 18)
        XCTAssertEqual(Set(visible.map(\.id)).count, 18)
        XCTAssertFalse(visible.contains { $0.id == .chargeLimitDiagnostic })
        XCTAssertTrue(DeviceControlPresentation.settings([makeDescriptor(access: .unverified)], in: .interface).isEmpty)
        XCTAssertEqual(makeDescriptor().writeVerification, .unverified, "Availability must not rewrite verification evidence")
    }

    func testRequestedBooleanDoesNotBecomeCurrentOrClaimTransportSuccess() {
        var state = makeState()
        state.requested = .boolean(value: true)
        state.status = .sentWithoutConfirmation
        XCTAssertNil(state.current)
        XCTAssertEqual(DeviceControlPresentation.value(state.requested, control: .boolean), "On")
        XCTAssertEqual(DeviceControlPresentation.status(state.status), "Sent; no confirmation")
        state.current = .boolean(value: false)
        XCTAssertEqual(DeviceControlPresentation.value(state.current, control: .boolean), "Off")
        XCTAssertEqual(DeviceControlPresentation.value(state.requested, control: .boolean), "On")
    }

    func testSourceLabelsKeepWheelReadbackSeparateFromRequests() {
        XCTAssertEqual(DeviceControlPresentation.sourceLabel(.liveReadback), "Wheel")
        XCTAssertEqual(DeviceControlPresentation.sourceLabel(.captureReplay), "Replay")
        XCTAssertEqual(DeviceControlPresentation.sourceLabel(.userRequest), "Requested")
        XCTAssertEqual(DeviceControlPresentation.sourceLabel(nil), "Wheel")
        XCTAssertEqual(
            DeviceControlPresentation.status(.sentWithoutConfirmation),
            "Sent; no confirmation"
        )
        XCTAssertEqual(localizedAppText("settings.state.timed_out"), "Not confirmed")
    }

    func testNumericDraftWinsOverChangingReadbackAndRespectsFixedPointBounds() {
        let control = DeviceSettingControl.number(minimum: -15, maximum: 15, step: 1, precision: 1, unit: .percent, canDisable: false)
        XCTAssertEqual(DeviceControlPresentation.steppedValue(draft: .number(value: 3), current: .number(value: -10), control: control, increasing: true), .number(value: 4))
        XCTAssertEqual(DeviceControlPresentation.steppedValue(draft: .number(value: 15), current: nil, control: control, increasing: true), .number(value: 15))
        XCTAssertEqual(DeviceControlPresentation.steppedValue(draft: .number(value: -15), current: nil, control: control, increasing: false), .number(value: -15))
        XCTAssertEqual(DeviceControlPresentation.value(.number(value: 4), control: control), "0.4%")
        XCTAssertEqual(DeviceControlPresentation.steppedValue(draft: nil, current: nil, control: control, increasing: true), .number(value: -15), "Only an explicit step creates the first draft")
    }

    func testBooleanSelectionAcknowledgesAcceptedCommandsButNeverFailures() {
        var state = makeState()
        XCTAssertNil(DeviceControlPresentation.booleanSelection(state))
        state.current = .boolean(value: false)
        state.requested = .boolean(value: true)
        for status: DeviceSettingStatus in [.waitingForConfirmation, .sentWithoutConfirmation] {
            state.status = status
            XCTAssertEqual(DeviceControlPresentation.booleanSelection(state), true)
            XCTAssertEqual(state.current, .boolean(value: false))
        }
        for status: DeviceSettingStatus in [.idle, .confirmed, .failed, .refused, .timedOut] {
            state.status = status
            XCTAssertEqual(DeviceControlPresentation.booleanSelection(state), false)
            state.current = nil
            XCTAssertNil(DeviceControlPresentation.booleanSelection(state))
            state.current = .boolean(value: false)
        }
    }

    func testNumericStepsAlignFractionalReadbackAndDoNotOverflow() {
        let control = DeviceSettingControl.number(minimum: 10, maximum: 95, step: 10, precision: 0, unit: .percent, canDisable: true)
        XCTAssertEqual(DeviceControlPresentation.steppedValue(draft: nil, current: .number(value: 26), control: control, increasing: true), .number(value: 30))
        XCTAssertEqual(DeviceControlPresentation.steppedValue(draft: nil, current: .number(value: 26), control: control, increasing: false), .number(value: 20))
        XCTAssertEqual(DeviceControlPresentation.steppedValue(draft: .number(value: 90), current: nil, control: control, increasing: true), .number(value: 90))
        XCTAssertEqual(DeviceControlPresentation.steppedValue(draft: .disabled, current: .number(value: 80), control: control, increasing: true), .number(value: 10))
        let wide = DeviceSettingControl.number(minimum: .min, maximum: .max, step: 1, precision: 0, unit: .level, canDisable: false)
        XCTAssertTrue(DeviceControlPresentation.options(wide).isEmpty)
        XCTAssertEqual(DeviceControlPresentation.steppedValue(draft: .number(value: .max), current: nil, control: wide, increasing: true), .number(value: .max))
    }

    func testApplyRequiresAnExplicitChangedDraft() {
        XCTAssertFalse(DeviceControlPresentation.hasChanges(draft: nil, current: nil))
        XCTAssertFalse(DeviceControlPresentation.hasChanges(draft: nil, current: .number(value: 70)))
        XCTAssertFalse(DeviceControlPresentation.hasChanges(draft: .number(value: 70), current: .number(value: 70)))
        XCTAssertTrue(DeviceControlPresentation.hasChanges(draft: .number(value: 71), current: .number(value: 70)))
        XCTAssertTrue(DeviceControlPresentation.hasChanges(draft: .disabled, current: nil))
        XCTAssertFalse(DeviceControlPresentation.hasChanges(draft: .disabled, current: .disabled))
        XCTAssertEqual(DeviceControlPresentation.value(nil, control: .boolean), "—")
        XCTAssertNil(DeviceControlPresentation.status(.confirmed))
        XCTAssertEqual(DeviceControlPresentation.actionStatus(.sentWithoutConfirmation), "Sent; no confirmation")
        XCTAssertNotNil(DeviceControlPresentation.status(.waitingForConfirmation))
        XCTAssertNotNil(DeviceControlPresentation.status(.timedOut))
        XCTAssertNotNil(DeviceControlPresentation.status(.refused))
    }

    func testSubmissionErrorsExplainFailure() {
        XCTAssertEqual(DeviceControlPresentation.error(DeviceSettingSubmissionError.ConnectionUnavailable), "Connect to the wheel before sending a command.")
        XCTAssertEqual(DeviceControlPresentation.error(DeviceSettingSubmissionError.Unverified), "Writing this control has not been verified.")
        XCTAssertEqual(DeviceControlPresentation.error(DeviceSettingSubmissionError.InvalidValue), "Choose one of the available values.")
    }

    private func makeDescriptor(access: DeviceSettingAccess = .writable) -> DeviceSettingDescriptor {
        .init(id: .highBeam, labelKey: "settings.high_beam.label", helpKey: nil, valueSemanticsKey: nil, group: .interface, order: 1, control: .boolean, access: access, writeVerification: .unverified, confirmationSupported: false)
    }

    private func makeState() -> DeviceSettingSnapshot {
        .init(id: .highBeam, current: nil, currentSource: nil, evidence: nil, requested: nil, status: .idle, ageMs: nil, refusal: nil)
    }

    func testActionButtonsFollowRustNextStepWithoutInferringFromStatus() {
        let descriptor = DeviceActionDescriptor(id: .gyroCalibration, labelKey: "actions.gyro_calibration.label", helpKey: "actions.gyro_calibration.help", order: 1, role: .procedure, access: .available, writeVerification: .unverified, confirmation: .progressReadback)
        var state = DeviceActionSnapshot(id: .gyroCalibration, progress: nil, status: .idle, ageMs: nil, requestedStep: nil, nextStep: .available(step: .prepareGyroCalibration), refusal: nil)
        XCTAssertTrue(DeviceControlPresentation.actionAvailable(descriptor: descriptor, state: state))
        XCTAssertEqual(DeviceControlPresentation.actionTitle(descriptor: descriptor, state: state), "Calibrate pedals")
        state.nextStep = .busy
        XCTAssertFalse(DeviceControlPresentation.actionAvailable(descriptor: descriptor, state: state))
        state.nextStep = .available(step: .startGyroCalibration)
        XCTAssertEqual(DeviceControlPresentation.actionTitle(descriptor: descriptor, state: state), "Start calibration")
        state.nextStep = .restartRequired
        XCTAssertFalse(DeviceControlPresentation.actionAvailable(descriptor: descriptor, state: state))
        XCTAssertNil(DeviceControlPresentation.actionStatus(.idle))
    }
}
