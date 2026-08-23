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

    let model: CutoutAppModel
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
        self.model = model
        self.openHistory = openHistory
        self.closeDetail = closeDetail
        self.initialHistoryID = initialHistoryID
        self.detailOnly = detailOnly
        _mode = State(initialValue: initialHistoryID == nil ? .live : .history)
    }

    var body: some View {
        @Bindable var model = model
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
                    searchText: Binding(
                        get: { model.rideMapHistorySearchText },
                        set: { model.rideMapHistorySearchText = $0 }
                    ),
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
            model: model,
            isDiscardConfirmationPresented: $isDiscardConfirmationPresented
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

private struct RideMapCanvasView: View {
    private struct SegmentPath: Identifiable {
        let id: UInt64
        let coordinates: [CLLocationCoordinate2D]
    }

    let points: [MobileRideMapPointDto]
    @Binding var mapPosition: MapCameraPosition
    @Binding var isApplyingCamera: Bool
    let cameraDidChange: () -> Void

    var body: some View {
        Map(position: $mapPosition, interactionModes: [.pan, .zoom]) {
            ForEach(segmentPaths) { segment in
                MapPolyline(coordinates: segment.coordinates)
                    .stroke(.blue, lineWidth: 4)
            }
            if let first = points.first {
                Marker(
                    localizedAppText("ride_map.start_marker"),
                    coordinate: coordinate(for: first)
                )
                .tint(.green)
            }
            if let last = points.last, points.count > 1 {
                Marker(
                    localizedAppText("ride_map.current_marker"),
                    coordinate: coordinate(for: last)
                )
                .tint(.blue)
            }
        }
        .mapStyle(.standard)
        .onMapCameraChange(frequency: .onEnd) { _ in
            if isApplyingCamera == false {
                cameraDidChange()
            }
        }
        .accessibilityLabel(localizedAppText("ride_map.map_alternative"))
        .accessibilityIdentifier("ride-map.map")
    }

    private var segmentPaths: [SegmentPath] {
        let grouped = Dictionary(grouping: points, by: \.segmentId)
        return grouped.keys.sorted().compactMap { segmentID in
            guard let points = grouped[segmentID] else { return nil }
            let coordinates = points.sorted { $0.sequence < $1.sequence }.map {
                CLLocationCoordinate2D(latitude: $0.latitudeDegrees, longitude: $0.longitudeDegrees)
            }
            guard !coordinates.isEmpty else { return nil }
            return SegmentPath(id: segmentID, coordinates: coordinates)
        }
    }

    private func coordinate(for point: MobileRideMapPointDto) -> CLLocationCoordinate2D {
        CLLocationCoordinate2D(latitude: point.latitudeDegrees, longitude: point.longitudeDegrees)
    }
}

private struct RideMapHistoryListView: View {
    let rides: [MobileRideMapHistorySummaryDto]
    @Binding var searchText: String
    let canLoadMore: Bool
    let select: (String) -> Void
    let loadMore: () -> Void

    var body: some View {
        VStack(spacing: 0) {
            List(rides, id: \.rideId) { ride in
                Button {
                    select(ride.rideId)
                } label: {
                    VStack(alignment: .leading, spacing: 4) {
                        Text(ride.rideId)
                            .font(.headline)
                        Text(localizedAppText("ride_map.distance", distanceText(for: ride.summary)))
                        Text(localizedAppText("ride_map.points", ride.summary.pointCount))
                        if let vehicle = ride.associatedVehicle {
                            Text(localizedAppText("ride_map.associated_vehicle", vehicle))
                        } else if let candidate = ride.candidateVehicle {
                            Text(localizedAppText("ride_map.candidate_vehicle", candidate))
                        }
                    }
                }
                .buttonStyle(.plain)
                .accessibilityIdentifier("ride-map.history-\(ride.rideId)")
            }
            .frame(maxHeight: 240)
            .searchable(text: $searchText)
            if canLoadMore {
                Button(localizedAppText("ride_map.history_load_more"), action: loadMore)
                    .buttonStyle(.bordered)
                    .accessibilityIdentifier("ride-map.history-load-more")
                    .padding(.vertical, 8)
            }
        }
    }

    private func distanceText(for summary: MobileRideMapSummaryDto) -> String {
        Measurement(value: summary.distanceMeters, unit: UnitLength.meters)
            .formatted(.measurement(width: .abbreviated, usage: .road))
    }
}

private struct RideMapRouteTruthView: View {
    let points: [MobileRideMapPointDto]
    let decision: MobileRideMapDecisionDto?

    var body: some View {
        VStack(alignment: .leading, spacing: 4) {
            Text(localizedAppText("ride_map.route_truth", segmentCount, telemetryText))
                .font(.caption)
                .foregroundStyle(PevColors.muted)
                .accessibilityIdentifier("ride-map.route-truth")
            if let decisionText {
                Label(decisionText, systemImage: decisionSystemImage)
                    .font(.caption)
                    .foregroundStyle(decisionIsAccepted ? .green : .orange)
                    .accessibilityIdentifier("ride-map.last-decision")
            }
        }
        .accessibilityElement(children: .combine)
    }

    private var segmentCount: UInt64 {
        UInt64(Set(points.map(\.segmentId)).count)
    }

    private var telemetryText: String {
        guard let state = points.last?.telemetryState else {
            return localizedAppText("ride_map.telemetry.gps_only")
        }
        switch state {
        case .gpsOnly:
            return localizedAppText("ride_map.telemetry.gps_only")
        case .associatedNoTelemetry:
            return localizedAppText("ride_map.telemetry.no_telemetry")
        case .associatedFresh:
            return localizedAppText("ride_map.telemetry.fresh")
        case .associatedStale:
            return localizedAppText("ride_map.telemetry.stale")
        }
    }

    private var decisionText: String? {
        guard let decision else { return nil }
        switch decision {
        case .accepted:
            return nil
        case let .rejected(reason), let .ignored(reason):
            switch reason {
            case .rideNotRecording:
                return localizedAppText("ride_map.decision.ride_not_recording")
            case .duplicateLocation:
                return localizedAppText("ride_map.decision.duplicate")
            case .timestampOutOfOrder:
                return localizedAppText("ride_map.decision.out_of_order")
            case .accuracyTooLow:
                return localizedAppText("ride_map.decision.accuracy")
            case .unrealisticJump:
                return localizedAppText("ride_map.decision.jump")
            }
        }
    }

    private var decisionIsAccepted: Bool {
        if case .accepted? = decision { return true }
        return false
    }

    private var decisionSystemImage: String {
        decisionIsAccepted ? "checkmark.circle" : "exclamationmark.triangle"
    }
}

private struct RideMapSummaryView: View {
    let snapshot: MobileRideMapSnapshotDto?

    var body: some View {
        if let snapshot {
            VStack(alignment: .leading, spacing: 4) {
                Text(localizedAppText("ride_map.distance", distanceText(for: snapshot)))
                Text(localizedAppText("ride_map.duration", durationText(for: snapshot)))
                Text(localizedAppText("ride_map.points", snapshot.summary.pointCount))
                if let vehicle = snapshot.associatedVehicle {
                    Text(localizedAppText("ride_map.associated_vehicle", vehicle))
                } else {
                    Text(localizedAppText("ride_map.gps_only"))
                }
            }
            .font(.subheadline.monospacedDigit())
            .accessibilityElement(children: .combine)
            .accessibilityIdentifier("ride-map.summary")
        } else {
            Text(localizedAppText("ride_map.no_active"))
                .font(.subheadline)
                .accessibilityIdentifier("ride-map.no-active")
        }
    }

    private func distanceText(for snapshot: MobileRideMapSnapshotDto) -> String {
        Measurement(value: snapshot.summary.distanceMeters, unit: UnitLength.meters)
            .formatted(.measurement(width: .abbreviated, usage: .road))
    }

    private func durationText(for snapshot: MobileRideMapSnapshotDto) -> String {
        let totalSeconds = snapshot.summary.durationMilliseconds / 1_000
        let hours = totalSeconds / 3_600
        let minutes = (totalSeconds % 3_600) / 60
        let seconds = totalSeconds % 60
        if hours > 0 {
            return "\(hours)h \(minutes)m \(seconds)s"
        }
        return "\(minutes)m \(seconds)s"
    }
}

private struct RideMapControlsView: View {
    let model: CutoutAppModel
    @Binding var isDiscardConfirmationPresented: Bool

    var body: some View {
        switch model.rideMapSnapshot?.state {
        case .recording:
            HStack(spacing: 12) {
                Button {
                    _ = model.pauseRideMap()
                } label: {
                    Label(localizedAppText("ride_map.pause"), systemImage: "pause.fill")
                }
                .buttonStyle(.bordered)
                .accessibilityIdentifier("ride-map.pause")

                stopButton(prominent: true)
            }
        case .paused:
            HStack(spacing: 12) {
                Button {
                    _ = model.resumeRideMap()
                } label: {
                    Label(localizedAppText("ride_map.resume"), systemImage: "play.fill")
                }
                .buttonStyle(.borderedProminent)
                .accessibilityIdentifier("ride-map.resume")

                stopButton(prominent: false)
            }
        case .stopped:
            HStack(spacing: 12) {
                Button {
                    _ = model.saveRideMap()
                } label: {
                    Label(localizedAppText("ride_map.save"), systemImage: "checkmark.circle.fill")
                }
                .buttonStyle(.borderedProminent)
                .accessibilityIdentifier("ride-map.save")

                Button(role: .destructive) {
                    isDiscardConfirmationPresented = true
                } label: {
                    Label(localizedAppText("ride_map.discard"), systemImage: "trash")
                }
                .buttonStyle(.bordered)
                .accessibilityIdentifier("ride-map.discard")
            }
        case .saved, .discarded:
            startButton(label: localizedAppText("ride_map.start_new"))
        case nil:
            startButton(label: localizedAppText("ride_map.start"))
        }
    }

    @ViewBuilder
    private func stopButton(prominent: Bool) -> some View {
        if prominent {
            Button(role: .destructive) {
                _ = model.stopRideMap()
            } label: {
                Label(localizedAppText("ride_map.stop"), systemImage: "stop.fill")
            }
            .buttonStyle(.borderedProminent)
            .accessibilityIdentifier("ride-map.stop")
        } else {
            Button(role: .destructive) {
                _ = model.stopRideMap()
            } label: {
                Label(localizedAppText("ride_map.stop"), systemImage: "stop.fill")
            }
            .buttonStyle(.bordered)
            .accessibilityIdentifier("ride-map.stop")
        }
    }

    private func startButton(label: String) -> some View {
        Button {
            _ = model.startGpsOnlyRide()
        } label: {
            Label(label, systemImage: "location.fill")
                .frame(maxWidth: .infinity, minHeight: 44)
        }
        .buttonStyle(.borderedProminent)
        .accessibilityIdentifier("ride-map.start")
    }
}
