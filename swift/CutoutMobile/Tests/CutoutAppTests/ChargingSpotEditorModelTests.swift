import CutoutMobileFFI
import Observation
import Synchronization
import XCTest

@testable import CutoutApp

@MainActor
final class ChargingSpotEditorModelTests: XCTestCase {
    func testNewSpotHasNoSelectedKindAndCannotSaveUntilOneIsChosen() async {
        let model = ChargingSpotEditorModel(
            coordinate: coordinate(),
            save: { _, _ in
                XCTFail("A new spot requires the user's category choice")
                return nil
            })
        model.confirmLocation()
        model.accessNote = "Outlet beside the shelter"
        XCTAssertNil(model.selectedKind)
        XCTAssertFalse(model.canSave)
        let receipt = await model.save()
        XCTAssertNil(receipt)
        model.chooseKind(.charging)
        XCTAssertEqual(model.selectedKind, .charging)
        XCTAssertEqual(model.cost, .free)
        XCTAssertNil(model.venue)
        XCTAssertTrue(model.canSave)
    }

    func testFoodChoiceSetsPaidWithoutChoosingAPlaceType() async {
        let recorder = ChargingEditorSaveRecorder()
        let model = ChargingSpotEditorModel(
            coordinate: coordinate(),
            save: { fields, _ in
                recorder.record(fields)
                return Self.receipt("food")
            })
        model.confirmLocation()
        model.chooseKind(.food)
        XCTAssertEqual(model.cost, .paid)
        XCTAssertNil(model.venue)
        XCTAssertFalse(model.areOptionalDetailsExpanded)
        model.venue = .cafe
        model.accessNote = "Ask the barista"
        _ = await model.save()
        XCTAssertEqual(recorder.fields?.cost, .paid)
        XCTAssertEqual(recorder.fields?.venue, .cafe)
        XCTAssertEqual(recorder.fields?.supplies, [])
    }

    func testEVChoiceKeepsPaidAsAnIndependentBooleanAndAddsNoHouseholdSupply() async {
        for cost in [MobileChargingCostDto.free, .paid] {
            let recorder = ChargingEditorSaveRecorder()
            let model = ChargingSpotEditorModel(
                coordinate: coordinate(), countryCode: "US",
                save: { fields, _ in
                    recorder.record(fields)
                    return Self.receipt("ev")
                })
            model.confirmLocation()
            model.chooseKind(.ev)
            XCTAssertEqual(model.cost, .free)
            model.cost = cost
            model.chooseKind(.ev)
            XCTAssertEqual(model.cost, cost)
            model.accessNote = "Adapter required"
            _ = await model.save()
            XCTAssertEqual(recorder.fields?.venue, .evChargingStation)
            XCTAssertEqual(recorder.fields?.cost, cost)
            XCTAssertEqual(recorder.fields?.supplies, [])
        }
    }

    func testChangingKindClearsInapplicableFieldsButKeepsTheNoteAndPin() {
        let model = ChargingSpotEditorModel(coordinate: coordinate())
        model.confirmLocation()
        model.accessNote = "West entrance"
        model.chooseKind(.ev)
        model.cost = .paid
        model.chooseKind(.food)
        XCTAssertNil(model.venue)
        XCTAssertEqual(model.cost, .paid)
        model.venue = .restaurant
        model.priceAmount = "2"
        model.priceCurrency = "USD"
        model.priceBasis = .perHour
        model.chooseKind(.charging)
        XCTAssertNil(model.venue)
        XCTAssertEqual(model.cost, .free)
        XCTAssertEqual(model.priceAmount, "")
        XCTAssertEqual(model.accessNote, "West entrance")
        XCTAssertEqual(model.coordinate, coordinate())
    }

    func testExistingPaidEVRestoresItsChoiceWithoutResettingThePriceOrPaidState() throws {
        var input = try XCTUnwrap(Self.spot().input)
        input.venue = .evChargingStation
        input.cost = .paid
        let model = ChargingSpotEditorModel(draft: input, save: { _, _ in nil })
        XCTAssertEqual(model.selectedKind, .ev)
        XCTAssertEqual(model.cost, .paid)
        model.chooseKind(.ev)
        XCTAssertEqual(model.cost, .paid)
        XCTAssertEqual(model.accessNote, input.accessNote)
    }

