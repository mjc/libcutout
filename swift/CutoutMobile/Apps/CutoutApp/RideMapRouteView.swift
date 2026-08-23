import CoreLocation
import CutoutMobile
import CutoutMobileFFI
import MapKit
import SwiftUI

struct RideMapRouteView: View {
    private enum Mode: String, CaseIterable {
        case live
        case history
    }

    @Bindable var model: CutoutAppModel
    private let openHistory: ((String) -> Void)?
    private let closeDetail: (() -> Void)?
    private let initialHistoryID: String?
    private let detailOnly: Bool
    @Environment(\.dismiss) private var dismiss
    @State private var mode = Mode.live
    @State private var mapPosition: MapCameraPosition = .automatic
    @State private var isDiscardConfirmationPresented = false
    @State private var followsLatestPoint = true
    @State private var isApplyingCamera = false

    init(
        model: CutoutAppModel,
        _ openHistory: ((String) -> Void)? = nil,
        initialHistoryID: String? = nil,
        detailOnly: Bool = false,
        closeDetail: (() -> Void)? = nil
    ) {
        self._model = Bindable(wrappedValue: model)
        self.openHistory = openHistory
        self.closeDetail = closeDetail
        self.initialHistoryID = initialHistoryID
        self.detailOnly = detailOnly
        _mode = State(initialValue: initialHistoryID == nil ? .live : .history)
    }

    var body: some View {
        VStack(spacing: 0) {
            if detailOnly {
                historyDetailContent
            } else {
                Picker(localizedAppText("navigation.section.map"), selection: $mode) {
                    Text(localizedAppText("ride_map.mode.live")).tag(Mode.live)
                    Text(localizedAppText("ride_map.mode.history")).tag(Mode.history)
                }
                .pickerStyle(.segmented)
                .padding(.horizontal, 20)
                .padding(.vertical, 12)
                .accessibilityIdentifier("ride-map.mode-picker")

                if mode == .live {
                    liveContent
                } else {
                    historyContent
                }
            }
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .top)
        .background(PevColors.pageBackground)
        .foregroundStyle(PevColors.primaryText)
        .accessibilityIdentifier("ride-map.screen")
        .onChange(of: model.rideMapPoints.last?.sequence) { _, _ in
            guard mode == .live, followsLatestPoint else { return }
            recenterOnLatestPoint()
        }
        .confirmationDialog(
            localizedAppText("ride_map.discard_confirm_title"),
            isPresented: $isDiscardConfirmationPresented,
            titleVisibility: .visible
        ) {
            Button(localizedAppText("ride_map.discard"), role: .destructive) {
                _ = model.discardRideMap()
            }
            Button(localizedAppText("common.cancel"), role: .cancel) {}
        }
    }

    private var historyDetailContent: some View {
        VStack(spacing: 0) {
            HStack {
                Button {
                    if let closeDetail {
                        closeDetail()
                    } else {
                        dismiss()
                    }
                } label: {
                    Label(
                        localizedAppText("ride_map.detail_back"),
                        systemImage: "chevron.left"
                    )
                }
                .buttonStyle(.bordered)
                Spacer()
                Text(localizedAppText("ride_map.detail_title"))
                    .font(.headline)
                Spacer()
            }
            .padding(.horizontal, 20)
            .padding(.vertical, 12)

            routeMap
                .frame(minHeight: 260, maxHeight: .infinity)

            if let ride = model.rideMapHistory.first(where: { $0.rideId == initialHistoryID }) {
                VStack(alignment: .leading, spacing: 8) {
                    Text(localizedAppText("ride_map.distance", distanceText(for: ride.summary)))
                    Text(localizedAppText("ride_map.points", ride.summary.pointCount))
                    RideMapRouteTruthView(points: model.rideMapHistoryPoints, decision: nil)
                    if model.rideMapHistoryPointsTruncated {
                        Text(localizedAppText("ride_map.history_truncated"))
                            .font(.caption)
                            .foregroundStyle(PevColors.muted)
                            .accessibilityIdentifier("ride-map.detail-truncated")
                    }
                }
                .font(.subheadline)
                .padding(20)
                .frame(maxWidth: .infinity, alignment: .leading)
            }
        }
        .task {
            if let initialHistoryID {
                if model.rideMapHistory.contains(where: { $0.rideId == initialHistoryID }) {
                    model.selectRideMapHistory(initialHistoryID)
                } else {
                    model.loadRideMapHistory(selecting: initialHistoryID)
                }
            } else if model.rideMapHistory.isEmpty {
                model.loadRideMapHistory()
            }
        }
        .accessibilityIdentifier("ride-map.detail")
    }

