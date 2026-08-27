import XCTest
@testable import CutoutApp
import CutoutMobile
import CoreLocation
import MapKit

final class CutoutAppRouteTests: XCTestCase {
    func testScreenRoutesMatchTopLevelSections() {
        XCTAssertEqual(CutoutAppRoute.route(for: .eucRide), .eucRide)
        XCTAssertEqual(CutoutAppRoute.route(for: .vescRide), .vescRide)
        XCTAssertEqual(CutoutAppRoute.route(for: .bmsOverview), .eucPack(.bmsOverview))
        XCTAssertEqual(CutoutAppRoute.route(for: .bmsCellMap6S), .eucPack(.bmsCellMap6S))
        XCTAssertEqual(CutoutAppRoute.route(for: .bmsCellMap40S), .eucPack(.bmsCellMap40S))
        XCTAssertEqual(CutoutAppRoute.route(for: .bmsCellDetail), .eucPack(.bmsCellDetail(nil)))
        XCTAssertEqual(CutoutAppRoute.route(for: .bmsUnknownTopology), .eucPack(.bmsUnknownTopology))
        XCTAssertEqual(CutoutAppRoute.route(for: .bmsNoData), .eucPack(.bmsNoData))
        XCTAssertEqual(CutoutAppRoute.route(for: .vescDebug), .vescDebug)
        XCTAssertEqual(CutoutAppRoute.route(forNavigationTarget: .rideMap), .rideMap)
    }

    @MainActor
    func testRouteTruthShowsCanonicalGapCountForLiveAndTruncatedRoutes() {
        XCTAssertTrue(
            RideMapRouteTruthView.shouldShowBackgroundGapCount(
                routeIsPresent: true,
                canonicalBackgroundGapCount: 1
            )
        )
        XCTAssertFalse(
            RideMapRouteTruthView.shouldShowBackgroundGapCount(
                routeIsPresent: true,
                canonicalBackgroundGapCount: 0
            )
        )
        XCTAssertFalse(
            RideMapRouteTruthView.shouldShowBackgroundGapCount(
                routeIsPresent: false,
                canonicalBackgroundGapCount: 1
            )
        )
    }

    func testNavigationLabelsResolveFromTheAppCatalog() {
        XCTAssertEqual(localizedAppText("navigation.tab.cells"), "Cells")
        XCTAssertEqual(localizedAppText("navigation.tab.faults"), "Faults")
        XCTAssertEqual(localizedAppText("picker.title"), "Choose device")
        XCTAssertEqual(localizedAppText("picker.subtitle.nearby_devices"), "Nearby Bluetooth devices")
        XCTAssertEqual(localizedAppText("bms.detail.back_to_cell_map"), "Back to cell map")
        XCTAssertEqual(localizedAppText("bms.detail.group", Int64(3)), "Group 3")
        XCTAssertEqual(localizedAppText("bms.detail.trend", "stable"), "Trend: stable")
        XCTAssertEqual(localizedAppText("bms.overview.usable_energy"), "Usable energy")
        XCTAssertEqual(localizedAppText("bms.overview.average_group"), "Average group")
        XCTAssertEqual(localizedAppText("bms.overview.lowest_group"), "Lowest group")
        XCTAssertEqual(localizedAppText("bms.overview.highest_temperature"), "Highest temperature")
        XCTAssertEqual(localizedAppText("bms.overview.balancing"), "Balancing")
        XCTAssertEqual(localizedAppText("bms.overview.fault_state"), "Fault state")
        XCTAssertEqual(localizedAppText("bms.overview.pack_telemetry"), "Pack telemetry")
        XCTAssertEqual(localizedAppText("bms.unknown.title"), "Do not pretend certainty")
        XCTAssertEqual(localizedAppText("bms.unknown.reported_voltage"), "Reported voltage")
        XCTAssertEqual(localizedAppText("bms.unknown.cell_count"), "Cell count")
        XCTAssertEqual(localizedAppText("bms.unknown.temperatures"), "Temperatures")
        XCTAssertEqual(localizedAppText("bms.unknown.fault_bits"), "Fault bits")
        XCTAssertEqual(localizedAppText("bms.unknown.next_capture_flow"), "Next capture flow")
        XCTAssertEqual(localizedAppText("bms.no_data.confidence.medium"), "Medium")
        XCTAssertEqual(localizedAppText("bms.no_data.confidence.low"), "Low")
        XCTAssertEqual(localizedAppText("bms.no_data.confidence.unknown"), "Unknown")
        XCTAssertEqual(localizedAppText("bms.no_data.confidence_detail.not_cell_safe"), "Not cell-safe")
        XCTAssertEqual(localizedAppText("bms.no_data.confidence_detail.telemetry_unavailable"), "Telemetry unavailable")
        XCTAssertEqual(localizedAppText("bms.no_data.pack_estimate"), "PACK ESTIMATE")
        XCTAssertEqual(localizedAppText("bms.no_data.confidence"), "CONFIDENCE")
        XCTAssertEqual(localizedAppText("bms.no_data.what_we_can_see"), "WHAT WE CAN SEE")
        XCTAssertEqual(localizedAppText("bms.no_data.pack_voltage"), "Pack voltage")
        XCTAssertEqual(localizedAppText("bms.no_data.ride_sag"), "Ride sag")
        XCTAssertEqual(localizedAppText("bms.no_data.load_now"), "Load now")
        XCTAssertEqual(localizedAppText("bms.no_data.what_is_unknown"), "WHAT IS UNKNOWN")
        XCTAssertEqual(localizedAppText("bms.no_data.pack_estimate_accessibility"), "Pack estimate")
        XCTAssertEqual(localizedAppText("bms.no_data.confidence_accessibility"), "Confidence")
        XCTAssertEqual(localizedAppText("bms.no_data.estimate_detail.recent_sag"), "Derived from voltage curve + recent sag")
        XCTAssertEqual(localizedAppText("bms.no_data.estimate_detail.voltage_curve"), "Derived from voltage curve only")
        XCTAssertEqual(localizedAppText("bms.no_data.estimate_detail.unavailable"), "Estimate unavailable")
        XCTAssertEqual(
            localizedAppText("ride_map.history_truncated_count", 16_384),
            "Showing 16,384 route points; the full route remains stored."
        )
        XCTAssertEqual(
            localizedAppText("ride_map.live_route_truncated_count", 4_096),
            "Showing the newest 4,096 points; the full route remains stored."
        )
        XCTAssertEqual(
            localizedAppText("ride_map.segments_omitted_by_budget"),
            "Some route segments are omitted from this map preview."
        )
        XCTAssertEqual(
            localizedAppText("ride_map.show_route_preview"),
            "Show route preview"
        )
        XCTAssertEqual(localizedAppText("bms.diagnostics.title"), "BMS diagnostics")
        XCTAssertEqual(localizedAppText("bms.diagnostics.detail"), "Raw readback, available when we need to debug")
        XCTAssertEqual(localizedAppText("bms.display_modes"), "Display modes")
        XCTAssertEqual(localizedAppText("picker.section.setup"), "Setup")
        XCTAssertEqual(localizedAppText("picker.advanced_capture"), "Capture unknown device")
        XCTAssertEqual(localizedAppText("picker.capture_kind.label"), "Device kind for capture")
        XCTAssertEqual(localizedAppText("picker.capture_kind.placeholder"), "Device model")
        XCTAssertEqual(localizedAppText("picker.capture_kind.hint"), "Enter the device family and model, for example EUC NOSFET Aeon")
        XCTAssertEqual(localizedAppText("picker.section.supported_now"), "Supported now")
        XCTAssertEqual(localizedAppText("picker.section.probe_first"), "Probe first")
        XCTAssertEqual(localizedAppText("picker.section.record_only"), "Record only")
        XCTAssertEqual(localizedAppText("picker.capture_kind_required_hint"), "Enter a device kind above to enable capture")
        XCTAssertEqual(localizedAppText("capture.stop"), "Finish capture")
        XCTAssertEqual(localizedAppText("picker.use_action.hint"), "Connect to this device")
        XCTAssertEqual(localizedAppText("picker.error.device_no_longer_available"), "Device is no longer available")
        XCTAssertEqual(localizedAppText("app.command.no_connected_device"), "No connected device")
        XCTAssertEqual(localizedAppText("app.command.disconnect"), "Disconnect")
        XCTAssertEqual(localizedAppText("app.command.navigate"), "Navigate")
        XCTAssertEqual(
            localizedAppText("bms.no_data.pack_estimate_accessibility_value", "71", "Derived from voltage curve"),
            "71%. Derived from voltage curve"
        )
        XCTAssertEqual(
            localizedAppText("bms.no_data.confidence_accessibility_value", "Medium", "Not cell-safe"),
            "Medium. Not cell-safe"
        )
        XCTAssertEqual(
            PevScreen(id: .bmsCellDetail, title: "", subtitle: "", secondaryValue: "").tabTitle,
            "Cells"
        )
        XCTAssertEqual(
            PevScreen(id: .bmsUnknownTopology, title: "", subtitle: "", secondaryValue: "").tabTitle,
            "Faults"
        )
    }

