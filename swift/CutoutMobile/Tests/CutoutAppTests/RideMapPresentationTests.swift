import MapKit
import SwiftUI
import XCTest

@testable import CutoutApp
@testable import CutoutMobile

@MainActor
final class RideMapPresentationTests: XCTestCase {
    func testChargingModeUsesTheSharedGeographicSpanAtItsExistingCenter() throws {
        let presentation = RideMapPresentationState()
        presentation.liveMapPosition = .region(
            MKCoordinateRegion(
                center: CLLocationCoordinate2D(latitude: 40, longitude: -105),
                span: MKCoordinateSpan(latitudeDelta: 0.01, longitudeDelta: 0.02)
            ))

        presentation.mode = .charging

        let region = try XCTUnwrap(presentation.chargingMapPosition.region)
        XCTAssertEqual(region.center.latitude, 40)
        XCTAssertEqual(region.center.longitude, -105)
        XCTAssertEqual(region.span.longitudeDelta, presentation.sharedCameraSpan.longitudeDelta)
    }

    func testUserZoomBecomesTheSharedGeographicSpanForTheNextMapMode() throws {
        let presentation = RideMapPresentationState()
        let span = MKCoordinateSpan(latitudeDelta: 0.07, longitudeDelta: 0.09)
        presentation.recordUserCameraSpan(span)
        presentation.liveMapPosition = .region(
            MKCoordinateRegion(
                center: CLLocationCoordinate2D(latitude: 40, longitude: -105),
                span: MKCoordinateSpan(latitudeDelta: 0.01, longitudeDelta: 0.02)
            ))

        presentation.mode = .charging

        let region = try XCTUnwrap(presentation.chargingMapPosition.region)
        XCTAssertEqual(presentation.sharedCameraSpan.longitudeDelta, span.longitudeDelta)
        XCTAssertEqual(region.span.longitudeDelta, span.longitudeDelta)
    }

    func testLiveAndHistoryModesApplyTheLatestSpanAtTheirOwnCenters() throws {
        let presentation = RideMapPresentationState()
        presentation.liveMapPosition = .camera(
            MapCamera(
                centerCoordinate: CLLocationCoordinate2D(latitude: 39, longitude: -104),
                distance: 20_000,
                heading: 15,
                pitch: 30
            ))
        presentation.historyMapPosition = .camera(
            MapCamera(
                centerCoordinate: CLLocationCoordinate2D(latitude: 41, longitude: -106),
                distance: 20_000,
                heading: 0,
                pitch: 0
            ))
        let span = MKCoordinateSpan(latitudeDelta: 0.06, longitudeDelta: 0.08)
        presentation.recordUserCameraSpan(span)

        presentation.mode = .live
        let live = try XCTUnwrap(presentation.liveMapPosition.region)
        XCTAssertEqual(live.center.latitude, 39)
        XCTAssertEqual(live.center.longitude, -104)
        XCTAssertEqual(live.span.longitudeDelta, span.longitudeDelta)

        presentation.mode = .history
        let history = try XCTUnwrap(presentation.historyMapPosition.region)
        XCTAssertEqual(history.center.latitude, 41)
        XCTAssertEqual(history.center.longitude, -106)
        XCTAssertEqual(history.span.longitudeDelta, span.longitudeDelta)
    }

    func testTerminalRideFitKeepsRouteExtentWhileHistoryOverviewUsesLocalSpan() throws {
        let routeCamera = MobileRideMapCameraRegion(
            centerLatitudeDegrees: 40,
            centerLongitudeDegrees: -105,
            latitudeSpanDegrees: 0.8,
            longitudeSpanDegrees: 1.2
        )

        let overview = try XCTUnwrap(
            RideMapCanvasView.localRegion(
                center: CLLocationCoordinate2D(latitude: 40, longitude: -105),
                span: RideMapPresentationState.defaultLocalSpan
            )
        )
        XCTAssertEqual(overview.center.latitude, 40)
        XCTAssertEqual(overview.center.longitude, -105)
        XCTAssertLessThan(overview.span.latitudeDelta, 0.000_01)
        XCTAssertEqual(overview.span.longitudeDelta, RideMapPresentationState.defaultLocalSpan.longitudeDelta)

        let detailFit = try XCTUnwrap(
            RideMapCanvasView.cameraPosition(for: routeCamera, distance: nil)?.region
        )
        XCTAssertEqual(detailFit.span.latitudeDelta, 0.8)
        XCTAssertEqual(detailFit.span.longitudeDelta, 1.2)
    }

