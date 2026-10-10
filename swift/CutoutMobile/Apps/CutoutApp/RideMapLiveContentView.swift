import CutoutMobile
import MapKit
import SwiftUI

#if os(iOS)
    import UIKit
#endif

struct RideMapLiveContentView: View {
    @Environment(\.musicCompactPlayerFrame) private var musicCompactPlayerFrame

    let displayPoints: [MobileRideMapRouteDisplayPoint]
    let routeID: String
    let projectionVersion: UInt64
    let endpointMetadata: MobileRideMapRouteEndpointMetadata
    let cameraRegion: MobileRideMapCameraRegion?
    let segments: [MobileRideMapSegmentDisplayMetadata]
    let snapshot: MobileRideMapSnapshotDto?
    let isInitialSnapshotPending: Bool
    let availability: MobileRideMapAvailability
    let storageError: String?
    let speed: SpeedReadout
    let vehicleName: String?
    let mapError: MobileRideMapError?
    let telemetryState: MobileRideMapTelemetryStateDto?
    let pointsTruncated: Bool
    let segmentsOmittedByBudget: Bool
    let canonicalBackgroundGapCount: UInt64
    let onVisibilityChange: (Bool) -> Void
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
        GeometryReader { viewport in
            let visibleHeight = RideMapViewportLayout.visibleHeight(
                in: viewport, musicPlayerFrame: musicCompactPlayerFrame)
            VStack(spacing: 0) {
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
                            fitsRouteOnChange: isTerminalRide,
                            cameraFitID: cameraFitID,
                            mapPosition: $mapPosition,
                            isApplyingCamera: $isApplyingCamera,
                            cameraDidChange: { region in
                                followSpan = region.span
                                followsLatestPoint = false
                            }
                        )
                        .frame(height: RideMapViewportLayout.mapHeight(for: visibleHeight))
                        .frame(maxWidth: .infinity)
                        .overlay(alignment: .topTrailing) {
                            RideMapCameraControlsView(
                                mode: cameraActionMode,
                                action: performCameraAction
                            )
                            .padding(12)
                        }

                        VStack(alignment: .leading, spacing: 12) {
                            RideMapLiveStatusView(
                                displayPointCount: displayPoints.count,
                                snapshot: snapshot,
                                isInitialSnapshotPending: isInitialSnapshotPending,
                                availability: availability,
                                storageError: storageError,
                                vehicleName: vehicleName,
                                mapError: mapError,
                                telemetryState: telemetryState,
                                pointsTruncated: pointsTruncated,
                                segmentsOmittedByBudget: segmentsOmittedByBudget,
                                segments: segments,
                                canonicalBackgroundGapCount: canonicalBackgroundGapCount
                            )
                            RideMapSummaryView(snapshot: snapshot, speed: speed, vehicleName: vehicleName)
                            RideMapControlsView(
                                state: snapshot?.state,
                                allowedActions: Self.allowedControlActions(
                                    isInitialSnapshotPending: isInitialSnapshotPending,
                                    availability: availability,
                                    storageError: storageError,
                                    mapError: mapError,
                                    snapshotAllowedActions: snapshot?.allowedActions
                                ),
                                isDiscardConfirmationPresented: $isDiscardConfirmationPresented,
                                pause: pause,
                                resume: resume,
                                save: save,
                                stop: stop,
                                start: start
                            )

                        }
                        .padding(.horizontal, 16)
                        .padding(.top, 16)
                        .padding(.bottom, 16)
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
            }
            .frame(height: visibleHeight, alignment: .top)
            // Native scroll views extend into TabView's safe area; bound drawing
            // and hit testing to the measured viewport above its music accessory.
            .clipped()
            .contentShape(Rectangle())
            .accessibilityElement(children: .contain)
            .accessibilityIdentifier("ride-map.live-viewport")
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
        .onAppear { onVisibilityChange(true) }
        .onDisappear { onVisibilityChange(false) }
    }

    enum CameraActionMode: Equatable {
        case following
        case notFollowing
        case showFullRide
        case unavailable
    }

    private var cameraActionMode: CameraActionMode {
        Self.cameraActionMode(
            state: snapshot?.state,
            hasPoints: displayPoints.isEmpty == false,
            hasCameraRegion: cameraRegion != nil,
            followsLatestPoint: followsLatestPoint
        )
    }

    static func cameraActionMode(
        state: MobileRideMapStateDto?,
        hasPoints: Bool,
        hasCameraRegion: Bool,
        followsLatestPoint: Bool
    ) -> CameraActionMode {
        guard hasPoints, hasCameraRegion else { return .unavailable }
        switch state {
        case .active, .paused:
            return followsLatestPoint ? .following : .notFollowing
        case .stopped, .interrupted, .discarded, .saved, .imported:
            return .showFullRide
        case .draft, nil:
            return .unavailable
        }
    }

    static func allowedControlActions(
        isInitialSnapshotPending: Bool,
        availability: MobileRideMapAvailability,
        storageError: String?,
        mapError: MobileRideMapError? = nil,
        snapshotAllowedActions: [MobileRideMapActionDto]?
    ) -> [MobileRideMapActionDto] {
        if let snapshotAllowedActions { return snapshotAllowedActions }
        guard !isInitialSnapshotPending,
            !hasStorageFailure(availability: availability, storageError: storageError, mapError: mapError)
        else { return [] }
        return [.start]
    }

    static func emptySnapshotStatusKey(
        isInitialSnapshotPending: Bool,
        availability: MobileRideMapAvailability,
        storageError: String?,
        mapError: MobileRideMapError? = nil
    ) -> String {
        if hasStorageFailure(availability: availability, storageError: storageError, mapError: mapError) {
            return "ride_map.persistence_unavailable"
        }
        return isInitialSnapshotPending ? "ride_map.initial_snapshot_loading" : "ride_map.no_active"
    }

    private static func hasStorageFailure(
        availability: MobileRideMapAvailability,
        storageError: String?,
        mapError: MobileRideMapError?
    ) -> Bool {
        if availability == .storageUnavailable || storageError != nil { return true }
        if case .storageError? = mapError { return true }
        return false
    }

    static func supportsLatestPointFollow(state: MobileRideMapStateDto?) -> Bool {
        switch state {
        case .active, .paused:
            return true
        case .draft, .stopped, .interrupted, .discarded, .saved, .imported, nil:
            return false
        }
    }

    private func showFullRide() {
        guard cameraActionMode == .showFullRide,
            let cameraRegion,
            let region = RideMapCanvasView.mapRegion(for: cameraRegion)
        else { return }
        followsLatestPoint = false
        followSpan = nil
        isApplyingCamera = true
        mapPosition = .region(region)
    }

    private var showsRecordedBounds: Bool {
        snapshot?.recordedBoundsAvailable == true
    }

    private var isTerminalRide: Bool {
        guard let state = snapshot?.state else { return false }
        switch state {
        case .active, .paused:
            return false
        case .draft, .stopped, .interrupted, .discarded, .saved, .imported:
            return true
        }
    }

    private var cameraFitID: String {
        isTerminalRide ? "\(routeID):terminal" : routeID
    }

    private func performCameraAction() {
        switch cameraActionMode {
        case .following, .notFollowing:
            recenterOnLatestPoint()
        case .showFullRide:
            showFullRide()
        case .unavailable:
            return
        }
    }

    private func recenterOnLatestPoint() {
        guard Self.supportsLatestPointFollow(state: snapshot?.state),
            let point = displayPoints.last,
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

private struct RideMapCameraControlsView: View {
    let mode: RideMapLiveContentView.CameraActionMode
    let action: () -> Void

    var body: some View {
        let button = Button(action: action) {
            Image(systemName: symbolName)
                .font(.body.weight(.semibold))
                .frame(width: 44, height: 44)
                .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .foregroundStyle(mode == .unavailable ? PevColors.muted : PevColors.brand)
        .background(PevColors.cardFill, in: RoundedRectangle(cornerRadius: 14))
        .overlay {
            RoundedRectangle(cornerRadius: 14).strokeBorder(PevColors.cardStroke, lineWidth: 1)
        }
        .disabled(mode == .unavailable)
        .accessibilityLabel(localizedAppText(mode == .showFullRide ? "ride_map.show_full_ride" : "ride_map.recenter"))
        .accessibilityIdentifier("ride-map.recenter")

        switch mode {
        case .following:
            button.accessibilityValue(localizedAppText("ride_map.following"))
        case .notFollowing:
            button.accessibilityValue(localizedAppText("ride_map.not_following"))
        case .showFullRide, .unavailable:
            button
        }
    }

    private var symbolName: String {
        switch mode {
        case .following:
            "location.fill"
        case .notFollowing, .unavailable:
            "location"
        case .showFullRide:
            "viewfinder"
        }
    }
}

private struct RideMapLiveStatusView: View {
    @Environment(\.openURL) private var openURL

    let displayPointCount: Int
    let snapshot: MobileRideMapSnapshotDto?
    let isInitialSnapshotPending: Bool
    let availability: MobileRideMapAvailability
    let storageError: String?
    let vehicleName: String?
    let mapError: MobileRideMapError?
    let telemetryState: MobileRideMapTelemetryStateDto?
    let pointsTruncated: Bool
    let segmentsOmittedByBudget: Bool
    let segments: [MobileRideMapSegmentDisplayMetadata]
    let canonicalBackgroundGapCount: UInt64

    var body: some View {

        if snapshot != nil && hasStorageFailure {
            Label(
                localizedAppText("ride_map.persistence_unavailable"),
                systemImage: "exclamationmark.triangle.fill"
            )
            .font(.subheadline)
            .foregroundStyle(.orange)
            .accessibilityIdentifier("ride-map.persistence-warning")
        } else if availability != .ready && availability != .storageUnavailable {
            Label(availabilityText, systemImage: "location.slash")
                .font(.subheadline)
                .foregroundStyle(.orange)
                .accessibilityIdentifier("ride-map.location-availability")
        }

        #if os(iOS)
            if snapshot?.locationAcquisition?.recoveryAction == .openSettings {
                Button {
                    openLocationSettings()
                } label: {
                    Label(localizedAppText("ride_map.open_settings"), systemImage: "gear")
                }
                .buttonStyle(.bordered)
                .accessibilityIdentifier("ride-map.location-settings")
            }
        #endif

        if mapError != nil && !isStorageError(mapError) {
            Label(
                localizedAppText("ride_map.command_failed"),
                systemImage: "exclamationmark.circle.fill"
            )
            .font(.subheadline)
            .foregroundStyle(.orange)
            .accessibilityIdentifier("ride-map.command-error")
        }

        HStack(spacing: 8) {
            if snapshot?.state == .active || snapshot?.state == .paused {
                Circle()
                    .fill(snapshot?.state == .paused ? PevColors.brand : PevColors.green)
                    .frame(width: 8, height: 8)
                    .accessibilityHidden(true)
            }
            Text(statusTitle)
                .font(.headline.weight(.semibold))
                .accessibilityAddTraits(.isHeader)
        }
        .accessibilityElement(children: .combine)
        .accessibilityIdentifier("ride-map.recording-pill")
        RideMapRouteTruthView(
            displayedPointCount: displayPointCount,
            recordedPointCount: snapshot?.summary.pointCount,
            rustSegmentCount: snapshot?.segmentCount ?? 0,
            showsRecordedBounds: snapshot?.recordedBoundsAvailable == true,
            pointsOmittedByBudget: pointsTruncated,
            segmentsOmittedByBudget: segmentsOmittedByBudget,
            segments: segments,
            canonicalBackgroundGapCount: canonicalBackgroundGapCount,
            telemetryState: telemetryState
        )
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
            localizedAppText(
                RideMapLiveContentView.emptySnapshotStatusKey(
                    isInitialSnapshotPending: isInitialSnapshotPending,
                    availability: availability,
                    storageError: storageError,
                    mapError: mapError
                )
            )
        }
    }

    private var hasStorageFailure: Bool {
        availability == .storageUnavailable || storageError != nil || isStorageError(mapError)
    }

    private func isStorageError(_ error: MobileRideMapError?) -> Bool {
        if case .storageError? = error { return true }
        return false
    }

    #if os(iOS)
        private func openLocationSettings() {
            guard let settingsURL = URL(string: UIApplication.openSettingsURLString) else { return }
            openURL(settingsURL)
        }
    #endif

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

}