    func testEucPackRouteRejectsNonPackScreens() {
        XCTAssertNil(EucPackScreen(screenID: .vescRide))
        XCTAssertNil(EucPackScreen(screenID: .vescDebug))
        XCTAssertEqual(EucPackScreen(screenID: .bmsOverview), .bmsOverview)
    }

    func testRouteFocusIdentityDistinguishesEveryDestination() {
        let routes: Set<CutoutAppRoute> = [
            .devicePicker,
            .eucRide,
            .eucPack(.bmsOverview),
            .eucPack(.bmsCellMap6S),
            .eucPack(.bmsCellMap40S),
            .eucPack(.bmsCellDetail(nil)),
            .eucPack(.bmsUnknownTopology),
            .eucPack(.bmsNoData),
            .eucPack(.root),
            .vescRide,
            .vescDebug,
            .capture,
        ]

        XCTAssertEqual(routes.count, 12)
    }

    func testBmsDetailRouteStaysSelectedOnlyWhileItsGroupExists() {
        XCTAssertTrue(EucPackScreen.bmsCellDetail(4).hasAvailableSelectedGroup(in: [1, 4, 7]))
        XCTAssertFalse(EucPackScreen.bmsCellDetail(4).hasAvailableSelectedGroup(in: [1, 7]))
        XCTAssertTrue(EucPackScreen.bmsCellDetail(4).hasAvailableSelectedGroup(in: nil))
        XCTAssertTrue(EucPackScreen.bmsCellDetail(nil).hasAvailableSelectedGroup(in: [1, 7]))
        XCTAssertTrue(EucPackScreen.bmsOverview.hasAvailableSelectedGroup(in: [1, 7]))
    }

    func testConnectionRoutesMatchRideSections() {
        XCTAssertEqual(CutoutAppRoute.route(for: DevicePickerConnectionRoute.electricUnicycle), .eucRide)
        XCTAssertEqual(CutoutAppRoute.route(for: DevicePickerConnectionRoute.vescOnewheel), .vescRide)
        XCTAssertEqual(CutoutAppRoute.route(for: nil), .devicePicker)
    }

    func testNavigationPathKeepsPickerAtRootAndReplacesConnectedDestinations() {
        XCTAssertEqual(CutoutAppRoute.navigationPath(for: .devicePicker), [])
        XCTAssertEqual(CutoutAppRoute.navigationPath(for: .eucRide), [.eucRide])
        XCTAssertEqual(CutoutAppRoute.navigationPath(for: .eucPack(.bmsOverview)), [.eucPack(.bmsOverview)])
        XCTAssertEqual(CutoutAppRoute.navigationPath(for: .vescDebug), [.vescDebug])
        XCTAssertEqual(CutoutAppRoute.navigationPath(for: .capture), [.capture])
        XCTAssertEqual(CutoutAppRoute.navigationPath(for: .rideMap), [.rideMap])
        XCTAssertEqual(
            CutoutAppRoute.navigationPath(for: .rideMapDetail(rideID: "ride-1")),
            [.rideMap, .rideMapDetail(rideID: "ride-1")]
        )
    }

    @MainActor
    func testRideMapSeparatesOverlappingEndpointMarkers() {
        let coordinate = CLLocationCoordinate2D(latitude: 39.7392, longitude: -104.9903)
        let offsets = RideMapCanvasView.markerOffsets(for: [coordinate, coordinate])

        XCTAssertNotEqual(offsets.start, .zero)
        XCTAssertNotEqual(offsets.end, .zero)
        XCTAssertEqual(offsets.start.width, -offsets.end.width)
    }

    @MainActor
    func testLiveCameraRegionUsesPrivacyProjectedCoordinates() {
        let points = [
            MobileRideMapRouteDisplayPoint(
                sequence: 0,
                segmentId: 0,
                latitudeDegrees: 39.7,
                longitudeDegrees: -104.9,
                privacyClass: .gridRedacted
            ),
            MobileRideMapRouteDisplayPoint(
                sequence: 1,
                segmentId: 0,
                latitudeDegrees: 39.71,
                longitudeDegrees: -104.89,
                privacyClass: .gridRedacted
            ),
        ]

        let region = RideMapLiveContentView.routeRegion(
            centeredOn: points[1],
            points: points
        )

        XCTAssertEqual(region.center.latitude, points[1].latitudeDegrees)
        XCTAssertEqual(region.center.longitude, points[1].longitudeDegrees)
        XCTAssertGreaterThan(region.span.latitudeDelta, 0)
        XCTAssertGreaterThan(region.span.longitudeDelta, 0)
    }

