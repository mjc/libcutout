import XCTest
@testable import CutoutMobile

final class PevScreenCatalogTests: XCTestCase {
    func testV2CatalogListsEveryScreenInDeviceInspectionOrder() {
        XCTAssertEqual(
            PevScreenCatalog.v2.screens.map(\.id),
            [
                .devicePicker,
                .eucRide,
                .eucMap,
                .eucTune,
                .liveActivity,
                .bmsOverview,
                .bmsCellMap6S,
                .bmsCellMap40S,
                .bmsCellDetail,
                .bmsUnknownTopology,
                .bmsNoData,
                .eucGarage,
                .vescDebug,
                .vescMap,
                .vescLogs,
            ]
        )
    }

    func testV2CatalogCarriesFixtureOnlyScreenData() {
        let screens = Dictionary(uniqueKeysWithValues: PevScreenCatalog.v2.screens.map { ($0.id, $0) })

        XCTAssertEqual(screens[.devicePicker]?.title, "Device picker")
        XCTAssertEqual(screens[.devicePicker]?.subtitle, "Scanning Bluetooth")
        XCTAssertEqual(screens[.devicePicker]?.primaryValue, "Aero-126V")
        XCTAssertEqual(screens[.devicePicker]?.secondaryValue, "Little FOCer BT")

        XCTAssertEqual(screens[.eucRide]?.title, "Aero-126V")
        XCTAssertEqual(screens[.eucRide]?.subtitle, "EUC - riding")
        XCTAssertEqual(screens[.eucRide]?.primaryValue, "31 mph")
        XCTAssertEqual(screens[.eucRide]?.secondaryValue, "PWM headroom 23%")

        XCTAssertEqual(screens[.eucMap]?.title, "Map")
        XCTAssertEqual(screens[.eucMap]?.subtitle, "EUC map shell")

        XCTAssertEqual(screens[.eucTune]?.title, "Tune")
        XCTAssertEqual(screens[.eucTune]?.subtitle, "EUC tune/settings shell")

        XCTAssertEqual(screens[.bmsOverview]?.title, "Pack overview")
        XCTAssertEqual(screens[.bmsOverview]?.primaryValue, "72%")
        XCTAssertEqual(screens[.bmsOverview]?.secondaryValue, "sag adjusted")

        XCTAssertEqual(screens[.bmsCellMap6S]?.title, "6S cell map")
        XCTAssertEqual(screens[.bmsCellMap6S]?.primaryValue, "12 mV spread")

        XCTAssertEqual(screens[.bmsCellMap40S]?.title, "40S cell map")
        XCTAssertEqual(screens[.bmsCellMap40S]?.secondaryValue, "scroll cells horizontally")

        XCTAssertEqual(screens[.bmsCellDetail]?.title, "Cell detail")
        XCTAssertEqual(screens[.bmsCellDetail]?.primaryValue, "4.071 V")

        XCTAssertEqual(screens[.bmsUnknownTopology]?.title, "Unknown BMS")
        XCTAssertEqual(screens[.bmsUnknownTopology]?.secondaryValue, "topology unverified")

        XCTAssertEqual(screens[.bmsNoData]?.title, "Battery")
        XCTAssertEqual(screens[.bmsNoData]?.secondaryValue, "limited data")

        XCTAssertEqual(screens[.eucGarage]?.title, "EUC health")
        XCTAssertEqual(screens[.eucGarage]?.primaryValue, "battery 85%")
        XCTAssertEqual(screens[.eucGarage]?.secondaryValue, "pack 115.8 V")

        XCTAssertEqual(screens[.vescDebug]?.title, "VESC state")
        XCTAssertEqual(screens[.vescDebug]?.primaryValue, "duty cycle 82%")
        XCTAssertEqual(screens[.vescDebug]?.secondaryValue, "pack 75.4 V")

        XCTAssertEqual(screens[.vescMap]?.title, "Map")
        XCTAssertEqual(screens[.vescMap]?.subtitle, "VESC map shell")

        XCTAssertEqual(screens[.vescLogs]?.title, "Logs")
        XCTAssertEqual(screens[.vescLogs]?.subtitle, "VESC logs shell")

        XCTAssertTrue(PevScreenCatalog.v2.screens.allSatisfy(\.isPreviewOnly))
    }

    func testVescRideTabsMarkMapAndLogsUnavailable() {
        let tabs = PevRideTabs.vescRideTabs()

        XCTAssertEqual(tabs.map(\.title), ["Ride", "Debug", "Map", "Logs"])
        XCTAssertEqual(tabs[2].disabledReason, "LIBCU-423")
        XCTAssertEqual(tabs[3].disabledReason, "LIBCU-423")
        XCTAssertFalse(tabs[2].isEnabled)
        XCTAssertFalse(tabs[3].isEnabled)
    }
}

extension PevScreenCatalogTests {
    func testDevicePickerScanningFallbackDoesNotRenderFixtureRows() {
        let state = DevicePickerScanState.scanning

        XCTAssertEqual(state.status, .scanning)
        XCTAssertTrue(state.rows.isEmpty)
        XCTAssertTrue(state.sections.supported.isEmpty)
        XCTAssertTrue(state.sections.unsupported.isEmpty)
        XCTAssertNil(state.sections.manual)
    }

    func testDevicePickerFixtureCarriesRowsAndActionsFromPreview() throws {
        let picker = try XCTUnwrap(PevScreenCatalog.v2.screen(id: .devicePicker))

        XCTAssertEqual(picker.pickerRows.map(\.title), [
            "Aero-126V",
            "Little FOCer BT",
            "NINEBOT-7A31",
            "HX Hoverboard",
            "Manual add / record unknown device",
        ])
        XCTAssertEqual(picker.pickerRows.map(\.state), [
            .supported(action: "Use"),
            .supported(action: "Use"),
            .unsupported(action: "Record"),
            .unsupported(action: "Record"),
            .manual(action: "later"),
        ])
        XCTAssertEqual(picker.pickerRows.map(\.connectionRoute), [
            .electricUnicycle,
            .vescOnewheel,
            nil,
            nil,
            nil,
        ])
    }
    func testDevicePickerFixtureRowsComeFromDiscoveryCandidates() throws {
        let picker = try XCTUnwrap(PevScreenCatalog.v2.screen(id: .devicePicker))

        XCTAssertEqual(picker.discoveryCandidates.map(\.displayName), [
            "Aero-126V",
            "Little FOCer BT",
            "NINEBOT-7A31",
            "HX Hoverboard",
            "Manual add / record unknown device",
        ])
        XCTAssertEqual(picker.pickerRows, picker.discoveryCandidates.map(\.pickerRow))
    }

