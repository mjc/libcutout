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

    func testRepeatedPublicationPersistsHistoryOnceAndDismissesWithoutClearingIt() async {
        var histories: [String] = []
        var dismissals = 0
        let suggestion = suggestion
        let model = ChargingStopPromptModel(
            query: { _ in .offer(prompt: suggestion) },
            acknowledge: { _ in "ride-specific-history" },
            dismiss: { dismissals += 1 },
            saveHistory: { histories.append($0) }
        )
        await model.refresh(location: nil)
        await model.refresh(location: nil)
        XCTAssertEqual(model.prompt, suggestion)
        XCTAssertEqual(histories, ["ride-specific-history"])
        model.dismiss()
        XCTAssertNil(model.prompt)
        XCTAssertEqual(dismissals, 1)
        XCTAssertEqual(histories, ["ride-specific-history"])
    }

    func testCancelledOrRetiredReadCannotConsumeAnUnseenLocation() async {
        let entered = expectation(description: "Rust read entered")
        var continuation: CheckedContinuation<MobileChargingStopDecisionDto, Never>?
        var acknowledgements = 0
        let model = ChargingStopPromptModel(
            query: { _ in
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
        read.cancel()
        continuation?.resume(returning: .offer(prompt: suggestion))
        await read.value
        XCTAssertNil(model.prompt)
        XCTAssertEqual(acknowledgements, 0)
    }

    func testEndedRideWithdrawsThePendingPrompt() async {
        var decision = MobileChargingStopDecisionDto.offer(prompt: suggestion)
        let model = ChargingStopPromptModel(
            query: { _ in decision }, acknowledge: { _ in "history" }, dismiss: {}, saveHistory: { _ in }
        )
        await model.refresh(location: nil)
        XCTAssertNotNil(model.prompt)
        decision = .rideEnded
        await model.refresh(location: nil)
        XCTAssertNil(model.prompt)
    }
}