    @MainActor
    func testRideMapSeparatesStartAndEndMarkersForSinglePointRoutes() {
        let coordinate = CLLocationCoordinate2D(latitude: 39.7392, longitude: -104.9903)
        let offsets = RideMapCanvasView.markerOffsets(for: [coordinate])

        XCTAssertNotEqual(offsets.start, .zero)
        XCTAssertNotEqual(offsets.end, .zero)
        XCTAssertEqual(offsets.start.width, -offsets.end.width)
    }

    @MainActor
    func testRideMapProjectionVersionInvalidatesSameShapePath() {
        let initialPoints = [
            MobileRideMapRouteDisplayPoint(
                sequence: 0,
                segmentId: 0,
                latitudeDegrees: 39.70,
                longitudeDegrees: -104.90,
                privacyClass: .precise
            ),
            MobileRideMapRouteDisplayPoint(
                sequence: 1,
                segmentId: 0,
                latitudeDegrees: 39.71,
                longitudeDegrees: -104.89,
                privacyClass: .precise
            ),
            MobileRideMapRouteDisplayPoint(
                sequence: 2,
                segmentId: 0,
                latitudeDegrees: 39.72,
                longitudeDegrees: -104.88,
                privacyClass: .precise
            ),
        ]
        let reprojectedPoints = [
            initialPoints[0],
            MobileRideMapRouteDisplayPoint(
                sequence: 1,
                segmentId: 0,
                latitudeDegrees: 39.90,
                longitudeDegrees: -104.50,
                privacyClass: .precise
            ),
            initialPoints[2],
        ]

        let initialKey = RideMapCanvasView.pathKey(
            routeID: "ride-1",
            projectionVersion: 1,
            points: initialPoints
        )
        let reprojectedKey = RideMapCanvasView.pathKey(
            routeID: "ride-1",
            projectionVersion: 2,
            points: reprojectedPoints
        )

        XCTAssertEqual(initialKey.routeID, reprojectedKey.routeID)
        XCTAssertEqual(initialKey.firstSequence, reprojectedKey.firstSequence)
        XCTAssertEqual(initialKey.lastSequence, reprojectedKey.lastSequence)
        XCTAssertEqual(initialKey.pointCount, reprojectedKey.pointCount)
        XCTAssertNotEqual(initialPoints[1], reprojectedPoints[1])
        XCTAssertNotEqual(initialKey, reprojectedKey)
    }

    @MainActor
    func testRideMapPathKeyTracksRustBoundedContextRoutes() {
        let contextRoute = MobileRideMapHistoryContextRoute(
            rideID: "context-1",
            projection: MobileRideMapRouteProjection(
                points: [
                    MobileRideMapRouteDisplayPoint(
                        sequence: 4,
                        segmentId: 0,
                        latitudeDegrees: 39.7,
                        longitudeDegrees: -104.9,
                        privacyClass: .precise
                    ),
                ],
                segments: [],
                sourcePointCount: 1,
                sourceSegmentCount: 1,
                candidatePointCount: 1,
                candidateSegmentCount: 1,
                displayedSegmentCount: 1,
                backgroundGapCount: 0,
                canonicalStartSequence: 4,
                canonicalEndSequence: 4,
                canonicalStartVisible: true,
                canonicalEndVisible: true
            )
        )
        let withoutContext = RideMapCanvasView.pathKey(
            routeID: "selected",
            projectionVersion: 1,
            points: []
        )
        let withContext = RideMapCanvasView.pathKey(
            routeID: "selected",
            projectionVersion: 1,
            points: [],
            contextRoutes: [contextRoute]
        )

        XCTAssertNotEqual(withoutContext, withContext)
        XCTAssertEqual(withContext.contextRouteMetadata.count, 1)
        XCTAssertEqual(withContext.contextRouteMetadata.first?.routeID, "context-1")
    }

    @MainActor
    func testRideMapRoutePresenceUsesRecordedCountWhenViewportIsEmpty() {
        XCTAssertTrue(
            RideMapRouteTruthView.routeExists(
                recordedPointCount: 7,
                displayedPointCount: 0
            )
        )
        XCTAssertFalse(
            RideMapRouteTruthView.routeExists(
                recordedPointCount: 0,
                displayedPointCount: 7
            )
        )
        XCTAssertTrue(
            RideMapRouteTruthView.routeExists(
                recordedPointCount: nil,
                displayedPointCount: 1
            )
        )
        XCTAssertFalse(
            RideMapRouteTruthView.routeExists(
                recordedPointCount: nil,
                displayedPointCount: 0
            )
        )
    }

    @MainActor
    func testRideHistoryDetailRouteIdentityDoesNotDependOnViewportTruncation() {
        XCTAssertEqual(
            RideMapHistoryDetailView.routeID(for: "ride-1"),
            RideMapHistoryDetailView.routeID(for: "ride-1")
        )
    }

    @MainActor
    func testRideMapMarkerPolicyScalesForAccessibilityText() {
        let coordinate = CLLocationCoordinate2D(latitude: 39.7392, longitude: -104.9903)
        let regular = RideMapCanvasView.markerOffsets(for: [coordinate, coordinate], dynamicTypeSize: .large)
        let accessibility = RideMapCanvasView.markerOffsets(
            for: [coordinate, coordinate],
            dynamicTypeSize: .accessibility1
        )

        XCTAssertEqual(RideMapCanvasView.markerTitleLineLimit(for: .large), 1)
        XCTAssertEqual(RideMapCanvasView.markerTitleLineLimit(for: .accessibility1), 2)
        XCTAssertGreaterThan(abs(accessibility.start.width), abs(regular.start.width))
    }

    @MainActor
    func testRideMapCameraProjectsFiniteViewportBounds() {
        let bounds = RideMapCanvasView.geoBounds(
            for: MKCoordinateRegion(
                center: CLLocationCoordinate2D(latitude: 40, longitude: -105),
                span: MKCoordinateSpan(latitudeDelta: 2, longitudeDelta: 4)
            )
        )

        XCTAssertEqual(bounds?.minimumLatitudeDegrees, 39)
        XCTAssertEqual(bounds?.maximumLatitudeDegrees, 41)
        XCTAssertEqual(bounds?.minimumLongitudeDegrees, -107)
        XCTAssertEqual(bounds?.maximumLongitudeDegrees, -103)
    }