    func testChargingModeUsesCurrentLocationWithoutARideAndPreservesAnExistingViewport() throws {
        let presentation = RideMapPresentationState()
        presentation.mode = .charging
        XCTAssertTrue(presentation.chargingMapPosition.followsUserLocation)

        let panned = MapCameraPosition.region(
            MKCoordinateRegion(
                center: CLLocationCoordinate2D(latitude: 41, longitude: -106),
                span: MKCoordinateSpan(latitudeDelta: 0.03, longitudeDelta: 0.04)
            ))
        presentation.chargingMapPosition = panned
        presentation.mode = .live
        presentation.mode = .charging
        let region = try XCTUnwrap(presentation.chargingMapPosition.region)
        XCTAssertEqual(region.center.latitude, 41)
        XCTAssertEqual(region.center.longitude, -106)
        XCTAssertEqual(region.span.longitudeDelta, presentation.sharedCameraSpan.longitudeDelta)
    }

    func testFirstLocationDoesNotReplaceAnAlreadyFittedDistantRide() throws {
        let presentation = RideMapPresentationState()
        presentation.historyMapPosition = .region(
            MKCoordinateRegion(
                center: CLLocationCoordinate2D(latitude: 10, longitude: 20),
                span: MKCoordinateSpan(latitudeDelta: 0.02, longitudeDelta: 0.03)
            ))

        presentation.updateCurrentLocation(CLLocationCoordinate2D(latitude: 40, longitude: -105))

        let history = try XCTUnwrap(presentation.historyMapPosition.region)
        XCTAssertEqual(history.center.latitude, 10)
        XCTAssertEqual(history.center.longitude, 20)
        XCTAssertEqual(history.span.longitudeDelta, 0.03)
    }

    func testInterruptedRideOffersResumeInsteadOfStartNew() {
        XCTAssertEqual(RideMapControlsView.controlSet(for: .interrupted), .resumable)
        XCTAssertEqual(RideMapControlsView.controlSet(for: .paused), .resumable)
        XCTAssertEqual(RideMapControlsView.controlSet(for: .stopped), .terminal)
    }

    func testHistoryMusicForgetUsesDestinationRideID() async {
        var forgottenRideID: String?
        let music = RideMapHistoryMusicDetail(
            rideID: "destination-ride",
            events: [],
            forgetMusicHistory: { rideID in
                forgottenRideID = rideID
                return true
            }
        )

        let didForget = await music.forget()
        XCTAssertTrue(didForget)
        XCTAssertEqual(forgottenRideID, "destination-ride")
    }

    func testHistoryMusicForgetRequiresRetainedMetadata() {
        let forget: @MainActor (String) async -> Bool = { _ in true }
        let missing = RideMapHistoryMusicDetail(
            rideID: "missing",
            events: [],
            state: .missing,
            forgetMusicHistory: forget
        )
        let redacted = RideMapHistoryMusicDetail(
            rideID: "redacted",
            events: [],
            state: .redacted,
            forgetMusicHistory: forget
        )
        let deleted = RideMapHistoryMusicDetail(
            rideID: "deleted",
            events: [],
            state: .deleted,
            forgetMusicHistory: forget
        )

        XCTAssertFalse(missing.canForget)
        XCTAssertTrue(redacted.canForget)
        XCTAssertFalse(deleted.canForget)
    }

    func testHistoryShowsRefreshStatusOnlyWhenExistingResultsRemainVisible() {
        XCTAssertFalse(
            RideMapHistoryContentView.shouldShowRefreshStatus(isLoading: true, rideCount: 0),
            "An initial load uses the full loading state"
        )
        XCTAssertTrue(
            RideMapHistoryContentView.shouldShowRefreshStatus(isLoading: true, rideCount: 1),
            "A filter reload keeps prior rides visible, so it needs a refresh status"
        )
        XCTAssertFalse(
            RideMapHistoryContentView.shouldShowRefreshStatus(isLoading: false, rideCount: 1)
        )
    }

    func testHistoryDetailOffersRetryWhenRefreshErrorKeepsRouteProjection() {
        XCTAssertTrue(
            RideMapHistoryDetailView.shouldShowRetainedRouteRetry(
                hasMatchingProjection: true,
                hasError: true
            )
        )
        XCTAssertFalse(
            RideMapHistoryDetailView.shouldShowRetainedRouteRetry(
                hasMatchingProjection: false,
                hasError: true
            ),
            "The unavailable state already owns retry when no matching route is retained"
        )
        XCTAssertFalse(
            RideMapHistoryDetailView.shouldShowRetainedRouteRetry(
                hasMatchingProjection: true,
                hasError: false
            )
        )
    }

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

