import CutoutMobileFFI
import XCTest

@testable import CutoutApp

final class ChargingSpotPresentationTests: XCTestCase {
    func testConflictSummaryShowsEachNameAndMakesMissingNamesExplicit() {
        XCTAssertEqual(ChargingSpotPresentation.conflictName("West entrance outlet"), "West entrance outlet")
        XCTAssertEqual(ChargingSpotPresentation.conflictName("Service entrance outlet"), "Service entrance outlet")
        XCTAssertEqual(ChargingSpotPresentation.conflictName(nil), ChargingSpotPresentation.unknownName)
        XCTAssertEqual(ChargingSpotPresentation.conflictName(" \n"), ChargingSpotPresentation.unknownName)
    }

    func testVoltageOptionsIncludeVoltagesFromAlreadyRetainedSpots() {
        let retainedSupply = MobileChargingSupplyDto(voltageVolts: 100, connectors: [.nema515], outletCount: nil)

        XCTAssertEqual(ChargingSpotPresentation.voltageOptions(in: [retainedSupply]), [100])
    }

    func testSavedSelectionPresentationDeduplicatesPageAndSeparatesOffPageFeedback() {
        XCTAssertEqual(
            ChargingSpotPresentation.savedSelectionVisibility(
                selectedID: "page-2", pageIDs: ["page-1", "page-2"], hasValidInput: true,
                hasVisibleStatus: true, filtersAreEmpty: true
            ),
            .page
        )
        XCTAssertEqual(
            ChargingSpotPresentation.savedSelectionVisibility(
                selectedID: "saved", pageIDs: ["first-page"], hasValidInput: true,
                hasVisibleStatus: true, filtersAreEmpty: true
            ),
            .separateAnnotation
        )
        XCTAssertEqual(
            ChargingSpotPresentation.savedSelectionVisibility(
                selectedID: "filtered", pageIDs: ["first-page"], hasValidInput: true,
                hasVisibleStatus: true, filtersAreEmpty: false
            ),
            .feedback
        )
        XCTAssertEqual(
            ChargingSpotPresentation.savedSelectionVisibility(
                selectedID: "invalid", pageIDs: [], hasValidInput: false,
                hasVisibleStatus: true, filtersAreEmpty: true
            ),
            .none
        )
        XCTAssertEqual(
            ChargingSpotPresentation.savedSelectionVisibility(
                selectedID: "removed", pageIDs: [], hasValidInput: true,
                hasVisibleStatus: false, filtersAreEmpty: true
            ),
            .none
        )
    }

    func testUnknownPowerDoesNotInventVoltageOrConnector() {
        let unknown = MobileChargingSupplyDto(voltageVolts: nil, connectors: [], outletCount: nil)
        XCTAssertEqual(ChargingSpotPresentation.supplyText(unknown), "Voltage unknown")
        XCTAssertEqual(ChargingSpotPresentation.connectorName(.unknown), "Unknown connector")
        XCTAssertEqual(
            ChargingSpotPresentation.connectorName(.other(label: "Workshop connector")), "Workshop connector")
    }

    func testDistinctSupplyGroupsKeepTheirOwnVoltageAndConnector() {
        XCTAssertEqual(
            ChargingSpotPresentation.supplyText(
                MobileChargingSupplyDto(voltageVolts: 120, connectors: [.nema515], outletCount: nil)),
            "120 V · US 3 Prong"
        )
        XCTAssertEqual(
            ChargingSpotPresentation.supplyText(
                MobileChargingSupplyDto(voltageVolts: 240, connectors: [.j1772], outletCount: nil)),
            "240 V · J1772"
        )
    }

    func testPricesKeepTheRecordedMinorUnitPrecision() {
        let yen = ChargingSpotPresentation.priceText(
            MobileChargingPriceDto(amountMinor: 120, currency: "JPY", minorUnitExponent: 0, basis: .perCharge)
        )
        let dinar = ChargingSpotPresentation.priceText(
            MobileChargingPriceDto(amountMinor: 1255, currency: "KWD", minorUnitExponent: 3, basis: .perHour)
        )
        XCTAssertTrue(yen.contains("120"), yen)
        XCTAssertTrue(dinar.contains("1.255"), dinar)
        XCTAssertTrue(yen.contains("per charge"), yen)
        XCTAssertTrue(dinar.contains("per hour"), dinar)
    }

    func testVoteAccessibilityValueIncludesPublishedCountAndSelection() {
        XCTAssertEqual(
            ChargingSpotPresentation.voteAccessibilityValue(
                count: 7, selected: true, locale: Locale(identifier: "en_US")),
            "7 votes, Selected"
        )
        XCTAssertEqual(
            ChargingSpotPresentation.voteAccessibilityValue(
                count: 1, selected: true, locale: Locale(identifier: "en_US")),
            "1 vote, Selected"
        )
        XCTAssertEqual(
            ChargingSpotPresentation.voteAccessibilityValue(
                count: 0, selected: false, locale: Locale(identifier: "en_US")),
            "0 votes, Not selected"
        )
    }

    func testVoteAccessibilityValueUsesLocalizedCountAndDoesNotInventLocalTotals() {
        let locale = Locale(identifier: "de_DE")
        let localizedCount = UInt64(1_234).formatted(.number.locale(locale))
        let value = ChargingSpotPresentation.voteAccessibilityValue(count: 1_234, selected: false, locale: locale)
        XCTAssertTrue(value.contains(localizedCount), value)
        XCTAssertTrue(value.contains("Not selected"), value)
        XCTAssertEqual(
            ChargingSpotPresentation.voteAccessibilityValue(count: nil, selected: true, locale: locale),
            "Selected"
        )
    }
}
