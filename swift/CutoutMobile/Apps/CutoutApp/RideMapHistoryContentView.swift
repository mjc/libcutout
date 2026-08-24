import MapKit
import SwiftUI
import CutoutMobile
import CutoutMobileFFI

struct RideMapHistoryContentView: View {
    let isRecording: Bool
    let rides: [MobileRideMapHistorySummaryDto]
    @Binding var searchText: String
    let canLoadMore: Bool
    let points: [MobileRideMapPointDto]
    let pointsTruncated: Bool
    let selectedRideID: String?
    let select: (String) -> Void
    let load: () -> Void
    let loadMore: () -> Void
    let returnToLive: () -> Void

    @State private var mapPosition: MapCameraPosition = .automatic
    @State private var isApplyingCamera = false

    var body: some View {
        VStack(spacing: 0) {
            if isRecording {
                HStack(spacing: 12) {
                    Label(
                        localizedAppText("ride_map.history_recording_continues"),
                        systemImage: "record.circle.fill"
                    )
                    .font(.subheadline)
                    Spacer()
                    Button(localizedAppText("ride_map.return_live"), action: returnToLive)
                        .buttonStyle(.bordered)
                        .accessibilityIdentifier("ride-map.return-live")
                }
                .padding(.horizontal, 20)
                .padding(.vertical, 10)
                .background(PevColors.pageBackground)
                .accessibilityIdentifier("ride-map.history-recording-banner")
            }

            if rides.isEmpty {
                emptyState
            } else {
                RideMapCanvasView(
                    points: points,
                    routeID: selectedRideID ?? "history",
                    showsEndMarker: true,
                    fitsRouteOnChange: true,
                    mapPosition: $mapPosition,
                    isApplyingCamera: $isApplyingCamera,
                    cameraDidChange: {}
                )
                .frame(minHeight: 260, maxHeight: .infinity)
                RideMapHistoryListView(
                    rides: rides,
                    searchText: $searchText,
                    canLoadMore: canLoadMore,
                    selectedRideID: selectedRideID,
                    select: select,
                    loadMore: loadMore
                )
                if pointsTruncated {
                    Text(localizedAppText("ride_map.history_truncated"))
                        .font(.caption)
                        .foregroundStyle(PevColors.muted)
                        .padding(.horizontal, 20)
                        .accessibilityIdentifier("ride-map.history-truncated")
                }
            }
        }
        .onAppear { loadHistoryIfNeeded() }
    }

    private var emptyState: some View {
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
            if canLoadMore {
                Button(localizedAppText("ride_map.history_load_more"), action: loadMore)
                    .buttonStyle(.bordered)
                    .accessibilityIdentifier("ride-map.history-load-more")
            }
        }
        .padding(24)
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
        .accessibilityElement(children: .combine)
        .accessibilityIdentifier("ride-map.history-empty")
    }

    private func loadHistoryIfNeeded() {
        if rides.isEmpty { load() }
    }
}
