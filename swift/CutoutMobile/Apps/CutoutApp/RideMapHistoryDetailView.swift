import MapKit
import SwiftUI
import CutoutMobile
import CutoutMobileFFI

struct RideMapHistoryDetailView: View {
    let initialHistoryID: String?
    let rides: [MobileRideMapHistorySummaryDto]
    let displayPoints: [MobileRideMapRouteDisplayPoint]
    let pointsTruncated: Bool
    let historyError: MobileRideMapError?
    let routeError: MobileRideMapError?
    let isLoading: Bool
    let selectedHistoryID: String?
    let select: (String) -> Void
    let load: () -> Void
    let retry: () -> Void
    let loadFullRide: () -> Void
    let vehicleName: (String?) -> String?
    let cameraDidChange: (MKCoordinateRegion) -> Void
    @Binding var mapPosition: MapCameraPosition
    @Binding var isApplyingCamera: Bool
    let close: () -> Void

    static func resolvedVehicleLabel(
        associatedVehicle: String?,
        candidateVehicle: String?,
        resolve: (String) -> String?,
        fallback: String
    ) -> String {
        let identity = associatedVehicle ?? candidateVehicle
        return identity.flatMap(resolve)
            ?? (identity == nil ? fallback : localizedAppText("ride_map.vehicle_name_unavailable"))
    }

    static func averageSpeedText(
        distanceMeters: Double,
        durationMilliseconds: UInt64
    ) -> String {
        guard distanceMeters.isFinite,
              distanceMeters > 0,
              durationMilliseconds > 0
        else {
            return localizedAppText("ride_map.speed_unavailable")
        }
        let metersPerSecond = distanceMeters / (Double(durationMilliseconds) / 1_000)
        guard metersPerSecond.isFinite, metersPerSecond >= 0 else {
            return localizedAppText("ride_map.speed_unavailable")
        }
        let milesPerHour = metersPerSecond * 2.236_936_292_054_4
        return "\(milesPerHour.formatted(.number.precision(.fractionLength(1)))) mph"
    }

    @MainActor
    static func shouldSelectHistory(
        initialHistoryID: String?,
        selectedHistoryID: String?,
        availableHistoryIDs: [String]
    ) -> Bool {
        guard let initialHistoryID,
              availableHistoryIDs.contains(initialHistoryID)
        else {
            return false
        }
        return selectedHistoryID != initialHistoryID
    }

    private var selectedRide: MobileRideMapHistorySummaryDto? {
        guard let initialHistoryID else { return nil }
        return rides.first(where: { $0.rideId == initialHistoryID })
    }

    private var selectionTaskID: String { initialHistoryID ?? "" }

    private var isRouteLoading: Bool {
        routeError == nil && isLoading
    }