    func testDevicePickerSectionsHideEmptyGroups() {
        let supported = PevPickerRow(
            title: "Aero-126V",
            subtitle: "Electric unicycle",
            detail: "strong signal",
            state: .supported(action: "Use"),
            symbolName: "circle"
        )
        let unsupported = PevPickerRow(
            title: "NINEBOT-7A31",
            subtitle: "Electric scooter",
            detail: "unsupported",
            state: .unsupported(action: "Not yet"),
            symbolName: "scooter"
        )
        let probeRecommended = PevPickerRow(
            title: "EUC-unknown",
            subtitle: "Electric unicycle",
            detail: "read-only probe recommended",
            state: .probeRecommended(action: "Probe"),
            symbolName: "questionmark.circle"
        )

        XCTAssertEqual(PevPickerSections(rows: [supported]).supported, [supported])
        XCTAssertTrue(PevPickerSections(rows: [supported]).probeRecommended.isEmpty)
        XCTAssertTrue(PevPickerSections(rows: [supported]).unsupported.isEmpty)
        XCTAssertNil(PevPickerSections(rows: [supported]).manual)

        let unrouteable = PevPickerSections(rows: [probeRecommended, unsupported])
        XCTAssertTrue(unrouteable.supported.isEmpty)
        XCTAssertEqual(unrouteable.probeRecommended, [probeRecommended])
        XCTAssertEqual(unrouteable.unsupported, [unsupported])
        XCTAssertNil(unrouteable.manual)
    }

    func testDevicePickerScanStateCoversUnavailableAndEmptyStates() {
        XCTAssertEqual(DevicePickerScanState.bluetoothUnavailable.statusText, "Bluetooth unavailable")
        XCTAssertEqual(DevicePickerScanState.permissionDenied.statusText, "Bluetooth permission denied")

        let empty = DevicePickerScanState(status: .idle, rows: [])
        XCTAssertEqual(empty.statusText, "No rideable devices found")
        XCTAssertTrue(empty.sections.supported.isEmpty)
        XCTAssertTrue(empty.sections.probeRecommended.isEmpty)
        XCTAssertTrue(empty.sections.unsupported.isEmpty)
        XCTAssertNil(empty.sections.manual)
    }

    func testDevicePickerScanStateCoversConnectionFailure() {
        let failed = DevicePickerScanState.failed("Connect failed: timeout")

        XCTAssertEqual(failed.statusText, "Connect failed: timeout")
        XCTAssertTrue(failed.rows.isEmpty)
    }

    func testDevicePickerScanStateFindsStoredSupportedPickerRow() {
        let supported = DevicePickerRow(
            id: "ios-local-aero",
            title: "NOSFET Aero",
            subtitle: "Electric unicycle",
            detail: "telemetry profile found",
            state: .supported(action: "Use"),
            symbolName: "circle"
        )
        let unsupported = DevicePickerRow(
            id: "ios-local-unknown",
            title: "Unknown",
            subtitle: "Electric unicycle",
            detail: "model not confirmed",
            state: .unsupported(action: "Record"),
            symbolName: "questionmark.circle"
        )
        let state = DevicePickerScanState(status: .scanning, rows: [unsupported, supported])

        XCTAssertEqual(state.storedSupportedRow(platformIdentifier: "ios-local-aero"), supported)
        XCTAssertNil(state.storedSupportedRow(platformIdentifier: "ios-local-unknown"))
        XCTAssertNil(state.storedSupportedRow(platformIdentifier: nil))
    }

    func testDiscoveryCandidatesMapToPickerRows() {
        let supported = DevicePickerDiscoveryCandidate(
            platformIdentifier: "ios-local-1",
            displayName: "Aero-126V",
            productCategory: "Electric unicycle",
            evidence: "telemetry profile found",
            detail: "126.0 V - strong signal",
            support: .supported(connectionRoute: .electricUnicycle, electricUnicycleModel: .aero),
            symbolName: "circle"
        )
        let unsupported = DevicePickerDiscoveryCandidate(
            platformIdentifier: "ios-local-2",
            displayName: "NINEBOT-7A31",
            productCategory: "Electric scooter",
            evidence: "known BLE advertisement",
            detail: "We can learn this later",
            support: .unsupported(disabledReason: "Not yet"),
            symbolName: "scooter"
        )

        XCTAssertEqual(supported.pickerRow.state, .supported(action: "Use"))
        XCTAssertEqual(supported.pickerRow.subtitle, "Electric unicycle - telemetry profile found")
        XCTAssertEqual(supported.pickerRow.connectionRoute, .electricUnicycle)
        XCTAssertEqual(unsupported.pickerRow.state, .unsupported(action: "Record"))
        XCTAssertNil(unsupported.pickerRow.connectionRoute)
    }

    func testManualDiscoveryCandidateMapsToManualPickerRow() {
        let candidate = DevicePickerDiscoveryCandidate(candidate: mobileManualDiscoveryCandidate())

        XCTAssertEqual(candidate.support, .manualPlaceholder(disabledReason: "Capture flow later"))
        XCTAssertEqual(candidate.pickerRow.title, "Manual add / record unknown device")
        XCTAssertEqual(candidate.pickerRow.state, .manual(action: "later"))
        XCTAssertNil(candidate.pickerRow.connectionRoute)
    }

    func testAmbiguousDiscoveryCandidateRequiresConfirmationWithoutRoute() {
        let candidate = DevicePickerDiscoveryCandidate(candidate: mobileAmbiguousDiscoveryCandidate(
            platformIdentifier: "ios-local-begode",
            displayName: "GotWay_002441",
            detail: "Falcon or Falcon variant"
        ))

        XCTAssertEqual(candidate.support, .ambiguous(disabledReason: "Needs user confirmation"))
        XCTAssertEqual(candidate.pickerRow.state, .unsupported(action: "Confirm"))
        XCTAssertNil(candidate.pickerRow.connectionRoute)
    }