    func testLiveRouteIdentityTracksRustRideIdentity() {
        let summary = MobileRideMapSummaryDto(
            pointCount: 0,
            distanceMeters: 0,
            durationMilliseconds: 0
        )
        let snapshot = MobileRideMapSnapshotDto(
            rideID: "ride-b",
            state: .active,
            summary: summary,
            segmentCount: 0,
            associatedVehicle: nil,
            recordedBoundsAvailable: false
        )

        XCTAssertEqual(RideMapRouteView.liveRouteID(for: nil), "live")
        XCTAssertEqual(RideMapRouteView.liveRouteID(for: snapshot), "ride-b")
    }

    private func summarySnapshot(
        lifecycle: MobileRideMapStateDto = .active,
        telemetry: MobileRideMapTelemetryStateDto
    ) -> MobileRideMapSnapshotDto {
        MobileRideMapSnapshotDto(
            rideID: "ride",
            state: lifecycle,
            telemetryState: telemetry,
            summary: MobileRideMapSummaryDto(pointCount: 0, distanceMeters: 0, durationMilliseconds: 0),
            segmentCount: 0,
            associatedVehicle: telemetry == .gpsOnly ? nil : "associated-wheel"
        )
    }

    func testLiveSummaryIndicatorChangesWithRustTelemetryHealthWithoutChangingVehicle() {
        let fresh = RideMapSummaryView.IndicatorState.state(for: summarySnapshot(telemetry: .associatedFresh))
        let missing = RideMapSummaryView.IndicatorState.state(for: summarySnapshot(telemetry: .associatedNoTelemetry))
        let stale = RideMapSummaryView.IndicatorState.state(for: summarySnapshot(telemetry: .associatedStale))
        let unknown = RideMapSummaryView.IndicatorState.state(for: summarySnapshot(telemetry: .unknown))

        XCTAssertNotEqual(fresh, missing, "A healthy wheel cannot look like a wheel with no data")
        XCTAssertNotEqual(fresh, stale, "Stale Rust telemetry cannot keep the healthy indicator")
        XCTAssertNotEqual(missing, stale, "Initial waiting and lost live data have different rider-facing status")
        XCTAssertEqual(unknown, .unavailable, "Remembered association cannot invent telemetry health")
        XCTAssertEqual(
            RideMapSummaryView.IndicatorState.state(for: summarySnapshot(telemetry: .gpsOnly)), .gpsOnly)
    }

    func testPausingRecordingDoesNotReplaceTheAssociatedWheelHealthIndicator() {
        for telemetry: MobileRideMapTelemetryStateDto in [.associatedFresh, .associatedNoTelemetry, .associatedStale] {
            XCTAssertEqual(
                RideMapSummaryView.IndicatorState.state(for: summarySnapshot(lifecycle: .paused, telemetry: telemetry)),
                RideMapSummaryView.IndicatorState.state(for: summarySnapshot(telemetry: telemetry)),
                "The separate recording pill owns Paused; this indicator owns wheel data health"
            )
        }
    }

    func testTerminalSummaryCannotClaimLiveWheelTelemetry() {
        for lifecycle: MobileRideMapStateDto in [.draft, .stopped, .saved, .discarded, .interrupted, .imported] {
            XCTAssertEqual(
                RideMapSummaryView.IndicatorState.state(
                    for: summarySnapshot(lifecycle: lifecycle, telemetry: .associatedFresh)), .terminal)
        }
        XCTAssertEqual(RideMapSummaryView.IndicatorState.state(for: nil), .unavailable)
    }

    @MainActor
    func testWheelHealthSpeechUsesRiderCopyWithoutRepeatingRecordingOrSourceState() {
        XCTAssertEqual(
            RideMapSummaryView.IndicatorState.state(for: summarySnapshot(telemetry: .associatedFresh))
                .accessibilityValue, "Receiving wheel data")
        XCTAssertEqual(
            RideMapSummaryView.IndicatorState.state(for: summarySnapshot(telemetry: .associatedNoTelemetry))
                .accessibilityValue, "Waiting for wheel data")
        for telemetry: MobileRideMapTelemetryStateDto in [.associatedStale, .unknown] {
            XCTAssertEqual(
                RideMapSummaryView.IndicatorState.state(for: summarySnapshot(telemetry: telemetry))
                    .accessibilityValue, "Wheel data unavailable")
        }
        XCTAssertEqual(
            RideMapSummaryView.IndicatorState.state(for: summarySnapshot(telemetry: .gpsOnly))
                .accessibilityValue, "")
        XCTAssertEqual(
            RideMapSummaryView.IndicatorState.state(
                for: summarySnapshot(lifecycle: .saved, telemetry: .associatedFresh)
            )
            .accessibilityValue, "")
        XCTAssertEqual(
            RideMapSummaryView.IndicatorState.state(
                for: summarySnapshot(lifecycle: .paused, telemetry: .associatedFresh)
            )
            .accessibilityValue, "Receiving wheel data")
    }

