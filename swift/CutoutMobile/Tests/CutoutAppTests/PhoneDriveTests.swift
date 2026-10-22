import CutoutMobile
import CutoutMobileFFI
import Foundation
import XCTest

@testable import CutoutApp

final class PhoneDriveTests: XCTestCase {
    func testAreaCoversBothHalvesIncludingHeaderAndLowerReadings() {
        let size = CGSize(width: 430, height: 835)
        for y: CGFloat in [0, 40, 417, 834] {
            XCTAssertEqual(PhoneDriveTouchGeometry.region(at: CGPoint(x: 1, y: y), size: size, mode: .area), .brake)
            XCTAssertEqual(
                PhoneDriveTouchGeometry.region(at: CGPoint(x: 429, y: y), size: size, mode: .area), .throttle)
        }
        XCTAssertNil(PhoneDriveTouchGeometry.region(at: CGPoint(x: 10, y: 835), size: size, mode: .area))
        XCTAssertNil(PhoneDriveTouchGeometry.region(at: CGPoint(x: -1, y: 20), size: size, mode: .area))
    }

    func testButtonsKeepTheirGapOutsideBothHoldRegions() {
        let size = CGSize(width: 382, height: 180)
        XCTAssertEqual(PhoneDriveTouchGeometry.region(at: CGPoint(x: 20, y: 10), size: size, mode: .buttons), .brake)
        XCTAssertEqual(
            PhoneDriveTouchGeometry.region(at: CGPoint(x: 360, y: 179), size: size, mode: .buttons), .throttle)
        XCTAssertNil(PhoneDriveTouchGeometry.region(at: CGPoint(x: 191, y: 90), size: size, mode: .buttons))
        XCTAssertNil(PhoneDriveTouchGeometry.region(at: CGPoint(x: 360, y: 180), size: size, mode: .buttons))
    }

    func testDisabledAndInvalidGeometryHaveNoDriveRegion() {
        for mode in [MobilePhoneControlModeDto.disabled, .buttons, .area] {
            XCTAssertNil(PhoneDriveTouchGeometry.region(at: .zero, size: .zero, mode: mode))
            XCTAssertNil(
                PhoneDriveTouchGeometry.region(
                    at: CGPoint(x: .nan, y: 10), size: CGSize(width: 430, height: 835), mode: mode))
        }
        XCTAssertNil(
            PhoneDriveTouchGeometry.region(
                at: CGPoint(x: 10, y: 10), size: CGSize(width: 430, height: 835), mode: .disabled))
    }

    @MainActor
    func testProductionUnavailableSnapshotNeverEnablesTouchCapture() {
        let core = CutoutSessionCore(testScript: CutoutUITestSessionFixture.eskate.testScript)
        let model = PhoneDriveModel(core: core)
        XCTAssertEqual(model.snapshot.mode, .disabled)
        XCTAssertFalse(model.capturesInput)
        XCTAssertFalse(model.isInputPreview)
        model.cancel(.navigation)
        XCTAssertFalse(model.capturesInput)
        XCTAssertEqual(model.snapshot.unavailability, .disconnected)
    }

    @MainActor
    func testPreferenceLoadFailureShowsRecoveryWithoutInventingAChoice() async {
        let model = PhoneDriveModel(core: CutoutSessionCore(testScript: CutoutUITestSessionFixture.eskate.testScript))
        await model.loadMode()
        XCTAssertTrue(model.settingsError)
        XCTAssertFalse(model.settingsLoaded)
        XCTAssertEqual(model.snapshot.mode, .disabled)
        XCTAssertFalse(model.capturesInput)
    }

    @MainActor
    func testSavedLayoutReloadsWithoutEnablingProductionInput() async throws {
        let directory = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: directory) }
        let database = try openRideDatabase(path: directory.appendingPathComponent("ride.sqlite").path)
        defer { try? database.shutdown() }
        let model = PhoneDriveModel(
            core: CutoutSessionCore(
                testScript: CutoutUITestSessionFixture.eskate.testScript, database: database
            ))
        await model.loadMode()
        XCTAssertTrue(model.settingsLoaded)
        await model.saveMode(.buttons)
        XCTAssertFalse(model.settingsError)
        XCTAssertEqual(model.snapshot.mode, .buttons)

        let reopened = PhoneDriveModel(
            core: CutoutSessionCore(
                testScript: CutoutUITestSessionFixture.eskate.testScript, database: database
            ))
        await reopened.loadMode()
        XCTAssertEqual(reopened.snapshot.mode, .buttons)
        XCTAssertFalse(reopened.capturesInput)
        if case .pressed = reopened.snapshot.phase { XCTFail("Restoring a layout must not restore a held gesture") }

        try database.shutdown()
        await reopened.saveMode(.area)
        XCTAssertTrue(reopened.settingsError)
        XCTAssertEqual(reopened.snapshot.mode, .buttons)
        XCTAssertFalse(reopened.capturesInput)
    }
}
