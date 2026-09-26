import CutoutMobileFFI
import XCTest

@testable import CutoutMobile

final class DeviceDetectionSessionTests: XCTestCase {
    private let begodeLiveFrame = Data([
        0x55, 0xaa, 0x17, 0x75, 0x05, 0x38, 0x00, 0x76,
        0x02, 0xee, 0xfb, 0x64, 0xf4, 0x94, 0x14, 0x81,
        0x00, 0x09, 0x00, 0x18, 0x5a, 0x5a, 0x5a, 0x5a,
    ])
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

    func testDefaultSessionWaitsForProtocolEvidenceBeforeIdentificationWrites() {
        let session = DeviceDetectionSession()
        _ = session.observeGatt(fingerprints: [
            DeviceDetectionGattFingerprint(
                service: BluetoothUuid.eucSerialFfe0.bytes,
                characteristic: BluetoothUuid.bluetooth16(0xffe1).bytes,
                roles: [.read, .write, .writeWithoutResponse, .notify],
                verification: .hardwareVerified
            )
        ])

        XCTAssertEqual(session.beginIdentificationProbe(at: MonotonicMilliseconds(999)), .unsupported)
        _ = session.observeNotification(bytes: begodeLiveFrame)
        guard case let .writes(writes) = session.beginIdentificationProbe(at: MonotonicMilliseconds(1_000)) else {
            return XCTFail("Begode wire evidence should enable identification probes")
        }
        XCTAssertEqual(writes.count, 1)
    }

    func testBegodeNameProbeRetainsModelBannerBytes() {
        let session = DeviceDetectionSession()

        _ = session.observeBegodeNameProbe()
        let resolution = session.observeNotification(bytes: Data("NAME=Falcon".utf8))

        XCTAssertEqual(resolution.modelBanner, Data("Falcon".utf8))
    }

    func testTwoReportedFalconsKeepAdvertisementNamesAndDistinctPickerIdentity() {
        let rows = [("falcon-00A1", "GotWay_002441"), ("falcon-00B2", "GotWay_002442")].map { identifier, name in
            let session = DeviceDetectionSession()
            _ = session.observeNotification(bytes: begodeLiveFrame)
            _ = session.observeBegodeNameProbe()
            let resolution = session.observeNotification(bytes: Data("NAME:Falcon\r\n".utf8))
            return DevicePickerDiscoveryCandidate(
                candidate: resolution.discoveryCandidate(
                    platformIdentifier: identifier, displayName: name
                )
            ).pickerRow
        }
        XCTAssertEqual(rows.map(\.title), ["Begode Falcon", "Begode Falcon"])
        XCTAssertEqual(rows.map(\.id), ["falcon-00A1", "falcon-00B2"])
        XCTAssertTrue(rows[0].detail.contains("GotWay_002441"))
        XCTAssertFalse(rows[0].detail.contains("GotWay_002442"))
        XCTAssertTrue(rows[1].detail.contains("GotWay_002442"))
        XCTAssertNotEqual(rows[0].useActionAccessibilityLabel, rows[1].useActionAccessibilityLabel)
    }

    func testProbeDispositionPromotesOnlyRustResolvedModels() {
        let begode = DeviceDetectionSession()
        _ = begode.observeBegodeNameProbe()
        let falcon = begode.observeNotification(bytes: Data("NAME=Falcon".utf8))

        let veteran = DeviceDetectionSession()
        let aero = veteran.observeNotification(bytes: syntheticVeteranFrameWithModelId43())

        XCTAssertEqual(
            falcon.probeDisposition(platformIdentifier: "begode", displayName: "Unknown EUC"),
            .promote(.falcon)
        )
        XCTAssertEqual(
            aero.probeDisposition(platformIdentifier: "veteran", displayName: "Unknown EUC"),
            .promote(.aero)
        )
        XCTAssertEqual(
            DeviceDetectionSession().resolution.probeDisposition(
                platformIdentifier: "unknown",
                displayName: "Unknown EUC"
            ),
            .pending
        )
    }