    @MainActor
    func testRideMapCameraPreservesAntimeridianViewportBounds() {
        let bounds = RideMapCanvasView.geoBounds(
            for: MKCoordinateRegion(
                center: CLLocationCoordinate2D(latitude: 0, longitude: 179),
                span: MKCoordinateSpan(latitudeDelta: 2, longitudeDelta: 6)
            )
        )

        XCTAssertEqual(bounds?.minimumLongitudeDegrees, 176)
        XCTAssertEqual(bounds?.maximumLongitudeDegrees, -178)
    }

    @MainActor
    func testRideMapOnlyUsesRecordedBoundsForTerminalStates() {
        XCTAssertFalse(RideMapLiveContentView.showsRecordedBounds(for: .recording))
        XCTAssertFalse(RideMapLiveContentView.showsRecordedBounds(for: .paused))
        XCTAssertTrue(RideMapLiveContentView.showsRecordedBounds(for: .stopped))
        XCTAssertTrue(RideMapLiveContentView.showsRecordedBounds(for: .saved))
        XCTAssertTrue(RideMapLiveContentView.showsRecordedBounds(for: .discarded))
    }

    @MainActor
    func testRideMapOnlyAnnotatesDisplayedCanonicalEndpoints() {
        let points = [
            MobileRideMapRouteDisplayPoint(
                sequence: 4,
                segmentId: 0,
                latitudeDegrees: 40,
                longitudeDegrees: -105,
                privacyClass: .precise
            ),
            MobileRideMapRouteDisplayPoint(
                sequence: 8,
                segmentId: 0,
                latitudeDegrees: 40.001,
                longitudeDegrees: -105.001,
                privacyClass: .precise
            )
        ]

        XCTAssertNil(
            RideMapCanvasView.canonicalEndpointPoint(
                in: points,
                sequence: 0,
                isVisible: true
            )
        )
        XCTAssertNil(
            RideMapCanvasView.canonicalEndpointPoint(
                in: points,
                sequence: 4,
                isVisible: false
            )
        )
        XCTAssertEqual(
            RideMapCanvasView.canonicalEndpointPoint(
                in: points,
                sequence: 8,
                isVisible: true
            )?.sequence,
            8
        )
    }

    @MainActor
    func testRideMapStylesOnlyBackgroundGapSegmentsAsGaps() {
        XCTAssertFalse(MobileRideMapSegmentStartReason.initial.isBackgroundGap)
        XCTAssertFalse(MobileRideMapSegmentStartReason.resume.isBackgroundGap)
        XCTAssertTrue(MobileRideMapSegmentStartReason.backgroundGap.isBackgroundGap)
        XCTAssertFalse(MobileRideMapSegmentStartReason.importBoundary.isBackgroundGap)
        XCTAssertFalse(MobileRideMapSegmentStartReason.unknown.isBackgroundGap)
    }

    @MainActor
    func testRideMapExposesSingletonSegmentsForMapAndAccessibility() {
        let points = [
            MobileRideMapRouteDisplayPoint(
                sequence: 0,
                segmentId: 0,
                latitudeDegrees: 40,
                longitudeDegrees: -105,
                privacyClass: .precise
            ),
            MobileRideMapRouteDisplayPoint(
                sequence: 1,
                segmentId: 1,
                latitudeDegrees: 40.001,
                longitudeDegrees: -105.001,
                privacyClass: .precise
            ),
            MobileRideMapRouteDisplayPoint(
                sequence: 2,
                segmentId: 1,
                latitudeDegrees: 40.002,
                longitudeDegrees: -105.002,
                privacyClass: .precise
            ),
            MobileRideMapRouteDisplayPoint(
                sequence: 3,
                segmentId: 2,
                latitudeDegrees: 40.003,
                longitudeDegrees: -105.003,
                privacyClass: .precise
            )
        ]
        let segments = [
            MobileRideMapSegmentDisplayMetadata(
                segmentId: 0,
                startReason: .initial,
                visiblePointCount: 1,
                firstVisibleSequence: 0,
                lastVisibleSequence: 0
            ),
            MobileRideMapSegmentDisplayMetadata(
                segmentId: 1,
                startReason: .resume,
                visiblePointCount: 2,
                firstVisibleSequence: 1,
                lastVisibleSequence: 2
            ),
            MobileRideMapSegmentDisplayMetadata(
                segmentId: 2,
                startReason: .backgroundGap,
                visiblePointCount: 1,
                firstVisibleSequence: 3,
                lastVisibleSequence: 3
            )
        ]

        XCTAssertEqual(
            RideMapCanvasView.retainedSingletonSegmentIDs(in: points, segments: segments),
            Set([UInt64(0), UInt64(2)])
        )
        XCTAssertEqual(MobileRideMapSegmentDisplayMetadata.visibleBackgroundGapCount(for: segments), 1)
        XCTAssertTrue(segments[2].isRetainedSingleton)
        XCTAssertFalse(segments[2].isCanonicalSingleton)
        XCTAssertTrue(segments[2].isBackgroundGap)
        XCTAssertFalse(segments[1].isRetainedSingleton)
        XCTAssertFalse(segments[1].isBackgroundGap)
        XCTAssertEqual(
            segments[2].startReason.retainedSingletonAccessibilityLabel,
            "Displayed route point; background location gap segment has one retained display point"
        )

        let canonicalSingleton = MobileRideMapSegmentDisplayMetadata(
            segmentId: 3,
            startReason: .importBoundary,
            visiblePointCount: 1,
            canonicalPointCount: 1,
            firstVisibleSequence: 4,
            lastVisibleSequence: 4
        )
        XCTAssertTrue(canonicalSingleton.isRetainedSingleton)
        XCTAssertTrue(canonicalSingleton.isCanonicalSingleton)
        XCTAssertEqual(
            canonicalSingleton.singletonAccessibilityLabel,
            "Imported route segment; segment contains one recorded point"
        )

        let lodSingleton = MobileRideMapSegmentDisplayMetadata(
            segmentId: 4,
            startReason: .backgroundGap,
            visiblePointCount: 1,
            canonicalPointCount: 12,
            firstVisibleSequence: 5,
            lastVisibleSequence: 5
        )
        XCTAssertEqual(
            lodSingleton.singletonAccessibilityLabel,
            "Background location gap; one display point represents 12 recorded points"
        )
        XCTAssertNil(segments[1].singletonAccessibilityLabel)
    }

    @MainActor
    func testRideMapPersistenceWarningComesOnlyFromStorageAvailability() {
        XCTAssertTrue(
            RideMapLiveContentView.showsPersistenceWarning(for: .storageUnavailable)
        )
        XCTAssertFalse(
            RideMapLiveContentView.showsPersistenceWarning(for: .ready)
        )
        XCTAssertFalse(
            RideMapLiveContentView.showsPersistenceWarning(for: .denied)
        )
    }

