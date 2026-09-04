import CutoutMobile
import Foundation
import MapKit
import SwiftUI

struct RideMapHistoryContentView: View {
    let isRecording: Bool
    let isPaused: Bool
    let rides: [MobileRideMapHistorySummaryDto]
    @Binding var searchText: String
    let canLoadMore: Bool
    let displayPoints: [MobileRideMapRouteDisplayPoint]
    /// Rust's bounded projection supplies the camera; the default keeps older route-shell
    /// callers source-compatible until they pass the projection metadata through.
    var cameraRegion: MobileRideMapCameraRegion? = nil
    let endpointMetadata: MobileRideMapRouteEndpointMetadata
    let segments: [MobileRideMapSegmentDisplayMetadata]
    let contextRoutes: [MobileRideMapHistoryContextRoute]
    let projectionVersion: UInt64
    let pointsTruncated: Bool
    let segmentsOmittedByBudget: Bool
    let isLoading: Bool
    let isRouteLoading: Bool
    let historyError: MobileRideMapError?
    let routeError: MobileRideMapError?
    let selectedRideID: String?
    let dateFilter: CutoutAppModel.RideMapHistoryDateFilter
    let vehicleFilter: String?
    let vehicleFilterOptions: [String]
    let select: (String) -> Void
    let load: () -> Void
    let loadMore: () -> Void
    let returnToLive: () -> Void
    let setDateFilter: (CutoutAppModel.RideMapHistoryDateFilter) -> Void
    let setVehicleFilter: (String?) -> Void
    let clearFilters: () -> Void
    let currentVehicleIdentity: String?
    let currentVehicleName: String?
    let vehicleName: (String?) -> String?
    let cameraDidChange: (MKCoordinateRegion) -> Void

    @Binding var mapPosition: MapCameraPosition
    @Binding var isApplyingCamera: Bool

    private var vehicleOptions: [RideMapVehicleOption] {
        Self.uniqueVehicleIdentities(
            vehicleFilterOptions + [vehicleFilter].compactMap { $0 }
        )
        .map { RideMapVehicleOption(identity: $0, label: vehicleLabel(for: $0)) }
        .sorted {
            $0.label.localizedCaseInsensitiveCompare($1.label)
                == .orderedAscending
        }
    }

    private var selectedRide: MobileRideMapHistorySummaryDto? {
        guard let selectedRideID else { return nil }
        return rides.first { $0.rideID == selectedRideID }
    }

    private var isSelectedRouteLoading: Bool {
        Self.selectedRouteIsLoading(
            routeLoading: isRouteLoading,
            hasSelectedRide: selectedRide != nil
        )
    }

    private var isSelectedRouteError: Bool {
        selectedRide != nil && displayPoints.isEmpty && routeError != nil
    }

    private var isSelectedRouteEmpty: Bool {
        guard let selectedRide else { return false }
        return !isLoading && routeError == nil && selectedRide.summary.pointCount == 0
    }

    private var selectedRouteState: RideMapHistoryRouteState {
        if isSelectedRouteLoading {
            return .loading
        }
        if isSelectedRouteError {
            return .error
        }
        if isSelectedRouteEmpty {
            return .empty
        }
        return .ready
    }

    private var hasActiveFilters: Bool {
        Self.hasActiveFilters(
            searchText: searchText,
            dateFilter: dateFilter,
            vehicleFilter: vehicleFilter
        )
    }

    private var dateFilterTitle: String {
        switch dateFilter {
        case .last30Days:
            localizedAppText("ride_map.history_last_30_days")
        case .allTime:
            localizedAppText("ride_map.history_all_time")
        }
    }

    @MainActor
    static func uniqueVehicleIdentities(_ identities: [String]) -> [String] {
        Array(Set(identities)).sorted()
    }

    @MainActor
    static func selectedRouteIsLoading(routeLoading: Bool, hasSelectedRide: Bool) -> Bool {
        routeLoading && hasSelectedRide
    }

    @MainActor
    static func hasActiveFilters(
        searchText: String,
        dateFilter: CutoutAppModel.RideMapHistoryDateFilter,
        vehicleFilter: String?
    ) -> Bool {
        !normalizedSearchText(searchText).isEmpty
            || dateFilter != .last30Days
            || vehicleFilter != nil
    }

    @MainActor
    static func searchDebounce(for searchText: String) -> Duration {
        normalizedSearchText(searchText).isEmpty ? .zero : .milliseconds(250)
    }