    func testConflictingDiscoveryCandidateStaysUnrouteable() {
        let candidate = DevicePickerDiscoveryCandidate(candidate: mobileConflictingDiscoveryCandidate(
            platformIdentifier: "ios-local-conflict",
            displayName: "Conflicting wheel",
            detail: "Veteran frame conflicts with Begode banner"
        ))

        XCTAssertEqual(candidate.support, .conflicting(disabledReason: "Conflicting identity evidence"))
        XCTAssertEqual(candidate.pickerRow.state, .unsupported(action: "Review"))
        XCTAssertNil(candidate.pickerRow.connectionRoute)
    }

    func testBegodeProbeConflictMapsToReviewRow() {
        let candidate = DevicePickerDiscoveryCandidate(candidate: mobileDiscoveryCandidateFromBegodeIdentityProbe(
            platformIdentifier: "ios-local-master",
            displayName: "GotWay_002441",
            probe: MobileBegodeIdentityProbeDto(
                reportedModel: "Master",
                reportedCodeName: "GW-MASTER",
                reportedImu: nil,
                reportedFirmwareVersion: nil,
                reportedSerial: nil,
                nominalVoltageHintMv: nil,
                missingProbeResponse: nil,
                malformedProbeResponse: nil
            )
        ))

        XCTAssertEqual(candidate.support, .conflicting(disabledReason: "Conflicting identity evidence"))
        XCTAssertEqual(candidate.pickerRow.state, .unsupported(action: "Review"))
        XCTAssertNil(candidate.pickerRow.connectionRoute)
    }

    func testMissingBegodeProbeResponseMapsToRecordRow() {
        let candidate = DevicePickerDiscoveryCandidate(candidate: mobileDiscoveryCandidateFromBegodeIdentityProbe(
            platformIdentifier: "ios-local-falcon",
            displayName: "GotWay_002441",
            probe: MobileBegodeIdentityProbeDto(
                reportedModel: nil,
                reportedCodeName: nil,
                reportedImu: nil,
                reportedFirmwareVersion: nil,
                reportedSerial: nil,
                nominalVoltageHintMv: nil,
                missingProbeResponse: .begodeName,
                malformedProbeResponse: nil
            )
        ))

        XCTAssertEqual(candidate.support, .unknownRecordable(disabledReason: "Missing Begode probe response"))
        XCTAssertEqual(candidate.pickerRow.state, .unsupported(action: "Record"))
        XCTAssertNil(candidate.pickerRow.connectionRoute)
    }

    func testMalformedBegodeProbeResponseMapsToRecordRow() {
        let candidate = DevicePickerDiscoveryCandidate(candidate: mobileDiscoveryCandidateFromBegodeIdentityProbe(
            platformIdentifier: "ios-local-falcon-malformed",
            displayName: "GotWay_002441",
            probe: MobileBegodeIdentityProbeDto(
                reportedModel: nil,
                reportedCodeName: nil,
                reportedImu: nil,
                reportedFirmwareVersion: nil,
                reportedSerial: nil,
                nominalVoltageHintMv: nil,
                missingProbeResponse: nil,
                malformedProbeResponse: .begodeName
            )
        ))

        XCTAssertEqual(candidate.support, .unknownRecordable(disabledReason: "Malformed Begode probe response"))
        XCTAssertEqual(candidate.pickerRow.state, .unsupported(action: "Record"))
        XCTAssertNil(candidate.pickerRow.connectionRoute)
    }

    func testFamilyOnlyBegodeProbeMapsToRecordRow() {
        let candidate = DevicePickerDiscoveryCandidate(candidate: mobileDiscoveryCandidateFromBegodeIdentityProbe(
            platformIdentifier: "ios-local-begode-family",
            displayName: "GotWay_002441",
            probe: MobileBegodeIdentityProbeDto(
                reportedModel: nil,
                reportedCodeName: nil,
                reportedImu: nil,
                reportedFirmwareVersion: nil,
                reportedSerial: nil,
                nominalVoltageHintMv: nil,
                missingProbeResponse: nil,
                malformedProbeResponse: nil
            )
        ))

        XCTAssertEqual(candidate.support, .unknownRecordable(disabledReason: "Begode model not confirmed"))
        XCTAssertEqual(candidate.pickerRow.state, .unsupported(action: "Record"))
        XCTAssertNil(candidate.pickerRow.connectionRoute)
    }

    func testCodeOnlyBegodeProbeMapsToRecordRow() {
        let candidate = DevicePickerDiscoveryCandidate(candidate: mobileDiscoveryCandidateFromBegodeIdentityProbe(
            platformIdentifier: "ios-local-falcon-code",
            displayName: "GotWay_002441",
            probe: MobileBegodeIdentityProbeDto(
                reportedModel: nil,
                reportedCodeName: "GW-FALCON",
                reportedImu: nil,
                reportedFirmwareVersion: nil,
                reportedSerial: nil,
                nominalVoltageHintMv: nil,
                missingProbeResponse: nil,
                malformedProbeResponse: nil
            )
        ))

        XCTAssertEqual(candidate.support, .unknownRecordable(disabledReason: "Unresolved Begode code banner"))
        XCTAssertEqual(candidate.pickerRow.state, .unsupported(action: "Record"))
        XCTAssertNil(candidate.support.connectionRoute)
        XCTAssertNil(candidate.support.electricUnicycleModel)
        XCTAssertNil(candidate.pickerRow.connectionRoute)
    }

    func testCoreBluetoothAdvertisementMapsToPickerCandidateWithoutMacAddress() {
        let advertisement = CoreBluetoothAdvertisement(
            peripheralIdentifier: CoreBluetoothPeripheralIdentifier("ios-local-aero"),
            localName: "NOSFET Aero",
            advertisedServiceUuids: [.bluetooth16(0xFFE0)]
        )
        let candidate = DevicePickerDiscoveryCandidate(advertisement: advertisement)

        XCTAssertEqual(candidate.platformIdentifier, "ios-local-aero")
        XCTAssertEqual(candidate.displayName, "NOSFET Aero")
        XCTAssertEqual(candidate.productCategory, "Electric unicycle")
        XCTAssertEqual(candidate.support, .provisionalRoute(connectionRoute: .electricUnicycle, electricUnicycleModel: .aero))
        XCTAssertEqual(candidate.pickerRow.id, "ios-local-aero")
        XCTAssertEqual(candidate.pickerRow.state, .supported(action: "Use"))
        XCTAssertEqual(candidate.pickerRow.connectionRoute, .electricUnicycle)
    }

