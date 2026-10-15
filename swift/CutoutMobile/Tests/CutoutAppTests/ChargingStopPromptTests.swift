import CutoutMobileFFI
import XCTest

@testable import CutoutApp

@MainActor
final class ChargingStopPromptTests: XCTestCase {
    private let suggestion = MobileChargingStopPromptDto(
        rideId: "00000000-0000-0000-0000-000000000001",
        coordinate: MobileMapCoordinateDto(latitudeDegrees: 40, longitudeDegrees: -105)
    )

    func testAcceptanceOpensTheExistingUnsavedEditorAtTheDetectedLocation() {
        let presentation = RideMapPresentationState()
        XCTAssertTrue(presentation.beginChargingStop(suggestion, locale: Locale(identifier: "en_US")))
        XCTAssertEqual(presentation.mode, .charging)
        XCTAssertEqual(presentation.chargingEditor?.stage, .details)
        XCTAssertEqual(presentation.chargingEditor?.coordinate, suggestion.coordinate)
        XCTAssertEqual(presentation.chargingEditor?.accessNote, "")
        XCTAssertFalse(presentation.chargingEditor?.canSave ?? true)
    }

    func testAcceptanceDoesNotReplaceAnExistingDraft() {
        let presentation = RideMapPresentationState()
        let existing = ChargingSpotEditorModel(coordinate: suggestion.coordinate)
        existing.accessNote = "Keep these entrance instructions"
        presentation.chargingEditor = existing
        XCTAssertFalse(presentation.beginChargingStop(suggestion, locale: .current))
        XCTAssertTrue(presentation.chargingEditor === existing)
        XCTAssertEqual(existing.accessNote, "Keep these entrance instructions")
    }

    func testHistoryIsPersistedOnlyAfterVisiblePromptAcknowledgesOnce() async throws {
        var histories: [String] = []
        var dismissals = 0
        let suggestion = suggestion
        let model = ChargingStopPromptModel(
            query: { _, _ in .offer(prompt: suggestion) },
            acknowledge: { _ in "ride-specific-history" },
            dismiss: { dismissals += 1 },
            saveHistory: { histories.append($0) }
        )
        await model.refresh(location: nil)
        await model.refresh(location: nil)
        XCTAssertEqual(model.prompt, suggestion)
        XCTAssertTrue(histories.isEmpty, "A candidate must not be acknowledged before native presentation")

        let generation = try XCTUnwrap(model.promptGeneration)
        XCTAssertTrue(model.didPresent(suggestion, generation: generation))
        XCTAssertFalse(model.didPresent(suggestion, generation: generation))
        XCTAssertEqual(histories, ["ride-specific-history"])
        model.dismiss()
        XCTAssertNil(model.prompt)
        XCTAssertEqual(dismissals, 1)
        XCTAssertEqual(histories, ["ride-specific-history"])
    }

    func testHiddenCandidateIsNotPublishedOrAcknowledged() async {
        let suggestion = suggestion
        var queriedAvailability: [Bool] = []
        var acknowledgements = 0
        let model = ChargingStopPromptModel(
            query: { _, canPresent in
                queriedAvailability.append(canPresent)
                return .offer(prompt: suggestion)
            },
            acknowledge: { _ in
                acknowledgements += 1
                return "hidden"
            },
            dismiss: {}, saveHistory: { _ in XCTFail("A hidden candidate must not be persisted") }
        )

        await model.refresh(location: nil, canPresent: false)

        XCTAssertNil(model.prompt)
        XCTAssertEqual(queriedAvailability, [false])
        XCTAssertEqual(acknowledgements, 0)
    }

    func testFailedNativePresentationRetiresCandidateWithoutPersistingHistory() async throws {
        let suggestion = suggestion
        var acknowledgements = 0
        var dismissals = 0
        var histories: [String] = []
        let model = ChargingStopPromptModel(
            query: { _, _ in .offer(prompt: suggestion) },
            acknowledge: { _ in
                acknowledgements += 1
                return "must-not-save"
            },
            dismiss: { dismissals += 1 },
            saveHistory: { histories.append($0) }
        )

        await model.refresh(location: nil)
        let generation = try XCTUnwrap(model.promptGeneration)
        XCTAssertFalse(model.didPresent(suggestion, generation: generation, canPresent: false))

        XCTAssertNil(model.prompt)
        XCTAssertNil(model.promptGeneration)
        XCTAssertEqual(acknowledgements, 0)
        XCTAssertEqual(dismissals, 1)
        XCTAssertTrue(histories.isEmpty)
    }