    func testRetainedConflictDraftCanBeEditedWithoutLoadingTheCommunityVersion() async throws {
        let recorder = ChargingEditorSaveRecorder()
        let draft = try XCTUnwrap(Self.spot().input)
        let model = ChargingSpotEditorModel(
            draft: draft,
            save: { fields, _ in
                recorder.record(fields)
                return Self.receipt("replacement")
            })

        XCTAssertEqual(model.stage, .details)
        XCTAssertTrue(model.isEditing)
        XCTAssertEqual(model.accessNote, draft.accessNote)
        XCTAssertTrue(model.areOptionalDetailsExpanded)
        model.accessNote = "Corrected entrance instructions"
        let receipt = await model.save()
        XCTAssertEqual(receipt, Self.receipt("replacement"))
        XCTAssertEqual(recorder.fields?.accessNote, "Corrected entrance instructions")
        XCTAssertEqual(recorder.fields?.supplies, draft.supplies)
    }

    func testReviewOnlyOffersFieldEditingForCreateAndEditOperations() {
        XCTAssertTrue(MobileChargingPendingActionDto.create.editsFields)
        XCTAssertTrue(MobileChargingPendingActionDto.edit.editsFields)
        XCTAssertFalse(MobileChargingPendingActionDto.withdraw.editsFields)
        XCTAssertFalse(MobileChargingPendingActionDto.verify.editsFields)
        XCTAssertFalse(MobileChargingPendingActionDto.vote(selection: .up).editsFields)
        XCTAssertFalse(MobileChargingPendingActionDto.vote(selection: nil).editsFields)
        XCTAssertEqual(MobileChargingPendingActionDto.vote(selection: .up).messageKey, "charging.vote.up")
        XCTAssertEqual(MobileChargingPendingActionDto.vote(selection: .down).messageKey, "charging.vote.down")
        XCTAssertEqual(MobileChargingPendingActionDto.vote(selection: nil).messageKey, "charging.sync.clear_vote")
    }

    func testCreateRequiresLocationConfirmationAndRequiredFields() async {
        let model = ChargingSpotEditorModel(coordinate: coordinate(), save: { _, _ in nil })

        XCTAssertEqual(model.stage, .moving)
        XCTAssertFalse(model.canSave)
        let premature = await model.save()
        XCTAssertNil(premature)
        model.confirmLocation()
        model.chooseKind(.charging)
        model.accessNote = "   "
        XCTAssertFalse(model.canSave)
        model.accessNote = "Outlet beside the picnic shelter"
        XCTAssertTrue(model.canSave)
    }

    func testMapAndManualCoordinatesUpdateWithoutFallbackToZero() {
        let model = ChargingSpotEditorModel(coordinate: coordinate())
        model.move(to: coordinate(latitude: 5, longitude: 7))
        XCTAssertEqual(model.latitudeText, "5.0")
        XCTAssertEqual(model.longitudeText, "7.0")
        model.confirmLocation()
        model.chooseKind(.charging)
        model.accessNote = "Ask staff"
        model.cost = .free
        model.latitudeText = "not a coordinate"
        XCTAssertFalse(model.canSave)
        XCTAssertEqual(model.coordinate, coordinate(latitude: 5, longitude: 7))
        model.latitudeText = "6.25"
        model.longitudeText = "8.5"
        model.accessNote = "Ask staff"
        model.cost = .paid
        XCTAssertTrue(model.canSave)
        XCTAssertEqual(model.coordinate, coordinate(latitude: 6.25, longitude: 8.5))
    }

    func testMovePinAndReturnToDetailsPreservesEnteredDraft() {
        let model = ChargingSpotEditorModel(coordinate: coordinate())
        model.confirmLocation()
        model.chooseKind(.charging)
        model.accessNote = "Use the west entrance"
        model.cost = .paid
        model.name = "Cafe"
        model.hours = "Open daily"
        model.venue = .cafe
        model.priceAmount = "2.50"
        model.priceCurrency = "USD"
        model.priceBasis = .perHour
        model.addSupply()
        model.supplies[0].voltageText = "120"
        model.supplies[0].set(.typeB, selected: true)
        model.latitudeText = "3.5"
        model.longitudeText = "4.5"

        model.areOptionalDetailsExpanded = true
        model.movePin()
        XCTAssertEqual(model.coordinate, coordinate(latitude: 3.5, longitude: 4.5))
        model.move(to: coordinate(latitude: 6, longitude: 7))
        model.confirmLocation()

        XCTAssertEqual(model.stage, .details)
        XCTAssertEqual(model.coordinate, coordinate(latitude: 6, longitude: 7))
        XCTAssertEqual(model.accessNote, "Use the west entrance")
        XCTAssertEqual(model.name, "Cafe")
        XCTAssertEqual(model.hours, "Open daily")
        XCTAssertEqual(model.venue, .cafe)
        XCTAssertEqual(model.priceAmount, "2.50")
        XCTAssertEqual(model.priceCurrency, "USD")
        XCTAssertEqual(model.priceBasis, .perHour)
        XCTAssertEqual(model.supplies[0].value()?.connectors, [.typeB])
        XCTAssertTrue(model.canSave)
        XCTAssertTrue(model.areOptionalDetailsExpanded)
    }

