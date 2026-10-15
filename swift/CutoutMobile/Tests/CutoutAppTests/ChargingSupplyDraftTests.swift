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
        XCTAssertEqual(draft.snapshot.includedSupply, draft.snapshot.selection)
    }

    func testRestoredCustomAndUnknownSuppliesRemainConfirmedAcrossCountryAndEVChanges() {
        let custom = MobileChargingSupplyDto(
            voltageVolts: 240,
            connectors: [.other(label: "über plug"), .type2],
            outletCount: nil
        )
        let draft = ChargingSupplyDraft(supply: custom, countryCode: "US", evStation: false)

        XCTAssertTrue(draft.isConfirmed)
        XCTAssertEqual(draft.value(), custom)
        draft.setCountryCode("DE")
        draft.setVenue(isEV: true)
        XCTAssertTrue(draft.isConfirmed)
        XCTAssertEqual(draft.value(), custom)

        let unknown = MobileChargingSupplyDto(voltageVolts: nil, connectors: [.unknown], outletCount: nil)
        let unknownDraft = ChargingSupplyDraft(supply: unknown, countryCode: nil, evStation: true)
        XCTAssertEqual(unknownDraft.value(), unknown)
    }

    func testRestoredCountOnlySupplyRemainsConfirmed() {
        let supply = MobileChargingSupplyDto(voltageVolts: nil, connectors: [], outletCount: 3)
        let draft = ChargingSupplyDraft(supply: supply, countryCode: "US", evStation: false)

        XCTAssertFalse(draft.isEmpty)
        XCTAssertTrue(draft.isConfirmed)
        XCTAssertEqual(draft.value(), supply)
    }

    func testNema515AliasIsDeduplicatedForDisplayAndExplicitSelectionReplacesLegacyTypeB() {
        let observed = MobileChargingSupplyDto(
            voltageVolts: 120, connectors: [.typeB, .nema515], outletCount: nil)
        let draft = ChargingSupplyDraft(supply: observed, countryCode: "US", evStation: false)

        XCTAssertEqual(draft.connectorSummary, [.nema515])
        XCTAssertEqual(draft.value(), observed)
        XCTAssertEqual(ChargingSpotPresentation.supplyText(observed), "120 V · US 3 Prong")

        draft.set(.nema515, selected: true)

        XCTAssertEqual(
            draft.value(),
            MobileChargingSupplyDto(voltageVolts: 120, connectors: [.nema515], outletCount: nil))
        draft.set(.nema515, selected: false)
        XCTAssertEqual(draft.value()?.connectors, [])
    }

    func testIncompleteEditCannotFallBackToThePreviouslyConfirmedSupply() {
        let observed = MobileChargingSupplyDto(voltageVolts: 240, connectors: [.type2], outletCount: nil)
        let draft = ChargingSupplyDraft(supply: observed, countryCode: nil, evStation: false)

        draft.voltageText = "24x"

        XCTAssertTrue(draft.hasInvalidVoltage)
        XCTAssertEqual(draft.snapshot.selection, observed)
        XCTAssertNil(draft.snapshot.includedSupply)
        XCTAssertNil(draft.value())
    }

    func testCountryUpdatePreservesPartialNativeText() {
        let draft = ChargingSupplyDraft(countryCode: "US", evStation: false)
        draft.voltageText = "24x"

        draft.setCountryCode("GB")

        XCTAssertEqual(draft.voltageText, "24x")
        XCTAssertTrue(draft.hasInvalidVoltage)
        XCTAssertEqual(draft.snapshot.origin, .manual)
        XCTAssertNil(draft.value())
        draft.setVenue(isEV: true)
        XCTAssertEqual(draft.voltageText, "24x")
        XCTAssertEqual(draft.snapshot.origin, .manual)
    }

    func testSnapshotLoadingDoesNotTurnSuggestionIntoManualEdit() {
        let draft = ChargingSupplyDraft(countryCode: "US", evStation: false)
        var changeCount = 0
        draft.onChange = { changeCount += 1 }

        draft.setCountryCode("GB")

        XCTAssertEqual(changeCount, 1)
        XCTAssertEqual(draft.snapshot.origin, .suggested)
        XCTAssertTrue(draft.requiresConfirmation)
    }

    func testRejectedCustomEditAndSnapshotLoadingKeepNativeConnectorText() {
        let observed = MobileChargingSupplyDto(voltageVolts: 240, connectors: [.type2], outletCount: nil)
        let draft = ChargingSupplyDraft(supply: observed, countryCode: "US", evStation: false)
        draft.addOtherConnector()
        let invalidLabel = String(repeating: "é", count: 41)
        draft.otherConnectors[0].label = invalidLabel

        draft.setCountryCode("DE")

        XCTAssertEqual(draft.otherConnectors[0].label, invalidLabel)
        XCTAssertFalse(draft.isEmpty)
        XCTAssertNil(draft.value())
        XCTAssertEqual(draft.snapshot.selection, observed)
        XCTAssertNil(draft.snapshot.includedSupply)
    }

    func testRejectedRestoredSupplyKeepsRawFieldsVisibleWithoutIncludingIt() {
        let invalid = MobileChargingSupplyDto(voltageVolts: 2_001, connectors: [.unknown], outletCount: nil)
        let draft = ChargingSupplyDraft(supply: invalid, countryCode: "US", evStation: false)

        XCTAssertEqual(draft.voltageText, "2001")
        XCTAssertEqual(draft.connectors, [.unknown])
        XCTAssertNil(draft.snapshot.includedSupply)
        XCTAssertNil(draft.value())
    }

    func testClearedObservedRowStaysEmptyWhenCountryChanges() {
        let observed = MobileChargingSupplyDto(voltageVolts: 240, connectors: [.type2], outletCount: nil)
        let draft = ChargingSupplyDraft(supply: observed, countryCode: "US", evStation: false)
        draft.voltageText = ""
        draft.set(.type2, selected: false)

        XCTAssertTrue(draft.isEmpty)
        XCTAssertNil(draft.value())
        draft.setCountryCode("GB")
        XCTAssertTrue(draft.isEmpty)
        XCTAssertEqual(draft.connectorSummary, [])
        XCTAssertNil(draft.value())
    }

    func testWhitespacePastedVoltageIsParsedConsistentlyForSaveEligibility() {
        let model = ChargingSpotEditorModel(
            coordinate: MobileMapCoordinateDto(latitudeDegrees: 40, longitudeDegrees: -105)
        )
        model.confirmLocation()
        model.chooseKind(.charging)
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
