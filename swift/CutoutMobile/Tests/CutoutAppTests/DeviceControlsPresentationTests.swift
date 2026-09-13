import CutoutMobile
import CutoutMobileFFI
import XCTest
@testable import CutoutApp

final class DeviceControlsPresentationTests: XCTestCase {
    func testPWMDutyKeepsWireValueAndOffDistinctFromUnknown() {
        let control = DeviceSettingControl.number(minimum: 10, maximum: 90, step: 1, precision: 0, unit: .pwmDutyPercent, canDisable: true)
        XCTAssertEqual(DeviceControlPresentation.value(.number(value: 80), control: control), "80% PWM duty")
        XCTAssertEqual(DeviceControlPresentation.value(.number(value: 20), control: control), "20% PWM duty")
        XCTAssertEqual(DeviceControlPresentation.value(.disabled, control: control), "Off")
        XCTAssertNotEqual(DeviceControlPresentation.value(nil, control: control), "Off")
        XCTAssertTrue(DeviceControlPresentation.options(control).contains(.disabled))
        XCTAssertEqual(DeviceControlPresentation.value(.boolean(value: true), control: .boolean), "On")
    }

    func testRawChargeValuesAndSubmissionFeedbackRemainExplicit() {
        for raw: Int32 in [0, 46, 70] {
            XCTAssertEqual(DeviceControlPresentation.value(.number(value: raw), control: .readOnly), String(raw))
        }
        XCTAssertEqual(localizedAppText("settings.charge_limit_diagnostic.label"), "Charge limit (raw)")
        XCTAssertEqual(localizedAppText("settings.high_beam.label"), "High beam")
        XCTAssertNil(DeviceControlPresentation.status(.idle))
        XCTAssertNil(DeviceControlPresentation.actionStatus(.idle))
        XCTAssertEqual(DeviceControlPresentation.status(.failed), "Failed")
        XCTAssertEqual(DeviceControlPresentation.status(.sentWithoutConfirmation), "Sent without confirmation")
        XCTAssertEqual(DeviceControlPresentation.refusal(.busy), "The wheel is processing another command.")
    }

    func testSpeedOptionsUseRideUnitsWithoutChangingCanonicalValues() {
        let control = DeviceSettingControl.number(minimum: 10, maximum: 600, step: 10, precision: 1, unit: .kilometresPerHour, canDisable: false)
        let value = DeviceSettingValue.number(value: 320)
        let ride = SpeedReadout(millimetersPerSecond: 8_889)
        XCTAssertEqual(DeviceControlPresentation.value(value, control: control), "\(ride.displayValue) \(ride.displayUnit)")
        XCTAssertTrue(DeviceControlPresentation.options(control).contains(value))
        XCTAssertFalse(DeviceControlPresentation.options(control).contains(.number(value: 319)))
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

    func testActionButtonsFollowRustNextStepWithoutInferringFromStatus() {
        let descriptor = DeviceActionDescriptor(id: .gyroCalibration, labelKey: "actions.gyro_calibration.label", helpKey: "actions.gyro_calibration.help", order: 1, role: .procedure, access: .available, confirmation: .progressReadback)
        var state = DeviceActionSnapshot(id: .gyroCalibration, progress: nil, status: .idle, ageMs: nil, requestedStep: nil, nextStep: .available(step: .prepareGyroCalibration), refusal: nil)
        XCTAssertTrue(DeviceControlPresentation.actionAvailable(descriptor: descriptor, state: state))
        XCTAssertEqual(DeviceControlPresentation.actionTitle(descriptor: descriptor, state: state), "Prepare calibration")
        state.nextStep = .busy
        XCTAssertFalse(DeviceControlPresentation.actionAvailable(descriptor: descriptor, state: state))
        state.nextStep = .available(step: .startGyroCalibration)
        XCTAssertEqual(DeviceControlPresentation.actionTitle(descriptor: descriptor, state: state), "Start calibration")
        state.nextStep = .restartRequired
        XCTAssertFalse(DeviceControlPresentation.actionAvailable(descriptor: descriptor, state: state))
        XCTAssertNil(DeviceControlPresentation.actionStatus(.idle))
    }
}