    func testFragmentedVeteranStreamPromotesAeroThroughFFI() {
        let session = DeviceDetectionSession()
        let frame = syntheticVeteranFrameWithModelId43()

        XCTAssertNil(session.observeNotification(bytes: frame.prefix(20)).protocolFamily)
        XCTAssertNil(session.observeNotification(bytes: frame.dropFirst(20).prefix(20)).protocolFamily)
        let resolution = session.observeNotification(bytes: frame.dropFirst(40))

        XCTAssertEqual(resolution.protocolFamily, .veteranLeaperkimNosfet)
        XCTAssertEqual(
            resolution.probeDisposition(platformIdentifier: "wheel", displayName: "Unknown EUC"),
            .promote(.aero)
        )
    }

    func testPassiveProtocolEvidenceRemainsEligibleForRideAdmission() {
        let session = DeviceDetectionSession()
        let frame = syntheticVeteranFrameWithModelId43()

        XCTAssertEqual(
            ProtocolDetectionFinishDecision(resolution: session.resolution),
            .awaitPassiveEvidence
        )

        _ = session.observeNotification(bytes: frame)

        XCTAssertEqual(
            ProtocolDetectionFinishDecision(resolution: session.resolution),
            .evaluateResolvedEvidence
        )
    }

    func testProbeDispositionRefusesMissingMalformedAndConflictingEvidence() {
        let missingSession = DeviceDetectionSession()
        _ = missingSession.observeBegodeNameProbe()
        let missing = missingSession.observeBegodeNameProbeTimeout()

        let malformedSession = DeviceDetectionSession()
        _ = malformedSession.observeBegodeNameProbe()
        let malformed = malformedSession.observeNotification(bytes: Data("NAME=Falcon\0".utf8))

        let conflictingSession = DeviceDetectionSession()
        _ = conflictingSession.observeNotification(bytes: syntheticVeteranFrameWithModelId43())
        let conflict = conflictingSession.observeNotification(
            bytes: Data([
                0x55, 0xaa, 0x17, 0x75, 0x05, 0x38, 0x00, 0x76, 0x02, 0xee, 0xfb, 0x64, 0xf4, 0x94, 0x14, 0x81, 0x00,
                0x09, 0x00, 0x18, 0x5a, 0x5a, 0x5a, 0x5a,
            ])
        )

        XCTAssertEqual(
            missing.probeDisposition(platformIdentifier: "missing", displayName: "Unknown EUC"),
            .refuse(.timedOut)
        )
        XCTAssertEqual(
            malformed.probeDisposition(platformIdentifier: "malformed", displayName: "Unknown EUC"),
            .refuse(.malformedResponse)
        )
        XCTAssertEqual(
            conflict.probeDisposition(platformIdentifier: "conflict", displayName: "Unknown EUC"),
            .refuse(.conflictingEvidence)
        )
    }