    func testBasicSaveOmitsEveryOptionalFieldAndDoesNotIncludeSuggestions() async {
        let recorder = ChargingEditorSaveRecorder()
        let expected = Self.receipt("created")
        let model = ChargingSpotEditorModel(
            coordinate: coordinate(), countryCode: "US",
            save: { fields, _ in
                recorder.record(fields)
                return expected
            })
        model.confirmLocation()
        model.chooseKind(.charging)
        model.accessNote = "  Outlet beside the picnic shelter  "

        let result = await model.save()

        XCTAssertEqual(result, expected)
        let fields = recorder.fields
        XCTAssertEqual(fields?.coordinate, coordinate())
        XCTAssertEqual(fields?.accessNote, "Outlet beside the picnic shelter")
        XCTAssertEqual(fields?.cost, .free)
        XCTAssertNil(fields?.name)
        XCTAssertNil(fields?.venue)
        XCTAssertNil(fields?.hours)
        XCTAssertNil(fields?.terms)
        XCTAssertNil(fields?.price)
        XCTAssertEqual(fields?.supplies, [])
    }

    func testMinimumCreateSaveUsesFreeDefaultWithOnlyPinAndAccessNote() async {
        let recorder = ChargingEditorSaveRecorder()
        let model = ChargingSpotEditorModel(
            coordinate: coordinate(),
            save: { fields, _ in
                recorder.record(fields)
                return Self.receipt("minimum")
            })
        model.confirmLocation()
        model.chooseKind(.charging)
        model.accessNote = "Ask staff"

        let receipt = await model.save()

        XCTAssertEqual(receipt, Self.receipt("minimum"))
        XCTAssertEqual(recorder.fields?.cost, .free)
        XCTAssertNil(recorder.fields?.name)
        XCTAssertNil(recorder.fields?.venue)
        XCTAssertNil(recorder.fields?.hours)
        XCTAssertNil(recorder.fields?.price)
        XCTAssertEqual(recorder.fields?.supplies, [])
    }

    func testNormalEditConflictRequiresLatestComparisonBeforeRevisionAdvances() async {
        let latest = Self.spot(localRevision: 4, accessNote: "Newest entrance instructions")
        let recorder = ChargingEditorSaveRecorder()
        recorder.conflictNextSave = true
        let editor = ChargingSpotEditorModel(
            spot: Self.spot(),
            reloadLatest: { latest },
            save: { _, revision in
                if recorder.recordAttempt(revision) { throw MobileChargingError.Conflict }
                return Self.receipt("edited")
            })!
        editor.accessNote = "My saved draft"

        let failedSave = await editor.save()
        XCTAssertNil(failedSave)
        XCTAssertTrue(editor.hasRevisionConflict)
        XCTAssertEqual(editor.accessNote, "My saved draft")
        XCTAssertEqual(recorder.attemptedRevisions, [3])

        await editor.loadLatestForReconciliation()
        XCTAssertEqual(editor.latestRevisionCandidate?.localRevision, 4)
        XCTAssertEqual(editor.latestRevisionCandidate?.input?.accessNote, "Newest entrance instructions")
        editor.keepDraftUsingLatestRevision()
        XCTAssertFalse(editor.hasRevisionConflict)
        XCTAssertEqual(editor.accessNote, "My saved draft")

        let retriedSave = await editor.save()
        XCTAssertEqual(retriedSave, Self.receipt("edited"))
        XCTAssertEqual(recorder.attemptedRevisions, [3, 4])
    }

    func testBlockedDraftCorrectionKeepsDraftAndUsesReviewedLatestRevision() async {
        let latest = Self.spot(localRevision: 5, accessNote: "Community version")
        let recorder = ChargingEditorSaveRecorder()
        recorder.conflictNextSave = true
        let editor = ChargingSpotEditorModel(
            draft: Self.spot().input!, expectedRevision: 3,
            reloadLatest: { latest },
            save: { _, revision in
                if recorder.recordAttempt(revision) { throw MobileChargingError.Conflict }
                return Self.receipt("corrected")
            })
        editor.accessNote = "Corrected blocked draft"

        let failedSave = await editor.save()
        XCTAssertNil(failedSave)
        XCTAssertEqual(editor.accessNote, "Corrected blocked draft")
        await editor.loadLatestForReconciliation()
        XCTAssertEqual(editor.latestRevisionCandidate?.input?.accessNote, "Community version")
        editor.keepDraftUsingLatestRevision()

        let retriedSave = await editor.save()
        XCTAssertEqual(retriedSave, Self.receipt("corrected"))
        XCTAssertEqual(recorder.attemptedRevisions, [3, 5])
        XCTAssertEqual(editor.accessNote, "Corrected blocked draft")
    }

