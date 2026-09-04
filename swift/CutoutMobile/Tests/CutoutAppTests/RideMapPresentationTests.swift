import MapKit
@testable import CutoutMobile
import XCTest

@testable import CutoutApp

@MainActor
final class RideMapPresentationTests: XCTestCase {
    private func point(
        sequence: UInt64,
        segmentId: UInt64 = 0,
        latitude: Double = 40,
        longitude: Double = -105
    ) -> MobileRideMapRouteDisplayPoint {
        MobileRideMapRouteDisplayPoint(
            sequence: sequence,
            segmentId: segmentId,
            latitudeDegrees: latitude,
            longitudeDegrees: longitude,
            privacyClass: .precise
        )
    }

    func testLiveMapOnlyShowsRecordedBoundsForPersistedTerminalStates() {
        let summary = MobileRideMapSummaryDto(
            pointCount: 0,
            distanceMeters: 0,
            durationMilliseconds: 0
        )
        let snapshot = { (state: MobileRideMapStateDto, available: Bool) in
            MobileRideMapSnapshotDto(
                rideID: "ride",
                state: state,
                summary: summary,
                segmentCount: 0,
                associatedVehicle: nil,
                recordedBoundsAvailable: available
            )
        }

        XCTAssertFalse(snapshot(.draft, false).recordedBoundsAvailable)
        XCTAssertFalse(snapshot(.active, false).recordedBoundsAvailable)
        XCTAssertFalse(snapshot(.paused, false).recordedBoundsAvailable)
        XCTAssertTrue(snapshot(.stopped, true).recordedBoundsAvailable)
        XCTAssertTrue(snapshot(.interrupted, true).recordedBoundsAvailable)
        XCTAssertTrue(snapshot(.saved, true).recordedBoundsAvailable)
        XCTAssertTrue(snapshot(.discarded, true).recordedBoundsAvailable)
        XCTAssertTrue(snapshot(.imported, true).recordedBoundsAvailable)
    }

    func testHistoryPresentationUsesSingularAndPluralPointStrings() {
        XCTAssertEqual(RideMapHistoryListView.pointCountText(1), "1 point")
        XCTAssertEqual(RideMapHistoryListView.pointCountText(2), "2 points")
    }

    func testHistorySelectionAccessibilityTextIsLocalized() {
        XCTAssertEqual(
            RideMapHistoryListView.selectionAccessibilityValue(isSelected: true),
            "Selected"
        )
        XCTAssertEqual(
            RideMapHistoryListView.selectionAccessibilityValue(isSelected: false),
            "Not selected"
        )
    }

    func testHistoryDetailUsesTheCurrentSelectionForRouteIdentity() {
        XCTAssertEqual(
            RideMapHistoryDetailView.activeHistoryID(
                initialHistoryID: "initial",
                selectedHistoryID: "selected"
            ),
            "selected"
        )
        XCTAssertEqual(
            RideMapHistoryDetailView.activeHistoryID(
                initialHistoryID: "initial",
                selectedHistoryID: nil
            ),
            "initial"
        )
    }

    func testHistoryDetailVehicleAndSpeedFormattingKeepUnavailableExplicit() {
        XCTAssertEqual(
            RideMapHistoryDetailView.resolvedVehicleLabel(
                associatedVehicle: "vehicle",
                candidateVehicle: nil,
                resolve: { _ in nil },
                fallback: "GPS-only"
            ),
            "Vehicle name unavailable"
        )
        XCTAssertEqual(
            RideMapHistoryDetailView.resolvedVehicleLabel(
                associatedVehicle: nil,
                candidateVehicle: nil,
                resolve: { _ in nil },
                fallback: "GPS-only"
            ),
            "GPS-only"
        )
        XCTAssertEqual(
            RideMapHistoryDetailView.averageSpeedText(
                millimetresPerSecond: nil,
                locale: Locale(identifier: "en_US")
            ),
            "Speed unavailable"
        )
        XCTAssertEqual(
            RideMapHistoryContentView.resolvedVehicleLabel(
                identity: "vehicle",
                currentIdentity: "vehicle",
                currentName: "",
                resolve: { _ in nil }
            ),
            "Vehicle name unavailable"
        )
        let imperial = RideMapHistoryDetailView.averageSpeedText(
            millimetresPerSecond: 1_000,
            locale: Locale(identifier: "en_US")
        )
        let metric = RideMapHistoryDetailView.averageSpeedText(
            millimetresPerSecond: 1_000,
            locale: Locale(identifier: "fr_FR")
        )
        XCTAssertTrue(imperial.contains("2.2"))
        XCTAssertTrue(imperial.localizedCaseInsensitiveContains("mph"))
        XCTAssertTrue(metric.contains("3,6"))
        XCTAssertTrue(metric.localizedCaseInsensitiveContains("km/h"))
    }