    func testTypedFalconDeviceKindMapsToFalconHint() {
        XCTAssertEqual(CutoutModelHint(deviceKind: "EUC falcon"), .falcon)
        XCTAssertEqual(CutoutModelHint(deviceKind: "scooter foo bar"), .unknown)
    }

    func testAdvertisementModelHintUsesTypedDeviceKindParser() {
        let advertisement = CoreBluetoothAdvertisement(
            peripheralIdentifier: CoreBluetoothPeripheralIdentifier("ios-local-gotway"),
            localName: "GotWay_002441",
            advertisedServiceUuids: [.bluetooth16(0xFFE0)]
        )

        XCTAssertEqual(advertisement.modelHint, .falcon)
    }

    func testRouteLessSupportedDiscoveryCandidateStaysUnrouteableInPicker() {
        let dto = DiscoveryCandidate(
            platformIdentifier: "ios-local-supported-future",
            displayName: "Future rideable",
            productCategory: "Rideable",
            evidence: "supported by core",
            detail: "route not mapped in previews yet",
            isPickerCandidate: true,
            support: .supported,
            recommendedAction: .use,
            section: .supported,
            connectionRoute: nil,
            electricUnicycleModel: nil,
            disabledReason: nil
        )
        let candidate = DevicePickerDiscoveryCandidate(candidate: dto)

        XCTAssertEqual(
            candidate.support,
            DevicePickerCandidateSupport.supported(connectionRoute: nil, electricUnicycleModel: nil)
        )
        XCTAssertEqual(candidate.pickerRow.state, PevPickerRowState.supported(action: "Use"))
        XCTAssertNil(candidate.pickerRow.connectionRoute)
    }

    func testProvisionalRouteDiscoveryCandidateOffersUsePickerAction() {
        let dto = DiscoveryCandidate(
            platformIdentifier: "ios-local-gotway",
            displayName: "GotWay_002441",
            productCategory: "Electric unicycle",
            evidence: "advertisement hint",
            detail: "Falcon provisional route",
            isPickerCandidate: true,
            support: .provisionalRoute,
            recommendedAction: .use,
            section: .supported,
            connectionRoute: .electricUnicycle,
            electricUnicycleModel: .falcon,
            disabledReason: nil
        )
        let candidate = DevicePickerDiscoveryCandidate(candidate: dto)

        XCTAssertEqual(
            candidate.support,
            .provisionalRoute(connectionRoute: .electricUnicycle, electricUnicycleModel: .falcon)
        )
        XCTAssertEqual(candidate.pickerRow.state, .supported(action: "Use"))
        XCTAssertEqual(candidate.pickerRow.connectionRoute, .electricUnicycle)
        XCTAssertEqual(candidate.pickerRow.symbolName, "circle.hexagongrid.circle")
    }

    func testUnsupportedDiscoveryCandidateOffersRecordOnlyPickerAction() {
        let dto = DiscoveryCandidate(
            platformIdentifier: "ios-local-unknown",
            displayName: "Unknown rideable",
            productCategory: "Rideable",
            evidence: "probable PEV advertisement",
            detail: "unsupported model",
            isPickerCandidate: true,
            support: .unsupported,
            recommendedAction: .record,
            section: .recordOnly,
            connectionRoute: nil,
            electricUnicycleModel: nil,
            disabledReason: "Model not confirmed"
        )
        let candidate = DevicePickerDiscoveryCandidate(candidate: dto)

        XCTAssertEqual(candidate.support, .unsupported(disabledReason: "Model not confirmed"))
        XCTAssertEqual(candidate.pickerRow.state, .unsupported(action: "Record"))
        XCTAssertNil(candidate.pickerRow.connectionRoute)
    }

    func testRejectedNoiseDiscoveryCandidateStaysUnrouteable() {
        let dto = DiscoveryCandidate(
            platformIdentifier: "ios-local-keyboard",
            displayName: "Keyboard",
            productCategory: "Unknown rideable",
            evidence: "advertisement observed",
            detail: "Rejected noise",
            isPickerCandidate: false,
            support: .rejectedNoise,
            recommendedAction: .record,
            section: .recordOnly,
            connectionRoute: nil,
            electricUnicycleModel: nil,
            disabledReason: "Rejected noise"
        )
        let candidate = DevicePickerDiscoveryCandidate(candidate: dto)

        XCTAssertEqual(candidate.support, .rejectedNoise(disabledReason: "Rejected noise"))
        XCTAssertEqual(candidate.pickerRow.state, .unsupported(action: "Record"))
        XCTAssertNil(candidate.pickerRow.connectionRoute)
    }

    func testKnownUnsupportedDiscoveryCandidateOffersRecordOnlyPickerAction() {
        let dto = DiscoveryCandidate(
            platformIdentifier: "ios-local-vesc",
            displayName: "Little FOCer",
            productCategory: "VESC Onewheel",
            evidence: "VESC advertisement hint",
            detail: "Not yet supported",
            isPickerCandidate: true,
            support: .knownUnsupported,
            recommendedAction: .record,
            section: .recordOnly,
            connectionRoute: nil,
            electricUnicycleModel: nil,
            disabledReason: "Not yet supported"
        )
        let candidate = DevicePickerDiscoveryCandidate(candidate: dto)

        XCTAssertEqual(candidate.support, .knownUnsupported(disabledReason: "Not yet supported"))
        XCTAssertEqual(candidate.pickerRow.state, .unsupported(action: "Record"))
        XCTAssertNil(candidate.pickerRow.connectionRoute)
    }

