import CutoutMobileFFI
import Foundation
import XCTest

@testable import CutoutApp

@MainActor
final class CaptureResearchDraftTests: XCTestCase {
    func testManualProfileKeepsBlankZeroOffAndBooleanDistinct() throws {
        let draft = CaptureResearchDraft()
        XCTAssertThrowsError(try draft.observation())
        draft.selectProfile("nosfet_aero")
        draft.values["display_brightness"] = "0"
        draft.states["pwm_tiltback"] = "off"
        draft.states["high_beam"] = "false"
        let snapshot = try draft.observation()
        XCTAssertEqual(snapshot.profile.selection, .manual)
        XCTAssertNil(snapshot.observedAtMs)
        XCTAssertFalse(snapshot.values.contains(where: { $0.settingId == "voltage_correction" }))
        XCTAssertEqual(
            snapshot.values.first(where: { $0.settingId == "display_brightness" })?.value, .entered(value: "0"))
        XCTAssertEqual(snapshot.values.first(where: { $0.settingId == "pwm_tiltback" })?.value, .off)
        XCTAssertEqual(snapshot.values.first(where: { $0.settingId == "high_beam" })?.value, .boolean(value: false))
    }

    func testUnusualValueAndOriginalUnitReachRustWithoutClamping() throws {
        let draft = CaptureResearchDraft()
        draft.selectProfile("nosfet_aero")
        draft.values["display_brightness"] = "145.25"
        draft.values["tiltback_speed"] = "55"
        draft.units["tiltback_speed"] = "mph"
        draft.useEarlierTime = true
        draft.earlierTime = "12.5"
        draft.source = .wheelDisplay
        let snapshot = try draft.observation()
        XCTAssertEqual(snapshot.observedAtMs, 12500)
        XCTAssertEqual(snapshot.source, .wheelDisplay)
        XCTAssertEqual(
            snapshot.values.first(where: { $0.settingId == "display_brightness" })?.value, .entered(value: "145.25"))
        XCTAssertEqual(snapshot.values.first(where: { $0.settingId == "tiltback_speed" })?.unit, "mph")
        draft.units["tiltback_speed"] = ""
        XCTAssertNil(try draft.observation().values.first(where: { $0.settingId == "tiltback_speed" })?.unit)
    }

    func testProfileChangeDoesNotApplyAeroValuesToFalcon() throws {
        let draft = CaptureResearchDraft()
        draft.selectProfile("nosfet_aero")
        draft.values["display_units"] = "1"
        draft.selectProfile("begode_falcon")
        XCTAssertTrue(draft.values.isEmpty)
        XCTAssertTrue(try draft.observation().values.isEmpty)
        XCTAssertFalse(try XCTUnwrap(draft.catalog).fields.contains(where: { $0.id == "display_units" }))
    }

    func testEarlierObservationRejectsInvalidNativeText() throws {
        let draft = CaptureResearchDraft()
        draft.selectProfile("nosfet_aero")
        draft.useEarlierTime = true
        for invalid in ["", "later", "-1", "inf", "1e50"] {
            draft.earlierTime = invalid
            XCTAssertThrowsError(try draft.observation(), invalid)
        }
    }

    func testLateSavedSnapshotDoesNotReplaceANewerCapture() async throws {
        let gate = CaptureResearchSaveGate()
        let saved = MobileCaptureSettingsObservationDto(
            captureId: UUID().uuidString,
            profile: MobileCaptureResearchProfileDto(
                profileId: "nosfet_aero", pevCategory: "euc", model: "NOSFET Aero", firmware: nil, selection: .manual),
            observedAtMs: 0, recordedAtMs: 0, source: .manufacturerApp,
            values: [
                MobileCaptureSettingObservationValueDto(
                    settingId: "display_brightness", value: .entered(value: "30"), unit: "%", note: nil)
            ], note: nil
        )
        let model = CaptureFeatureModel(
            sessionState: CutoutSessionStateHandle(),
            saveSettingsObservation: { _, _ in
                await gate.suspend()
                return saved
            })
        model.deliverCaptureEvent(.started(fileURL: URL(fileURLWithPath: "/tmp/research-old.jsonl")), origin: .manual)
        model.researchDraft.selectProfile("nosfet_aero")
        model.researchDraft.values["display_brightness"] = "30"
        let draft = try model.researchDraft.observation()
        let task = Task { try await model.recordSettingsObservation(draft) }
        await gate.waitUntilPending()
        model.deliverCaptureEvent(.finished(fileURL: URL(fileURLWithPath: "/tmp/research-old.jsonl")))
        model.deliverCaptureEvent(.started(fileURL: URL(fileURLWithPath: "/tmp/research-new.jsonl")), origin: .manual)
        await gate.release()
        try await task.value
        XCTAssertTrue(model.settingsObservations.isEmpty)
        XCTAssertTrue(model.researchDraft.values.isEmpty)
    }

    func testCaptureActionsExposeStationaryAndSettingsWithoutBms() {
        XCTAssertTrue(CaptureQuickLabel.activity.contains(.poweredOnStationary))
        XCTAssertTrue(CaptureQuickLabel.activity.contains(.rollingForward))
        XCTAssertTrue(CaptureQuickLabel.actions.contains(.displayUnits))
        XCTAssertTrue(CaptureQuickLabel.actions.contains(.powerCycle))
        XCTAssertFalse(CaptureQuickLabel.allCases.contains(where: { $0.domainLabel == .bmsScreen }))
    }
}

private actor CaptureResearchSaveGate {
    private var pending: CheckedContinuation<Void, Never>?
    private var observer: CheckedContinuation<Void, Never>?
    func suspend() async {
        await withCheckedContinuation { continuation in
            pending = continuation
            observer?.resume()
            observer = nil
        }
    }
    func waitUntilPending() async {
        guard pending == nil else { return }
        await withCheckedContinuation { observer = $0 }
    }
    func release() {
        pending?.resume()
        pending = nil
    }
}