    func testRideMapStringsResolveFromTheAppCatalog() {
        XCTAssertEqual(localizedAppText("ride_map.status.recording"), "Recording")
        XCTAssertEqual(localizedAppText("ride_map.start"), "Start GPS-only ride")
        XCTAssertEqual(localizedAppText("ride_map.stop"), "Stop")
        XCTAssertEqual(localizedAppText("ride_map.history_recent"), "Recent rides")
        XCTAssertEqual(localizedAppText("ride_map.vehicle_name_unavailable"), "Vehicle name unavailable")
    }

    func testMapUsesRustCameraRegionAndCanRecenterWithoutRecomputingBounds() {
        let camera = MobileRideMapCameraRegion(
            centerLatitudeDegrees: 40,
            centerLongitudeDegrees: -105,
            latitudeSpanDegrees: 0.25,
            longitudeSpanDegrees: 0.5
        )

        let fitted = RideMapCanvasView.mapRegion(for: camera)
        XCTAssertEqual(fitted.center.latitude, 40)
        XCTAssertEqual(fitted.center.longitude, -105)
        XCTAssertEqual(fitted.span.latitudeDelta, 0.25)
        XCTAssertEqual(fitted.span.longitudeDelta, 0.5)

        let recentered = RideMapCanvasView.mapRegion(for: camera, centeredOn: point(sequence: 9))
        XCTAssertEqual(recentered.center.latitude, 40)
        XCTAssertEqual(recentered.center.longitude, -105)
        XCTAssertEqual(recentered.span.latitudeDelta, fitted.span.latitudeDelta)
        XCTAssertEqual(recentered.span.longitudeDelta, fitted.span.longitudeDelta)
    }

