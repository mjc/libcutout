import CutoutMobileFFI
import XCTest

@testable import CutoutApp

@MainActor
final class ChargingEditorValidationPresentationTests: XCTestCase {
    func testSupplyCountAndConnectorCountGetDifferentFeedback() {
        XCTAssertEqual(
            message(field: .supply, reason: .tooMany),
            "charging.editor.error.supplies")
        XCTAssertEqual(
            message(field: .supply, reason: .tooMany, supplyIndex: 0),
            "charging.editor.error.connectors")
    }

    func testNoteErrorsAndUnconfirmedPowerKeepSpecificFeedback() {
        XCTAssertEqual(
            message(field: .accessNote, reason: .tooLong),
            "charging.editor.error.access_note_too_long")
        XCTAssertEqual(
            message(field: .accessNote, reason: .controlCharacter),
            "charging.editor.error.access_note_control_character")
        XCTAssertEqual(
            message(field: .supply, reason: .required, supplyIndex: 0),
            "charging.editor.error.supply_confirmation")
    }

    private func message(
        field: MobileChargingValidationFieldDto,
        reason: MobileChargingValidationReasonDto,
        supplyIndex: UInt32? = nil
    ) -> String {
        ChargingEditorValidationPresentation.messageKey(
            for: MobileChargingFieldIssueDto(field: field, reason: reason, supplyIndex: supplyIndex))
    }
}