    @MainActor
    func testRideHistoryVehicleOptionsRemainAfterAFilteredOrEmptyPage() {
        let cached = CutoutAppModel.mergeRideMapHistoryVehicleIdentities(
            existing: ["vehicle-old"],
            incoming: ["vehicle-new", "vehicle-old"]
        )

        XCTAssertEqual(cached, ["vehicle-new", "vehicle-old"])
        XCTAssertEqual(
            CutoutAppModel.mergeRideMapHistoryVehicleIdentities(
                existing: cached,
                incoming: []
            ),
            cached
        )
    }

    @MainActor
    func testRideMapPresentationStateStoresIndependentSurfaceFlags() {
        let presentation = RideMapPresentationState()

        presentation.followsLatestPoint = false
        presentation.liveIsApplyingCamera = true
        presentation.historyIsApplyingCamera = true
        presentation.detailIsApplyingCamera = true

        XCTAssertFalse(presentation.followsLatestPoint)
        XCTAssertTrue(presentation.liveIsApplyingCamera)
        XCTAssertTrue(presentation.historyIsApplyingCamera)
        XCTAssertTrue(presentation.detailIsApplyingCamera)
    }

    @MainActor
    func testRideDetailResolvesPersistedVehicleNamesBeforeIdentityFallback() {
        XCTAssertEqual(
            RideMapHistoryDetailView.resolvedVehicleLabel(
                associatedVehicle: "corebluetooth-1",
                candidateVehicle: "candidate-1",
                resolve: { $0 == "corebluetooth-1" ? "NF2557" : nil },
                fallback: "GPS-only ride"
            ),
            "NF2557"
        )
        XCTAssertEqual(
            RideMapHistoryDetailView.resolvedVehicleLabel(
                associatedVehicle: nil,
                candidateVehicle: "candidate-1",
                resolve: { _ in nil },
                fallback: "GPS-only ride"
            ),
            "Vehicle name unavailable"
        )
    }

    @MainActor
    func testRideHistoryVehicleFallbackDoesNotExposePlatformIdentity() {
        XCTAssertEqual(
            RideMapHistoryContentView.resolvedVehicleLabel(
                identity: "corebluetooth-1",
                currentIdentity: nil,
                currentName: nil,
                resolve: { _ in nil }
            ),
            "Vehicle name unavailable"
        )
        XCTAssertEqual(
            RideMapHistoryContentView.resolvedVehicleLabel(
                identity: "corebluetooth-1",
                currentIdentity: "corebluetooth-1",
                currentName: "NF2557",
                resolve: { _ in nil }
            ),
            "NF2557"
        )
    }

    @MainActor
    func testRideHistoryVehicleOptionsDeduplicateRepeatedIdentitiesWithoutCollapsingNames() {
        XCTAssertEqual(
            RideMapHistoryContentView.uniqueVehicleIdentities([
                "corebluetooth-a",
                "corebluetooth-a",
                "corebluetooth-b"
            ]),
            ["corebluetooth-a", "corebluetooth-b"]
        )
    }

    @MainActor
    func testRideHistoryFiltersRemainActiveAndClearableWhenResultsAreEmpty() {
        XCTAssertTrue(
            RideMapHistoryContentView.hasActiveFilters(
                searchText: "NF2557",
                dateFilter: .last30Days,
                vehicleFilter: nil
            )
        )
        XCTAssertTrue(
            RideMapHistoryContentView.hasActiveFilters(
                searchText: "",
                dateFilter: .allTime,
                vehicleFilter: nil
            )
        )
        XCTAssertTrue(
            RideMapHistoryContentView.hasActiveFilters(
                searchText: "",
                dateFilter: .last30Days,
                vehicleFilter: "vehicle-1"
            )
        )
        XCTAssertFalse(
            RideMapHistoryContentView.hasActiveFilters(
                searchText: "",
                dateFilter: .last30Days,
                vehicleFilter: nil
            )
        )
        XCTAssertFalse(
            RideMapHistoryContentView.hasActiveFilters(
                searchText: "  \n",
                dateFilter: .last30Days,
                vehicleFilter: nil
            )
        )
    }

    @MainActor
    func testRideHistorySearchTaskDebouncesOnlyNonemptyQueries() {
        XCTAssertEqual(
            RideMapHistoryContentView.searchDebounce(for: ""),
            .zero
        )
        XCTAssertEqual(
            RideMapHistoryContentView.searchDebounce(for: "NF2557"),
            .milliseconds(250)
        )
        XCTAssertEqual(
            RideMapHistoryContentView.searchDebounce(for: "  \n"),
            .zero
        )
        XCTAssertEqual(
            RideMapHistoryContentView.normalizedSearchText("  NF2557  "),
            "NF2557"
        )
    }

    @MainActor
    func testRideHistoryRouteLoadingIsVisibleAfterThePageAlreadyLoaded() {
        XCTAssertTrue(
            RideMapHistoryContentView.selectedRouteIsLoading(
                routeLoading: true,
                hasSelectedRide: true
            )
        )
        XCTAssertFalse(
            RideMapHistoryContentView.selectedRouteIsLoading(
                routeLoading: true,
                hasSelectedRide: false
            )
        )
        XCTAssertFalse(
            RideMapHistoryContentView.selectedRouteIsLoading(
                routeLoading: false,
                hasSelectedRide: true
            )
        )
    }

    @MainActor
    func testRideHistoryDetailDoesNotReselectAnAlreadySelectedRide() {
        XCTAssertFalse(
            RideMapHistoryDetailView.shouldSelectHistory(
                initialHistoryID: "ride-1",
                selectedHistoryID: "ride-1",
                availableHistoryIDs: ["ride-1", "ride-2"]
            )
        )
        XCTAssertTrue(
            RideMapHistoryDetailView.shouldSelectHistory(
                initialHistoryID: "ride-1",
                selectedHistoryID: "ride-2",
                availableHistoryIDs: ["ride-1", "ride-2"]
            )
        )
        XCTAssertFalse(
            RideMapHistoryDetailView.shouldSelectHistory(
                initialHistoryID: "ride-1",
                selectedHistoryID: nil,
                availableHistoryIDs: ["ride-2"]
            )
        )
    }

    @MainActor
    func testRideDetailShowsAverageSpeedWhenDistanceAndDurationExist() {
        XCTAssertEqual(
            RideMapHistoryDetailView.averageSpeedText(
                millimetresPerSecond: 2_980
            ),
            "6.7 mph"
        )
        XCTAssertEqual(
            RideMapHistoryDetailView.averageSpeedText(millimetresPerSecond: nil),
            "N/A"
        )
    }