    func testProbeRecommendedDiscoveryCandidateOffersProbePickerAction() {
        let dto = DiscoveryCandidate(
            platformIdentifier: "ios-local-unknown-euc",
            displayName: "EUC-unknown",
            productCategory: "Electric unicycle",
            evidence: "FFE0/FFE1 transport hint",
            detail: "Read-only probe recommended",
            isPickerCandidate: true,
            support: .probeRecommended,
            recommendedAction: .probe,
            section: .probeFirst,
            connectionRoute: nil,
            electricUnicycleModel: nil,
            disabledReason: "Read-only probe recommended"
        )
        let candidate = DevicePickerDiscoveryCandidate(candidate: dto)

        XCTAssertEqual(dto.recommendedAction, .probe)
        XCTAssertEqual(dto.section, .probeFirst)
        XCTAssertEqual(candidate.support, .probeRecommended(disabledReason: "Read-only probe recommended"))
        XCTAssertEqual(candidate.pickerRow.state, .probeRecommended(action: "Probe"))
        XCTAssertEqual(candidate.pickerRow.section, .probeFirst)
        XCTAssertEqual(candidate.pickerRow.captureActionTitle, "Start probe")
        XCTAssertNil(candidate.pickerRow.connectionRoute)
    }

    func testUnknownRecordableDiscoveryCandidateOffersRecordOnlyPickerAction() {
        let dto = DiscoveryCandidate(
            platformIdentifier: "ios-local-unknown-euc",
            displayName: "EUC-unknown",
            productCategory: "Electric unicycle",
            evidence: "FFE0/FFE1 transport hint",
            detail: "Model not confirmed",
            isPickerCandidate: true,
            support: .unknownRecordable,
            recommendedAction: .record,
            section: .recordOnly,
            connectionRoute: nil,
            electricUnicycleModel: nil,
            disabledReason: "Model not confirmed"
        )
        let candidate = DevicePickerDiscoveryCandidate(candidate: dto)

        XCTAssertEqual(candidate.support, .unknownRecordable(disabledReason: "Model not confirmed"))
        XCTAssertEqual(candidate.pickerRow.state, .unsupported(action: "Record"))
        XCTAssertEqual(candidate.pickerRow.captureActionTitle, "Start capture")
        XCTAssertNil(candidate.pickerRow.connectionRoute)
    }

    func testCoreBluetoothAdvertisementsMapToScanningPickerState() {
        let state = DevicePickerScanState(
            status: .scanning,
            advertisements: [
                CoreBluetoothAdvertisement(
                    peripheralIdentifier: CoreBluetoothPeripheralIdentifier("ios-local-aero"),
                    localName: "NOSFET Aero",
                    advertisedServiceUuids: [.bluetooth16(0xFFE0)]
                ),
                CoreBluetoothAdvertisement(
                    peripheralIdentifier: CoreBluetoothPeripheralIdentifier("ios-local-gotway"),
                    localName: "GotWay_002441",
                    advertisedServiceUuids: [.bluetooth16(0xFFE0)]
                ),
                CoreBluetoothAdvertisement(
                    peripheralIdentifier: CoreBluetoothPeripheralIdentifier("ios-local-nf"),
                    localName: "NF2557",
                    advertisedServiceUuids: [.bluetooth16(0xFFE0)]
                ),
                CoreBluetoothAdvertisement(
                    peripheralIdentifier: CoreBluetoothPeripheralIdentifier("ios-local-unknown"),
                    localName: "Little FOCer",
                    advertisedServiceUuids: [.bluetooth16(0xFFF0)]
                ),
                CoreBluetoothAdvertisement(
                    peripheralIdentifier: CoreBluetoothPeripheralIdentifier("ios-local-keyboard"),
                    localName: "Keyboard",
                    advertisedServiceUuids: []
                ),
            ]
        )

        XCTAssertEqual(state.statusText, "Scanning Bluetooth")
        XCTAssertEqual(state.rows.map(\.title), ["NOSFET Aero", "GotWay_002441", "NF2557", "Little FOCer"])
        XCTAssertEqual(state.rows.map(\.connectionRoute), [.electricUnicycle, .electricUnicycle, .electricUnicycle, .vescOnewheel])
        XCTAssertEqual(state.sections.supported.map(\.title), ["NOSFET Aero", "GotWay_002441", "NF2557", "Little FOCer"])
        XCTAssertTrue(state.sections.unsupported.isEmpty)
        XCTAssertNil(state.sections.manual)
    }

    func testEucRideFixtureCarriesDashboardStructureFromPreview() throws {
        let ride = try XCTUnwrap(PevScreenCatalog.v2.screen(id: .eucRide))

        XCTAssertEqual(ride.safetyBars, [
            PevSafetyBar(label: "PWM headroom", value: "23%", progress: 0.77, accent: .yellow),
            PevSafetyBar(label: "sag-adjusted energy", value: "62%", progress: 0.62, accent: .cyan),
        ])
        XCTAssertEqual(
            ride.warningCard,
            PevWarningCard(title: "Reduce acceleration", detail: "Voltage sag under load: 9.4 V")
        )
        XCTAssertEqual(ride.dashboardTiles, [
            PevDashboardTile(label: "pack", value: "115.8", unit: "V", detail: "-9.4 V sag", accent: .cyan),
            PevDashboardTile(label: "power", value: "4.2", unit: "kW", detail: "regen -0.3 kW", accent: .yellow),
            PevDashboardTile(label: "thermal", value: "61", unit: "°C", detail: "ESC 48 · motor 61", accent: .green),
            PevDashboardTile(label: "limp-home", value: "14.2", unit: "mi", detail: "at this pace", accent: .cyan),
        ])
        XCTAssertEqual(ride.tabs, [
            PevScreenTab(title: "Ride", isSelected: true),
            PevScreenTab(title: "Pack", isSelected: false, destinationScreenID: .bmsOverview),
            PevScreenTab(title: "Map", isSelected: false, disabledReason: "LIBCU-423"),
            PevScreenTab(title: "Tune", isSelected: false, disabledReason: "LIBCU-423"),
        ])
        XCTAssertEqual(ride.tabs.first { $0.title == "Pack" }?.destinationScreenID, .bmsOverview)
    }
    func testEucGarageFixtureCarriesPackHealthStructureFromPreview() throws {
        let garage = try XCTUnwrap(PevScreenCatalog.v2.screen(id: .eucGarage))

        XCTAssertEqual(
            garage.deviceCard,
            PevDeviceCard(
                title: "Aero-126V",
                detail: "126 V nominal · 20s? mapped profile · BLE",
                status: "Safe",
                accent: .green
            )
        )
        XCTAssertEqual(garage.dashboardTiles, [
            PevDashboardTile(label: "battery", value: "85", unit: "%", detail: "115.8 V", accent: .cyan),
            PevDashboardTile(kind: .beepMargin, label: "beep margin", value: "11.6", unit: "mph", detail: "to configured alarm", accent: .yellow),
            PevDashboardTile(kind: .tiltback, label: "tiltback", value: "42", unit: "mph", detail: "wheel setting", accent: .orange),
            PevDashboardTile(kind: .pedalMode, label: "pedal mode", value: "72", unit: "%", detail: "hardness normalized", accent: .purple),
        ])
        XCTAssertEqual(garage.summaryTitle, "Cell / BMS summary")
        XCTAssertEqual(garage.summaryRows, [
            PevSummaryRow(label: "high group", value: "4.18 V", accent: nil),
            PevSummaryRow(label: "low group", value: "4.13 V", accent: nil),
            PevSummaryRow(label: "delta", value: "0.05 V", accent: .green),
        ])
        XCTAssertEqual(
            garage.eucGarageSnapshot,
            EucGarageSnapshot(
                pack: EucPackHealthSnapshot(
                    energyPercent: BatteryLevel(value: 85),
                    voltage: Voltage(value: 115_800),
                    highGroupVoltage: Voltage(value: 4_180),
                    lowGroupVoltage: Voltage(value: 4_130),
                    cellDelta: VoltageDelta(value: 50)
                ),
                settings: EucGarageSettingsSnapshot(
                    beepMargin: .available(Speed(value: 5_186)),
                    tiltback: .available(Speed(value: 18_776)),
                    pedalMode: .available(PedalMode(hardnessPercent: 72))
                ),
                faultHistory: .none(sinceDistance: Distance(value: 61_456_941))
            )
        )
        XCTAssertEqual(garage.faultCard, PevFaultCard(title: "Last fault", detail: "none since 38.2 mi ago", accent: .green))
    }

