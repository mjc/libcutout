import XCTest

@testable import CutoutMobileLiveValidator

@MainActor
final class LiveValidatorInvocationTests: XCTestCase {
    func testSettingsSweepIsRejectedBeforeConstructingTransport() {
        var starts = 0
        var records: [String] = []
        let status = LiveValidatorInvocation.run(
            arguments: ["180", "--settings"], environment: [:],
            observeConnection: { _ in
                starts += 1
                return true
            },
            record: { records.append($0) }
        )
        XCTAssertNotEqual(status, 0)
        XCTAssertEqual(starts, 0)
        XCTAssertTrue(records.contains { $0.contains("settings_validation_disabled") })
    }

    func testEveryLegacyMutationOptInFailsBeforeTransportEvenWithoutSettingsArgument() {
        for key in [
            "CUTOUT_AERO_SETTINGS_TEST",
            "CUTOUT_AERO_ALLOW_UNRESTORABLE_WRITES",
            "CUTOUT_AERO_INCLUDE_HEADLIGHT",
            "CUTOUT_AERO_INCLUDE_AUDIBLE",
            "CUTOUT_AERO_INCLUDE_ALARM_MODES",
            "CUTOUT_AERO_INCLUDE_TRIP_RESET",
        ] {
            var starts = 0
            let status = LiveValidatorInvocation.run(
                arguments: [], environment: [key: "1"],
                observeConnection: { _ in
                    starts += 1
                    return true
                }, record: { _ in }
            )
            XCTAssertNotEqual(status, 0, key)
            XCTAssertEqual(starts, 0, key)
        }
    }

    func testInvalidArgumentsNeverStartTransport() {
        for arguments in [["0"], ["-1"], ["nan"], ["inf"], ["601"], ["1", "2"], ["--unknown"]] {
            var starts = 0
            XCTAssertNotEqual(
                LiveValidatorInvocation.run(
                    arguments: arguments, environment: [:],
                    observeConnection: { _ in
                        starts += 1
                        return true
                    }, record: { _ in }
                ), 0, arguments.description)
            XCTAssertEqual(starts, 0, arguments.description)
        }
    }

    func testFailedConnectionRunExecutesOnceAndCannotReportSettingsSuccess() {
        var starts = 0
        var records: [String] = []
        let status = LiveValidatorInvocation.run(
            arguments: ["12"], environment: [:],
            observeConnection: { timeout in
                XCTAssertEqual(timeout, 12)
                starts += 1
                return false
            }, record: { records.append($0) }
        )
        XCTAssertNotEqual(status, 0)
        XCTAssertEqual(starts, 1)
        XCTAssertEqual(records.last, "connection_validation=failed settings_validation=not_run")
    }

    func testConnectionSuccessIsNotSettingsAcceptance() {
        var records: [String] = []
        XCTAssertEqual(
            LiveValidatorInvocation.run(
                arguments: [], environment: ["CUTOUT_AERO_SETTINGS_TEST": "0"],
                observeConnection: { timeout in
                    XCTAssertEqual(timeout, 45)
                    return true
                }, record: { records.append($0) }
            ), 0)
        XCTAssertEqual(records.last, "connection_validation=ok settings_validation=not_run")
        XCTAssertFalse(records.contains("validation=ok"))
    }
}
