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

    private struct SegmentPath: Identifiable {
        let id: UInt64
        let coordinates: [CLLocationCoordinate2D]
    }

    let model: CutoutAppModel
    @State private var mode = Mode.live
    @State private var mapPosition: MapCameraPosition = .automatic
    @State private var isDiscardConfirmationPresented = false
    @State private var followsLatestPoint = true
    @State private var isApplyingCamera = false

    var body: some View {
        VStack(spacing: 0) {
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

    private var liveContent: some View {
        VStack(spacing: 0) {
            routeMap
                .frame(minHeight: 260, maxHeight: .infinity)

            VStack(alignment: .leading, spacing: 12) {
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
        Map(position: $mapPosition, interactionModes: [.pan, .zoom]) {
            ForEach(segmentPaths) { segment in
                MapPolyline(coordinates: segment.coordinates)
                    .stroke(.blue, lineWidth: 4)
            }
            if let first = visiblePoints.first {
                Marker(localizedAppText("ride_map.start_marker"), coordinate: coordinate(for: first))
                    .tint(.green)
            }
            if let last = visiblePoints.last, visiblePoints.count > 1 {
                Marker(localizedAppText("ride_map.current_marker"), coordinate: coordinate(for: last))
                    .tint(.blue)
            }
        }
        .mapStyle(.standard)
        .onMapCameraChange(frequency: .onEnd) { _ in
            if isApplyingCamera == false {
                followsLatestPoint = false
            }
        }
        .accessibilityLabel(localizedAppText("ride_map.map_alternative"))
        .accessibilityIdentifier("ride-map.map")
    }

    private var historyContent: some View {
        VStack(spacing: 0) {
            if model.rideMapHistory.isEmpty {
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
                }
                .padding(24)
                .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
                .accessibilityElement(children: .combine)
                .accessibilityIdentifier("ride-map.history-empty")
            } else {
                routeMap
                    .frame(minHeight: 260, maxHeight: .infinity)
                List(model.rideMapHistory, id: \.rideId) { ride in
                    Button {
                        model.selectRideMapHistory(ride.rideId)
                    } label: {
                        VStack(alignment: .leading, spacing: 4) {
                            Text(ride.rideId)
                                .font(.headline)
                            Text(localizedAppText("ride_map.distance", distanceText(for: ride.summary)))
                            Text(localizedAppText("ride_map.points", ride.summary.pointCount))
                        }
                    }
                    .buttonStyle(.plain)
                    .accessibilityIdentifier("ride-map.history-\(ride.rideId)")
                }
                .frame(maxHeight: 240)
                if model.rideMapHistoryPointsTruncated {
                    Text(localizedAppText("ride_map.history_truncated"))
                        .font(.caption)
                        .foregroundStyle(PevColors.muted)
                        .padding(.horizontal, 20)
                        .accessibilityIdentifier("ride-map.history-truncated")
                }
            }
        }
        .onAppear { model.loadRideMapHistory() }
    }

    @ViewBuilder
    private var summary: some View {
        if let snapshot = model.rideMapSnapshot {
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

    @ViewBuilder
    private var controls: some View {
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

                Button(role: .destructive) {
                    _ = model.stopRideMap()
                } label: {
                    Label(localizedAppText("ride_map.stop"), systemImage: "stop.fill")
                }
                .buttonStyle(.borderedProminent)
                .accessibilityIdentifier("ride-map.stop")
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

                Button(role: .destructive) {
                    _ = model.stopRideMap()
                } label: {
                    Label(localizedAppText("ride_map.stop"), systemImage: "stop.fill")
                }
                .buttonStyle(.bordered)
                .accessibilityIdentifier("ride-map.stop")
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

    private var segmentPaths: [SegmentPath] {
        let points = visiblePoints
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
                followsLatestPoint = true
                recenterOnLatestPoint()
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

    private func distanceText(for summary: MobileRideMapSummaryDto) -> String {
        Measurement(value: summary.distanceMeters, unit: UnitLength.meters)
            .formatted(.measurement(width: .abbreviated, usage: .road))
    }
}