    func testEucFaultHistoryCanRepresentStructuredUnknownFault() {
        let fault = EucFaultHistoryState.fault(
            code: FaultCode.unknown(id: 0x0040, value: 1),
            sinceDistance: Distance(value: 61_456_941)
        )

        XCTAssertEqual(
            fault,
            .fault(
                code: FaultCode.unknown(id: 0x0040, value: 1),
                sinceDistance: Distance(value: 61_456_941)
            )
        )
    }

    func testVescDebugFixtureCarriesGuardedReadOnlyStateFromPreview() throws {
        let debug = try XCTUnwrap(PevScreenCatalog.v2.screen(id: .vescDebug))

        XCTAssertEqual(
            debug.deviceCard,
            PevDeviceCard(
                title: "Profile: Street stable",
                detail: "VESC Express · FW 6.x · UART bridge",
                status: "",
                accent: .cyan
            )
        )
        XCTAssertEqual(debug.dashboardTiles, [
            PevDashboardTile(label: "duty cycle", value: "82", unit: "%", detail: "max seen 87%", accent: .orange),
            PevDashboardTile(label: "pack", value: "75.4", unit: "V", detail: "20s lithium", accent: .cyan),
            PevDashboardTile(label: "battery limit", value: "45", unit: "A", detail: "current max", accent: .yellow),
            PevDashboardTile(label: "motor limit", value: "90", unit: "A", detail: "phase current", accent: .orange),
        ])
        XCTAssertEqual(debug.summaryTitle, "Fault / app channels")
        XCTAssertEqual(debug.summaryRows, [
            PevSummaryRow(label: "last fault", value: "FAULT_CODE_NONE", accent: .green),
            PevSummaryRow(label: "input app", value: "ADC + balance", accent: nil),
            PevSummaryRow(label: "CAN status", value: "single controller", accent: nil),
            PevSummaryRow(label: "logging", value: "local CSV armed", accent: .yellow),
        ])
        XCTAssertEqual(
            debug.faultCard,
            PevFaultCard(
                title: "Guardrails",
                detail: "Hide dangerous writes until parked + confirmed.",
                accent: .orange
            )
        )
    }

    func testBmsFixturesCarryTypedSnapshotsForEveryBmsScreen() throws {
        let screenIDs: [PevScreenID] = [
            .bmsOverview,
            .bmsCellMap6S,
            .bmsCellMap40S,
            .bmsCellDetail,
            .bmsUnknownTopology,
            .bmsNoData,
        ]

        let screens = try screenIDs.map {
            try XCTUnwrap(PevScreenCatalog.v2.screen(id: $0))
        }

        XCTAssertEqual(screens.map(\.bmsContent?.kind), [
            .overview,
            .cellMapInline,
            .cellMapScrollable,
            .cellDetail,
            .unknownTopology,
            .noData,
        ])
        XCTAssertTrue(screens.allSatisfy { $0.bmsContent?.snapshot != nil })
    }

    func testLiveActivityFixtureHarnessIsAvailableFromTheCatalog() throws {
        let liveActivity = try XCTUnwrap(PevScreenCatalog.v2.screen(id: .liveActivity))

        XCTAssertEqual(liveActivity.title, "Live Activity")
        XCTAssertTrue(liveActivity.isPreviewOnly)
        XCTAssertEqual(liveActivity.metrics.map(\.label), ["preview states", "presentation modes"])
    }

    func testBmsOverviewFixtureCapturesTopologyAndFaultSummary() throws {
        let overview = try XCTUnwrap(PevScreenCatalog.v2.screen(id: .bmsOverview))
        let content = try XCTUnwrap(overview.bmsContent)
        let snapshot = content.snapshot

        XCTAssertEqual(snapshot.topology.layoutLabel, "20S4P split pack")
        XCTAssertEqual(snapshot.topology.seriesGroupCount, 20)
        XCTAssertEqual(snapshot.topology.parallelCount, 4)
        XCTAssertEqual(snapshot.topology.packCount, 2)
        XCTAssertEqual(snapshot.topology.bmsCount, 2)
        XCTAssertEqual(snapshot.energyPercent, BatteryLevel(value: 72))
        XCTAssertEqual(snapshot.voltage, Voltage(value: 81_600))
        XCTAssertEqual(snapshot.cellDelta, VoltageDelta(value: 18))
        XCTAssertEqual(snapshot.lowestGroupIndex, 17)
        XCTAssertEqual(snapshot.balancingSummary, "idle • top groups only")
        XCTAssertEqual(snapshot.faultSummary, "no active faults")
    }