    func testEndpointMarkersRequireTheRustCanonicalSequenceAndVisibility() {
        let points = [point(sequence: 4), point(sequence: 9)]

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
                sequence: 9,
                isVisible: false
            )
        )
        XCTAssertEqual(
            RideMapCanvasView.canonicalEndpointPoint(
                in: points,
                sequence: 9,
                isVisible: true
            )?.sequence,
            9
        )
    }

    func testSingletonMarkersOnlyRetainDisplayedSingletonSegments() {
        let points = [point(sequence: 0, segmentId: 1), point(sequence: 1, segmentId: 2)]
        let segments = [
            MobileRideMapSegmentDisplayMetadata(
                segmentId: 1,
                startReason: .initial,
                visiblePointCount: 1,
                canonicalPointCount: 3,
                firstVisibleSequence: 0,
                lastVisibleSequence: 0
            ),
            MobileRideMapSegmentDisplayMetadata(
                segmentId: 2,
                startReason: .resume,
                visiblePointCount: 2,
                canonicalPointCount: 2,
                firstVisibleSequence: 1,
                lastVisibleSequence: 1
            ),
            MobileRideMapSegmentDisplayMetadata(
                segmentId: 3,
                startReason: .backgroundGap,
                visiblePointCount: 1,
                canonicalPointCount: 1,
                firstVisibleSequence: 2,
                lastVisibleSequence: 2
            ),
        ]

        XCTAssertEqual(
            RideMapCanvasView.retainedSingletonSegmentIDs(in: points, segments: segments),
            [1]
        )
    }

    func testRouteTruthUsesCanonicalCountsAndOnlyBackgroundGaps() {
        XCTAssertTrue(
            RideMapRouteTruthView.routeExists(recordedPointCount: 4, displayedPointCount: 0)
        )
        XCTAssertFalse(
            RideMapRouteTruthView.routeExists(recordedPointCount: 0, displayedPointCount: 4)
        )
        XCTAssertTrue(
            RideMapRouteTruthView.routeExists(recordedPointCount: nil, displayedPointCount: 1)
        )
        XCTAssertFalse(
            RideMapRouteTruthView.shouldShowBackgroundGapCount(
                routeIsPresent: false,
                canonicalBackgroundGapCount: 2
            )
        )
        XCTAssertTrue(
            RideMapRouteTruthView.shouldShowBackgroundGapCount(
                routeIsPresent: true,
                canonicalBackgroundGapCount: 2
            )
        )
        XCTAssertEqual(
            MobileRideMapSegmentDisplayMetadata.visibleBackgroundGapCount(for: [
                MobileRideMapSegmentDisplayMetadata(
                    segmentId: 1,
                    startReason: .resume,
                    visiblePointCount: 1,
                    firstVisibleSequence: 0,
                    lastVisibleSequence: 0
                ),
                MobileRideMapSegmentDisplayMetadata(
                    segmentId: 2,
                    startReason: .backgroundGap,
                    visiblePointCount: 1,
                    firstVisibleSequence: 1,
                    lastVisibleSequence: 1
                ),
                MobileRideMapSegmentDisplayMetadata(
                    segmentId: 3,
                    startReason: .importBoundary,
                    visiblePointCount: 1,
                    firstVisibleSequence: 2,
                    lastVisibleSequence: 2
                ),
            ]),
            1
        )
    }

    func testRouteDisplayMetadataDistinguishesCanonicalAndDisplaySingletons() {
        let canonical = MobileRideMapSegmentDisplayMetadata(
            segmentId: 1,
            startReason: .initial,
            visiblePointCount: 1,
            canonicalPointCount: 1,
            firstVisibleSequence: 4,
            lastVisibleSequence: 4
        )
        let sampled = MobileRideMapSegmentDisplayMetadata(
            segmentId: 2,
            startReason: .resume,
            visiblePointCount: 1,
            canonicalPointCount: 20,
            firstVisibleSequence: 9,
            lastVisibleSequence: 9
        )

        XCTAssertTrue(canonical.isRetainedSingleton)
        XCTAssertTrue(canonical.isCanonicalSingleton)
        XCTAssertTrue(sampled.isRetainedSingleton)
        XCTAssertFalse(sampled.isCanonicalSingleton)
        XCTAssertNotEqual(canonical.singletonAccessibilityLabel, sampled.singletonAccessibilityLabel)
    }

    func testMarkerOffsetsKeepSingletonAndNearbyEndpointsReadable() {
        let singleton = RideMapCanvasView.markerOffsets(for: [CLLocationCoordinate2D(latitude: 40, longitude: -105)])
        XCTAssertEqual(singleton.start.width, -36)
        XCTAssertEqual(singleton.end.width, 36)

        let nearby = RideMapCanvasView.markerOffsets(for: [
            CLLocationCoordinate2D(latitude: 40, longitude: -105),
            CLLocationCoordinate2D(latitude: 40.0001, longitude: -105),
        ])
        XCTAssertEqual(nearby.start.width, -56)
        XCTAssertEqual(nearby.end.width, 56)

        let distant = RideMapCanvasView.markerOffsets(for: [
            CLLocationCoordinate2D(latitude: 40, longitude: -105),
            CLLocationCoordinate2D(latitude: 41, longitude: -105),
        ])
        XCTAssertEqual(distant.start, .zero)
        XCTAssertEqual(distant.end, .zero)
    }
}
