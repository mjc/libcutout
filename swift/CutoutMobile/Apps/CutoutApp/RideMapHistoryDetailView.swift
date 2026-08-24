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

    var body: some View {
        VStack(spacing: 0) {
            HStack {
                Button {
                    close()
                } label: {
                    Label(localizedAppText("ride_map.detail_back"), systemImage: "chevron.left")
                }
                .buttonStyle(.bordered)
                Spacer()
                Text(localizedAppText("ride_map.detail_title"))
                    .font(.headline)
                Spacer()
            }
            .padding(.horizontal, 20)
            .padding(.vertical, 12)

            RideMapCanvasView(
                points: points,
                routeID: initialHistoryID ?? "history-detail",
                showsEndMarker: true,
                fitsRouteOnChange: true,
                mapPosition: $mapPosition,
                isApplyingCamera: $isApplyingCamera,
                cameraDidChange: {}
            )
            .frame(minHeight: 260, maxHeight: .infinity)

            if let ride = rides.first(where: { $0.rideId == initialHistoryID }) {
                VStack(alignment: .leading, spacing: 8) {
                    Text(localizedAppText("ride_map.distance", distanceText(for: ride.summary)))
                    Text(localizedAppText("ride_map.points", ride.summary.pointCount))
                    RideMapRouteTruthView(points: points, decision: nil)
                    if pointsTruncated {
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
        .task(id: rides.map(\.rideId)) { loadSelectionIfNeeded() }
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
}
