import CutoutMobile
import MapKit
import SwiftUI

struct RideMapVehicleOption: Hashable, Identifiable {
    let identity: String
    let label: String

    var id: String { identity }
}

enum RideMapHistoryRouteState: Equatable {
    case loading
    case error
    case empty
    case emptyViewport
    case ready
}

struct RideMapHistoryFilterBar: View {
    @Environment(\.dynamicTypeSize) private var dynamicTypeSize

    let dateTitle: String
    let vehicleTitle: String
    let vehicleOptions: [RideMapVehicleOption]
    let hasActiveFilters: Bool
    let setDateFilter: (RideHistoryModel.DateFilter) -> Void
    let setVehicleFilter: (String?) -> Void
    let clearFilters: () -> Void

    var body: some View {
        Group {
            // Text size chooses the arrangement; filter values never add or remove rows.
            if dynamicTypeSize.isAccessibilitySize {
                VStack(spacing: 0) {
                    HStack(spacing: 0) {
                        dateMenu
                        clearButton
                    }
                    Divider().padding(.horizontal, 12)
                    vehicleMenu
                }
            } else {
                HStack(spacing: 0) {
                    dateMenu
                    Divider().frame(height: 20)
                    vehicleMenu
                    Divider().frame(height: 20)
                    clearButton
                }
            }
        }
        .buttonStyle(.plain)
        .menuStyle(.button)
        .menuIndicator(.hidden)
        .background(PevColors.cardFill, in: RoundedRectangle(cornerRadius: 14))
        .overlay {
            RoundedRectangle(cornerRadius: 14)
                .strokeBorder(PevColors.cardStroke.opacity(0.5), lineWidth: 1)
        }
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("ride-map.history-filters")
    }

    private var dateMenu: some View {
        Menu {
            Button(localizedAppText("ride_map.history_last_30_days")) {
                setDateFilter(.last30Days)
            }
            Button(localizedAppText("ride_map.history_all_time")) {
                setDateFilter(.allTime)
            }
        } label: {
            filterLabel(title: dateTitle)
        }
        .frame(maxWidth: .infinity, minHeight: 44)
        .accessibilityIdentifier("ride-map.history-date-filter")
    }

    private var vehicleMenu: some View {
        Menu {
            Button(localizedAppText("ride_map.history_all_vehicles")) {
                setVehicleFilter(nil)
            }
            ForEach(vehicleOptions) { vehicle in
                Button(vehicle.label) { setVehicleFilter(vehicle.identity) }
            }
        } label: {
            filterLabel(title: vehicleTitle)
        }
        .frame(maxWidth: .infinity, minHeight: 44)
        .accessibilityIdentifier("ride-map.history-vehicle-filter")
    }

    private var clearButton: some View {
        Button(action: clearFilters) {
            Image(systemName: "xmark")
                .font(.subheadline.weight(.semibold))
                .frame(width: 44, height: 44)
                .contentShape(Rectangle())
        }
        .foregroundStyle(hasActiveFilters ? PevColors.brand : Color.secondary)
        .disabled(!hasActiveFilters)
        .accessibilityLabel(localizedAppText("ride_map.history_clear_filters"))
        .accessibilityIdentifier("ride-map.history-clear-filters")
        .help(localizedAppText("ride_map.history_clear_filters"))
    }

    private func filterLabel(title: String) -> some View {
        HStack(spacing: 6) {
            Text(title)
                .lineLimit(1)
                .truncationMode(.tail)
                .frame(maxWidth: .infinity, alignment: .leading)
            Image(systemName: "chevron.down")
                .font(.caption2.weight(.semibold))
                .foregroundStyle(Color.secondary)
                .accessibilityHidden(true)
        }
        .font(.subheadline.weight(.semibold))
        .foregroundStyle(PevColors.primaryText)
        .padding(.horizontal, 12)
        .padding(.vertical, 10)
        .frame(minHeight: 44)
        .contentShape(Rectangle())
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(title)
    }
}

struct RideMapHistoryRouteSection: View {
    let displayPoints: [MobileRideMapRouteDisplayPoint]
    let routeID: String
    let projectionVersion: UInt64
    let endpointMetadata: MobileRideMapRouteEndpointMetadata
    let cameraRegion: MobileRideMapCameraRegion?
    let segments: [MobileRideMapSegmentDisplayMetadata]
    let cameraFitVersion: UInt64
    let mapHeight: CGFloat
    let state: RideMapHistoryRouteState
    let pointsTruncated: Bool
    let segmentsOmittedByBudget: Bool
    @Binding var mapPosition: MapCameraPosition
    @Binding var isApplyingCamera: Bool
    @State private var fittedRouteID: String?
    let cameraDidChange: (MKCoordinateRegion) -> Void

