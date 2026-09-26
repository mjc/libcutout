import CutoutMobile
import MapKit
import SwiftUI

struct RideMapLiveContentView: View {
    let displayPoints: [MobileRideMapRouteDisplayPoint]
    let routeID: String
    let projectionVersion: UInt64
    let endpointMetadata: MobileRideMapRouteEndpointMetadata
    let cameraRegion: MobileRideMapCameraRegion?
    let segments: [MobileRideMapSegmentDisplayMetadata]
    let snapshot: MobileRideMapSnapshotDto?
    let availability: MobileRideMapAvailability
    let speed: SpeedReadout
    let vehicleName: String?
    let mapError: MobileRideMapError?
    let lastDecision: MobileRideMapDecisionDto?
    let telemetryState: MobileRideMapTelemetryStateDto?
    let pointsTruncated: Bool
    let segmentsOmittedByBudget: Bool
    let canonicalBackgroundGapCount: UInt64
    @Binding var mapPosition: MapCameraPosition
    @Binding var isApplyingCamera: Bool
    @Binding var followsLatestPoint: Bool
    let pause: () -> Void
    let resume: () -> Void
    let save: () -> Void
    let stop: () -> Void
    let start: () -> Void
    let discard: () -> Void

    @State private var isDiscardConfirmationPresented = false
    @State private var followSpan: MKCoordinateSpan?

    var body: some View {
        ScrollView(.vertical, showsIndicators: false) {
            VStack(spacing: 0) {
                RideMapCanvasView(
                    points: displayPoints,
                    routeID: routeID,
                    projectionVersion: projectionVersion,
                    showsStartMarker: showsRecordedBounds,
                    showsEndMarker: showsRecordedBounds,
                    showsCurrentMarker: true,
                    endpointMetadata: endpointMetadata,
                    cameraRegion: cameraRegion,
                    segments: segments,
                    contextRoutes: [],
                    fitsRouteOnChange: false,
                    mapPosition: $mapPosition,
                    isApplyingCamera: $isApplyingCamera,
                    cameraDidChange: { region in
                        followSpan = region.span
                        followsLatestPoint = false
                    }
                )
                // Keep the live hero compact enough that the metrics and controls
                // remain above a connected TabView on the smallest iPhone.
                .frame(height: 330)
                .frame(maxWidth: .infinity)

                VStack(alignment: .leading, spacing: 12) {
                    RideMapLiveStatusView(
                        displayPointCount: displayPoints.count,
                        snapshot: snapshot,
                        availability: availability,
                        vehicleName: vehicleName,
                        mapError: mapError,
                        lastDecision: lastDecision,
                        telemetryState: telemetryState,
                        pointsTruncated: pointsTruncated,
                        segmentsOmittedByBudget: segmentsOmittedByBudget,
                        segments: segments,
                        canonicalBackgroundGapCount: canonicalBackgroundGapCount
                    )
                    RideMapSummaryView(snapshot: snapshot, speed: speed, vehicleName: vehicleName)
                    RideMapControlsView(
                        state: snapshot?.state,
                        allowedActions: snapshot?.allowedActions ?? [.start],
                        isDiscardConfirmationPresented: $isDiscardConfirmationPresented,
                        pause: pause,
                        resume: resume,
                        save: save,
                        stop: stop,
                        start: start
                    )
                    RideMapCameraControlsView(
                        followsLatestPoint: $followsLatestPoint,
                        recenter: recenterOnLatestPoint
                    )
                }
                .padding(.horizontal, 16)
                .padding(.top, 16)
                .padding(.bottom, 72)
                .frame(maxWidth: .infinity, alignment: .leading)
                .background(
                    PevColors.pageBackground,
                    in: UnevenRoundedRectangle(
                        topLeadingRadius: 28,
                        bottomLeadingRadius: 0,
                        bottomTrailingRadius: 0,
                        topTrailingRadius: 28
                    )
                )
            }
        }
        .onChange(of: routeID) { _, _ in
            followsLatestPoint = true
            followSpan = nil
        }
        .onChange(of: projectionVersion, initial: true) { _, _ in
            guard followsLatestPoint else { return }
            recenterOnLatestPoint()
        }
        .confirmationDialog(
            localizedAppText("ride_map.discard_confirm_title"),
            isPresented: $isDiscardConfirmationPresented,
            titleVisibility: .visible
        ) {
            Button(localizedAppText("ride_map.discard"), role: .destructive, action: discard)
            Button(localizedAppText("common.cancel"), role: .cancel) {}
        }
    }

    private var showsRecordedBounds: Bool {
        snapshot?.recordedBoundsAvailable == true
    }

    private func recenterOnLatestPoint() {
        guard let point = displayPoints.last,
            let cameraRegion,
            let baseRegion = RideMapCanvasView.mapRegion(for: cameraRegion),
            let region = Self.followRegion(
                centeredOn: point,
                span: followSpan ?? Self.stableFollowSpan(for: baseRegion.span)
            )
        else {
            return
        }
        followSpan = region.span
        followsLatestPoint = true
        isApplyingCamera = true
        mapPosition = .region(region)
    }

    static func stableFollowSpan(for baseSpan: MKCoordinateSpan) -> MKCoordinateSpan {
        MKCoordinateSpan(
            latitudeDelta: min(max(baseSpan.latitudeDelta, 0.01), 0.05),
            longitudeDelta: min(max(baseSpan.longitudeDelta, 0.01), 0.05)
        )
    }