    func testSuggestionIsIncludedOnlyAfterExplicitAddAndManualOverrideSurvivesCountryUpdate() async {
        let recorder = ChargingEditorSaveRecorder()
        let model = ChargingSpotEditorModel(
            coordinate: coordinate(), countryCode: "US",
            save: { fields, _ in
                recorder.record(fields)
                return Self.receipt("suggested")
            })
        model.confirmLocation()
        model.chooseKind(.charging)
        model.accessNote = "Ask staff"
        model.cost = .paid
        XCTAssertTrue(model.supplies.isEmpty)

        model.addSuggestedSupply()
        XCTAssertEqual(model.supplies.count, 1)
        XCTAssertFalse(model.supplies[0].requiresConfirmation)
        XCTAssertTrue(model.canSave)
        model.setCountryCode("GB")
        XCTAssertTrue(model.supplies[0].requiresConfirmation)
        XCTAssertNil(model.supplies[0].value())
        XCTAssertFalse(model.canSave)
        model.supplies[0].confirm()
        XCTAssertEqual(model.supplies[0].value(), mobileChargingPowerSuggestion(countryCode: "GB", evStation: false))

        model.supplies[0].voltageText = "240"
        model.setCountryCode("DE")
        XCTAssertEqual(model.supplies[0].value()?.voltageVolts, 240)

        _ = await model.save()
        XCTAssertEqual(recorder.fields?.supplies.first?.voltageVolts, 240)
    }

    func testRustValidationRejectsLongAndControlCharacterAccessNotesWithTypedFeedback() {
        let model = ChargingSpotEditorModel(coordinate: coordinate())
        model.confirmLocation()
        model.chooseKind(.charging)
        model.accessNote = String(repeating: "a", count: 2_001)
        XCTAssertFalse(model.canSave)
        XCTAssertEqual(model.validationIssues.first?.field, .accessNote)
        XCTAssertEqual(model.validationIssues.first?.reason, .tooLong)

        model.accessNote = "Ask\u{0}staff"
        XCTAssertEqual(model.validationIssues.first?.field, .accessNote)
        XCTAssertEqual(model.validationIssues.first?.reason, .controlCharacter)
    }

    func testIncompletePaidPriceAndSupplyRowsStayVisibleAsTypedIssues() {
        let model = ChargingSpotEditorModel(coordinate: coordinate())
        model.confirmLocation()
        model.chooseKind(.charging)
        model.accessNote = "Ask staff"
        model.cost = .paid
        model.priceAmount = "2."
        XCTAssertTrue(model.validationIssues.contains { $0.field == .payment && $0.reason == .incomplete })

        model.addSupply()
        model.supplies[0].voltageText = "2001"
        XCTAssertTrue(
            model.validationIssues.contains { $0.field == .supply && $0.reason == .incomplete && $0.supplyIndex == 0 })
        XCTAssertFalse(model.canSave)
    }

    func testSupplyDraftMutationDeliversObservationForSaveEligibility() {
        let model = ChargingSpotEditorModel(coordinate: coordinate())
        model.confirmLocation()
        model.chooseKind(.charging)
        model.accessNote = "Ask staff"
        model.addSuggestedSupply()
        XCTAssertTrue(model.canSave)
        let changed = Mutex(false)
        withObservationTracking {
            _ = model.canSave
        } onChange: {
            changed.withLock { $0 = true }
        }

        model.setCountryCode("GB")

        XCTAssertTrue(changed.withLock { $0 })
        XCTAssertFalse(model.canSave)
    }

    func testEVClearsSuggestionButPreservesExplicitSupply() {
        let model = ChargingSpotEditorModel(coordinate: coordinate(), countryCode: "US")
        model.addSuggestedSupply()
        model.addSupply()
        model.supplies[1].voltageText = "400"
        model.supplies[1].set(.ccs2, selected: true)

        model.setVenue(.evChargingStation)

        XCTAssertEqual(model.supplies.count, 1)
        XCTAssertEqual(model.supplies[0].value()?.voltageVolts, 400)
        XCTAssertEqual(model.supplies[0].value()?.connectors, [.ccs2])
        XCTAssertEqual(model.powerSuggestion.voltageVolts, nil)
        XCTAssertEqual(model.powerSuggestion.connectors, [.unknown])

        model.setCountryCode("DE")
        XCTAssertEqual(model.supplies[0].value()?.voltageVolts, 400)
        XCTAssertEqual(model.supplies[0].value()?.connectors, [.ccs2])
    }