    @MainActor
    static func normalizedSearchText(_ searchText: String) -> String {
        searchText.trimmingCharacters(in: .whitespacesAndNewlines)
    }

    var body: some View {
        ScrollView(.vertical, showsIndicators: false) {
            VStack(spacing: 0) {
                if isLoading && rides.isEmpty {
                    ProgressView(localizedAppText("ride_map.history_loading"))
                        .tint(PevColors.yellow)
                        .frame(maxWidth: .infinity, alignment: .top)
                        .padding(24)
                } else {
                    RideMapHistoryFilterBar(
                        dateTitle: dateFilterTitle,
                        vehicleTitle: vehicleFilter.map(vehicleLabel)
                            ?? localizedAppText("ride_map.history_all_vehicles"),
                        vehicleOptions: vehicleOptions,
                        setDateFilter: setDateFilter,
                        setVehicleFilter: setVehicleFilter
                    )
                    .padding(.horizontal, 16)
                    .padding(.bottom, 8)

                    if hasActiveFilters {
                        Button(
                            localizedAppText("ride_map.history_clear_filters"), action: clearFilters
                        )
                        .font(.subheadline.weight(.semibold))
                        .buttonStyle(.plain)
                        .foregroundStyle(PevColors.yellow)
                        .frame(minHeight: 44)
                        .frame(maxWidth: .infinity, alignment: .trailing)
                        .contentShape(Rectangle())
                        .padding(.horizontal, 16)
                        .padding(.bottom, 8)
                        .accessibilityIdentifier("ride-map.history-clear-filters")
                    }

                    if rides.isEmpty, historyError != nil {
                        RideMapHistoryErrorState(load: load)
                    } else if rides.isEmpty {
                        RideMapHistoryEmptyState(canLoadMore: canLoadMore, loadMore: loadMore)
                    } else {
                        if historyError != nil {
                            Label(
                                localizedAppText("ride_map.command_failed"),
                                systemImage: "exclamationmark.triangle"
                            )
                            .font(.caption)
                            .foregroundStyle(.orange)
                            .padding(.horizontal, 16)
                            .padding(.bottom, 8)
                            .accessibilityIdentifier("ride-map.history-error")
                        }

                        RideMapHistoryRouteSection(
                            displayPoints: displayPoints,
                            routeID: selectedRideID ?? "history",
                            projectionVersion: projectionVersion,
                            endpointMetadata: endpointMetadata,
                            cameraRegion: cameraRegion,
                            segments: segments,
                            contextRoutes: contextRoutes,
                            state: selectedRouteState,
                            pointsTruncated: pointsTruncated,
                            segmentsOmittedByBudget: segmentsOmittedByBudget,
                            mapPosition: $mapPosition,
                            isApplyingCamera: $isApplyingCamera,
                            cameraDidChange: cameraDidChange
                        )

                        RideMapHistoryListSection(
                            isRecording: isRecording,
                            isPaused: isPaused,
                            rides: rides,
                            canLoadMore: canLoadMore,
                            selectedRideID: selectedRideID,
                            returnToLive: returnToLive,
                            select: select,
                            loadMore: loadMore
                        )
                    }
                }
            }
        }
        .task(id: searchText) { await reloadHistory(for: searchText) }
        .searchable(text: $searchText)
        .safeAreaInset(edge: .bottom, spacing: 0) {
            Color.clear.frame(height: 92)
        }
    }

    private func vehicleLabel(for identity: String) -> String {
        Self.resolvedVehicleLabel(
            identity: identity,
            currentIdentity: currentVehicleIdentity,
            currentName: currentVehicleName,
            resolve: vehicleName
        )
    }

    @MainActor
    static func resolvedVehicleLabel(
        identity: String,
        currentIdentity: String?,
        currentName: String?,
        resolve: (String) -> String?
    ) -> String {
        resolve(identity)
            .flatMap { $0.isEmpty ? nil : $0 }
            ?? (identity == currentIdentity ? currentName : nil)
            ?? localizedAppText("ride_map.vehicle_name_unavailable")
    }

    private func reloadHistory(for searchText: String) async {
        let debounce = Self.searchDebounce(for: searchText)
        if debounce != .zero {
            do {
                try await Task.sleep(for: debounce)
            } catch {
                return
            }
        }
        guard !Task.isCancelled else { return }
        load()
    }
}