    func testLateAppearanceReceiptForRetiredCandidateCannotAcknowledge() async throws {
        let suggestion = suggestion
        var acknowledgements = 0
        var histories: [String] = []
        let model = ChargingStopPromptModel(
            query: { _, _ in .offer(prompt: suggestion) },
            acknowledge: { _ in
                acknowledgements += 1
                return "retired"
            },
            dismiss: {}, saveHistory: { histories.append($0) }
        )

        await model.refresh(location: nil)
        let retiredGeneration = try XCTUnwrap(model.promptGeneration)
        await model.refresh(location: nil, canPresent: false)
        model.didPresent(suggestion, generation: retiredGeneration)

        XCTAssertNil(model.prompt)
        XCTAssertEqual(acknowledgements, 0)
        XCTAssertTrue(histories.isEmpty)
    }

    func testLateAppearanceReceiptCannotAcknowledgeAReplacementWithTheSameCandidate() async throws {
        let suggestion = suggestion
        var acknowledgements = 0
        var histories: [String] = []
        let model = ChargingStopPromptModel(
            query: { _, canPresent in canPresent ? .offer(prompt: suggestion) : .noAssociatedRide },
            acknowledge: { _ in
                acknowledgements += 1
                return "current"
            },
            dismiss: {}, saveHistory: { histories.append($0) }
        )

        await model.refresh(location: nil)
        let retiredGeneration = try XCTUnwrap(model.promptGeneration)
        XCTAssertFalse(model.didPresent(suggestion, generation: retiredGeneration, canPresent: false))
        await model.refresh(location: nil)
        let currentGeneration = try XCTUnwrap(model.promptGeneration)

        XCTAssertNotEqual(currentGeneration, retiredGeneration)
        XCTAssertFalse(model.didPresent(suggestion, generation: retiredGeneration, canPresent: false))
        XCTAssertEqual(model.prompt, suggestion, "A stale failure receipt must not retire the replacement")
        XCTAssertTrue(histories.isEmpty)
        XCTAssertTrue(model.didPresent(suggestion, generation: currentGeneration))
        XCTAssertEqual(acknowledgements, 1)
        XCTAssertEqual(histories, ["current"])
    }

    func testCancelledReadCannotConsumeAnUnseenLocation() async {
        await assertLateReadIsIgnored(cancel: true)
    }

    func testDismissedReadCannotResurrectThePromptWithoutTaskCancellation() async {
        await assertLateReadIsIgnored(cancel: false)
    }

    private func assertLateReadIsIgnored(cancel: Bool) async {
        let entered = expectation(description: "Rust read entered")
        var continuation: CheckedContinuation<MobileChargingStopDecisionDto, Never>?
        var acknowledgements = 0
        let model = ChargingStopPromptModel(
            query: { _, _ in
                await withCheckedContinuation {
                    continuation = $0
                    entered.fulfill()
                }
            },
            acknowledge: { _ in
                acknowledgements += 1
                return "unseen"
            },
            dismiss: {}, saveHistory: { _ in XCTFail("Unseen locations must not be persisted") }
        )
        let read = Task { await model.refresh(location: nil) }
        await fulfillment(of: [entered], timeout: 2)
        model.dismiss()
        if cancel { read.cancel() }
        continuation?.resume(returning: .offer(prompt: suggestion))
        await read.value
        XCTAssertNil(model.prompt)
        XCTAssertEqual(acknowledgements, 0)
    }

    func testEndedRideWithdrawsThePendingPrompt() async {
        let source = DecisionSource(.offer(prompt: suggestion))
        let model = ChargingStopPromptModel(
            query: { _, _ in source.decision }, acknowledge: { _ in "history" }, dismiss: {}, saveHistory: { _ in }
        )
        await model.refresh(location: nil)
        XCTAssertNotNil(model.prompt)
        source.decision = .rideEnded
        await model.refresh(location: nil)
        XCTAssertNil(model.prompt)
    }
}

@MainActor
private final class DecisionSource {
    var decision: MobileChargingStopDecisionDto

    init(_ decision: MobileChargingStopDecisionDto) {
        self.decision = decision
    }
}
