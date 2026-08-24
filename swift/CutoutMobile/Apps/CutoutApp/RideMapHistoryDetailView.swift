import MapKit
import SwiftUI
import CutoutMobile
import CutoutMobileFFI

struct RideMapHistoryDetailView: View {
    let initialHistoryID: String?
    let rides: [MobileRideMapHistorySummaryDto]
    let points: [MobileRideMapPointDto]
    let pointsTruncated: Bool
    let select: (String) -> Void
    let load: () -> Void
    let close: () -> Void
    @State private var mapPosition: MapCameraPosition = .automatic
    @State private var isApplyingCamera = false

    private var selectedRide: MobileRideMapHistorySummaryDto? {
        guard let initialHistoryID else { return nil }
        return rides.first(where: { $0.rideId == initialHistoryID })
    }

    private var selectionTaskID: [String] {
        [initialHistoryID ?? ""] + rides.map(\.rideId)
    }

    private var isRouteLoading: Bool {
        guard let selectedRide else { return initialHistoryID != nil }
        return selectedRide.summary.pointCount > 0 && points.isEmpty
    }

    var body: some View {
        VStack(spacing: 0) {
            HStack(alignment: .firstTextBaseline) {
                Button {
                    close()
                } label: {
                    Label(localizedAppText("ride_map.detail_back"), systemImage: "chevron.left")
                }
                .buttonStyle(.plain)
                .foregroundStyle(PevColors.yellow)
                Spacer()
                Text(localizedAppText("ride_map.detail_title"))
                    .font(.headline.weight(.bold))
                Spacer()
                Color.clear.frame(width: 44)
            }
            .padding(.horizontal, 16)
            .padding(.vertical, 10)

            ZStack {
                RideMapCanvasView(
                    points: points,
                    routeID: initialHistoryID ?? "history-detail",
                    showsEndMarker: true,
                    fitsRouteOnChange: true,
                    mapPosition: $mapPosition,
                    isApplyingCamera: $isApplyingCamera,
                    cameraDidChange: {}
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
                    .background(.ultraThinMaterial, in: Capsule())
                    .accessibilityIdentifier("ride-map.detail-loading")
                }
            }
            .frame(minHeight: 360, maxHeight: .infinity)

            if let ride = selectedRide {
                RideMapHistoryDetailSummary(
                    distance: distanceText(for: ride.summary),
                    duration: durationText(for: ride.summary),
                    points: points,
                    pointsTruncated: pointsTruncated,
                    isLoading: isRouteLoading,
                    mapPosition: $mapPosition
                )
            }
        }
        .task(id: selectionTaskID) { loadSelectionIfNeeded() }
        .accessibilityIdentifier("ride-map.detail")
    }

    private func loadSelectionIfNeeded() {
        guard let initialHistoryID else {
            if rides.isEmpty { load() }
            return
        }
        if rides.contains(where: { $0.rideId == initialHistoryID }) {
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
            .formatted(.units(allowed: [.hours, .minutes], width: .abbreviated))
    }
}

private struct RideMapHistoryDetailSummary: View {
    let distance: String
    let duration: String
    let points: [MobileRideMapPointDto]
    let pointsTruncated: Bool
    let isLoading: Bool
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
                        value: "—",
                        label: localizedAppText("ride_map.metric_speed")
                    )
                }
                RideMapRouteTruthView(points: points, decision: nil)
                if pointsTruncated {
                    Text(localizedAppText("ride_map.history_truncated"))
                        .font(.caption)
                        .foregroundStyle(PevColors.muted)
                        .accessibilityIdentifier("ride-map.detail-truncated")
                }

                HStack(spacing: 10) {
                    Button("Show full ride") {
                        mapPosition = .automatic
                    }
                    .buttonStyle(.bordered)
                    .tint(PevColors.primaryText)

                    Button {
                        // Export/share is owned by LIBCU-411; keep the affordance
                        // visible without inventing a second export path here.
                    } label: {
                        Label("Share", systemImage: "square.and.arrow.up")
                    }
                    .buttonStyle(.bordered)
                    .tint(PevColors.primaryText)
                    .disabled(true)
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