    func testBmsCellFixturesPreserveHighlightedGroupsAndDetailSelection() throws {
        let inline = try XCTUnwrap(PevScreenCatalog.v2.screen(id: .bmsCellMap6S)?.bmsContent)
        XCTAssertEqual(inline.snapshot.groups.map(\.index), [1, 2, 3, 4, 5, 6])
        XCTAssertEqual(inline.highlightedGroupIndices, [3, 6])

        let scrollable = try XCTUnwrap(PevScreenCatalog.v2.screen(id: .bmsCellMap40S)?.bmsContent)
        XCTAssertEqual(scrollable.snapshot.groups.count, 40)
        XCTAssertEqual(scrollable.highlightedGroupIndices, [17, 18, 19, 31])

        let detail = try XCTUnwrap(PevScreenCatalog.v2.screen(id: .bmsCellDetail)?.bmsContent)
        let selectedGroup = detail.snapshot.groups.first { $0.index == 17 }
        XCTAssertEqual(detail.selectedGroupIndex, 17)
        XCTAssertEqual(selectedGroup?.voltage, Voltage(value: 4_071))
        XCTAssertEqual(selectedGroup?.resistance, Resistance(value: 21))
    }

    func testResolvedBmsContentPrefersLiveSnapshotAndDerivesSmallPackLayout() throws {
        let screen = try XCTUnwrap(PevScreenCatalog.v2.screen(id: .eucGarage))
        let liveSnapshot = BmsSnapshot(
            topology: BmsTopology(
                layoutLabel: "6S1P pack",
                seriesGroupCount: 6,
                parallelCount: 1,
                packCount: 1,
                bmsCount: 1,
                confidence: .verified
            ),
            voltage: Voltage(value: 25_200),
            lowestGroupIndex: 3,
            groups: [
                BmsGroupSnapshot(index: 1, label: "G1", voltage: Voltage(value: 4_201)),
                BmsGroupSnapshot(index: 2, label: "G2", voltage: Voltage(value: 4_199)),
                BmsGroupSnapshot(index: 3, label: "G3", voltage: Voltage(value: 4_150)),
                BmsGroupSnapshot(index: 4, label: "G4", voltage: Voltage(value: 4_203)),
                BmsGroupSnapshot(index: 5, label: "G5", voltage: Voltage(value: 4_205)),
                BmsGroupSnapshot(index: 6, label: "G6", voltage: Voltage(value: 4_202)),
            ]
        )

        let presented = PevScreenCatalog.v2.presentedScreen(for: screen, liveBmsSnapshot: liveSnapshot)
        let resolved = try XCTUnwrap(presented.bmsContent)

        XCTAssertEqual(resolved.kind, .cellMapInline)
        XCTAssertEqual(resolved.snapshot, liveSnapshot)
        XCTAssertEqual(resolved.highlightedGroupIndices, [3])
        XCTAssertEqual(resolved.selectedGroupIndex, nil)
        XCTAssertEqual(resolved.modeTitles, ["balance view"])
    }

    func testResolvedBmsContentDerivesScrollableLiveModeTitles() throws {
        let screen = try XCTUnwrap(PevScreenCatalog.v2.screen(id: .eucGarage))
        let liveSnapshot = BmsSnapshot(
            topology: BmsTopology(
                layoutLabel: "large EUC pack",
                seriesGroupCount: 40,
                parallelCount: 4,
                packCount: 1,
                bmsCount: 1,
                confidence: .verified
            ),
            lowestGroupIndex: 17,
            groups: [
                BmsGroupSnapshot(index: 17, voltage: Voltage(value: 4_071), temperature: Temperature(value: 34_900), resistance: Resistance(value: 21), alertLevel: .warning),
                BmsGroupSnapshot(index: 18, voltage: Voltage(value: 4_089), alertLevel: .nominal)
            ] + (19...56).map { index in
                BmsGroupSnapshot(index: index, voltage: Voltage(value: 4_080), alertLevel: .nominal)
            }
        )

        let presented = PevScreenCatalog.v2.presentedScreen(for: screen, liveBmsSnapshot: liveSnapshot)
        let resolved = try XCTUnwrap(presented.bmsContent)

        XCTAssertEqual(resolved.kind, .cellMapScrollable)
        XCTAssertEqual(resolved.modeTitles, ["overview", "strip", "raw table", "temps"])
    }

    func testPresentedScreenRoutesPackToLiveBmsScreen() throws {
        let packScreen = try XCTUnwrap(PevScreenCatalog.v2.screen(id: .eucGarage))
        let liveSnapshot = BmsSnapshot(
            topology: BmsTopology(
                layoutLabel: "6S1P pack",
                seriesGroupCount: 6,
                parallelCount: 1,
                packCount: 1,
                bmsCount: 1,
                confidence: .verified
            ),
            groups: [
                BmsGroupSnapshot(index: 1, label: "G1", voltage: Voltage(value: 4_201)),
                BmsGroupSnapshot(index: 2, label: "G2", voltage: Voltage(value: 4_199)),
                BmsGroupSnapshot(index: 3, label: "G3", voltage: Voltage(value: 4_150)),
                BmsGroupSnapshot(index: 4, label: "G4", voltage: Voltage(value: 4_203)),
                BmsGroupSnapshot(index: 5, label: "G5", voltage: Voltage(value: 4_205)),
                BmsGroupSnapshot(index: 6, label: "G6", voltage: Voltage(value: 4_202)),
            ]
        )

        let presented = PevScreenCatalog.v2.presentedScreen(for: packScreen, liveBmsSnapshot: liveSnapshot)

        XCTAssertEqual(presented.id, .bmsCellMap6S)
    }

    func testPresentedScreenRoutesPackWithoutLiveBmsToNoDataWithoutFixtureValues() throws {
        let packScreen = try XCTUnwrap(PevScreenCatalog.v2.screen(id: .eucGarage))

        let presented = PevScreenCatalog.v2.presentedScreen(
            for: packScreen,
            liveBmsSnapshot: nil,
            previewFallback: false
        )

        XCTAssertEqual(presented.id, .bmsNoData)
        XCTAssertEqual(presented.primaryValue, "--")
        XCTAssertEqual(presented.secondaryValue, "no live BMS")
        XCTAssertTrue(presented.metrics.isEmpty)
        XCTAssertEqual(presented.bmsContent?.snapshot.availability, .unavailable)
        XCTAssertEqual(presented.bmsContent?.snapshot.topology.layoutLabel, "live BMS readback unavailable")
        XCTAssertNil(presented.bmsContent?.snapshot.energyPercent)
        XCTAssertNil(presented.bmsContent?.snapshot.voltage)
        XCTAssertTrue(presented.bmsContent?.snapshot.groups.isEmpty ?? false)
    }