    @MainActor
    func testRideDetailMapHeightStaysWithinAvailableViewport() {
        XCTAssertEqual(RideMapHistoryDetailView.mapHeight(for: 0), 0)
        XCTAssertEqual(RideMapHistoryDetailView.mapHeight(for: 200), 200)
        XCTAssertEqual(RideMapHistoryDetailView.mapHeight(for: 500), 290)
        XCTAssertEqual(RideMapHistoryDetailView.mapHeight(for: 1_000), 520)
    }

    @MainActor
    func testRideHistoryPointCountUsesSingularAndPluralCopy() {
        XCTAssertEqual(RideMapHistoryListView.pointCountText(1), "1 point")
        XCTAssertEqual(RideMapHistoryListView.pointCountText(0), "0 points")
        XCTAssertEqual(RideMapHistoryListView.pointCountText(2), "2 points")
    }

    @MainActor
    func testRideHistorySelectionHasAnAccessibleValue() {
        XCTAssertEqual(RideMapHistoryListView.selectionAccessibilityValue(isSelected: true), "Selected")
        XCTAssertEqual(RideMapHistoryListView.selectionAccessibilityValue(isSelected: false), "Not selected")
    }

    func testRouteOwnsTheSameTabsUsedByWindowCommandsAndContent() {
        XCTAssertTrue(CutoutAppRoute.devicePicker.navigationTabs.isEmpty)
        XCTAssertTrue(CutoutAppRoute.capture.navigationTabs.isEmpty)
        XCTAssertEqual(CutoutAppRoute.eucRide.navigationTabs.map(\.id), [.ride, .pack, .map, .tune])
        XCTAssertEqual(CutoutAppRoute.vescRide.navigationTabs.map(\.id), [.ride, .debug, .map, .logs])
        XCTAssertTrue(
            CutoutAppRoute.eucPack(.bmsOverview).navigationTabs.first(where: { $0.id == .pack })?.isSelected == true
        )
        XCTAssertTrue(
            CutoutAppRoute.eucPack(.root).navigationTabs.first(where: { $0.id == .pack })?.isSelected == true
        )
        XCTAssertTrue(
            CutoutAppRoute.vescDebug.navigationTabs.first(where: { $0.id == .debug })?.isSelected == true
        )
        XCTAssertEqual(CutoutAppRoute.route(forNavigationTarget: .vescRide), .vescRide)
        XCTAssertEqual(CutoutAppRoute.route(forNavigationTarget: .screen(.bmsOverview)), .eucPack(.bmsOverview))
        XCTAssertEqual(CutoutNavigationCommands.shortcut(for: .ride), "1")
        XCTAssertEqual(CutoutNavigationCommands.shortcut(for: .pack), "2")
        XCTAssertEqual(CutoutNavigationCommands.shortcut(for: .map), "3")
        XCTAssertEqual(CutoutNavigationCommands.shortcut(for: .tune), "4")
        XCTAssertEqual(CutoutNavigationCommands.shortcut(for: .debug), "5")
        XCTAssertEqual(CutoutNavigationCommands.shortcut(for: .logs), "6")
    }

    func testNativeNavigationIncludesTheSharedMapDestination() {
        XCTAssertEqual(CutoutAppRoute.eucRide.availableNavigationTabs.map(\.id), [.ride, .pack, .map])
        XCTAssertEqual(CutoutAppRoute.eucPack(.bmsOverview).availableNavigationTabs.map(\.id), [.ride, .pack, .map])
        XCTAssertEqual(CutoutAppRoute.vescRide.availableNavigationTabs.map(\.id), [.ride, .debug, .map])
        XCTAssertEqual(CutoutAppRoute.vescDebug.availableNavigationTabs.map(\.id), [.ride, .debug, .map])
        XCTAssertTrue(CutoutAppRoute.devicePicker.availableNavigationTabs.isEmpty)
        XCTAssertTrue(CutoutAppRoute.capture.availableNavigationTabs.isEmpty)
        XCTAssertTrue(CutoutAppRoute.rideMap.availableNavigationTabs.isEmpty)
    }

    func testConnectedMapRouteKeepsTheRideNavigationTabsVisible() {
        let eucTabs = CutoutAppRoute.rideMap
            .availableNavigationTabs(for: .electricUnicycle)
        XCTAssertEqual(eucTabs.map(\.id), [.ride, .pack, .map])
        XCTAssertEqual(eucTabs.filter(\.isSelected).map(\.id), [.map])

        let vescTabs = CutoutAppRoute.rideMapDetail(rideID: "ride-1")
            .availableNavigationTabs(for: .vescOnewheel)
        XCTAssertTrue(vescTabs.isEmpty)
        XCTAssertTrue(CutoutAppRoute.rideMap.availableNavigationTabs(for: nil).isEmpty)
    }

    func testNestedPackRouteSurvivesSharedTabRendering() {
        let nestedPackRoute = CutoutAppRoute.eucPack(.bmsCellDetail(7))
        let tabs = nestedPackRoute.availableNavigationTabs

        XCTAssertEqual(nestedPackRoute.destination(for: tabs[0]), .eucRide)
        XCTAssertEqual(nestedPackRoute.destination(for: tabs[1]), nestedPackRoute)
        XCTAssertEqual(CutoutAppRoute.vescDebug.destination(for: CutoutAppRoute.vescDebug.availableNavigationTabs[1]), .vescDebug)
    }

    func testMapTabUsesTheSharedRideMapDestination() {
        let mapTab = CutoutAppRoute.eucRide.navigationTabs[2]

        XCTAssertEqual(CutoutAppRoute.eucRide.destination(for: mapTab), .rideMap)
    }

    @MainActor
    func testConnectedMapUsesSessionChromeWhileHomeMapKeepsItsOwnHeader() {
        XCTAssertTrue(ContentView.usesConnectedMapShell(for: .rideMap, isConnected: true))
        XCTAssertFalse(ContentView.usesConnectedMapShell(for: .rideMap, isConnected: false))
        XCTAssertFalse(
            ContentView.usesConnectedMapShell(
                for: .rideMapDetail(rideID: "ride-1"),
                isConnected: true
            )
        )
    }

    func testOnlyLivePhaseOpensTheRideSurface() {
        XCTAssertFalse(SessionConnectionPhase.connecting(model: .falcon).opensRideScreen)
        XCTAssertFalse(SessionConnectionPhase.discoveringServices.opensRideScreen)
        XCTAssertFalse(SessionConnectionPhase.subscribing.opensRideScreen)
        XCTAssertTrue(SessionConnectionPhase.live.opensRideScreen)
        XCTAssertFalse(SessionConnectionPhase.starting.opensRideScreen)
        XCTAssertFalse(SessionConnectionPhase.scanning.opensRideScreen)
    }