    var body: some View {
        GeometryReader { proxy in
            ScrollView(.vertical, showsIndicators: false) {
                VStack(spacing: 0) {
                    ZStack {
                        RideMapCanvasView(
                            points: displayPoints,
                            routeID: "\(initialHistoryID ?? "history-detail")-\(pointsTruncated ? "preview" : "full")",
                            showsStartMarker: true,
                            showsEndMarker: !pointsTruncated,
                            fitsRouteOnChange: true,
                            mapPosition: $mapPosition,
                            isApplyingCamera: $isApplyingCamera,
                            cameraDidChange: cameraDidChange
                        )

                        if isRouteLoading {
                            HStack(spacing: 8) {
                                ProgressView()
                                    .tint(PevColors.yellow)
                                Text(localizedAppText("ride_map.history_loading"))
                                    .font(.subheadline.weight(.semibold))
                            }
                            .padding(.horizontal, 14)
                            .padding(.vertical, 10)
                            .modifier(RideMapLoadingSurface())
                            .accessibilityIdentifier("ride-map.detail-loading")
                        } else if routeError != nil {
                            ContentUnavailableView(
                                localizedAppText("ride_map.detail_error_title"),
                                systemImage: "exclamationmark.triangle"
                            )
                            .accessibilityIdentifier("ride-map.detail-map-error")
                        } else if selectedRide?.summary.pointCount == 0 {
                            ContentUnavailableView(
                                localizedAppText("ride_map.no_points"),
                                systemImage: "location.slash"
                            )
                            .accessibilityIdentifier("ride-map.detail-no-points")
                        }
                    }
                    .frame(height: min(max(proxy.size.height * 0.58, 240), 520))

                    if let ride = selectedRide {
                        RideMapHistoryDetailSummary(
                            distance: distanceText(for: ride.summary),
                            duration: durationText(for: ride.summary),
                            averageSpeed: Self.averageSpeedText(
                                distanceMeters: ride.summary.distanceMeters,
                                durationMilliseconds: ride.summary.durationMilliseconds
                            ),
                            recordedAt: recordedAtText(for: ride.createdAtMilliseconds),
                            vehicle: vehicleLabel(for: ride),
                            telemetryState: ride.telemetryState,
                            displayPointCount: displayPoints.count,
                            recordedPointCount: ride.summary.pointCount,
                            pointsTruncated: pointsTruncated,
                            segmentCount: ride.segmentCount,
                            error: routeError,
                            isLoading: isRouteLoading,
                            loadFullRide: loadFullRide,
                            shareText: shareText(for: ride),
                            mapPosition: $mapPosition
                        )
                    } else if initialHistoryID != nil {
                        VStack(spacing: 12) {
                            ContentUnavailableView(
                                historyError == nil
                                    ? localizedAppText("ride_map.history_empty")
                                    : localizedAppText("ride_map.detail_error_title"),
                                systemImage: historyError == nil ? "map" : "exclamationmark.triangle"
                            )
                            if historyError != nil {
                                Button(localizedAppText("ride_map.history_retry"), action: retry)
                                    .buttonStyle(.borderedProminent)
                                    .tint(PevColors.yellow)
                                    .accessibilityIdentifier("ride-map.detail-retry")
                            }
                        }
                        .frame(maxWidth: .infinity, maxHeight: .infinity)
                    }
                }
            }
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .top)
        .task(id: selectionTaskID) { loadSelectionIfNeeded() }
        .accessibilityIdentifier("ride-map.detail")
    }

    private func loadSelectionIfNeeded() {
        guard let initialHistoryID else {
            if rides.isEmpty { load() }
            return
        }
        if rides.contains(where: { $0.rideId == initialHistoryID }) {
            guard Self.shouldSelectHistory(
                initialHistoryID: initialHistoryID,
                selectedHistoryID: selectedHistoryID,
                availableHistoryIDs: rides.map(\.rideId)
            ) else {
                return
            }
            select(initialHistoryID)
        } else {
            load()
        }
    }

    private func distanceText(for summary: MobileRideMapSummaryDto) -> String {
        Measurement(value: summary.distanceMeters, unit: UnitLength.meters)
            .formatted(.measurement(width: .abbreviated, usage: .road))
    }

    private func durationText(for summary: MobileRideMapSummaryDto) -> String {
        Duration.seconds(Double(summary.durationMilliseconds) / 1_000)
            .formatted(.units(allowed: [.hours, .minutes, .seconds], width: .abbreviated))
    }

    private func recordedAtText(for milliseconds: UInt64) -> String {
        Date(timeIntervalSince1970: Double(milliseconds) / 1_000)
            .formatted(.dateTime.month(.abbreviated).day().year().hour().minute())
    }

    private func vehicleLabel(for ride: MobileRideMapHistorySummaryDto) -> String {
        Self.resolvedVehicleLabel(
            associatedVehicle: ride.associatedVehicle,
            candidateVehicle: ride.candidateVehicle,
            resolve: { vehicleName($0) },
            fallback: localizedAppText("ride_map.gps_only")
        )
    }

    private func shareText(for ride: MobileRideMapHistorySummaryDto) -> String {
        let distance = distanceText(for: ride.summary)
        let duration = durationText(for: ride.summary)
        let title = localizedAppText("ride_map.detail_title")
        return "\(title)\n\(distance) · \(duration) · \(RideMapHistoryListView.pointCountText(ride.summary.pointCount))"
    }
}