    private var liveContent: some View {
        VStack(spacing: 0) {
            routeMap
                .frame(minHeight: 260, maxHeight: .infinity)

            VStack(alignment: .leading, spacing: 12) {
                if model.rideMapAvailability != .ready {
                    Label(
                        rideMapAvailabilityText,
                        systemImage: "location.slash"
                    )
                    .font(.subheadline)
                    .foregroundStyle(.orange)
                    .accessibilityIdentifier("ride-map.location-availability")
                }
                if model.rideMapStorageError != nil {
                    Label(
                        localizedAppText("ride_map.persistence_unavailable"),
                        systemImage: "exclamationmark.triangle.fill"
                    )
                    .font(.subheadline)
                    .foregroundStyle(.orange)
                    .accessibilityIdentifier("ride-map.persistence-warning")
                }
                if model.rideMapError != nil {
                    Label(
                        localizedAppText("ride_map.command_failed"),
                        systemImage: "exclamationmark.circle.fill"
                    )
                    .font(.subheadline)
                    .foregroundStyle(.orange)
                    .accessibilityIdentifier("ride-map.command-error")
                }
                Text(statusTitle)
                    .font(.headline)
                    .accessibilityAddTraits(.isHeader)

                Text(accessibilityRouteSummary)
                    .font(.subheadline)
                    .foregroundStyle(PevColors.muted)
                    .accessibilityLabel(localizedAppText("ride_map.map_alternative"))

                RideMapRouteTruthView(
                    points: visiblePoints,
                    decision: mode == .live ? model.rideMapLastDecision : nil
                )

                if model.rideMapLivePointsTruncated {
                    Text(localizedAppText("ride_map.live_route_truncated"))
                        .font(.caption)
                        .foregroundStyle(PevColors.muted)
                        .accessibilityIdentifier("ride-map.live-truncated")
                }

                summary
                controls
                cameraControls
            }
            .padding(.horizontal, 20)
            .padding(.top, 14)
            .padding(.bottom, 20)
            .frame(maxWidth: .infinity, alignment: .leading)
            .background(PevColors.pageBackground)
        }
    }

    private var routeMap: some View {
        RideMapCanvasView(
            points: visiblePoints,
            routeID: mode == .live
                ? model.rideMapSnapshot?.rideId ?? "live"
                : model.selectedRideMapHistoryID ?? "history",
            mapPosition: $mapPosition,
            isApplyingCamera: $isApplyingCamera,
            cameraDidChange: { followsLatestPoint = false }
        )
    }

