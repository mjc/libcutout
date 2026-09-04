import CutoutMobile
import MapKit
import SwiftUI

struct RideMapVehicleOption: Hashable, Identifiable {
    let identity: String
    let label: String

    var id: String { identity }
}

struct RideMapHistoryFilterBar: View {
    let dateTitle: String
    let vehicleTitle: String
    let vehicleOptions: [RideMapVehicleOption]
    let setDateFilter: (CutoutAppModel.RideMapHistoryDateFilter) -> Void
    let setVehicleFilter: (String?) -> Void

    var body: some View {
        RideMapHistoryFilterRow(
            dateMenu: { dateMenu },
            vehicleMenu: { vehicleMenu }
        )
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
            filterLabel(title: dateTitle, systemImage: "calendar")
        }
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
            filterLabel(title: vehicleTitle, systemImage: "car")
        }
    }

    private func filterLabel(title: String, systemImage: String) -> some View {
        Label(title, systemImage: systemImage)
            .font(.subheadline.weight(.semibold))
            .lineLimit(1)
            .frame(maxWidth: .infinity)
            .labelStyle(.titleAndIcon)
            .foregroundStyle(PevColors.primaryText)
            .padding(.horizontal, 14)
            .padding(.vertical, 9)
            .modifier(RideMapFilterSurface())
    }
}

struct RideMapHistoryRouteSection: View {
    let displayPoints: [MobileRideMapRouteDisplayPoint]
    let routeID: String
    let projectionVersion: UInt64
    let endpointMetadata: MobileRideMapRouteEndpointMetadata
    let cameraRegion: MobileRideMapCameraRegion?
    let segments: [MobileRideMapSegmentDisplayMetadata]
    let contextRoutes: [MobileRideMapHistoryContextRoute]
    let isSelectedRouteLoading: Bool
    let isSelectedRouteError: Bool
    let isSelectedRouteEmpty: Bool
    let pointsTruncated: Bool
    let segmentsOmittedByBudget: Bool
    @Binding var mapPosition: MapCameraPosition
    @Binding var isApplyingCamera: Bool
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
                    contextRoutes: contextRoutes,
                    fitsRouteOnChange: true,
                    mapPosition: $mapPosition,
                    isApplyingCamera: $isApplyingCamera,
                    cameraDidChange: cameraDidChange
                )

                if isSelectedRouteLoading {
                    RideMapHistoryLoadingSurface(identifier: "ride-map.history-map-loading")
                } else if isSelectedRouteError {
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
                } else if isSelectedRouteEmpty {
                    ContentUnavailableView(
                        localizedAppText("ride_map.no_points"),
                        systemImage: "location.slash"
                    )
                    .accessibilityIdentifier("ride-map.history-map-empty")
                }
            }
            .frame(height: 340)
            .frame(maxWidth: .infinity)

            if pointsTruncated {
                Text(localizedAppText("ride_map.history_truncated_count", displayPoints.count))
                    .font(.caption)
                    .foregroundStyle(PevColors.muted)
                    .padding(.horizontal, 16)
                    .padding(.vertical, 8)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .accessibilityIdentifier("ride-map.history-truncated")
            }

            if segmentsOmittedByBudget {
                Label(
                    localizedAppText("ride_map.segments_omitted_by_budget"),
                    systemImage: "exclamationmark.triangle"
                )
                .font(.caption)
                .foregroundStyle(.orange)
                .padding(.horizontal, 16)
                .padding(.bottom, 8)
                .accessibilityIdentifier("ride-map.history-segments-omitted")
            }
        }
    }
}

struct RideMapHistoryListSection: View {
    let isRecording: Bool
    let isPaused: Bool
    let rides: [MobileRideMapHistorySummaryDto]
    let canLoadMore: Bool
    let selectedRideID: String?
    let returnToLive: () -> Void
    let select: (String) -> Void
    let loadMore: () -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            if isRecording || isPaused {
                HStack(spacing: 10) {
                    Circle()
                        .fill(PevColors.green)
                        .frame(width: 9, height: 9)
                    Text(
                        isPaused
                            ? localizedAppText("ride_map.history_paused")
                            : localizedAppText("ride_map.history_recording_continues")
                    )
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
        .background(
            PevColors.cardFill,
            in: UnevenRoundedRectangle(
                topLeadingRadius: 28,
                bottomLeadingRadius: 0,
                bottomTrailingRadius: 0,
                topTrailingRadius: 28
            )
        )
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