    var body: some View {
        VStack(spacing: 0) {
            ZStack {
                RideMapCanvasView(
                    points: displayPoints,
                    routeID: routeID,
                    projectionVersion: projectionVersion,
                    showsStartMarker: true,
                    showsEndMarker: true,
                    showsCurrentMarker: false,
                    endpointMetadata: endpointMetadata,
                    cameraRegion: cameraRegion,
                    segments: segments,
                    contextRoutes: [],
                    fitsRouteOnChange: true,
                    cameraFitID: RideMapCanvasView.cameraFitID(
                        routeID: routeID,
                        fitVersion: cameraFitVersion
                    ),
                    mapPosition: $mapPosition,
                    isApplyingCamera: $isApplyingCamera,
                    cameraDidChange: cameraDidChange
                )

                if state == .loading {
                    RideMapHistoryLoadingSurface(identifier: "ride-map.history-map-loading")
                } else if state == .error {
                    Label(
                        localizedAppText("ride_map.command_failed"),
                        systemImage: "exclamationmark.triangle"
                    )
                    .font(.subheadline.weight(.semibold))
                    .foregroundStyle(.orange)
                    .padding(.horizontal, 14)
                    .padding(.vertical, 10)
                    .background(PevColors.cardFill, in: Capsule())
                    .accessibilityIdentifier("ride-map.history-map-error")
                } else if state == .empty {
                    ContentUnavailableView(
                        localizedAppText("ride_map.no_points"),
                        systemImage: "location.slash"
                    )
                    .accessibilityIdentifier("ride-map.history-map-empty")
                }
            }
            .frame(height: mapHeight)
            .frame(maxWidth: .infinity)

            if pointsTruncated || segmentsOmittedByBudget {
                Text(localizedAppText("ride_map.history_map_preview"))
                    .font(.caption)
                    .foregroundStyle(PevColors.muted)
                    .padding(.horizontal, 16)
                    .padding(.vertical, 8)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .accessibilityIdentifier("ride-map.history-preview")
            }
        }
    }
}

struct RideMapHistorySearchField: View {
    @Binding var searchText: String

    var body: some View {
        HStack(spacing: 10) {
            Image(systemName: "magnifyingglass")
                .foregroundStyle(PevColors.muted)
                .accessibilityHidden(true)
            TextField(localizedAppText("ride_map.history_search"), text: $searchText)
                .textFieldStyle(.plain)
                .submitLabel(.search)
                .accessibilityIdentifier("ride-map.history-search")
        }
        .font(.subheadline)
        .padding(.horizontal, 12)
        .frame(minHeight: 44)
        .background(PevColors.cardFill, in: RoundedRectangle(cornerRadius: 14))
    }
}

struct RideMapHistoryRecordingStatus: View {
    let isPaused: Bool
    let returnToLive: () -> Void

    var body: some View {
        Button(action: returnToLive) {
            HStack(spacing: 10) {
                Circle()
                    .fill(isPaused ? PevColors.brand : PevColors.green)
                    .frame(width: 8, height: 8)
                    .accessibilityHidden(true)
                Text(localizedAppText(isPaused ? "ride_map.history_paused" : "ride_map.history_recording_continues"))
                    .font(.subheadline.weight(.semibold))
                    .fixedSize(horizontal: false, vertical: true)
                    .frame(maxWidth: .infinity, alignment: .leading)
                Image(systemName: "chevron.right")
                    .font(.caption.weight(.semibold))
                    .accessibilityHidden(true)
            }
            .frame(minHeight: 44)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .foregroundStyle(PevColors.primaryText)
        .accessibilityHint(localizedAppText("ride_map.return_live"))
        .accessibilityIdentifier("ride-map.return-live")
    }
}

struct RideMapHistoryListSection: View {
    let rides: [MobileRideMapHistorySummaryDto]
    let canLoadMore: Bool
    let selectedRideID: String?
    let select: (String) -> Void
    let loadMore: () -> Void

    var body: some View {
        RideMapHistoryListView(
            rides: rides,
            canLoadMore: canLoadMore,
            selectedRideID: selectedRideID,
            select: select,
            loadMore: loadMore
        )
        .padding(16)
        .background(PevColors.cardFill)
        .padding(.bottom, 8)
    }
}

struct RideMapHistoryEmptyState: View {
    let canLoadMore: Bool
    let loadMore: () -> Void

    var body: some View {
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
}

struct RideMapHistoryErrorState: View {
    let load: () -> Void

    var body: some View {
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
}

struct RideMapHistoryLoadingSurface: View {
    let identifier: String

    var body: some View {
        HStack(spacing: 8) {
            ProgressView()
                .tint(PevColors.yellow)
            Text(localizedAppText("ride_map.history_loading"))
                .font(.subheadline.weight(.semibold))
        }
        .padding(.horizontal, 14)
        .padding(.vertical, 10)
        .modifier(RideMapLoadingSurface())
        .accessibilityIdentifier(identifier)
    }
}