    func testRideMapVehicleLabelProjectionKeepsIdentityFallbackPolicy() {
        XCTAssertEqual(
            RideMapMetricFormatting.vehicleLabel(
                identity: nil,
                resolve: { _ in "ignored" },
                noIdentityFallback: "GPS-only"
            ),
            "GPS-only"
        )
        XCTAssertEqual(
            RideMapMetricFormatting.vehicleLabel(
                identity: "vehicle",
                resolve: { _ in "" },
                noIdentityFallback: "GPS-only"
            ),
            localizedAppText("ride_map.vehicle_name_unavailable")
        )
        XCTAssertEqual(
            RideMapMetricFormatting.vehicleLabel(
                identity: "vehicle",
                resolve: { _ in "Floatwheel" },
                noIdentityFallback: "GPS-only"
            ),
            "Floatwheel"
        )
    }

    func testRideMapMetricFormattingKeepsSharedProjectionContracts() {
        let summary = MobileRideMapSummaryDto(
            pointCount: 2,
            distanceMeters: 1_234.5,
            durationMilliseconds: 61_000
        )

        XCTAssertEqual(
            RideMapMetricFormatting.distanceText(for: summary),
            Measurement(value: summary.distanceMeters, unit: UnitLength.meters)
                .formatted(.measurement(width: .abbreviated, usage: .road))
        )
        XCTAssertEqual(
            RideMapMetricFormatting.recordedAtText(for: 0),
            localizedAppText("ride_map.untitled_ride")
        )
    }

    func testMovingTimeFormattingDistinguishesUnavailableMeasuredZeroAndPartialCoverage() {
        func summary(_ movingTime: MobileRideMapMovingTimeDto) -> MobileRideMapSummaryDto {
            MobileRideMapSummaryDto(
                pointCount: 2, distanceMeters: 10, durationMilliseconds: 999_000, movingTime: movingTime
            )
        }
        let zero = Duration.seconds(0)
            .formatted(.units(allowed: [.hours, .minutes, .seconds], width: .abbreviated))
        let moving = Duration.seconds(61)
            .formatted(.units(allowed: [.hours, .minutes, .seconds], width: .abbreviated))
        XCTAssertEqual(RideMapMetricFormatting.movingTimeText(for: summary(.unavailable)), "--")
        XCTAssertEqual(
            RideMapMetricFormatting.movingTimeSpokenValue(for: summary(.unavailable)),
            localizedAppText("ride_map.moving_time_unavailable")
        )
        XCTAssertEqual(
            localizedAppText(
                "ride_map.moving_time", RideMapMetricFormatting.movingTimeSpokenValue(for: summary(.unavailable))),
            "Moving time: Unavailable"
        )
        XCTAssertEqual(
            RideMapMetricFormatting.movingTimeText(
                for: summary(.complete(movingMilliseconds: 0, observedMilliseconds: 10_000))), zero)
        XCTAssertEqual(
            RideMapMetricFormatting.movingTimeText(
                for: summary(
                    .partial(
                        movingMilliseconds: 61_000, observedMilliseconds: 100_000, unknownMilliseconds: 30_000))),
            "≥ \(moving)"
        )
        XCTAssertEqual(
            RideMapMetricFormatting.movingTimeSpokenValue(
                for: summary(
                    .partial(
                        movingMilliseconds: 61_000, observedMilliseconds: 100_000, unknownMilliseconds: 30_000))),
            localizedAppText("ride_map.moving_time_partial", moving)
        )
    }

