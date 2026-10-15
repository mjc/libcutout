import CutoutMobileFFI
import XCTest

@testable import CutoutApp

@MainActor
final class ChargingSupplyDraftTests: XCTestCase {
    func testCountryReplacementNeedsConfirmationAndCanBeIncludedExplicitly() {
        let draft = ChargingSupplyDraft(countryCode: "US", evStation: false)

        XCTAssertTrue(draft.requiresConfirmation)
        XCTAssertNil(draft.value())
        XCTAssertNil(draft.value())
        XCTAssertTrue(draft.requiresConfirmation)

        draft.confirm()
        XCTAssertTrue(draft.isConfirmed)
        XCTAssertNotNil(draft.value())

        draft.setCountryCode("GB")
        XCTAssertTrue(draft.requiresConfirmation)
        XCTAssertNil(draft.value())
        draft.confirm()
        XCTAssertEqual(draft.snapshot?.includedSupply, draft.snapshot?.selection)
    }

    func testRestoredCustomAndUnknownSuppliesRemainConfirmedAcrossCountryAndEVChanges() {
        let custom = MobileChargingSupplyDto(
            voltageVolts: 240,
            connectors: [.other(label: "über plug"), .type2]
        )
        let draft = ChargingSupplyDraft(supply: custom, countryCode: "US", evStation: false)

        XCTAssertTrue(draft.isConfirmed)
        XCTAssertEqual(draft.value(), custom)
        draft.setCountryCode("DE")
        draft.setVenue(isEV: true)
        XCTAssertTrue(draft.isConfirmed)
        XCTAssertEqual(draft.value(), custom)

        let unknown = MobileChargingSupplyDto(voltageVolts: nil, connectors: [.unknown])
        let unknownDraft = ChargingSupplyDraft(supply: unknown, countryCode: nil, evStation: true)
        XCTAssertEqual(unknownDraft.value(), unknown)
    }

    func testIncompleteEditCannotFallBackToThePreviouslyConfirmedSupply() {
        let observed = MobileChargingSupplyDto(voltageVolts: 240, connectors: [.type2])
        let draft = ChargingSupplyDraft(supply: observed, countryCode: nil, evStation: false)

        draft.voltageText = "24x"

        XCTAssertTrue(draft.hasInvalidVoltage)
        XCTAssertNil(draft.value())
    }

    func testCountryUpdatePreservesPartialNativeText() {
        let draft = ChargingSupplyDraft(countryCode: "US", evStation: false)
        draft.voltageText = "24x"

        draft.setCountryCode("GB")

        XCTAssertEqual(draft.voltageText, "24x")
        XCTAssertTrue(draft.hasInvalidVoltage)
        XCTAssertEqual(draft.snapshot?.origin, .manual)
        XCTAssertNil(draft.value())
        draft.setVenue(isEV: true)
        XCTAssertEqual(draft.voltageText, "24x")
        XCTAssertEqual(draft.snapshot?.origin, .manual)
    }

    func testSnapshotLoadingDoesNotTurnSuggestionIntoManualEdit() {
        let draft = ChargingSupplyDraft(countryCode: "US", evStation: false)
        var changeCount = 0
        draft.onChange = { changeCount += 1 }

        draft.setCountryCode("GB")

        XCTAssertEqual(changeCount, 1)
        XCTAssertEqual(draft.snapshot?.origin, .suggested)
        XCTAssertTrue(draft.requiresConfirmation)
    }

    func testRejectedCustomEditAndSnapshotLoadingKeepNativeConnectorText() {
        let observed = MobileChargingSupplyDto(voltageVolts: 240, connectors: [.type2])
        let draft = ChargingSupplyDraft(supply: observed, countryCode: "US", evStation: false)
        draft.addOtherConnector()
        let invalidLabel = String(repeating: "é", count: 41)
        draft.otherConnectors[0].label = invalidLabel

        draft.setCountryCode("DE")

        XCTAssertEqual(draft.otherConnectors[0].label, invalidLabel)
        XCTAssertFalse(draft.isEmpty)
        XCTAssertNil(draft.value())
        XCTAssertEqual(draft.snapshot?.selection, observed)
        XCTAssertEqual(draft.snapshot?.confirmation, .unconfirmed)
    }

    func testClearedObservedRowStaysEmptyWhenCountryChanges() {
        let observed = MobileChargingSupplyDto(voltageVolts: 240, connectors: [.type2])
        let draft = ChargingSupplyDraft(supply: observed, countryCode: "US", evStation: false)
        draft.voltageText = ""
        draft.set(.type2, selected: false)

        XCTAssertTrue(draft.isEmpty)
        XCTAssertNil(draft.value())
        draft.setCountryCode("GB")
        XCTAssertTrue(draft.isEmpty)
        XCTAssertNil(draft.value())
    }

    func testWhitespacePastedVoltageIsParsedConsistentlyForSaveEligibility() {
        let model = ChargingSpotEditorModel(
            coordinate: MobileMapCoordinateDto(latitudeDegrees: 40, longitudeDegrees: -105)
        )
        model.confirmLocation()
        model.accessNote = "Ask staff"
        model.addSupply()
        let draft = model.supplies[0]
        draft.voltageText = "\n 120 \r\n"
        draft.set(.typeB, selected: true)

        XCTAssertFalse(draft.hasInvalidVoltage)
        XCTAssertEqual(draft.value()?.voltageVolts, 120)
        XCTAssertTrue(model.canSave)
    }
}