private struct RideMapHistoryDetailSummary: View {
    let distance: String
    let duration: String
    let averageSpeed: String
    let recordedAt: String
    let vehicle: String
    let telemetryState: MobileRideMapTelemetryStateDto
    let displayPointCount: Int
    let recordedPointCount: UInt64
    let pointsTruncated: Bool
    let segmentCount: UInt64
    let error: MobileRideMapError?
    let isLoading: Bool
    let loadFullRide: () -> Void
    let shareText: String
    @Binding var mapPosition: MapCameraPosition

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            if isLoading {
                HStack(spacing: 8) {
                    ProgressView()
                        .tint(PevColors.yellow)
                    Text(localizedAppText("ride_map.history_loading"))
                        .font(.subheadline.weight(.semibold))
                        .foregroundStyle(PevColors.muted)
                }
                .accessibilityIdentifier("ride-map.detail-summary-loading")
            } else {
                if error != nil {
                    Label(
                        localizedAppText("ride_map.command_failed"),
                        systemImage: "exclamationmark.triangle"
                    )
                    .font(.caption)
                    .foregroundStyle(.orange)
                    .accessibilityIdentifier("ride-map.detail-error")
                }
                VStack(alignment: .leading, spacing: 3) {
                    Text(recordedAt)
                        .font(.subheadline.weight(.semibold))
                    Text(vehicle)
                        .font(.caption)
                        .foregroundStyle(PevColors.muted)
                }
                .accessibilityElement(children: .combine)
                HStack(alignment: .firstTextBaseline, spacing: 0) {
                    RideMapDetailMetric(
                        value: distance,
                        label: localizedAppText("ride_map.metric_distance")
                    )
                    RideMapDetailMetric(
                        value: duration,
                        label: localizedAppText("ride_map.metric_elapsed")
                    )
                    RideMapDetailMetric(
                        value: averageSpeed,
                        label: localizedAppText("ride_map.metric_average_speed")
                    )
                }
                RideMapRouteTruthView(
                    points: [],
                    recordedPointCount: recordedPointCount,
                    rustSegmentCount: segmentCount,
                    decision: nil,
                    showsRecordedBounds: !pointsTruncated,
                    hasRoute: displayPointCount > 0,
                    telemetryState: telemetryState
                )
                if pointsTruncated {
                    Text(localizedAppText("ride_map.history_truncated_count", displayPointCount))
                        .font(.caption)
                        .foregroundStyle(PevColors.muted)
                        .accessibilityIdentifier("ride-map.detail-truncated")
                }

                HStack(spacing: 10) {
                    if pointsTruncated {
                        Button(localizedAppText("ride_map.show_full_ride")) {
                            loadFullRide()
                            mapPosition = .automatic
                        }
                        .buttonStyle(.bordered)
                        .tint(PevColors.primaryText)
                    }

                    ShareLink(item: shareText) {
                        Label(localizedAppText("ride_map.share"), systemImage: "square.and.arrow.up")
                    }
                    .buttonStyle(.bordered)
                    .tint(PevColors.primaryText)
                }
            }
        }
        .font(.subheadline)
        .padding(16)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(PevColors.cardFill, in: UnevenRoundedRectangle(
            topLeadingRadius: 28,
            bottomLeadingRadius: 0,
            bottomTrailingRadius: 0,
            topTrailingRadius: 28
        ))
        .padding(.bottom, 8)
    }

}

private struct RideMapLoadingSurface: ViewModifier {
    func body(content: Content) -> some View {
        if #available(iOS 26, macOS 26, *) {
            content.glassEffect(.regular, in: .capsule)
        } else {
            content
                .background(PevColors.cardFill, in: Capsule())
                .overlay { Capsule().stroke(PevColors.cardStroke, lineWidth: 1) }
        }
    }
}

private struct RideMapDetailMetric: View {
    let value: String
    let label: String

    var body: some View {
        VStack(alignment: .leading, spacing: 3) {
            Text(value)
                .font(.title2.weight(.bold).monospacedDigit())
                .lineLimit(1)
                .minimumScaleFactor(0.72)
            Text(label)
                .font(.caption)
                .foregroundStyle(PevColors.muted)
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }
}
