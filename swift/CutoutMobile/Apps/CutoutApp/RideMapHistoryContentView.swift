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
    @State private var dateFilter = "Last 30 days"
    @State private var vehicleFilter = "All vehicles"
    private enum FilterKind {
        case date
        case vehicle
    }

    private var vehicleOptions: [String] {
        Set(rides.flatMap { [$0.associatedVehicle, $0.candidateVehicle].compactMap { $0 } }).sorted()
    }

    var body: some View {
        VStack(spacing: 0) {
            if rides.isEmpty {
                emptyState
            } else {
                HStack(spacing: 8) {
                    filterMenu(kind: .date, title: dateFilter, systemImage: "calendar")
                    filterMenu(kind: .vehicle, title: vehicleFilter, systemImage: "car")
                }
                .padding(.horizontal, 16)
                .padding(.bottom, 8)

                RideMapCanvasView(
                    points: points,
                    routeID: selectedRideID ?? "history",
                    showsEndMarker: true,
                    fitsRouteOnChange: true,
                    mapPosition: $mapPosition,
                    isApplyingCamera: $isApplyingCamera,
                    cameraDidChange: {}
                )
                .frame(minHeight: 290, maxHeight: .infinity)

                VStack(alignment: .leading, spacing: 10) {
                    if isRecording {
                        HStack(spacing: 10) {
                            Circle()
                                .fill(PevColors.green)
                                .frame(width: 9, height: 9)
                            Text(localizedAppText("ride_map.history_recording_continues"))
                                .font(.subheadline.weight(.semibold))
                                .foregroundStyle(PevColors.green)
                            Spacer()
                            Button(localizedAppText("ride_map.return_live"), action: returnToLive)
                                .font(.subheadline.weight(.semibold))
                                .buttonStyle(.plain)
                                .foregroundStyle(PevColors.green)
                                .accessibilityIdentifier("ride-map.return-live")
                        }
                        .accessibilityIdentifier("ride-map.history-recording-banner")
                    }

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
                            .accessibilityIdentifier("ride-map.history-truncated")
                    }
                }
                .padding(16)
                .background(PevColors.cardFill, in: UnevenRoundedRectangle(
                    topLeadingRadius: 28,
                    bottomLeadingRadius: 0,
                    bottomTrailingRadius: 0,
                    topTrailingRadius: 28
                ))
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

    private func filterMenu(
        kind: FilterKind,
        title: String,
        systemImage: String
    ) -> some View {
        Menu {
            switch kind {
            case .date:
                Button("Last 30 days") { dateFilter = "Last 30 days" }
                Button("All time") { dateFilter = "All time" }
            case .vehicle:
                Button("All vehicles") { vehicleFilter = "All vehicles" }
                ForEach(vehicleOptions, id: \.self) { vehicle in
                    Button(vehicle) { vehicleFilter = vehicle }
                }
            }
        } label: {
            Label(title, systemImage: systemImage)
                .font(.subheadline.weight(.semibold))
                .lineLimit(1)
                .frame(maxWidth: .infinity)
        }
        .buttonStyle(.bordered)
        .tint(PevColors.cardStroke)
    }
}
