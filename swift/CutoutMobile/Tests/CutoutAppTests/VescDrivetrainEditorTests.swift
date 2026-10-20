import CutoutMobile
import CutoutMobileFFI
import XCTest

@testable import CutoutApp

@MainActor
final class VescDrivetrainEditorTests: XCTestCase {
    func testFocusingUnchangedBlankFieldsDoesNotClearRustValidation() async throws {
        let editor = VescDrivetrainEditor(
            token: try makeToken(), load: { _ in nil },
            save: { _, _ in throw MobileVescDrivetrainError.InvalidWheelDiameter }
        )
        await editor.load()
        let saved = await editor.save()
        XCTAssertFalse(saved)
        // SwiftUI can write the existing binding value when the invalid field gains focus.
        let unchanged = editor.draft
        editor.wheelDiameterMm = unchanged.wheelDiameterMm
        editor.motorPolePairs = unchanged.motorPolePairs
        editor.motorToWheelRatio = unchanged.motorToWheelRatio
        XCTAssertEqual(editor.errorKey(for: .wheelDiameter), "vesc.drivetrain.error.wheel_diameter")
        editor.wheelDiameterMm = "90"
        XCTAssertNil(editor.failure)
    }

    func testFailedInitialReadCannotSaveUntilRetrySucceeds() async throws {
        var loads = 0
        var saves = 0
        let editor = VescDrivetrainEditor(
            token: try makeToken(),
            load: { _ in
                loads += 1
                if loads == 1 { throw MobileVescDrivetrainError.PersistenceUnavailable }
                return nil
            },
            save: { _, draft in
                saves += 1
                return draft
            }
        )
        await editor.load()
        XCTAssertFalse(editor.hasLoaded)
        XCTAssertEqual(editor.generalErrorKey, "vesc.drivetrain.error.load")
        editor.wheelDiameterMm = "90"
        XCTAssertEqual(editor.generalErrorKey, "vesc.drivetrain.error.load")
        let refused = await editor.save()
        XCTAssertFalse(refused)
        XCTAssertEqual(saves, 0)
        await editor.load()
        XCTAssertTrue(editor.hasLoaded)
        XCTAssertNil(editor.failure)
        XCTAssertEqual(loads, 2)
        let saved = await editor.save()
        XCTAssertTrue(saved)
        XCTAssertEqual(saves, 1)
    }

    func testUnknownGeometryStaysBlankAndLoadsOnlyOnce() async throws {
        let token = try makeToken()
        var loads = 0
        let editor = VescDrivetrainEditor(
            token: token,
            load: { received in
                XCTAssertEqual(received, token)
                loads += 1
                return nil
            },
            save: { _, draft in draft }
        )
        await editor.load()
        XCTAssertEqual(editor.wheelDiameterMm, "")
        XCTAssertEqual(editor.motorPolePairs, "")
        XCTAssertEqual(editor.motorToWheelRatio, "")
        XCTAssertFalse(editor.isBusy, "Blank fields must reach Rust validation through Save")
        editor.motorToWheelRatio = "2.5"
        await editor.load()
        XCTAssertEqual(loads, 1)
        XCTAssertEqual(editor.motorToWheelRatio, "2.5")
    }

    func testSavePassesTextUnchangedAndUsesRustReturnedDraft() async throws {
        let token = try makeToken()
        let canonical = MobileVescDrivetrainDraft(wheelDiameterMm: "90", motorPolePairs: "7", motorToWheelRatio: "2.5")
        var submitted: MobileVescDrivetrainDraft?
        let editor = VescDrivetrainEditor(
            token: token, load: { _ in nil },
            save: { received, draft in
                XCTAssertEqual(received, token)
                submitted = draft
                return canonical
            }
        )
        await editor.load()
        editor.wheelDiameterMm = "090"
        editor.motorPolePairs = "07"
        editor.motorToWheelRatio = "2.500"
        let saved = await editor.save()
        XCTAssertTrue(saved)
        XCTAssertEqual(submitted?.wheelDiameterMm, "090")
        XCTAssertEqual(submitted?.motorPolePairs, "07")
        XCTAssertEqual(submitted?.motorToWheelRatio, "2.500")
        XCTAssertEqual(editor.draft, canonical)
    }

    func testRustFieldErrorsStayBesideTheirFieldsWithoutReplacingDrafts() async throws {
        let cases: [(MobileVescDrivetrainError, VescDrivetrainEditor.Field)] = [
            (.InvalidWheelDiameter, .wheelDiameter),
            (.InvalidMotorPolePairs, .motorPolePairs),
            (.InvalidReduction, .reduction),
        ]
        for (error, field) in cases {
            let editor = VescDrivetrainEditor(
                token: try makeToken(), load: { _ in nil }, save: { _, _ in throw error }
            )
            await editor.load()
            editor.wheelDiameterMm = "wheel text"
            editor.motorPolePairs = "pole text"
            editor.motorToWheelRatio = "ratio text"
            let saved = await editor.save()
            XCTAssertFalse(saved)
            XCTAssertEqual(editor.errorKey(for: field), field.errorKey)
            XCTAssertNil(editor.generalErrorKey)
            XCTAssertEqual(editor.wheelDiameterMm, "wheel text")
            XCTAssertEqual(editor.motorPolePairs, "pole text")
            XCTAssertEqual(editor.motorToWheelRatio, "ratio text")
            editor.wheelDiameterMm = "90"
            XCTAssertNil(editor.failure)
        }
    }

    func testEditorRetainsItsTokenWhenTheConnectionChanges() async throws {
        let state = CutoutSessionStateHandle()
        let original = try XCTUnwrap(state.beginConnectionAttempt(platformIdentifier: "first", nowMs: 0).token)
        let editor = VescDrivetrainEditor(
            token: original, load: { _ in nil },
            save: { received, _ in
                XCTAssertEqual(received, original)
                throw MobileVescDrivetrainError.ConnectionUnavailable
            }
        )
        await editor.load()
        _ = state.beginConnectionAttempt(platformIdentifier: "replacement", nowMs: 1)
        let saved = await editor.save()
        XCTAssertFalse(saved)
        XCTAssertEqual(editor.token, original)
        XCTAssertEqual(editor.generalErrorKey, "vesc.drivetrain.error.connection")
    }

    func testUnsupportedAndStorageFailuresRemainGeneralErrors() async throws {
        for (error, key) in [
            (MobileVescDrivetrainError.UnsupportedDevice, "vesc.drivetrain.error.unsupported"),
            (.InvalidControllerTarget, "vesc.drivetrain.error.unsupported"),
            (.PersistenceUnavailable, "vesc.drivetrain.error.persistence"),
        ] {
            let editor = VescDrivetrainEditor(
                token: try makeToken(), load: { _ in nil }, save: { _, _ in throw error }
            )
            await editor.load()
            let saved = await editor.save()
            XCTAssertFalse(saved)
            XCTAssertEqual(editor.generalErrorKey, key)
        }
    }

    private func makeToken() throws -> ConnectionAttemptToken {
        try XCTUnwrap(
            CutoutSessionStateHandle().beginConnectionAttempt(platformIdentifier: "controller", nowMs: 0).token)
    }
}