    func testPickerStatusNeverShowsScanningWhenBluetoothIsUnavailableOrScanStateIsMissing() {
        XCTAssertEqual(
            DevicePickerConnectionPresentation(
                scanState: nil,
                phase: .bluetoothUnavailable(rawState: 4)
            ),
            .init(title: "Bluetooth unavailable", showsActivity: false, symbolName: "bolt.slash.fill")
        )
        XCTAssertEqual(
            DevicePickerConnectionPresentation(scanState: nil, phase: .starting),
            .init(title: "Starting Bluetooth…", showsActivity: false, symbolName: "bolt.horizontal.circle")
        )
        XCTAssertEqual(
            DevicePickerConnectionPresentation(
                scanState: .scanning,
                phase: .scanning
            ),
            .init(title: "Scanning Bluetooth", showsActivity: true)
        )
    }

    func testPickerStatusUsesTypedSymbolsForNonScanningStates() {
        XCTAssertEqual(
            DevicePickerConnectionPresentation(
                scanState: nil,
                phase: .bluetoothUnavailable(rawState: 4)
            ).symbolName,
            "bolt.slash.fill"
        )
        XCTAssertEqual(
            DevicePickerConnectionPresentation(
                scanState: nil,
                phase: .failed(.connectFailed("timed out"))
            ).symbolName,
            "xmark.octagon.fill"
        )
        XCTAssertEqual(
            DevicePickerConnectionPresentation(scanState: nil, phase: .live).symbolName,
            "checkmark.circle.fill"
        )
    }

    func testPickerPermissionDenialUsesDistinctVisualAndSpokenRecovery() {
        XCTAssertEqual(
            DevicePickerConnectionPresentation(scanState: nil, phase: .bluetoothPermissionDenied),
            .init(
                title: "Bluetooth permission denied",
                showsActivity: false,
                symbolName: "lock.slash.fill"
            )
        )
        XCTAssertEqual(
            SessionConnectionPhase.bluetoothPermissionDenied.accessibilityAnnouncement,
            "Bluetooth permission denied. Allow Bluetooth access in Settings to scan for rides."
        )
    }

    func testConnectionAnnouncementsCoverMeaningfulTransitionsWithoutChatter() {
        XCTAssertNil(SessionConnectionPhase.starting.accessibilityAnnouncement)
        XCTAssertNil(SessionConnectionPhase.scanning.accessibilityAnnouncement)
        XCTAssertNil(SessionConnectionPhase.discoveringServices.accessibilityAnnouncement)
        XCTAssertNil(SessionConnectionPhase.subscribing.accessibilityAnnouncement)
        XCTAssertEqual(
            SessionConnectionPhase.bluetoothUnavailable(rawState: 4).accessibilityAnnouncement,
            "Bluetooth unavailable. Turn on Bluetooth to reconnect."
        )
        XCTAssertEqual(
            SessionConnectionPhase.connecting(model: .falcon).accessibilityAnnouncement,
            "Connecting to Falcon."
        )
        XCTAssertEqual(SessionConnectionPhase.live.accessibilityAnnouncement, "Connected.")
        XCTAssertEqual(
            SessionConnectionPhase.failed(.connectFailed("timed out")).accessibilityAnnouncement,
            "Connection failed. Choose a device to try again. Connect failed: timed out"
        )
        XCTAssertEqual(
            SessionConnectionPhase.failed(.identificationFailed(.timedOut)).accessibilityAnnouncement,
            "Connection failed. Choose a device to try again. Device identification timed out"
        )
        XCTAssertEqual(
            SessionConnectionPhase.failed(.identificationFailed(.malformedResponse)).accessibilityAnnouncement,
            "Connection failed. Choose a device to try again. Device returned an invalid identification response"
        )
        XCTAssertEqual(
            SessionConnectionPhase.failed(.identificationFailed(.conflictingEvidence)).accessibilityAnnouncement,
            "Connection failed. Choose a device to try again. Device identification found conflicting evidence"
        )
        XCTAssertEqual(
            SessionConnectionPhase.failed(.identificationFailed(.unsupported)).accessibilityAnnouncement,
            "Connection failed. Choose a device to try again. Device does not support this identification probe"
        )
    }

    func testConnectionAnnouncementsSpeakRejectedPickerActionOnlyOnce() {
        var announcements = ConnectionAccessibilityAnnouncements()
        announcements.beginUserInitiatedAttempt()
        let rejectedAction = DevicePickerScanState.failed("Device is no longer available")

        XCTAssertEqual(
            announcements.next(for: rejectedAction),
            "Device is no longer available"
        )
        XCTAssertNil(announcements.next(for: .failed(.connectFailed("timed out"))))
    }

    func testReconnectLoopAnnouncesConnectionLossOnlyOnce() {
        var announcements = ConnectionAccessibilityAnnouncements()
        let messages = [
            SessionConnectionPhase.discoveringServices,
            .subscribing,
            .failed(.connectFailed("timed out")),
            .scanning,
            .discoveringServices,
            .failed(.connectFailed("still timed out")),
        ].compactMap { announcements.next(for: $0) }

        XCTAssertEqual(messages, ["Connection failed. Choose a device to try again. Connect failed: timed out"])
        XCTAssertEqual(announcements.next(for: .live), "Connected.")
        XCTAssertEqual(
            announcements.next(for: .failed(.connectFailed("lost after connecting"))),
            "Connection failed. Choose a device to try again. Connect failed: lost after connecting"
        )

        announcements.beginUserInitiatedAttempt()
        XCTAssertEqual(
            announcements.next(for: .failed(.connectFailed("timed out again"))),
            "Connection failed. Choose a device to try again. Connect failed: timed out again"
        )
    }

    func testReconnectStateAnnouncesRetryOnlyOnce() {
        let selection = ConnectionSelection(
            platformIdentifier: "vesc-1234",
            title: "VESC",
            route: .vescOnewheel
        )
        let retry = SessionConnectionRetry(
            platformIdentifier: selection.platformIdentifier,
            attempt: 1,
            deadline: MonotonicMilliseconds(0),
            failure: .connectFailed("timed out")
        )
        var announcements = ConnectionAccessibilityAnnouncements()

        XCTAssertEqual(
            announcements.next(for: .retrying(selection, retry: retry)),
            "Connection lost. Retrying connection."
        )
        XCTAssertNil(announcements.next(for: .retrying(selection, retry: retry)))
    }