    func testAttemptScopedIdentificationProbeOutcomeDistinguishesProbeNeededAndUnsupported() {
        let aeroState = CutoutSessionStateHandle()
        _ = aeroState.observeDiscovery(
            observation: DiscoveryObservation(
                platformIdentifier: "ios-local-aero",
                advertisedName: Data("BLE device".utf8),
                advertisedServiceUuids: [.eucSerialFfe0],
                manufacturerData: [],
                rssiDbm: -48
            ))
        _ = aeroState.selectDiscoveredPlatform(platformIdentifier: "ios-local-aero")
        XCTAssertNotNil(
            aeroState.beginConnectionAttempt(platformIdentifier: "ios-local-aero", nowMs: 1_000).token
        )

        let vescState = CutoutSessionStateHandle()
        _ = vescState.observeDiscovery(
            observation: DiscoveryObservation(
                platformIdentifier: "ios-local-vesc",
                advertisedName: Data("Controller".utf8),
                advertisedServiceUuids: [.vescNordicUart],
                manufacturerData: [],
                rssiDbm: -48
            ))
        _ = vescState.selectDiscoveredPlatform(platformIdentifier: "ios-local-vesc")
        _ = vescState.beginConnectionAttempt(platformIdentifier: "ios-local-vesc", nowMs: 1_000).token

        let aeroSession = DeviceDetectionSession(sessionState: aeroState)
        XCTAssertEqual(aeroSession.beginIdentificationProbe(at: MonotonicMilliseconds(1_000)), .unsupported)
        _ = aeroSession.observeNotification(bytes: begodeLiveFrame)
        guard
            case let .writes(writes) =
                aeroSession
                .beginIdentificationProbe(at: MonotonicMilliseconds(1_000))
        else {
            return XCTFail("Begode protocol evidence should schedule one query")
        }
        XCTAssertEqual(writes.count, 1)
        XCTAssertEqual(
            DeviceDetectionSession(sessionState: vescState)
                .beginIdentificationProbe(at: MonotonicMilliseconds(1_000)),
            .unsupported
        )
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
        let candidate = DevicePickerDiscoveryCandidate(
            candidate: resolution.discoveryCandidate(
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
        let candidate = DevicePickerDiscoveryCandidate(
            candidate: resolution.discoveryCandidate(
                platformIdentifier: "ios-local-falcon-malformed",
                displayName: "GotWay_002441"
            ))

        XCTAssertEqual(resolution.malformedProbeResponse, .begodeName)
        XCTAssertEqual(candidate.support, .unknownRecordable(disabledReason: "Malformed Begode probe response"))
        XCTAssertEqual(candidate.pickerRow.state, .unsupported(action: "Record"))
        XCTAssertEqual(candidate.pickerRow.section, .recordOnly)
        XCTAssertNil(candidate.pickerRow.connectionRoute)
    }

    func testMissingBegodeDetectionResolutionDoesNotUseStaleModelBanner() {
        let session = DeviceDetectionSession()

        _ = session.observeBegodeNameProbe()
        _ = session.observeNotification(bytes: Data("NAME=Falcon".utf8))
        _ = session.observeBegodeNameProbe()
        let resolution = session.observeBegodeNameProbeTimeout()
        let candidate = DevicePickerDiscoveryCandidate(
            candidate: resolution.discoveryCandidate(
                platformIdentifier: "ios-local-falcon-missing",
                displayName: "GotWay_002441"
            ))

        XCTAssertEqual(resolution.missingProbeResponse, .begodeName)
        XCTAssertEqual(candidate.support, .unknownRecordable(disabledReason: "Missing Begode probe response"))
        XCTAssertEqual(candidate.pickerRow.state, .unsupported(action: "Record"))
        XCTAssertEqual(candidate.pickerRow.section, .recordOnly)
        XCTAssertNil(candidate.pickerRow.connectionRoute)
    }

    func testEmptyDetectionResolutionDoesNotProjectPickerCandidate() {
        let session = DeviceDetectionSession()

        let candidate = session.resolution.discoveryCandidate(
            platformIdentifier: "ios-local-empty",
            displayName: "Unknown peripheral"
        )

        XCTAssertFalse(candidate.isPickerCandidate)
    }

    func testVeteranDetectionResolutionProjectsSupportedCandidate() {
        let session = DeviceDetectionSession()

        let resolution = session.observeNotification(bytes: syntheticVeteranFrameWithModelId43())
        let candidate = DevicePickerDiscoveryCandidate(
            candidate: resolution.discoveryCandidate(
                platformIdentifier: "ios-local-aero",
                displayName: "NF2557"
            ))

        XCTAssertEqual(
            candidate.support,
            .supported(connectionRoute: .electricUnicycle, electricUnicycleModel: .aero)
        )
        XCTAssertEqual(candidate.pickerRow.connectionRoute, .electricUnicycle)
    }

    func testVeteranFamilyOnlyDetectionResolutionRequiresProbe() {
        let candidate = DevicePickerDiscoveryCandidate(
            candidate: mobileDiscoveryCandidateFromDetectionResolution(
                platformIdentifier: "ios-local-veteran-family",
                displayName: "Veteran stream",
                resolution: DeviceDetectionResolutionRecord(
                    protocolFamily: .veteranLeaperkimNosfet,
                    protocolConflict: false,
                    veteranProtocolModelId: nil,
                    advertisedName: nil,
                    modelBanner: nil,
                    firmwareBanner: nil,
                    imuBanner: nil,
                    missingProbeResponse: nil,
                    malformedProbeResponse: nil
                )
            ))

        XCTAssertEqual(
            candidate.support,
            .probeRecommended(disabledReason: "Veteran/NOSFET model identity probe required")
        )
        XCTAssertEqual(candidate.pickerRow.state, .probeRecommended(action: "Use"))
        XCTAssertEqual(candidate.pickerRow.section, .probeFirst)
        XCTAssertNil(candidate.pickerRow.connectionRoute)
    }

    func testResolvedProtocolFamilyWinsOverUnrelatedProbeTimeout() {
        let resolution = DeviceDetectionResolution(
            DeviceDetectionResolutionRecord(
                protocolFamily: .veteranLeaperkimNosfet,
                protocolConflict: false,
                veteranProtocolModelId: nil,
                advertisedName: nil,
                modelBanner: nil,
                firmwareBanner: nil,
                imuBanner: nil,
                missingProbeResponse: .begodeName,
                malformedProbeResponse: nil
            )
        )

        XCTAssertEqual(
            resolution.connectionDisposition(
                platformIdentifier: "ios-local-aero",
                displayName: "NF2557"
            ),
            .refuse(.timedOut)
        )
        XCTAssertEqual(
            resolution.connectionDisposition(
                platformIdentifier: "ios-local-aero",
                displayName: "NF2557",
                allowClosestMatch: true
            ),
            .promote(route: .electricUnicycle, model: .aero)
        )
    }

    func testBegodeFamilyOnlyDetectionResolutionRequiresProbe() {
        let candidate = DevicePickerDiscoveryCandidate(
            candidate: mobileDiscoveryCandidateFromDetectionResolution(
                platformIdentifier: "ios-local-begode-family",
                displayName: "Begode stream",
                resolution: DeviceDetectionResolutionRecord(
                    protocolFamily: .begodeGotway,
                    protocolConflict: false,
                    veteranProtocolModelId: nil,
                    advertisedName: nil,
                    modelBanner: nil,
                    firmwareBanner: nil,
                    imuBanner: nil,
                    missingProbeResponse: nil,
                    malformedProbeResponse: nil
                )
            ))

        XCTAssertEqual(
            candidate.support,
            .probeRecommended(disabledReason: "Begode/Gotway model identity probe required")
        )
        XCTAssertEqual(candidate.pickerRow.state, .probeRecommended(action: "Use"))
        XCTAssertEqual(candidate.pickerRow.section, .probeFirst)
        XCTAssertNil(candidate.pickerRow.connectionRoute)
    }

    func testVescFamilyOnlyDetectionResolutionProjectsSupportedCandidate() {
        let candidate = DevicePickerDiscoveryCandidate(
            candidate: mobileDiscoveryCandidateFromDetectionResolution(
                platformIdentifier: "ios-local-vesc-family",
                displayName: "VESC stream",
                resolution: DeviceDetectionResolutionRecord(
                    protocolFamily: .vesc,
                    protocolConflict: false,
                    veteranProtocolModelId: nil,
                    advertisedName: nil,
                    modelBanner: nil,
                    firmwareBanner: nil,
                    imuBanner: nil,
                    missingProbeResponse: nil,
                    malformedProbeResponse: nil
                )
            ))

        XCTAssertEqual(candidate.productCategory, "VESC Onewheel")
        XCTAssertEqual(candidate.support, .supported(connectionRoute: .vescOnewheel, electricUnicycleModel: nil))
        XCTAssertEqual(candidate.pickerRow.state, .supported(action: "Use"))
        XCTAssertEqual(candidate.pickerRow.section, .supported)
        XCTAssertEqual(candidate.pickerRow.connectionRoute, .vescOnewheel)
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
        let candidate = DevicePickerDiscoveryCandidate(
            candidate: resolution.discoveryCandidate(
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

    func testResetClearsDeviceIdentityEvidence() {
        let session = DeviceDetectionSession()

        _ = session.observeAdvertisement(name: Data("GotWay_002441".utf8))
        _ = session.observeBegodeNameProbe()
        let observed = session.observeNotification(bytes: Data("NAME=Falcon".utf8))

        XCTAssertNotNil(observed.advertisedName)
        XCTAssertNotNil(observed.modelBanner)

        session.reset()

        XCTAssertNil(session.resolution.advertisedName)
        XCTAssertNil(session.resolution.modelBanner)
        XCTAssertNil(session.resolution.protocolFamily)
        XCTAssertFalse(session.resolution.protocolConflict)
    }
}
