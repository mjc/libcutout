import XCTest
@testable import CutoutMobile

final class DeviceDetectionSessionTests: XCTestCase {
    private func syntheticVeteranFrameWithModelId43() -> Data {
        var bytes = Array(repeating: UInt8(0), count: 42)
        bytes.replaceSubrange(0..<4, with: [0xdc, 0x5a, 0x5c, 38])
        bytes.replaceSubrange(28..<30, with: [0xa7, 0xf8])
        return Data(bytes)
    }

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

    func testMalformedBegodeDetectionResolutionProjectsRecordOnlyCandidate() {
        let session = DeviceDetectionSession()

        _ = session.observeBegodeNameProbe()
        let resolution = session.observeNotification(
            bytes: Data([0x4e, 0x41, 0x4d, 0x45, 0x3d, 0x46, 0x61, 0x6c, 0x63, 0x6f, 0x6e, 0x00])
        )
        let candidate = DevicePickerDiscoveryCandidate(candidate: resolution.discoveryCandidate(
            platformIdentifier: "ios-local-falcon-malformed",
            displayName: "GotWay_002441"
        ))

        XCTAssertEqual(resolution.malformedProbeResponse, .begodeName)
        XCTAssertEqual(candidate.support, .unknownRecordable(disabledReason: "Malformed Begode probe response"))
        XCTAssertEqual(candidate.pickerRow.state, .unsupported(action: "Record"))
        XCTAssertEqual(candidate.pickerRow.section, .recordOnly)
        XCTAssertNil(candidate.pickerRow.connectionRoute)
    }

    func testMixedProtocolFamiliesProjectConflictingCandidate() {
        let session = DeviceDetectionSession()
        let begodeFrame = Data([
            0x55, 0xaa, 0x17, 0x75, 0x05, 0x38, 0x00, 0x76,
            0x02, 0xee, 0xfb, 0x64, 0xf4, 0x94, 0x14, 0x81,
            0x00, 0x09, 0x00, 0x18, 0x5a, 0x5a, 0x5a, 0x5a,
        ])
        _ = session.observeNotification(bytes: syntheticVeteranFrameWithModelId43())

        let resolution = session.observeNotification(bytes: begodeFrame)
        let candidate = DevicePickerDiscoveryCandidate(candidate: resolution.discoveryCandidate(
            platformIdentifier: "ios-local-conflict",
            displayName: "Conflicting wheel"
        ))

        XCTAssertTrue(resolution.protocolConflict)
        XCTAssertEqual(candidate.support, .conflicting(disabledReason: "Conflicting identity evidence"))
        XCTAssertEqual(candidate.pickerRow.state, .unsupported(action: "Review"))
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