    func testElapsedTimeFormattingDistinguishesUnavailableZeroAndPartialCoverage() {
        let localFixture = MobileRideMapSummaryDto(pointCount: 0, distanceMeters: 0, durationMilliseconds: 0)
        XCTAssertEqual(localFixture.elapsedTime, .unavailable)

        func summary(_ elapsedTime: MobileRideMapElapsedTimeDto) -> MobileRideMapSummaryDto {
            MobileRideMapSummaryDto(
                pointCount: 2, distanceMeters: 10, durationMilliseconds: 999_000, elapsedTime: elapsedTime
            )
        }
        let zero = Duration.seconds(0)
            .formatted(.units(allowed: [.hours, .minutes, .seconds], width: .abbreviated))
        let elapsed = Duration.seconds(61)
            .formatted(.units(allowed: [.hours, .minutes, .seconds], width: .abbreviated))

        XCTAssertEqual(RideMapMetricFormatting.elapsedTimeText(for: summary(.unavailable)), "--")
        XCTAssertEqual(
            RideMapMetricFormatting.elapsedTimeSpokenValue(for: summary(.unavailable)),
            localizedAppText("ride_map.elapsed_time_unavailable")
        )
        XCTAssertEqual(
            RideMapMetricFormatting.elapsedTimeText(for: summary(.complete(durationMilliseconds: 0))), zero
        )
        XCTAssertEqual(
            RideMapMetricFormatting.elapsedTimeText(for: summary(.complete(durationMilliseconds: 61_000))), elapsed
        )
        XCTAssertEqual(
            RideMapMetricFormatting.elapsedTimeText(for: summary(.partial(durationMilliseconds: 61_000))),
            "≥ \(elapsed)"
        )
        XCTAssertEqual(
            RideMapMetricFormatting.elapsedTimeSpokenValue(for: summary(.partial(durationMilliseconds: 61_000))),
            localizedAppText("ride_map.elapsed_time_partial", elapsed)
        )
        XCTAssertEqual(
            localizedAppText(
                "ride_map.elapsed_time",
                RideMapMetricFormatting.elapsedTimeSpokenValue(
                    for: summary(.unavailable)
                )),
            "Elapsed time: Unavailable"
        )
    }

