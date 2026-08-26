import CoreLocation
import MapKit
import SwiftUI
import CutoutMobile
import CutoutMobileFFI

struct RideMapLiveContentView: View {
    let points: [MobileRideMapPointDto]
    let displayPoints: [MobileRideMapRouteDisplayPoint]
    let routeID: String
    let snapshot: MobileRideMapSnapshotDto?
    let availability: MobileRideMapAvailability
    let speed: SpeedReadout
    let vehicleName: String?
    let mapError: MobileRideMapError?
    let lastDecision: MobileRideMapDecisionDto?
    let pointsTruncated: Bool
    @Binding var mapPosition: MapCameraPosition
    @Binding var isApplyingCamera: Bool
    @Binding var followsLatestPoint: Bool
    let pause: () -> Void
    let resume: () -> Void
    let save: () -> Void
    let stop: () -> Void
    let start: () -> Void
    let discard: () -> Void
    let refreshDuration: () -> Void

    @State private var isDiscardConfirmationPresented = false

    var body: some View {
        ScrollView(.vertical, showsIndicators: false) {
            VStack(spacing: 0) {
            RideMapCanvasView(
                points: displayPoints,
                routeID: routeID,
                projectionVersion: 0,
                showsStartMarker: showsRecordedBounds,
                showsEndMarker: showsRecordedBounds,
                showsCurrentMarker: true,
                fitsRouteOnChange: false,
                mapPosition: $mapPosition,
                isApplyingCamera: $isApplyingCamera,
                cameraDidChange: { _ in followsLatestPoint = false }
            )
            // Keep the live hero compact enough that the metrics and controls
            // remain above a connected TabView on the smallest iPhone.
            .frame(height: 330)
            .frame(maxWidth: .infinity)

            VStack(alignment: .leading, spacing: 12) {
                if snapshot?.state == .recording {
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

                if Self.showsPersistenceWarning(for: availability) {
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
                    points: points,
                    recordedPointCount: snapshot?.summary.pointCount,
                    rustSegmentCount: snapshot?.segmentCount ?? 0,
                    decision: lastDecision,
                    showsRecordedBounds: showsRecordedBounds
                )
                if pointsTruncated {
                    Text(localizedAppText("ride_map.live_route_truncated_count", points.count))
                        .font(.caption)
                        .foregroundStyle(PevColors.muted)
                        .accessibilityIdentifier("ride-map.live-truncated")
                }
                RideMapSummaryView(snapshot: snapshot, speed: speed, vehicleName: vehicleName)
                RideMapControlsView(
                    state: snapshot?.state,
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
            .background(PevColors.pageBackground, in: UnevenRoundedRectangle(
                topLeadingRadius: 28,
                bottomLeadingRadius: 0,
                bottomTrailingRadius: 0,
                topTrailingRadius: 28
            ))
            }
        }
        // Connected presentations place a floating TabView over the bottom
        // safe area. Keep the final metrics/control rows above that surface.
        .safeAreaInset(edge: .bottom, spacing: 0) {
            Color.clear.frame(height: 92)
        }
        .onChange(of: displayPoints.last?.sequence, initial: true) { _, _ in
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
        .task(id: snapshot?.state) {
            guard snapshot?.state == .recording else { return }
            while Task.isCancelled == false {
                refreshDuration()
                do {
                    try await Task.sleep(for: .seconds(1))
                } catch {
                    return
                }
            }
        }
    }

    private var statusTitle: String {
        switch snapshot?.state {
        case .recording:
            localizedAppText("ride_map.status.recording")
        case .paused:
            localizedAppText("ride_map.status.paused")
        case .stopped:
            localizedAppText("ride_map.status.stopped")
        case .saved:
            localizedAppText("ride_map.status.saved")
        case .discarded:
            localizedAppText("ride_map.status.discarded")
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
        case .storageUnavailable:
            localizedAppText("ride_map.persistence_unavailable")
        }
    }

    private var recordingPillText: String {
        let source = snapshot?.associatedVehicle == nil
            ? localizedAppText("ride_map.gps_only")
            : vehicleName
                ?? localizedAppText("ride_map.vehicle_name_unavailable")
        // Keep the localized lifecycle label and preserve the device's persisted
        // display name (`NF2557`) exactly as the user knows it.
        return "\(localizedAppText("ride_map.status.recording")) · \(source)"
    }

    static func showsRecordedBounds(for state: MobileRideMapStateDto?) -> Bool {
        switch state {
        case .stopped, .saved, .discarded:
            true
        case .recording, .paused, nil:
            false
        }
    }

    static func showsRecordedBounds(
        for state: MobileRideMapStateDto?,
        pointsTruncated: Bool
    ) -> Bool {
        showsRecordedBounds(for: state) && !pointsTruncated
    }

    @MainActor
    static func showsPersistenceWarning(for availability: MobileRideMapAvailability) -> Bool {
        availability == .storageUnavailable
    }

    private var showsRecordedBounds: Bool {
        Self.showsRecordedBounds(for: snapshot?.state, pointsTruncated: pointsTruncated)
    }

    private func recenterOnLatestPoint() {
        guard let point = displayPoints.last else { return }
        followsLatestPoint = true
        isApplyingCamera = true
        mapPosition = .region(Self.routeRegion(centeredOn: point, points: displayPoints))
    }

    static func routeRegion(
        centeredOn latest: MobileRideMapRouteDisplayPoint,
        points: [MobileRideMapRouteDisplayPoint]
    ) -> MKCoordinateRegion {
        let center = CLLocationCoordinate2D(
            latitude: latest.latitudeDegrees,
            longitude: latest.longitudeDegrees
        )
        let span = RideMapCanvasView.region(for: points)?.span
            ?? MKCoordinateSpan(latitudeDelta: 0.01, longitudeDelta: 0.01)
        return MKCoordinateRegion(center: center, span: span)
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