    func testVenueChangePreservesIncompleteEditsToSuggestedSupply() {
        let model = ChargingSpotEditorModel(coordinate: coordinate(), countryCode: "US")
        model.addSuggestedSupply()
        model.supplies[0].voltageText = "12x"

        model.setVenue(.evChargingStation)

        XCTAssertEqual(model.supplies.count, 1)
        XCTAssertEqual(model.supplies[0].voltageText, "12x")
        XCTAssertFalse(model.canSave)
    }

    func testRemovingCustomConnectorDoesNotRestoreItAndUnknownRetainsDraftText() {
        let draft = ChargingSupplyDraft(
            supply: MobileChargingSupplyDto(
                voltageVolts: 120, connectors: [.other(label: "Workshop plug"), .typeB]
            ), countryCode: nil, evStation: false)
        guard let customID = draft.otherConnectors.first?.id else {
            XCTFail("Expected the original custom connector")
            return
        }
        draft.removeOtherConnector(id: customID)
        XCTAssertEqual(draft.value()?.connectors, [.typeB])

        let customDraft = ChargingSupplyDraft(
            supply: MobileChargingSupplyDto(
                voltageVolts: 120, connectors: [.other(label: "Workshop plug")]
            ), countryCode: nil, evStation: false)
        customDraft.set(.unknown, selected: true)
        XCTAssertEqual(customDraft.value()?.connectors, [.unknown])
        XCTAssertEqual(customDraft.otherConnectors.first?.label, "Workshop plug")
        customDraft.set(.unknown, selected: false)
        XCTAssertEqual(customDraft.value()?.connectors, [.other(label: "Workshop plug")])
    }

    func testPlugSummaryIncludesCustomConnectorAndUnknownTakesPrecedence() {
        let draft = ChargingSupplyDraft()
        draft.addOtherConnector()
        draft.otherConnectors[0].label = "Workshop plug"
        XCTAssertEqual(draft.connectorSummary, [.other(label: "Workshop plug")])

        draft.set(.typeB, selected: true)
        XCTAssertEqual(draft.connectorSummary, [.typeB, .other(label: "Workshop plug")])

        draft.set(.unknown, selected: true)
        XCTAssertEqual(draft.connectorSummary, [.unknown])
    }

    func testConnectorOrderingAndDefaultsUseGPSCountryRatherThanLanguage() {
        let model = ChargingSpotEditorModel(
            coordinate: coordinate(), countryCode: "GB", inputLocale: Locale(identifier: "de_DE"))
        XCTAssertEqual(model.connectorChoices.first, .typeG)
        XCTAssertEqual(model.powerSuggestion.connectors, [.typeG])

        model.setCountryCode("US")
        XCTAssertEqual(model.countryCode, "US")
        XCTAssertEqual(model.powerSuggestion.voltageVolts, 120)
        XCTAssertEqual(model.connectorChoices, mobileChargingConnectorChoices(locationCountryCode: "US"))

        model.setCountryCode("gb")
        XCTAssertEqual(model.countryCode, "GB")
        XCTAssertEqual(model.connectorChoices.first, .typeG)
        XCTAssertEqual(model.powerSuggestion.connectors, [.typeG])
    }

    func testConnectorReorderingDoesNotSelectOrReplaceExistingSupplyChoices() {
        let model = ChargingSpotEditorModel(coordinate: coordinate())
        model.setCountryCode("US")
        XCTAssertTrue(model.supplies.isEmpty)
        model.addSupply()
        model.supplies[0].voltageText = "240"
        model.supplies[0].set(.typeI, selected: true)
        let original = model.supplies[0].value()

        model.setCountryCode("GB")

        XCTAssertEqual(model.connectorChoices.first, .typeG)
        XCTAssertEqual(model.supplies[0].value(), original)
        XCTAssertTrue(model.connectorChoices.contains(.typeI))
        XCTAssertEqual(Set(model.connectorChoices).count, model.connectorChoices.count)
        XCTAssertEqual(
            Set(model.connectorChoices), Set(ChargingSpotPresentation.connectorOptions.map(\.value)))
    }