    func testSharedRideSummaryUsesTheSameElapsedAndMovingLabels() {
        let summary = MobileRideMapSummaryDto(
            pointCount: 2,
            distanceMeters: 1_000,
            durationMilliseconds: 180_000,
            movingTime: .complete(movingMilliseconds: 121_000, observedMilliseconds: 180_000),
            elapsedTime: .complete(durationMilliseconds: 180_000)
        )
        let ride = MobileRideMapHistorySummaryDto(
            rideID: "ride",
            state: .saved,
            summary: summary,
            segmentCount: 1,
            createdAtMilliseconds: 1,
            candidateVehicle: nil,
            associatedVehicle: nil,
            telemetryState: .gpsOnly
        )
        let elapsed = localizedAppText(
            "ride_map.elapsed_time", RideMapMetricFormatting.elapsedTimeText(for: summary)
        )
        let moving = localizedAppText(
            "ride_map.moving_time", RideMapMetricFormatting.movingTimeText(for: summary)
        )

        XCTAssertEqual(
            RideMapHistoryDetailView.shareText(for: ride),
            "\(localizedAppText("ride_map.detail_title"))\n\(RideMapMetricFormatting.distanceText(for: summary)) · "
                + "\(elapsed) · \(moving)"
        )
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

    func testHistoryDetailKeepsTheDestinationIdentityWhenSelectionIsStale() {
        XCTAssertEqual(
            RideMapHistoryDetailView.activeHistoryID(
                initialHistoryID: "initial",
                selectedHistoryID: "selected"
            ),
            "initial"
        )
        XCTAssertEqual(
            RideMapHistoryDetailView.activeHistoryID(
                initialHistoryID: "initial",
                selectedHistoryID: nil
            ),
            "initial"
        )
    }

    func testMapViewportDoesNotSubtractTheNativeAccessorySafeAreaTwice() {
        let player = CGRect(x: 21, y: 737, width: 360, height: 48)
        XCTAssertEqual(
            RideMapViewportLayout.visibleHeight(
                contentFrame: CGRect(x: 0, y: 147, width: 402, height: 590),
                musicPlayerFrame: player
            ),
            590,
            "The observed native TabView proposal already ends at the player"
        )
        XCTAssertEqual(
            RideMapViewportLayout.visibleHeight(
                contentFrame: CGRect(x: 0, y: 147, width: 402, height: 727),
                musicPlayerFrame: player
            ),
            590,
            "A proposal extending behind the player is capped at its measured top"
        )
    }

    func testMapViewportKeepsItsProposalWithoutAnOverlappingPlayerAndClampsAtZero() {
        let frame = CGRect(x: 0, y: 147, width: 402, height: 590)
        XCTAssertEqual(
            RideMapViewportLayout.visibleHeight(contentFrame: frame, musicPlayerFrame: nil), 590
        )
        XCTAssertEqual(
            RideMapViewportLayout.visibleHeight(
                contentFrame: frame,
                musicPlayerFrame: CGRect(x: 21, y: 791, width: 360, height: 48)
            ),
            590
        )
        XCTAssertEqual(
            RideMapViewportLayout.visibleHeight(
                contentFrame: frame,
                musicPlayerFrame: CGRect(x: 21, y: 100, width: 360, height: 48)
            ),
            0
        )
    }

    func testHistoryDetailWaitsForTheRequestedProjectionInsteadOfShowingEmptyHistory() {
        let requestedRide = "october-4"
        for previousProjection in [nil, "another-ride"] {
            // Covers loading the summary page and loading an already selected ride.
            XCTAssertTrue(
                RideMapHistoryDetailView.initialProjectionIsLoading(
                    activeHistoryID: requestedRide, projectionRideID: previousProjection,
                    hasSelectedRide: false, isLoading: true, hasError: false
                )
            )
            XCTAssertTrue(
                RideMapHistoryDetailView.initialProjectionIsLoading(
                    activeHistoryID: requestedRide, projectionRideID: previousProjection,
                    hasSelectedRide: true, isLoading: false, hasError: false
                )
            )
        }
        XCTAssertFalse(
            RideMapHistoryDetailView.initialProjectionIsLoading(
                activeHistoryID: requestedRide, projectionRideID: requestedRide,
                hasSelectedRide: true, isLoading: true, hasError: false
            ),
            "A matching projection stays visible during refresh"
        )
    }

    func testHistoryDetailDoesNotHideErrorsOrMissingRidesBehindInitialLoading() {
        XCTAssertFalse(
            RideMapHistoryDetailView.initialProjectionIsLoading(
                activeHistoryID: "october-4", projectionRideID: nil,
                hasSelectedRide: true, isLoading: true, hasError: true
            ),
            "A storage failure must remain actionable even when loading flags are stale"
        )
        XCTAssertFalse(
            RideMapHistoryDetailView.initialProjectionIsLoading(
                activeHistoryID: "missing", projectionRideID: nil,
                hasSelectedRide: false, isLoading: false, hasError: false
            )
        )
        XCTAssertFalse(
            RideMapHistoryDetailView.initialProjectionIsLoading(
                activeHistoryID: nil, projectionRideID: nil,
                hasSelectedRide: false, isLoading: true, hasError: false
            )
        )
    }

    func testHistoryDetailShowsLoadingBeforeTheInitialSelectionTaskStarts() {
        XCTAssertTrue(
            RideMapHistoryDetailView.initialProjectionIsLoading(
                activeHistoryID: "deep-linked-ride", projectionRideID: nil,
                hasSelectedRide: false, isLoading: false, hasError: false,
                hasStartedSelectionRequest: false
            ),
            "Do not flash confirmed-empty copy before the view can start its requested-ride lookup"
        )
        XCTAssertFalse(
            RideMapHistoryDetailView.initialProjectionIsLoading(
                activeHistoryID: "deep-linked-ride", projectionRideID: nil,
                hasSelectedRide: false, isLoading: false, hasError: false,
                hasStartedSelectionRequest: true
            ),
            "Once the requested lookup finishes without a matching ride, show the unavailable state"
        )
    }

    func testHistoryDetailRouteIdentityChangesForAReplacementFitWithoutChangingRideIdentity() {
        let initial = RideMapHistoryDetailView.routeID(
            for: "ride",
            cameraFitVersion: 1
        )
        let replacement = RideMapHistoryDetailView.routeID(
            for: "ride",
            cameraFitVersion: 2
        )

        XCTAssertNotEqual(initial, replacement)
        XCTAssertTrue(initial.hasPrefix("ride:"))
        XCTAssertTrue(replacement.hasPrefix("ride:"))
    }

    func testHistoryCameraFitIdentityChangesForAReplacementProjection() {
        let initial = RideMapCanvasView.cameraFitID(routeID: "ride", fitVersion: 1)
        let replacement = RideMapCanvasView.cameraFitID(routeID: "ride", fitVersion: 2)

        XCTAssertNotEqual(initial, replacement)
        XCTAssertTrue(initial.hasPrefix("ride:fit-"))
    }

    func testHistoryOverviewFitIdentityDoesNotChangeForProjectionRefresh() {
        XCTAssertEqual(
            RideMapCanvasView.cameraFitID(routeID: "ride"),
            RideMapCanvasView.cameraFitID(routeID: "ride")
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

        let fitted = try! XCTUnwrap(RideMapCanvasView.mapRegion(for: camera))
        XCTAssertEqual(fitted.center.latitude, 40)
        XCTAssertEqual(fitted.center.longitude, -105)
        XCTAssertEqual(fitted.span.latitudeDelta, 0.25)
        XCTAssertEqual(fitted.span.longitudeDelta, 0.5)

        let recentered = try! XCTUnwrap(
            RideMapCanvasView.mapRegion(for: camera, centeredOn: point(sequence: 9))
        )
        XCTAssertEqual(recentered.center.latitude, 40)
        XCTAssertEqual(recentered.center.longitude, -105)
        XCTAssertEqual(recentered.span.latitudeDelta, fitted.span.latitudeDelta)
        XCTAssertEqual(recentered.span.longitudeDelta, fitted.span.longitudeDelta)
    }

    func testLiveFollowKeepsItsViewportSpanAsTheRouteGrows() throws {
        let first = try XCTUnwrap(
            RideMapLiveContentView.followRegion(
                centeredOn: point(sequence: 1),
                span: MKCoordinateSpan(latitudeDelta: 0.25, longitudeDelta: 0.5)
            )
        )
        let second = try XCTUnwrap(
            RideMapLiveContentView.followRegion(
                centeredOn: point(sequence: 2, latitude: 41, longitude: -106),
                span: first.span
            )
        )
        XCTAssertEqual(second.center.latitude, 41)
        XCTAssertEqual(second.center.longitude, -106)
        XCTAssertEqual(second.span.latitudeDelta, first.span.latitudeDelta)
        XCTAssertEqual(second.span.longitudeDelta, first.span.longitudeDelta)
    }

    func testLiveFollowProjectionVersionChangesWhenTerminalSequenceIsReused() {
        let first = RideMapCanvasView.pathKey(
            routeID: "live",
            projectionVersion: 1,
            points: [point(sequence: 0, latitude: 40, longitude: -105)]
        )
        let replacement = RideMapCanvasView.pathKey(
            routeID: "live",
            projectionVersion: 2,
            points: [point(sequence: 0, latitude: 41, longitude: -106)]
        )

        XCTAssertEqual(first.lastSequence, replacement.lastSequence)
        XCTAssertNotEqual(first.projectionVersion, replacement.projectionVersion)
    }

    func testDefaultLocalViewportIsSharedGeographicSpanAroundActualCenter() {
        let center = CLLocationCoordinate2D(latitude: 40, longitude: -105)
        let local = RideMapPresentationState.defaultLocalRegion(center: center)
        XCTAssertEqual(local.center.latitude, 40)
        XCTAssertEqual(local.center.longitude, -105)
        XCTAssertLessThan(local.span.latitudeDelta, 0.000_01)
        XCTAssertEqual(local.span.longitudeDelta, RideMapPresentationState.sharedLocalSpan.longitudeDelta)
    }

    func testSharedZoomUsesLongitudeSpanAndDoesNotRatchetWithViewportHeight() throws {
        let presentation = RideMapPresentationState()
        presentation.recordUserCameraSpan(
            MKCoordinateSpan(latitudeDelta: 0.08, longitudeDelta: 0.04)
        )
        let firstViewport = try XCTUnwrap(
            RideMapPresentationState.localRegion(
                center: CLLocationCoordinate2D(latitude: 40, longitude: -105),
                longitudeDelta: presentation.sharedCameraSpan.longitudeDelta
            )
        )
        presentation.recordUserCameraSpan(
            MKCoordinateSpan(latitudeDelta: 0.22, longitudeDelta: firstViewport.span.longitudeDelta)
        )
        let secondViewport = try XCTUnwrap(
            RideMapPresentationState.localRegion(
                center: CLLocationCoordinate2D(latitude: 41, longitude: -106),
                longitudeDelta: presentation.sharedCameraSpan.longitudeDelta
            )
        )

        XCTAssertEqual(firstViewport.span.longitudeDelta, secondViewport.span.longitudeDelta)
        XCTAssertEqual(secondViewport.span.longitudeDelta, 0.04)
        XCTAssertEqual(secondViewport.span.latitudeDelta, 0.000_001)
    }

    func testFirstRealLocationCentersUnpositionedMapModes() throws {
        let presentation = RideMapPresentationState()
        presentation.updateCurrentLocation(CLLocationCoordinate2D(latitude: 40, longitude: -105))

        let live = try XCTUnwrap(presentation.liveMapPosition.region)
        let history = try XCTUnwrap(presentation.historyMapPosition.region)
        let charging = try XCTUnwrap(presentation.chargingMapPosition.region)
        for region in [live, history, charging] {
            XCTAssertEqual(region.center.latitude, 40)
            XCTAssertEqual(region.center.longitude, -105)
            XCTAssertEqual(region.span.longitudeDelta, RideMapPresentationState.sharedLocalSpan.longitudeDelta)
        }
    }

    func testMapRejectsInvalidRecenterInputBeforeItReachesMapKit() {
        let camera = MobileRideMapCameraRegion(
            centerLatitudeDegrees: 40,
            centerLongitudeDegrees: -105,
            latitudeSpanDegrees: 0.25,
            longitudeSpanDegrees: 0.5
        )

        XCTAssertNil(
            RideMapCanvasView.mapRegion(
                for: camera,
                centeredOn: point(sequence: 9, latitude: .nan)
            )
        )
    }

    func testMapClampsCameraSpansToMapKitDomain() throws {
        let camera = MobileRideMapCameraRegion(
            centerLatitudeDegrees: 40,
            centerLongitudeDegrees: -105,
            latitudeSpanDegrees: 243,
            longitudeSpanDegrees: 486
        )

        let region = try XCTUnwrap(RideMapCanvasView.mapRegion(for: camera))
        XCTAssertEqual(region.span.latitudeDelta, 180)
        XCTAssertEqual(region.span.longitudeDelta, 360)
    }

    func testMapGeoBoundsPreserveAntimeridianCrossing() {
        let bounds = RideMapCanvasView.geoBounds(
            for: MKCoordinateRegion(
                center: CLLocationCoordinate2D(latitude: 10, longitude: 179),
                span: MKCoordinateSpan(latitudeDelta: 2, longitudeDelta: 4)
            )
        )

        XCTAssertEqual(bounds?.minimumLatitudeDegrees, 9)
        XCTAssertEqual(bounds?.maximumLatitudeDegrees, 11)
        XCTAssertEqual(bounds?.minimumLongitudeDegrees, 177)
        XCTAssertEqual(bounds?.maximumLongitudeDegrees, -179)
    }

    func testMapGeoBoundsRejectInvalidSpansAndCoverTheWholeLongitudeRange() {
        XCTAssertNil(
            RideMapCanvasView.geoBounds(
                for: MKCoordinateRegion(
                    center: CLLocationCoordinate2D(latitude: 10, longitude: 20),
                    span: MKCoordinateSpan(latitudeDelta: 0, longitudeDelta: 1)
                )
            )
        )

        let world = RideMapCanvasView.geoBounds(
            for: MKCoordinateRegion(
                center: CLLocationCoordinate2D(latitude: 10, longitude: 20),
                span: MKCoordinateSpan(latitudeDelta: 2, longitudeDelta: 360)
            )
        )
        XCTAssertEqual(world?.minimumLongitudeDegrees, -180)
        XCTAssertEqual(world?.maximumLongitudeDegrees, 180)
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

    func testMarkerOffsetsOnlySeparateVisibleMarkerPairs() {
        let singleton = RideMapCanvasView.markerOffsets(for: [CLLocationCoordinate2D(latitude: 40, longitude: -105)])
        XCTAssertEqual(singleton.start, .zero, "A lone visible latest-point marker stays on its route coordinate")
        XCTAssertEqual(singleton.end, .zero)

        let sameCoordinateEndpoints = RideMapCanvasView.markerOffsets(for: [
            CLLocationCoordinate2D(latitude: 40, longitude: -105),
            CLLocationCoordinate2D(latitude: 40, longitude: -105),
        ])
        XCTAssertEqual(sameCoordinateEndpoints.start.width, -56)
        XCTAssertEqual(sameCoordinateEndpoints.end.width, 56)

        let nearby = RideMapCanvasView.markerOffsets(for: [
            CLLocationCoordinate2D(latitude: 40, longitude: -105),
            CLLocationCoordinate2D(latitude: 40.0001, longitude: -105),
        ])
        XCTAssertEqual(nearby.start.width, -56)
        XCTAssertEqual(nearby.start.height, -24)
        XCTAssertEqual(nearby.end.width, 56)
        XCTAssertEqual(nearby.end.height, 0, "The end dot stays anchored when its caption sits above it")

        let nearbyAtAccessibilitySize = RideMapCanvasView.markerOffsets(
            for: [
                CLLocationCoordinate2D(latitude: 40, longitude: -105),
                CLLocationCoordinate2D(latitude: 40.0001, longitude: -105),
            ],
            dynamicTypeSize: .accessibility3
        )
        XCTAssertEqual(nearbyAtAccessibilitySize.start.width, -84)
        XCTAssertEqual(nearbyAtAccessibilitySize.start.height, -36)
        XCTAssertEqual(nearbyAtAccessibilitySize.end.width, 84)
        XCTAssertEqual(nearbyAtAccessibilitySize.end.height, 0)

        let distant = RideMapCanvasView.markerOffsets(for: [
            CLLocationCoordinate2D(latitude: 40, longitude: -105),
            CLLocationCoordinate2D(latitude: 41, longitude: -105),
        ])
        XCTAssertEqual(distant.start, .zero)
        XCTAssertEqual(distant.end, .zero)
    }

    func testRideMapMarkerLabelsDescribeRecordedPointsAndRideEndpoints() {
        XCTAssertEqual(localizedAppText("ride_map.current_marker"), "Latest recorded point")
        XCTAssertEqual(localizedAppText("ride_map.end_marker"), "Ride end")
    }
}
