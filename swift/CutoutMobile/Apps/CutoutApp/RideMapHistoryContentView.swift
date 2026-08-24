import MapKit
import SwiftUI
import Foundation
import CutoutMobile
import CutoutMobileFFI

struct RideMapHistoryContentView: View {
    let isRecording: Bool
    let isPaused: Bool
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
    let currentVehicleIdentity: String?
    let currentVehicleName: String?
    let vehicleName: (String?) -> String?

    @State private var mapPosition: MapCameraPosition = .automatic
    @State private var isApplyingCamera = false
    @State private var dateFilter = DateFilter.last30Days
    @State private var vehicleFilter: String?
    private enum DateFilter: String {
        case last30Days = "Last 30 days"
        case allTime = "All time"
    }

    private struct VehicleOption: Hashable, Identifiable {
        let identity: String
        let label: String
        var id: String { identity }
    }
    private enum FilterKind {
        case date
        case vehicle
    }

    private var vehicleOptions: [VehicleOption] {
        Dictionary(uniqueKeysWithValues: rides
            .flatMap { [$0.associatedVehicle, $0.candidateVehicle].compactMap { $0 } }
            .map { ($0, VehicleOption(identity: $0, label: vehicleLabel(for: $0))) })
            .values
            .sorted { $0.label.localizedCaseInsensitiveCompare($1.label) == .orderedAscending }
    }

    private func vehicleLabel(for identity: String) -> String {
        vehicleName(identity)
            ?? (identity == currentVehicleIdentity ? currentVehicleName : nil)
            ?? identity
    }

    private var filteredRides: [MobileRideMapHistorySummaryDto] {
        let cutoff = Date().addingTimeInterval(-30 * 24 * 60 * 60)
        return rides.filter { ride in
            let dateMatches = dateFilter == .allTime
                || Date(timeIntervalSince1970: Double(ride.createdAtMilliseconds) / 1_000) >= cutoff
            let vehicleMatches = vehicleFilter == nil
                || ride.associatedVehicle == vehicleFilter
                || ride.candidateVehicle == vehicleFilter
            return dateMatches && vehicleMatches
        }
    }

    var body: some View {
        VStack(spacing: 0) {
            if filteredRides.isEmpty {
                emptyState
            } else {
                HStack(spacing: 8) {
                    filterMenu(kind: .date, title: dateFilter.rawValue, systemImage: "calendar")
                    filterMenu(
                        kind: .vehicle,
                        title: vehicleFilter.map(vehicleLabel) ?? "All vehicles",
                        systemImage: "car"
                    )
                }
                .padding(.horizontal, 16)
                .padding(.bottom, 8)

                RideMapCanvasView(
                    points: points,
                    routeID: selectedRideID ?? "history",
                    showsStartMarker: true,
                    showsEndMarker: true,
                    fitsRouteOnChange: true,
                    mapPosition: $mapPosition,
                    isApplyingCamera: $isApplyingCamera,
                    cameraDidChange: {}
                )
                // Keep the map as the fixed visual hero; the ride list remains
                // a distinct bottom sheet instead of being pushed off-screen.
                .frame(height: 340)
                .frame(maxWidth: .infinity)

                VStack(alignment: .leading, spacing: 10) {
                    if isRecording {
                        HStack(spacing: 10) {
                            Circle()
                                .fill(PevColors.green)
                                .frame(width: 9, height: 9)
                            Text(isPaused
                                ? localizedAppText("ride_map.history_paused")
                                : localizedAppText("ride_map.history_recording_continues"))
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
                        rides: filteredRides,
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
                .padding(.bottom, 8)
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
                Button(DateFilter.last30Days.rawValue) { dateFilter = .last30Days }
                Button(DateFilter.allTime.rawValue) { dateFilter = .allTime }
            case .vehicle:
                Button("All vehicles") { vehicleFilter = nil }
                ForEach(vehicleOptions) { vehicle in
                    Button(vehicle.label) { vehicleFilter = vehicle.identity }
                }
            }
        } label: {
            Label(title, systemImage: systemImage)
                .font(.subheadline.weight(.semibold))
                .lineLimit(1)
                .frame(maxWidth: .infinity)
        }
        .labelStyle(.titleAndIcon)
        .foregroundStyle(PevColors.primaryText)
        .padding(.horizontal, 14)
        .padding(.vertical, 9)
        .background(PevColors.pageBackground.opacity(0.72), in: Capsule())
        .overlay {
            Capsule().stroke(PevColors.cardStroke.opacity(0.45), lineWidth: 1)
        }
    }
}
