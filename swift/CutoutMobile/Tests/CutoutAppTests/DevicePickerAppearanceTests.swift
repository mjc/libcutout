import CutoutMobile
import CutoutMobileFFI
import SwiftUI
import XCTest

@testable import CutoutApp

func aeroAdvertisementPickerRow(platformIdentifier: String) -> DevicePickerRow {
    let candidate = mobileDiscoveryCandidateFromAdvertisement(
        platformIdentifier: platformIdentifier, localName: "NF2557",
        advertisedServiceUuids: [.eucSerialFfe0])
    return DevicePickerDiscoveryCandidate(candidate: candidate, advertisedName: "NF2557").pickerRow
}

final class DevicePickerAppearanceTests: XCTestCase {
    func testAeroAdvertisementIsVisuallyEnabledWhileAwaitingIdentification() {
        let row = aeroAdvertisementPickerRow(platformIdentifier: "aero")

        XCTAssertTrue(row.isProbeRecommended)
        XCTAssertNil(row.electricUnicycleModel)
        XCTAssertTrue(row.state.isSupported || row.isProbeRecommended)
        XCTAssertEqual(row.titleColor, PevColors.primaryText)
        XCTAssertEqual(row.secondaryTextColor, PevColors.muted)
    }

    func testUnavailableAndManualRowsRetainDisabledColors() {
        for state in [DevicePickerRowState.unsupported(action: "Record"), .manual(action: "Later")] {
            let row = DevicePickerRow(
                title: "Device", subtitle: "", detail: "", state: state, symbolName: "circle")
            XCTAssertEqual(row.titleColor, PevColors.disabledText)
            XCTAssertEqual(row.secondaryTextColor, PevColors.disabledSecondaryText)
        }
    }
}
