import CutoutMobile
import CutoutMobileFFI
import Foundation
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

    func testLocaleDraftRoundTripsCanonicalFixedPointTextThroughLoadSaveAndReopen() async throws {
        let canonical = MobileVescDrivetrainDraft(
            wheelDiameterMm: "90.125", motorPolePairs: "7", motorToWheelRatio: "2.505")
        for (identifier, separator) in [("de_DE", ","), ("en_US_POSIX", ".")] {
            let locale = Locale(identifier: identifier)
            let token = try makeToken()
            var submitted: MobileVescDrivetrainDraft?
            let editor = VescDrivetrainEditor(
                token: token, locale: locale, load: { _ in canonical },
                save: { received, draft in
                    XCTAssertEqual(received, token)
                    submitted = draft
                    return canonical
                }
            )
            await editor.load()
            XCTAssertEqual(editor.wheelDiameterMm, "90\(separator)125", identifier)
            XCTAssertEqual(editor.motorPolePairs, "7", identifier)
            XCTAssertEqual(editor.motorToWheelRatio, "2\(separator)505", identifier)
            XCTAssertEqual(editor.draft, canonical, identifier)
            editor.wheelDiameterMm = "090\(separator)1250"
            editor.motorPolePairs = "07"
            editor.motorToWheelRatio = "2\(separator)5050"
            let saved = await editor.save()
            XCTAssertTrue(saved, identifier)
            XCTAssertEqual(submitted?.wheelDiameterMm, "090.1250", identifier)
            XCTAssertEqual(submitted?.motorPolePairs, "07", identifier)
            XCTAssertEqual(submitted?.motorToWheelRatio, "2.5050", identifier)
            XCTAssertEqual(editor.wheelDiameterMm, "90\(separator)125", identifier)
            XCTAssertEqual(editor.motorPolePairs, "7", identifier)
            XCTAssertEqual(editor.motorToWheelRatio, "2\(separator)505", identifier)
            XCTAssertEqual(editor.draft, canonical, identifier)

            let reopened = VescDrivetrainEditor(
                token: token, locale: locale, load: { _ in canonical }, save: { _, draft in draft })
            await reopened.load()
            XCTAssertEqual(reopened.wheelDiameterMm, "90\(separator)125", identifier)
            XCTAssertEqual(reopened.motorToWheelRatio, "2\(separator)505", identifier)
            XCTAssertEqual(reopened.draft, canonical, identifier)
        }
    }

    func testLocalizedMalformedGeometryStillReachesRustFieldValidationWithoutRounding() async throws {
        let cases: [(String, VescDrivetrainEditor.Field, String, String)] = [
            ("de_DE", .reduction, "2,5001", "2.5001"),
            ("de_DE", .wheelDiameter, "1.234,5", "1.234.5"),
            ("de_DE", .wheelDiameter, "90,5.2", "90.5.2"),
            ("en_US_POSIX", .wheelDiameter, "1,234.5", "1,234.5"),
            ("de_DE", .motorPolePairs, "7,0", "7,0"),
        ]
        for (identifier, field, input, expected) in cases {
            let state = CutoutSessionStateHandle()
            let token = try XCTUnwrap(state.beginConnectionAttempt(platformIdentifier: "controller", nowMs: 0).token)
            var submitted: MobileVescDrivetrainDraft?
            let editor = VescDrivetrainEditor(
                token: token, locale: Locale(identifier: identifier), load: { _ in nil },
                save: { received, draft in
                    XCTAssertEqual(received, token)
                    submitted = draft
                    return try state.saveConnectionVescDrivetrain(token: received, draft: draft, updatedAtMs: 1)
                }
            )
            await editor.load()
            editor.wheelDiameterMm = "90.125"
            editor.motorPolePairs = "7"
            editor.motorToWheelRatio = "2.505"
            switch field {
            case .wheelDiameter: editor.wheelDiameterMm = input
            case .motorPolePairs: editor.motorPolePairs = input
            case .reduction: editor.motorToWheelRatio = input
            }
            let saved = await editor.save()
            XCTAssertFalse(saved, input)
            XCTAssertEqual(editor.errorKey(for: field), field.errorKey, input)
            XCTAssertNil(editor.generalErrorKey, input)
            switch field {
            case .wheelDiameter:
                XCTAssertEqual(submitted?.wheelDiameterMm, expected)
                XCTAssertEqual(editor.wheelDiameterMm, input)
            case .motorPolePairs:
                XCTAssertEqual(submitted?.motorPolePairs, expected)
                XCTAssertEqual(editor.motorPolePairs, input)
            case .reduction:
                XCTAssertEqual(submitted?.motorToWheelRatio, expected)
                XCTAssertEqual(editor.motorToWheelRatio, input)
            }
        }
    }

    private func makeToken() throws -> ConnectionAttemptToken {
        try XCTUnwrap(
            CutoutSessionStateHandle().beginConnectionAttempt(platformIdentifier: "controller", nowMs: 0).token)
    }
}