    func testUnavailableCountryUsesFallbackWithoutAddingAnExtraRequiredField() {
        let model = ChargingSpotEditorModel(coordinate: coordinate(), countryCode: "DE")
        model.confirmLocation()
        model.chooseKind(.charging)
        model.accessNote = "Outlet beside the door"
        model.setCountryCode(nil)
        XCTAssertNil(model.countryCode)
        XCTAssertEqual(model.connectorChoices, mobileChargingConnectorChoices(locationCountryCode: nil))
        XCTAssertTrue(model.canSave)
        model.setCountryCode("not-a-country")
        XCTAssertNil(model.countryCode)
        XCTAssertTrue(model.canSave)
    }

    func testBlankOptionalSupplyRowsAreOmittedOnSave() async {
        let recorder = ChargingEditorSaveRecorder()
        let model = ChargingSpotEditorModel(
            coordinate: coordinate(),
            save: { fields, _ in
                recorder.record(fields)
                return Self.receipt("without-empty-supply")
            })
        model.confirmLocation()
        model.chooseKind(.charging)
        model.accessNote = "Ask staff"
        model.cost = .free
        model.addSupply()
        model.supplies[0].addOtherConnector()
        model.supplies[0].otherConnectors[0].label = "   "

        XCTAssertTrue(model.canSave)
        _ = await model.save()
        XCTAssertEqual(recorder.fields?.supplies, [])
    }

    func testChangingPaidSpotToFreeClearsPaidOnlyDraftMetadata() async {
        let recorder = ChargingEditorSaveRecorder()
        let model = ChargingSpotEditorModel(
            coordinate: coordinate(),
            save: { fields, _ in
                recorder.record(fields)
                return Self.receipt("free")
            })
        model.confirmLocation()
        model.chooseKind(.charging)
        model.accessNote = "Ask staff"
        model.cost = .paid
        model.terms = .both
        model.priceAmount = "1.50"
        model.priceCurrency = "USD"
        model.priceBasis = .perHour
        model.cost = .free

        _ = await model.save()

        XCTAssertEqual(recorder.fields?.cost, .free)
        XCTAssertNil(recorder.fields?.terms)
        XCTAssertNil(recorder.fields?.price)
        XCTAssertEqual(model.priceAmount, "")
        XCTAssertEqual(model.priceCurrency, "")
        XCTAssertNil(model.priceBasis)
    }

    func testExactPriceUsesCurrencyMinorUnitsWithoutFloatingPointRounding() {
        XCTAssertEqual(
            ChargingPriceDraft.makePrice(
                amount: "12.34", currency: "USD", basis: .perHour, locale: Locale(identifier: "en_US")
            ),
            MobileChargingPriceDto(amountMinor: 1_234, currency: "USD", minorUnitExponent: 2, basis: .perHour)
        )
        XCTAssertNil(
            ChargingPriceDraft.makePrice(
                amount: "12.345", currency: "USD", basis: .perHour, locale: Locale(identifier: "en_US")
            ))
        XCTAssertNil(
            ChargingPriceDraft.makePrice(
                amount: "12.34", currency: "", basis: .perHour, locale: Locale(identifier: "en_US")
            ))
    }

    func testPriceDraftUsesLocaleDecimalSeparatorAndRejectsGroupingOrJunk() {
        let expected = MobileChargingPriceDto(
            amountMinor: 1_234, currency: "EUR", minorUnitExponent: 2, basis: .perHour
        )
        XCTAssertEqual(
            ChargingPriceDraft.makePrice(
                amount: "12.34", currency: "EUR", basis: .perHour, locale: Locale(identifier: "en_US")
            ), expected
        )
        XCTAssertEqual(
            ChargingPriceDraft.makePrice(
                amount: "12,34", currency: "EUR", basis: .perHour, locale: Locale(identifier: "de_DE")
            ), expected
        )
        XCTAssertEqual(
            ChargingPriceDraft.makePrice(
                amount: "12,34", currency: "EUR", basis: .perHour, locale: Locale(identifier: "fr_FR")
            ), expected
        )
        XCTAssertEqual(
            ChargingPriceDraft.makePrice(
                amount: "١٢٫٣٤", currency: "EUR", basis: .perHour, locale: Locale(identifier: "ar_EG")
            ), expected
        )
        XCTAssertNil(
            ChargingPriceDraft.makePrice(
                amount: "1,234.56", currency: "EUR", basis: .perHour, locale: Locale(identifier: "en_US")
            ))
        XCTAssertNil(
            ChargingPriceDraft.makePrice(
                amount: "1.234,56", currency: "EUR", basis: .perHour, locale: Locale(identifier: "de_DE")
            ))
        XCTAssertNil(
            ChargingPriceDraft.makePrice(
                amount: "12,3x", currency: "EUR", basis: .perHour, locale: Locale(identifier: "de_DE")
            ))
        XCTAssertNil(
            ChargingPriceDraft.makePrice(
                amount: "12.34", currency: "EUR", basis: .perHour, locale: Locale(identifier: "de_DE")
            ))
        XCTAssertEqual(
            ChargingPriceDraft.makePrice(
                amount: "12", currency: "EUR", basis: .perHour, locale: Locale(identifier: "en_US")
            )?.amountMinor,
            1_200
        )
        XCTAssertEqual(
            ChargingPriceDraft.makePrice(
                amount: "12", currency: "EUR", basis: .perHour, locale: Locale(identifier: "de_DE")
            )?.amountMinor,
            1_200
        )
    }

