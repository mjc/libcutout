import XCTest
@testable import CutoutMobile

final class DeviceDetectionSessionTests: XCTestCase {
    func testAdvertisementRetainsRawBytes() {
        let session = DeviceDetectionSession()

        let resolution = session.observeAdvertisement(name: Data([0x4e, 0x46, 0xff]))

        XCTAssertEqual(resolution.advertisedName, Data([0x4e, 0x46, 0xff]))
        XCTAssertNil(resolution.protocolFamily)
    }

    func testBegodeNameProbeRetainsModelBannerBytes() {
        let session = DeviceDetectionSession()

        _ = session.observeBegodeNameProbe()
        let resolution = session.observeNotification(bytes: Data("NAME=Falcon".utf8))

        XCTAssertEqual(resolution.modelBanner, Data("Falcon".utf8))
    }

    func testBegodeFirmwareProbeRetainsFirmwareBannerBytes() {
        let session = DeviceDetectionSession()

        _ = session.observeBegodeFirmwareProbe()
        let resolution = session.observeNotification(bytes: Data("GW FALCON 1.0".utf8))

        XCTAssertEqual(resolution.firmwareBanner, Data("GW FALCON 1.0".utf8))
    }

    func testBegodeImuProbeRetainsImuBannerBytes() {
        let session = DeviceDetectionSession()

        _ = session.observeBegodeImuProbe()
        let resolution = session.observeNotification(bytes: Data("MPU6500".utf8))

        XCTAssertEqual(resolution.imuBanner, Data("MPU6500".utf8))
    }

    func testBegodeNameProbeTimeoutIsExposed() {
        let session = DeviceDetectionSession()

        _ = session.observeBegodeNameProbe()
        let resolution = session.observeBegodeNameProbeTimeout()

        XCTAssertEqual(resolution.missingProbeResponse, .begodeName)
        XCTAssertNil(resolution.modelBanner)
    }

    func testBegodeDetectionResolutionProjectsRecordOnlyCandidate() {
        let session = DeviceDetectionSession()

        _ = session.observeBegodeNameProbe()
        let resolution = session.observeBegodeNameProbeTimeout()
        let candidate = DevicePickerDiscoveryCandidate(candidate: resolution.discoveryCandidate(
            platformIdentifier: "ios-local-falcon",
            displayName: "GotWay_002441"
        ))

        XCTAssertEqual(candidate.support, .unknownRecordable(disabledReason: "Missing Begode probe response"))
        XCTAssertEqual(candidate.pickerRow.state, .unsupported(action: "Record"))
        XCTAssertEqual(candidate.pickerRow.section, .recordOnly)
        XCTAssertNil(candidate.pickerRow.connectionRoute)
    }

    func testMalformedBegodeNameProbeRetainsRawControlByte() {
        let session = DeviceDetectionSession()

        _ = session.observeBegodeNameProbe()
        let resolution = session.observeNotification(
            bytes: Data([0x4e, 0x41, 0x4d, 0x45, 0x3d, 0x46, 0x61, 0x6c, 0x63, 0x6f, 0x6e, 0x00])
        )

        XCTAssertEqual(resolution.modelBanner, Data([0x46, 0x61, 0x6c, 0x63, 0x6f, 0x6e, 0x00]))
    }
}
