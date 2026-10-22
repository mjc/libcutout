import CutoutMobile
import CutoutMobileFFI
import Foundation
import XCTest

@testable import CutoutApp

#if os(iOS)
    import UIKit
#endif

final class PhoneDriveTests: XCTestCase {
    #if os(iOS)
        @MainActor
        func testColdWindowEpochCanObserveAllUpForAssistiveActivationWithoutATouchEvent() {
            let observer = PhoneDriveNativeContactObserver()
            let window = UIWindow(frame: CGRect(x: 0, y: 0, width: 430, height: 900))
            observer.now = { 500 }
            observer.currentGeneration = { 1 }
            var observations: [(MobilePhoneDriveContactsDto, UInt64, UInt64)] = []
            observer.observe = { observations.append(($0, $1, $2)) }
            observer.attach(to: window)

            observer.reobserveAllUp()

            XCTAssertEqual(observations.count, 1)
            XCTAssertEqual(observations.first?.0, .released)
            XCTAssertEqual(observations.first?.1, 1)
            XCTAssertEqual(observations.first?.2, 500)
            observer.attach(to: nil)
        }

        @MainActor
        func testContinuouslyEmptyWindowSamplesFreshAllUpAfterAnIdleAssistiveAction() {
            let observer = PhoneDriveNativeContactObserver()
            let window = UIWindow(frame: CGRect(x: 0, y: 0, width: 430, height: 900))
            var now: UInt64 = 500
            observer.now = { now }
            var generation: UInt64 = 1
            observer.currentGeneration = { generation }
            var observations: [(MobilePhoneDriveContactsDto, UInt64, UInt64)] = []
            observer.observe = { observations.append(($0, $1, $2)) }
            observer.attach(to: window)
            observer.reobserveAllUp()

            now = 1_000
            generation = 2
            observer.reobserveAllUp()

            XCTAssertEqual(observations.count, 2)
            XCTAssertEqual(observations.last?.0, .released)
            XCTAssertEqual(observations.last?.1, 2)
            XCTAssertEqual(
                observations.last?.2, 1_000, "A current empty sample must not replay an expired event timestamp")
            observer.attach(to: nil)
        }

        @MainActor
        func testWindowObservationLossRemainsUnknownAfterReattachment() {
            let observer = PhoneDriveNativeContactObserver()
            let window = UIWindow(frame: CGRect(x: 0, y: 0, width: 430, height: 900))
            observer.now = { 500 }
            observer.currentGeneration = { 2 }
            var observations: [MobilePhoneDriveContactsDto] = []
            observer.observe = { contacts, _, _ in observations.append(contacts) }
            var cancellations: [MobilePhoneDriveCancelReasonDto] = []
            observer.cancel = { cancellations.append($0) }
            observer.attach(to: window)
            observer.attach(to: nil)
            observer.attach(to: window)

            observer.reobserveAllUp()
            observer.discreteAction(.throttle)

            XCTAssertEqual(cancellations, [.sceneInactive])
            XCTAssertEqual(
                observations, [.multiple],
                "An unknown contact set cannot become all-up through remount or assistive input")
            observer.attach(to: nil)
        }
    #endif

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
                    at: CGPoint(x: CGFloat.nan, y: 10), size: CGSize(width: 430, height: 835), mode: mode))
        }
        XCTAssertNil(
            PhoneDriveTouchGeometry.region(
                at: CGPoint(x: 10, y: 10), size: CGSize(width: 430, height: 835), mode: .disabled))
    }

    @MainActor
    func testDisconnectedSnapshotCannotEnableTouchCapture() {
        let core = CutoutSessionCore(testScript: CutoutUITestSessionFixture.eskate.testScript)
        let model = PhoneDriveModel(core: core)
        XCTAssertEqual(model.snapshot.mode, .disabled)
        XCTAssertFalse(model.capturesInput)
        XCTAssertFalse(model.isInputPreview)
        model.cancel(.navigation)
        XCTAssertFalse(model.capturesInput)
        XCTAssertEqual(model.snapshot.availability, .unavailable(reason: .disconnected))
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
    @MainActor
    func testBudgetDraftsUseRustValidationAndRestoreCanonicalSavedLimits() async throws {
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
        XCTAssertEqual(model.throttleAmpsText, "3")
        XCTAssertEqual(model.brakeAmpsText, "3")
        model.throttleAmpsText = "2.500"
        model.brakeAmpsText = "1.25"
        await model.saveLimits()
        XCTAssertFalse(model.limitsError)
        XCTAssertEqual(model.snapshot.limits.throttleMilliamps, 2_500)
        XCTAssertEqual(model.snapshot.limits.brakeMilliamps, 1_250)
        XCTAssertEqual(model.throttleAmpsText, "2.5")
        let saved = model.snapshot.limits
        model.throttleAmpsText = "not a current"
        await model.saveLimits()
        XCTAssertTrue(model.limitsError)
        XCTAssertEqual(model.snapshot.limits, saved)
        XCTAssertEqual(model.throttleAmpsText, "not a current")
        let reopened = PhoneDriveModel(
            core: CutoutSessionCore(
                testScript: CutoutUITestSessionFixture.eskate.testScript, database: database
            ))
        await reopened.loadMode()
        XCTAssertEqual(reopened.throttleAmpsText, "2.5")
        XCTAssertEqual(reopened.brakeAmpsText, "1.25")
        XCTAssertFalse(reopened.capturesInput)
    }

    @MainActor
    func testLateLimitSaveDoesNotReplaceANewerNativeDraft() async throws {
        let fixture = try await SQLiteRideStall.make()
        defer { fixture.release.signal() }
        let core = CutoutSessionCore(
            testScript: CutoutUITestSessionFixture.eskate.testScript, database: fixture.database)
        let model = PhoneDriveModel(core: core)
        await model.loadMode()
        let previous = model.snapshot.limits
        defer {
            _ = try? core.rideSessionStateHandle.savePhoneDriveLimits(
                throttleAmpsText: String(Double(previous.throttleMilliamps) / 1_000),
                brakeAmpsText: String(Double(previous.brakeMilliamps) / 1_000))
        }
        await fixture.holdWrite()
        model.throttleAmpsText = "2.5"
        model.brakeAmpsText = "1.25"
        let saving = Task { await model.saveLimits() }
        for _ in 0..<1_000 where !model.settingsBusy { await Task.yield() }
        XCTAssertTrue(model.settingsBusy)
        XCTAssertFalse(fixture.unlocked.withLock { $0 })
        model.throttleAmpsText = "4"
        fixture.release.signal()
        await saving.value
        XCTAssertFalse(model.limitsError)
        XCTAssertEqual(model.snapshot.limits.throttleMilliamps, 2_500)
        XCTAssertEqual(model.throttleAmpsText, "4", "An older durable receipt must not erase a newer draft")
        try await fixture.finish()
    }

}