    func testPriceDraftEnforcesCurrencyPrecisionAndMinorUnitOverflow() {
        for locale in [Locale(identifier: "en_US"), Locale(identifier: "de_DE")] {
            XCTAssertEqual(
                ChargingPriceDraft.makePrice(amount: "12", currency: "JPY", basis: .perHour, locale: locale)?
                    .amountMinor,
                12
            )
        }
        XCTAssertNil(
            ChargingPriceDraft.makePrice(
                amount: "12.1", currency: "JPY", basis: .perHour, locale: Locale(identifier: "en_US")
            ))
        XCTAssertEqual(
            ChargingPriceDraft.makePrice(
                amount: "1.234", currency: "KWD", basis: .perHour, locale: Locale(identifier: "en_US")
            )?.amountMinor,
            1_234
        )
        XCTAssertEqual(
            ChargingPriceDraft.makePrice(
                amount: "1.2300", currency: "USD", basis: .perHour, locale: Locale(identifier: "en_US")
            )?.amountMinor,
            123
        )
        XCTAssertNil(
            ChargingPriceDraft.makePrice(
                amount: "1.2301", currency: "USD", basis: .perHour, locale: Locale(identifier: "en_US")
            ))
        XCTAssertEqual(
            ChargingPriceDraft.makePrice(
                amount: "90000000000000.00", currency: "USD", basis: .perHour,
                locale: Locale(identifier: "en_US")
            )?.amountMinor,
            9_000_000_000_000_000
        )
        XCTAssertNil(
            ChargingPriceDraft.makePrice(
                amount: "90000000000000.01", currency: "USD", basis: .perHour,
                locale: Locale(identifier: "en_US")
            ))
    }

    func testLocalizedEditPriceRoundTripsWithItsInputLocale() async {
        let recorder = ChargingEditorSaveRecorder()
        let model = ChargingSpotEditorModel(
            draft: Self.spot().input!, inputLocale: Locale(identifier: "de_DE"),
            save: { fields, _ in
                recorder.record(fields)
                return Self.receipt("localized-price")
            }
        )
        XCTAssertEqual(model.priceAmount, "1,25")
        model.priceAmount = "1,50"
        _ = await model.save()
        XCTAssertEqual(recorder.fields?.price?.amountMinor, 150)
    }

    func testEditPreloadsAndSavesAllOptionalMetadataAndMultipleSupplies() async {
        let original = Self.spot()
        let recorder = ChargingEditorSaveRecorder()
        let editor = ChargingSpotEditorModel(
            spot: original, countryCode: "GB",
            save: { fields, _ in
                recorder.record(fields)
                return Self.receipt("edited")
            })
        XCTAssertNotNil(editor)
        guard let editor else { return }
        XCTAssertEqual(editor.stage, .details)
        XCTAssertTrue(editor.isEditing)
        XCTAssertEqual(editor.name, "Cafe")
        XCTAssertEqual(editor.venue, .cafe)
        XCTAssertEqual(editor.hours, "Open daily")
        XCTAssertEqual(editor.terms, .both)
        XCTAssertEqual(editor.priceAmount, "1.25")
        XCTAssertEqual(editor.priceCurrency, "USD")
        XCTAssertEqual(editor.supplies.count, 2)

        let result = await editor.save()

        XCTAssertEqual(result, Self.receipt("edited"))
        let fields = recorder.fields
        XCTAssertEqual(fields, original.input)
    }

    func testEditingPreservesExistingPriceExponentAndConnectorOrder() async {
        let original = Self.spot(priceExponent: 4)
        let recorder = ChargingEditorSaveRecorder()
        let editor = ChargingSpotEditorModel(
            spot: original,
            save: { fields, _ in
                recorder.record(fields)
                return Self.receipt("edited-exactly")
            })
        XCTAssertNotNil(editor)
        _ = await editor?.save()

        let fields = recorder.fields
        XCTAssertEqual(fields, original.input)
    }

