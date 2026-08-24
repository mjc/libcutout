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
    let isLoading: Bool
    let error: MobileRideMapError?
    let selectedRideID: String?
    let dateFilter: CutoutAppModel.RideMapHistoryDateFilter
    let vehicleFilter: String?
    let select: (String) -> Void
    let load: () -> Void
    let loadMore: () -> Void
    let returnToLive: () -> Void
    let setDateFilter: (CutoutAppModel.RideMapHistoryDateFilter) -> Void
    let setVehicleFilter: (String?) -> Void
    let currentVehicleIdentity: String?
    let currentVehicleName: String?
    let vehicleName: (String?) -> String?

    @Binding var mapPosition: MapCameraPosition
    @Binding var isApplyingCamera: Bool

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

    var body: some View {
        ScrollView(.vertical, showsIndicators: false) {
            VStack(spacing: 0) {
                if isLoading && rides.isEmpty {
                    ProgressView(localizedAppText("ride_map.history_loading"))
                        .tint(PevColors.yellow)
                        .frame(maxWidth: .infinity, alignment: .top)
                        .padding(24)
                } else if rides.isEmpty, error != nil {
                    historyErrorState
                } else if rides.isEmpty {
                    emptyState
                } else {
                    RideMapHistoryFilterRow(
                        dateMenu: { filterMenu(kind: .date, title: dateFilterTitle, systemImage: "calendar") },
                        vehicleMenu: {
                            filterMenu(
                                kind: .vehicle,
                                title: vehicleFilter.map(vehicleLabel)
                                    ?? localizedAppText("ride_map.history_all_vehicles"),
                                systemImage: "car"
                            )
                        }
                    )
                    .padding(.horizontal, 16)
                    .padding(.bottom, 8)

                    if error != nil {
                        Label(localizedAppText("ride_map.command_failed"), systemImage: "exclamationmark.triangle")
                            .font(.caption)
                            .foregroundStyle(.orange)
                            .padding(.horizontal, 16)
                            .padding(.bottom, 8)
                            .accessibilityIdentifier("ride-map.history-error")
                    }

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
                    .frame(height: 340)
                    .frame(maxWidth: .infinity)

                    if pointsTruncated {
                        Text(localizedAppText("ride_map.history_truncated"))
                            .font(.caption)
                            .foregroundStyle(PevColors.muted)
                            .padding(.horizontal, 16)
                            .padding(.vertical, 8)
                            .frame(maxWidth: .infinity, alignment: .leading)
                            .accessibilityIdentifier("ride-map.history-truncated")
                    }

                    VStack(alignment: .leading, spacing: 10) {
                        if isRecording || isPaused {
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
                            rides: rides,
                            canLoadMore: canLoadMore,
                            selectedRideID: selectedRideID,
                            select: select,
                            loadMore: loadMore
                        )
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
        }
        // Search is Rust-owned, but keystrokes should not launch one query per
        // character. A task keyed to the text cancels stale queries and also
        // reloads when the user clears the field.
        .task(id: searchText) {
            do {
                try await Task.sleep(for: .milliseconds(250))
            } catch {
                return
            }
            guard Task.isCancelled == false else { return }
            load()
        }
        // Search belongs to the screen container so it stays attached to the
        // Map navigation context instead of the inner fixed-height list.
        .searchable(text: $searchText)
        // Connected Map is rendered underneath a floating TabView. Keep the
        // selected route, list rows, and truncation notice above that chrome;
        // the same inset is harmless on the home-route presentation.
        .safeAreaInset(edge: .bottom, spacing: 0) {
            Color.clear.frame(height: 92)
        }
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

    private var historyErrorState: some View {
        VStack(alignment: .leading, spacing: 12) {
            Image(systemName: "exclamationmark.triangle")
                .font(.largeTitle)
                .foregroundStyle(.orange)
                .accessibilityHidden(true)
            Text(localizedAppText("ride_map.history_error_title"))
                .font(.title2.weight(.semibold))
            Text(localizedAppText("ride_map.history_error_detail"))
                .foregroundStyle(PevColors.muted)
            Button(localizedAppText("ride_map.history_retry"), action: load)
                .buttonStyle(.borderedProminent)
                .tint(PevColors.yellow)
                .accessibilityIdentifier("ride-map.history-retry")
        }
        .padding(24)
        .frame(maxWidth: .infinity, alignment: .topLeading)
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("ride-map.history-error-state")
    }

    private func filterMenu(
        kind: FilterKind,
        title: String,
        systemImage: String
    ) -> some View {
        Menu {
            switch kind {
            case .date:
                Button(localizedAppText("ride_map.history_last_30_days")) {
                    setDateFilter(.last30Days)
                }
                Button(localizedAppText("ride_map.history_all_time")) {
                    setDateFilter(.allTime)
                }
            case .vehicle:
                Button(localizedAppText("ride_map.history_all_vehicles")) {
                    setVehicleFilter(nil)
                }
                ForEach(vehicleOptions) { vehicle in
                    Button(vehicle.label) { setVehicleFilter(vehicle.identity) }
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
        .modifier(RideMapFilterSurface())
    }

    private var dateFilterTitle: String {
        switch dateFilter {
        case .last30Days:
            localizedAppText("ride_map.history_last_30_days")
        case .allTime:
            localizedAppText("ride_map.history_all_time")
        }
    }
}

private struct RideMapFilterSurface: ViewModifier {
    func body(content: Content) -> some View {
        if #available(iOS 26, macOS 26, *) {
            content.glassEffect(.regular.interactive(), in: .capsule)
        } else {
            content
                .background(PevColors.pageBackground.opacity(0.72), in: Capsule())
                .overlay { Capsule().stroke(PevColors.cardStroke.opacity(0.45), lineWidth: 1) }
        }
    }
}

private struct RideMapHistoryFilterRow<DateMenu: View, VehicleMenu: View>: View {
    @ViewBuilder let dateMenu: () -> DateMenu
    @ViewBuilder let vehicleMenu: () -> VehicleMenu

    var body: some View {
        Group {
            if #available(iOS 26, macOS 26, *) {
                GlassEffectContainer(spacing: 8) {
                    menus
                }
            } else {
                menus
            }
        }
    }

    private var menus: some View {
        HStack(spacing: 8) {
            dateMenu()
            vehicleMenu()
        }
    }
}
