import CoreLocation
import MapKit
import SwiftUI
import CutoutMobile
import CutoutMobileFFI

struct RideMapLiveContentView: View {
    let points: [MobileRideMapPointDto]
    let routeID: String
    let snapshot: MobileRideMapSnapshotDto?
    let availability: MobileRideMapAvailability
    let vehicleName: String?
    let storageError: String?
    let mapError: MobileRideMapError?
    let lastDecision: MobileRideMapDecisionDto?
    let pointsTruncated: Bool
    let pause: () -> Void
    let resume: () -> Void
    let save: () -> Void
    let stop: () -> Void
    let start: () -> Void
    let discard: () -> Void
    let refreshDuration: () -> Void

    @State private var mapPosition: MapCameraPosition = .automatic
    @State private var isApplyingCamera = false
    @State private var followsLatestPoint = true
    @State private var isDiscardConfirmationPresented = false

    var body: some View {
        ScrollView(.vertical, showsIndicators: false) {
            VStack(spacing: 0) {
            RideMapCanvasView(
                points: points,
                routeID: routeID,
                showsStartMarker: snapshot?.state != .recording,
                showsEndMarker: showsEndMarker,
                fitsRouteOnChange: false,
                mapPosition: $mapPosition,
                isApplyingCamera: $isApplyingCamera,
                cameraDidChange: { followsLatestPoint = false }
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
                        .padding(.horizontal, 10)
                        .padding(.vertical, 6)
                        .background(PevColors.green, in: Capsule())
                        .accessibilityIdentifier("ride-map.recording-pill")
                }

                if availability != .ready {
                    Label(availabilityText, systemImage: "location.slash")
                        .font(.subheadline)
                        .foregroundStyle(.orange)
                        .accessibilityIdentifier("ride-map.location-availability")
                }
                if storageError != nil {
                    Label(
                        localizedAppText("ride_map.persistence_unavailable"),
                        systemImage: "exclamationmark.triangle.fill"
                    )
                    .font(.subheadline)
                    .foregroundStyle(.orange)
                    .accessibilityIdentifier("ride-map.persistence-warning")
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
                Text(accessibilityRouteSummary)
                    .font(.subheadline)
                    .foregroundStyle(PevColors.muted)
                    .accessibilityLabel(localizedAppText("ride_map.map_alternative"))

                RideMapRouteTruthView(
                    points: points,
                    rustSegmentCount: snapshot?.segmentCount,
                    decision: lastDecision,
                    showsRecordedBounds: snapshot?.state != .recording
                )
                if pointsTruncated {
                    Text(localizedAppText("ride_map.live_route_truncated"))
                        .font(.caption)
                        .foregroundStyle(PevColors.muted)
                        .accessibilityIdentifier("ride-map.live-truncated")
                }
                RideMapSummaryView(snapshot: snapshot, vehicleName: vehicleName)
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
        .onChange(of: points.last?.sequence, initial: true) { _, _ in
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
                ?? localizedAppText("ride_map.associated_vehicle", snapshot?.associatedVehicle ?? "")
        // Keep the lifecycle label emphatic, but preserve the device's persisted
        // display name (`NF2557`) exactly as the user knows it.
        return "\(localizedAppText("ride_map.status.recording").uppercased()) · \(source)"
    }

    private var showsEndMarker: Bool {
        switch snapshot?.state {
        case .stopped, .saved, .discarded:
            true
        case .recording, .paused, nil:
            false
        }
    }

    private var accessibilityRouteSummary: String {
        points.isEmpty
            ? localizedAppText("ride_map.no_points")
            : localizedAppText("ride_map.points", points.count)
    }

    private func recenterOnLatestPoint() {
        guard let point = points.last else { return }
        isApplyingCamera = true
        mapPosition = routeRegion(centeredOn: point)
    }

    private func routeRegion(centeredOn latest: MobileRideMapPointDto) -> MapCameraPosition {
        guard let region = RideMapCanvasView.region(for: points) else {
            return .region(MKCoordinateRegion(
                center: CLLocationCoordinate2D(
                    latitude: latest.latitudeDegrees,
                    longitude: latest.longitudeDegrees
                ),
                span: MKCoordinateSpan(latitudeDelta: 0.01, longitudeDelta: 0.01)
            ))
        }
        return .region(region)
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