    func testWriteIsGatedWhileInFlightAndAfterDurableReceipt() async {
        let gate = ChargingEditorSaveGate()
        let recorder = ChargingEditorSaveRecorder()
        let model = ChargingSpotEditorModel(
            coordinate: coordinate(),
            save: { fields, _ in
                recorder.record(fields)
                await gate.wait()
                return Self.receipt("created")
            })
        model.confirmLocation()
        model.chooseKind(.charging)
        model.accessNote = "Ask staff"
        model.cost = .paid

        let first = Task { await model.save() }
        await gate.waitUntilStarted()
        XCTAssertEqual(model.stage, .saving)
        let overlapping = await model.save()
        XCTAssertNil(overlapping)
        await gate.release()
        let firstReceipt = await first.value
        XCTAssertEqual(firstReceipt, Self.receipt("created"))
        let repeated = await model.save()
        XCTAssertNil(repeated)
        XCTAssertEqual(model.stage, .saving)
        XCTAssertEqual(recorder.count, 1)
    }

    func testFieldValidationFailureRetainsDraftAndReturnsToDetails() async {
        let model = ChargingSpotEditorModel(
            coordinate: coordinate(),
            save: { _, _ in
                throw MobileChargingError.AccessNoteTooLong
            })
        model.confirmLocation()
        model.chooseKind(.charging)
        model.accessNote = "Use the east gate"
        model.cost = .paid

        let result = await model.save()

        XCTAssertNil(result)
        XCTAssertEqual(model.stage, .details)
        XCTAssertEqual(model.accessNote, "Use the east gate")
        XCTAssertEqual(model.cost, .paid)
        XCTAssertEqual(model.errorKey, "charging.editor.error.access_note_too_long")
    }

    private func coordinate(latitude: Double = 1, longitude: Double = 2) -> MobileMapCoordinateDto {
        MobileMapCoordinateDto(latitudeDegrees: latitude, longitudeDegrees: longitude)
    }

    private nonisolated static func receipt(_ id: String) -> MobileChargingWriteReceiptDto {
        MobileChargingWriteReceiptDto(id: id, localRevision: 1, operationId: "operation-\(id)")
    }

    private nonisolated static func spot(
        priceExponent: UInt8 = 2, localRevision: UInt64 = 3,
        accessNote: String = "Ask the barista"
    ) -> MobileChargingSpotDto {
        let input = MobileChargingInputDto(
            coordinate: MobileMapCoordinateDto(latitudeDegrees: 1, longitudeDegrees: 2),
            accessNote: accessNote,
            cost: .paid,
            name: "Cafe",
            venue: .cafe,
            hours: "Open daily",
            terms: .both,
            price: MobileChargingPriceDto(
                amountMinor: priceExponent == 4 ? 12_500 : 125,
                currency: "USD", minorUnitExponent: priceExponent, basis: .perHour
            ),
            supplies: [
                MobileChargingSupplyDto(voltageVolts: 120, connectors: [.other(label: "Workshop plug"), .typeB]),
                MobileChargingSupplyDto(voltageVolts: 240, connectors: [.j1772]),
            ]
        )
        return MobileChargingSpotDto(
            id: "spot-id", input: input, owned: true, status: .queued,
            localRevision: localRevision, remoteId: nil, remoteRevision: 0, changeSequence: localRevision,
            verifiedAtMs: nil, vote: nil, voteRevision: 0, votesUp: 0, votesDown: 0,
            pendingOperations: 1, pendingVote: false
        )
    }
}

@MainActor
private final class ChargingEditorSaveRecorder {
    private(set) var fields: MobileChargingInputDto?
    private(set) var count = 0
    private(set) var attemptedRevisions = [UInt64?]()
    var conflictNextSave = false
    func record(_ fields: MobileChargingInputDto) {
        self.fields = fields
        count += 1
    }
    func recordAttempt(_ revision: UInt64?) -> Bool {
        attemptedRevisions.append(revision)
        defer { conflictNextSave = false }
        return conflictNextSave
    }
}

private actor ChargingEditorSaveGate {
    private var continuation: CheckedContinuation<Void, Never>?
    private var startedContinuation: CheckedContinuation<Void, Never>?
    private var started = false
    func wait() async {
        started = true
        startedContinuation?.resume()
        startedContinuation = nil
        await withCheckedContinuation { continuation = $0 }
    }
    func waitUntilStarted() async {
        if started { return }
        await withCheckedContinuation { startedContinuation = $0 }
    }
    func release() {
        continuation?.resume()
        continuation = nil
    }
}