    func testReconnectStateAnnouncesAgainAfterConnectionRestores() {
        let selection = ConnectionSelection(
            platformIdentifier: "vesc-1234",
            title: "VESC",
            route: .vescOnewheel
        )
        let retry = SessionConnectionRetry(
            platformIdentifier: selection.platformIdentifier,
            attempt: 1,
            deadline: MonotonicMilliseconds(0),
            failure: .connectFailed("timed out")
        )
        var announcements = ConnectionAccessibilityAnnouncements()

        XCTAssertEqual(
            announcements.next(for: .retrying(selection, retry: retry)),
            "Connection lost. Retrying connection."
        )
        XCTAssertNil(announcements.next(for: .connected(selection)))
        XCTAssertEqual(
            announcements.next(for: .retrying(selection, retry: retry)),
            "Connection lost. Retrying connection."
        )
    }

    func testSafetyAnnouncementCopyResolvesFromTheAppCatalog() {
        XCTAssertEqual(
            localizedAppText("accessibility.euc_warning.caution"),
            "Caution. Riding headroom is getting low."
        )
        XCTAssertEqual(
            localizedAppText("accessibility.euc_warning.reduce_acceleration"),
            "Warning. Reduce acceleration."
        )
        XCTAssertEqual(
            localizedAppText("accessibility.euc_warning.limp_home"),
            "Critical warning. Slow down and stop safely."
        )
        XCTAssertEqual(
            localizedAppText("accessibility.vesc_warning.duty_pushback"),
            "Warning. Pushback soon. Reduce acceleration."
        )
        XCTAssertEqual(localizedAppText("vesc.warning.wheelslip"), "Wheel slip")
        let stopCopy: [(String, String)] = [
            ("vesc.stop.pitch", "Stopped: pitch"),
            ("vesc.stop.roll", "Stopped: roll"),
            ("vesc.stop.switch_half", "Half-footpad stop"),
            ("vesc.stop.switch_full", "Footpad stop"),
            ("vesc.stop.reverse", "Reverse stop"),
            ("vesc.stop.quick_stop", "Quick stop"),
            ("vesc.stop.detail", "Board stopped balancing. Re-engage only when safe."),
        ]
        for (key, expected) in stopCopy {
            XCTAssertEqual(localizedAppText(key), expected)
        }
        XCTAssertEqual(
            localizedAppText("accessibility.bms_alert.warning"),
            "Battery warning. Check BMS details."
        )
        XCTAssertEqual(
            localizedAppText("accessibility.bms_alert.critical"),
            "Critical battery warning. Check BMS details."
        )
    }

    func testLiveActivityLifecycleErrorsHaveTypedAnnouncements() {
        XCTAssertEqual(
            LiveActivityRideLifecycleError.authorizationDenied.accessibilityAnnouncement,
            "Live Activity permission is unavailable."
        )
        XCTAssertEqual(
            LiveActivityRideLifecycleError.requestFailed.accessibilityAnnouncement,
            "Couldn't start the Live Activity."
        )
        XCTAssertEqual(
            LiveActivityRideLifecycleError.activityUnavailable.accessibilityAnnouncement,
            "The Live Activity is unavailable."
        )
    }

    func testSafetyAnnouncementsCoverTypedEscalationsWithoutTelemetryChatter() {
        XCTAssertNil(EucRideWarningSeverity.normal.accessibilityAnnouncement)
        XCTAssertEqual(
            EucRideWarningSeverity.caution.accessibilityAnnouncement,
            "Caution. Riding headroom is getting low."
        )
        XCTAssertEqual(
            EucRideWarningSeverity.reduceAcceleration.accessibilityAnnouncement,
            "Warning. Reduce acceleration."
        )
        XCTAssertEqual(
            EucRideWarningSeverity.limpHome.accessibilityAnnouncement,
            "Critical warning. Slow down and stop safely."
        )
        XCTAssertNil(EucRideWarningSeverity.unavailable.accessibilityAnnouncement)
        XCTAssertNil(EucRideWarningSeverity.failed.accessibilityAnnouncement)

        XCTAssertNil(VescRideWarning.none.accessibilityAnnouncement)
        let vescAnnouncements: [(VescRideWarning, String)] = [
            (.lowVoltage, "Warning. Controller voltage is low. Slow down and stop safely."),
            (.highVoltage, "Warning. Controller voltage is high. Stop safely."),
            (.mosfetTemperature, "Warning. Controller temperature is high. Stop safely and let it cool."),
            (.motorTemperature, "Warning. Motor temperature is high. Stop safely and let it cool."),
            (.current, "Warning. Current limit reached. Reduce acceleration."),
            (.dutyPushback, "Warning. Pushback soon. Reduce acceleration."),
            (.temperaturePushback, "Warning. Temperature pushback. Stop safely and let the board cool."),
            (.wheelslip, "Warning. Wheel slip detected. Reduce acceleration and regain control."),
            (.sensors, "Warning. Stop safely and check the board sensors."),
            (.lowBattery, "Warning. Battery is low. Slow down and stop safely."),
            (.error, "Critical warning. Controller error. Stop safely."),
        ]
        for (warning, announcement) in vescAnnouncements {
            XCTAssertEqual(warning.accessibilityAnnouncement, announcement)
        }
        XCTAssertNil(VescRideWarning.unknown.accessibilityAnnouncement)

        let stopAnnouncements: [(VescRideStopReason, String)] = [
            (.pitch, "Board stopped balancing because of pitch. Re-engage only when safe."),
            (.roll, "Board stopped balancing because of roll. Re-engage only when safe."),
            (.switchHalf, "Board stopped balancing because half the footpad released. Re-engage only when safe."),
            (.switchFull, "Board stopped balancing because the footpad released. Re-engage only when safe."),
            (.reverse, "Board stopped with reverse-stop. Re-engage only when safe."),
            (.quickStop, "Board quick-stopped. Re-engage only when safe."),
        ]
        XCTAssertNil(VescRideStopReason.none.accessibilityAnnouncement)
        for (reason, announcement) in stopAnnouncements {
            XCTAssertEqual(reason.accessibilityAnnouncement, announcement)
        }
    }

    func testBmsAnnouncementUsesHighestTypedGroupSeverity() {
        let snapshot = BmsSnapshot(
            topology: BmsTopology(
                layoutLabel: "test",
                seriesGroupCount: 3,
                parallelCount: 1,
                packCount: 1,
                bmsCount: 1,
                confidence: .verified
            ),
            groups: [
                BmsGroupSnapshot(index: 0, alertLevel: .nominal),
                BmsGroupSnapshot(index: 1, alertLevel: .critical),
                BmsGroupSnapshot(index: 2, alertLevel: .warning),
            ]
        )

        XCTAssertEqual(snapshot.accessibilityAlertLevel, .critical)
        XCTAssertEqual(
            snapshot.accessibilityAlertLevel.accessibilityAnnouncement,
            "Critical battery warning. Check BMS details."
        )
        XCTAssertNil(BmsAlertLevel.nominal.accessibilityAnnouncement)
        XCTAssertNil(BmsAlertLevel.unknown.accessibilityAnnouncement)
    }

}