    func testPresentedScreenDerivesLiveBmsTitleAndChips() throws {
        let packScreen = try XCTUnwrap(PevScreenCatalog.v2.screen(id: .eucGarage))
        let liveSnapshot = BmsSnapshot(
            topology: BmsTopology(
                layoutLabel: "6S1P pack",
                seriesGroupCount: 6,
                parallelCount: 1,
                packCount: 1,
                bmsCount: 1,
                confidence: .verified
            ),
            groups: [
                BmsGroupSnapshot(index: 1, label: "G1", voltage: Voltage(value: 4_201)),
                BmsGroupSnapshot(index: 2, label: "G2", voltage: Voltage(value: 4_199)),
                BmsGroupSnapshot(index: 3, label: "G3", voltage: Voltage(value: 4_150)),
                BmsGroupSnapshot(index: 4, label: "G4", voltage: Voltage(value: 4_203)),
                BmsGroupSnapshot(index: 5, label: "G5", voltage: Voltage(value: 4_205)),
                BmsGroupSnapshot(index: 6, label: "G6", voltage: Voltage(value: 4_202)),
            ]
        )

        let presented = PevScreenCatalog.v2.presentedScreen(for: packScreen, liveBmsSnapshot: liveSnapshot)

        XCTAssertEqual(presented.title, "6S cell map")
        XCTAssertEqual(presented.bmsContent?.chips.map(\.title), ["live readback", "6S1P pack"])
    }

    func testPresentedScreenDerivesNoDataHeaderFromLiveSnapshot() throws {
        let packScreen = try XCTUnwrap(PevScreenCatalog.v2.screen(id: .eucGarage))
        let liveSnapshot = BmsSnapshot(
            availability: .unsupported,
            topology: BmsTopology(
                layoutLabel: "non-smart BMS",
                seriesGroupCount: nil,
                parallelCount: nil,
                packCount: 1,
                bmsCount: 0,
                confidence: .inferred
            ),
            captureActionState: "limited data"
        )

        let presented = PevScreenCatalog.v2.presentedScreen(for: packScreen, liveBmsSnapshot: liveSnapshot)

        XCTAssertEqual(presented.id, .bmsNoData)
        XCTAssertEqual(presented.title, "Battery")
        XCTAssertEqual(presented.subtitle, "non-smart BMS · controller-only estimate")
        XCTAssertEqual(presented.secondaryValue, "limited data")
    }

    func testPresentedScreenDerivesLiveBmsMetadataForDirectBmsScreen() throws {
        let directScreen = try XCTUnwrap(PevScreenCatalog.v2.screen(id: .bmsCellMap6S))
        let liveSnapshot = BmsSnapshot(
            topology: BmsTopology(
                layoutLabel: "12S2P pack",
                seriesGroupCount: 12,
                parallelCount: 2,
                packCount: 1,
                bmsCount: 1,
                confidence: .verified
            ),
            groups: (1...12).map { index in
                BmsGroupSnapshot(index: index, label: "G\(index)", voltage: Voltage(value: Int32(4_100 + index)))
            }
        )

        let presented = PevScreenCatalog.v2.presentedScreen(for: directScreen, liveBmsSnapshot: liveSnapshot)

        XCTAssertEqual(presented.id, PevScreenID.bmsCellMap6S)
        XCTAssertEqual(presented.title, "12S cell map")
        XCTAssertEqual(presented.bmsContent?.chips.map { $0.title }, ["live readback", "12S2P pack"])
    }

    func testPresentedOverviewScreenDoesNotMorphToUnknownTopology() throws {
        let directScreen = try XCTUnwrap(PevScreenCatalog.v2.screen(id: .bmsOverview))
        let liveSnapshot = BmsSnapshot(
            topology: BmsTopology(
                layoutLabel: "2 observed BMS groups",
                seriesGroupCount: nil,
                parallelCount: nil,
                packCount: 1,
                bmsCount: 1,
                confidence: .unverified
            ),
            pageSelector: 2,
            pageKind: "metadata",
            voltage: Voltage(value: 95_800),
        )

        let presented = PevScreenCatalog.v2.presentedScreen(for: directScreen, liveBmsSnapshot: liveSnapshot)

        XCTAssertEqual(presented.id, PevScreenID.bmsOverview)
        XCTAssertEqual(presented.title, "Pack overview")
        XCTAssertEqual(presented.bmsContent?.kind, .overview)
    }

    func testUnknownTopologyFixtureKeepsConfidenceLowAndAvoidsFakeMapping() throws {
        let unknown = try XCTUnwrap(PevScreenCatalog.v2.screen(id: .bmsUnknownTopology)?.bmsContent)
        let snapshot = unknown.snapshot

        XCTAssertEqual(snapshot.topology.confidence, .unverified)
        XCTAssertNil(snapshot.topology.seriesGroupCount)
        XCTAssertEqual(snapshot.groups.count, 0)
        XCTAssertEqual(snapshot.faults.map(\.code), ["0x0040"])
        XCTAssertEqual(snapshot.captureActionTitle, "record unsupported pack")
    }

    func testNoDataFixtureMarksControllerOnlyEstimate() throws {
        let noData = try XCTUnwrap(PevScreenCatalog.v2.screen(id: .bmsNoData)?.bmsContent)
        let snapshot = noData.snapshot

        XCTAssertEqual(snapshot.topology.layoutLabel, "non-smart BMS")
        XCTAssertEqual(snapshot.topology.confidence, .inferred)
        XCTAssertEqual(snapshot.energyPercent, BatteryLevel(value: 71))
        XCTAssertEqual(snapshot.voltage, Voltage(value: 117_600))
        XCTAssertEqual(snapshot.current, BatteryCurrent(value: 38_000))
        XCTAssertEqual(snapshot.captureActionState, "limited data")
    }
}