    static func followRegion(
        centeredOn point: MobileRideMapRouteDisplayPoint,
        span: MKCoordinateSpan
    ) -> MKCoordinateRegion? {
        let center = CLLocationCoordinate2D(
            latitude: point.latitudeDegrees,
            longitude: point.longitudeDegrees
        )
        guard CLLocationCoordinate2DIsValid(center),
            span.latitudeDelta.isFinite,
            span.longitudeDelta.isFinite,
            span.latitudeDelta > 0,
            span.longitudeDelta > 0
        else { return nil }
        return MKCoordinateRegion(center: center, span: span)
    }
}

private struct RideMapLiveStatusView: View {
    let displayPointCount: Int
    let snapshot: MobileRideMapSnapshotDto?
    let availability: MobileRideMapAvailability
    let vehicleName: String?
    let mapError: MobileRideMapError?
    let lastDecision: MobileRideMapDecisionDto?
    let telemetryState: MobileRideMapTelemetryStateDto?
    let pointsTruncated: Bool
    let segmentsOmittedByBudget: Bool
    let segments: [MobileRideMapSegmentDisplayMetadata]
    let canonicalBackgroundGapCount: UInt64

    var body: some View {
        if snapshot?.state == .active {
            Text(recordingPillText)
                .font(.caption.weight(.black))
                .foregroundStyle(.black)
                .lineLimit(1)
                .minimumScaleFactor(0.72)
                .padding(.horizontal, 10)
                .padding(.vertical, 6)
                .background(PevColors.green, in: Capsule())
                .accessibilityIdentifier("ride-map.recording-pill")
        }

        if availability == .storageUnavailable {
            Label(
                localizedAppText("ride_map.persistence_unavailable"),
                systemImage: "exclamationmark.triangle.fill"
            )
            .font(.subheadline)
            .foregroundStyle(.orange)
            .accessibilityIdentifier("ride-map.persistence-warning")
        } else if availability != .ready {
            Label(availabilityText, systemImage: "location.slash")
                .font(.subheadline)
                .foregroundStyle(.orange)
                .accessibilityIdentifier("ride-map.location-availability")
        }

        if mapError != nil {
            Label(
                localizedAppText("ride_map.command_failed"),
                systemImage: "exclamationmark.circle.fill"
            )
            .font(.subheadline)
            .foregroundStyle(.orange)
            .accessibilityIdentifier("ride-map.command-error")
        }

        Text(statusTitle)
            .font(.title3.weight(.bold))
            .accessibilityAddTraits(.isHeader)
        RideMapRouteTruthView(
            displayedPointCount: displayPointCount,
            recordedPointCount: snapshot?.summary.pointCount,
            rustSegmentCount: snapshot?.segmentCount ?? 0,
            decision: lastDecision,
            showsRecordedBounds: snapshot?.recordedBoundsAvailable == true,
            segmentsOmittedByBudget: segmentsOmittedByBudget,
            segments: segments,
            canonicalBackgroundGapCount: canonicalBackgroundGapCount,
            telemetryState: telemetryState
        )
        if pointsTruncated {
            Text(localizedAppText("ride_map.live_route_truncated_count", displayPointCount))
                .font(.caption)
                .foregroundStyle(PevColors.muted)
                .accessibilityIdentifier("ride-map.live-truncated")
        }
    }

    private var statusTitle: String {
        switch snapshot?.state {
        case .active:
            localizedAppText("ride_map.status.recording")
        case .paused:
            localizedAppText("ride_map.status.paused")
        case .stopped:
            localizedAppText("ride_map.status.stopped")
        case .saved:
            localizedAppText("ride_map.status.saved")
        case .discarded:
            localizedAppText("ride_map.status.discarded")
        case .draft:
            localizedAppText("ride_map.status.draft")
        case .interrupted:
            localizedAppText("ride_map.status.interrupted")
        case .imported:
            localizedAppText("ride_map.status.imported")
        case nil:
            localizedAppText("ride_map.no_active")
        }
    }

    private var availabilityText: String {
        switch availability {
        case .checking:
            localizedAppText("ride_map.location_checking")
        case .ready:
            ""
        case .permissionRequired:
            localizedAppText("ride_map.location_permission_required")
        case .denied:
            localizedAppText("ride_map.location_denied")
        case .restricted:
            localizedAppText("ride_map.location_restricted")
        case .servicesDisabled:
            localizedAppText("ride_map.location_services_disabled")
        case .locationUnavailable:
            localizedAppText("ride_map.location_unavailable")
        case .storageUnavailable:
            localizedAppText("ride_map.persistence_unavailable")
        }
    }

    private var recordingPillText: String {
        let source = RideMapMetricFormatting.vehicleLabel(
            identity: snapshot?.associatedVehicle,
            resolve: { _ in vehicleName },
            noIdentityFallback: localizedAppText("ride_map.gps_only")
        )
        return "\(localizedAppText("ride_map.status.recording")) · \(source)"
    }
}

private struct RideMapCameraControlsView: View {
    @Binding var followsLatestPoint: Bool
    let recenter: () -> Void

    var body: some View {
        HStack(spacing: 12) {
            Button(action: recenter) {
                Label(
                    localizedAppText("ride_map.recenter"),
                    systemImage: followsLatestPoint ? "location.fill" : "location"
                )
            }
            .buttonStyle(.bordered)
            .accessibilityValue(
                localizedAppText(
                    followsLatestPoint ? "ride_map.following" : "ride_map.not_following"
                )
            )
            .accessibilityIdentifier("ride-map.recenter")

            if followsLatestPoint {
                Text(localizedAppText("ride_map.following"))
                    .font(.caption)
                    .foregroundStyle(PevColors.muted)
            }
        }
    }
}