    private var historyContent: some View {
        VStack(spacing: 0) {
            if model.isRideMapRecording {
                HStack(spacing: 12) {
                    Label(
                        localizedAppText("ride_map.history_recording_continues"),
                        systemImage: "record.circle.fill"
                    )
                    .font(.subheadline)
                    Spacer()
                    Button(localizedAppText("ride_map.return_live")) {
                        returnToLive()
                    }
                    .buttonStyle(.bordered)
                    .accessibilityIdentifier("ride-map.return-live")
                }
                .padding(.horizontal, 20)
                .padding(.vertical, 10)
                .background(PevColors.pageBackground)
                .accessibilityIdentifier("ride-map.history-recording-banner")
            }
            if model.filteredRideMapHistory.isEmpty {
                VStack(alignment: .leading, spacing: 12) {
                    Image(systemName: "clock.arrow.circlepath")
                        .font(.largeTitle)
                        .accessibilityHidden(true)
                    Text(localizedAppText("ride_map.mode.history"))
                        .font(.title2.weight(.semibold))
                    Text(localizedAppText("ride_map.history_empty"))
                        .foregroundStyle(PevColors.muted)
                    Text(localizedAppText("ride_map.map_alternative"))
                        .font(.subheadline)
                        .foregroundStyle(PevColors.muted)
                    if model.rideMapHistoryCanLoadMore {
                        Button(localizedAppText("ride_map.history_load_more")) {
                            model.loadMoreRideMapHistory()
                        }
                        .buttonStyle(.bordered)
                        .accessibilityIdentifier("ride-map.history-load-more")
                    }
                }
                .padding(24)
                .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
                .accessibilityElement(children: .combine)
                .accessibilityIdentifier("ride-map.history-empty")
            } else {
                routeMap
                    .frame(minHeight: 260, maxHeight: .infinity)
                RideMapHistoryListView(
                    rides: model.filteredRideMapHistory,
                    searchText: $model.rideMapHistorySearchText,
                    canLoadMore: model.rideMapHistoryCanLoadMore,
                    select: { rideID in
                        model.selectRideMapHistory(rideID)
                        openHistory?(rideID)
                    },
                    loadMore: { model.loadMoreRideMapHistory() }
                )
                if model.rideMapHistoryPointsTruncated {
                    Text(localizedAppText("ride_map.history_truncated"))
                        .font(.caption)
                        .foregroundStyle(PevColors.muted)
                        .padding(.horizontal, 20)
                        .accessibilityIdentifier("ride-map.history-truncated")
                }
            }
        }
        .onAppear {
            if model.rideMapHistory.isEmpty {
                model.loadRideMapHistory()
            }
        }
    }

    private var summary: some View {
        RideMapSummaryView(snapshot: model.rideMapSnapshot)
    }

    private var controls: some View {
        RideMapControlsView(
            state: model.rideMapSnapshot?.state,
            isDiscardConfirmationPresented: $isDiscardConfirmationPresented,
            pause: { _ = model.pauseRideMap() },
            resume: { _ = model.resumeRideMap() },
            save: { _ = model.saveRideMap() },
            stop: { _ = model.stopRideMap() },
            start: { _ = model.startGpsOnlyRide() }
        )
    }

    private var statusTitle: String {
        switch model.rideMapSnapshot?.state {
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

    private var rideMapAvailabilityText: String {
        switch model.rideMapAvailability {
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

    private var visiblePoints: [MobileRideMapPointDto] {
        mode == .live ? model.rideMapPoints : model.rideMapHistoryPoints
    }

    private func coordinate(for point: MobileRideMapPointDto) -> CLLocationCoordinate2D {
        CLLocationCoordinate2D(latitude: point.latitudeDegrees, longitude: point.longitudeDegrees)
    }

    @ViewBuilder
    private var cameraControls: some View {
        HStack(spacing: 12) {
            Button {
                returnToLive()
            } label: {
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

    private func recenterOnLatestPoint() {
        guard let point = visiblePoints.last else { return }
        isApplyingCamera = true
        mapPosition = .region(
            MKCoordinateRegion(
                center: coordinate(for: point),
                span: MKCoordinateSpan(latitudeDelta: 0.01, longitudeDelta: 0.01)
            )
        )
        DispatchQueue.main.async {
            isApplyingCamera = false
        }
    }

    private func returnToLive() {
        mode = .live
        followsLatestPoint = true
        recenterOnLatestPoint()
    }

    private var accessibilityRouteSummary: String {
        let points = visiblePoints
        guard !points.isEmpty else {
            return localizedAppText("ride_map.no_points")
        }
        return localizedAppText("ride_map.points", points.count)
    }

    private func distanceText(for snapshot: MobileRideMapSnapshotDto) -> String {
        Measurement(value: snapshot.summary.distanceMeters, unit: UnitLength.meters)
            .formatted(.measurement(width: .abbreviated, usage: .road))
    }

    private func distanceText(for summary: MobileRideMapSummaryDto) -> String {
        Measurement(value: summary.distanceMeters, unit: UnitLength.meters)
            .formatted(.measurement(width: .abbreviated, usage: .road))
    }
}
