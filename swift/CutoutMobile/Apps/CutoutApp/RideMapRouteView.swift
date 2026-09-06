import SwiftUI
import MapKit
import Observation
import CutoutMobile

@MainActor
@Observable
final class RideMapPresentationState {
    var liveMapPosition: MapCameraPosition = .automatic
    var historyMapPosition: MapCameraPosition = .automatic
    var detailMapPosition: MapCameraPosition = .automatic
    var liveIsApplyingCamera = false
    var historyIsApplyingCamera = false
    var detailIsApplyingCamera = false
    var followsLatestPoint = true
}

struct RideMapRouteView: View {
    @Bindable var model: CutoutAppModel
    @Bindable var presentation: RideMapPresentationState
    private let openHistory: ((String) -> Void)?
    private let closeDetail: (() -> Void)?
    private let initialHistoryID: String?
    private let detailOnly: Bool
    private let showsNavigationHeader: Bool
    private let showBackButton: Bool
    private let back: (() -> Void)?
    @Environment(\.dismiss) private var dismiss
    init(
        model: CutoutAppModel,
        presentation: RideMapPresentationState,
        _ openHistory: ((String) -> Void)? = nil,
        initialHistoryID: String? = nil,
        detailOnly: Bool = false,
        showsNavigationHeader: Bool = true,
        closeDetail: (() -> Void)? = nil,
        showBackButton: Bool = false,
        back: (() -> Void)? = nil
    ) {
        self._model = Bindable(wrappedValue: model)
        self._presentation = Bindable(wrappedValue: presentation)
        self.openHistory = openHistory
        self.closeDetail = closeDetail
        self.initialHistoryID = initialHistoryID
        self.detailOnly = detailOnly
        self.showsNavigationHeader = showsNavigationHeader
        self.showBackButton = showBackButton
        self.back = back
    }

    var body: some View {
        VStack(spacing: 0) {
            if detailOnly {
                detailContent
            } else {
                if showsNavigationHeader {
                    RideMapNavigationHeader(showBackButton: showBackButton, back: back)
                }

                HStack(spacing: 0) {
                    Picker(localizedAppText("navigation.section.map"), selection: $model.rideMapMode) {
                        Text(localizedAppText("ride_map.mode.live")).tag(CutoutAppModel.RideMapMode.live)
                        Text(localizedAppText("ride_map.mode.history")).tag(CutoutAppModel.RideMapMode.history)
                    }
                    .pickerStyle(.segmented)
                }
                .padding(.horizontal, 18)
                .padding(.bottom, 10)
                // SwiftUI does not consistently forward identifiers from a
                // segmented Picker itself to the UIKit accessibility tree. Keep
                // the picker as the source of truth, but expose a stable wrapper
                // for UI tests and assistive technology.
                .accessibilityElement(children: .contain)
                .accessibilityIdentifier("ride-map.mode-picker")

                if model.rideMapMode == .live {
                    liveContent
                } else {
                    historyContent
                }
            }
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .top)
        .background(PevColors.pageBackground)
        .foregroundStyle(PevColors.primaryText)
        .accessibilityIdentifier("ride-map.screen")
        .appMusicCompactPlayer(model: model)
#if os(iOS)
        // Ride Detail owns its compact header so it remains attached to the map
        // when the destination is pushed from either Map entry path. Leaving the
        // NavigationStack bar visible creates a second title band and pushes the
        // detail content toward the middle of the screen.
        .toolbar(.hidden, for: .navigationBar)
#endif
    }

    private var liveContent: some View {
        RideMapLiveContentView(
            displayPoints: model.rideMapLiveDisplayPoints,
            routeID: "live",
            projectionVersion: model.rideMapLiveProjectionVersion,
            endpointMetadata: model.rideMapLiveEndpointMetadata,
            cameraRegion: model.rideMapLiveCameraRegion,
            segments: model.rideMapLiveSegments,
            snapshot: model.rideMapSnapshot,
            availability: model.rideMapAvailability,
            speed: model.speed,
            vehicleName: model.rideMapVehicleName,
            mapError: model.rideMapLiveError,
            lastDecision: model.rideMapLastDecision,
            telemetryState: model.rideMapLiveTelemetryState,
            pointsTruncated: model.rideMapLivePointsTruncated,
            segmentsOmittedByBudget: model.rideMapLiveSegmentsOmittedByBudget,
            canonicalBackgroundGapCount: model.rideMapLiveBackgroundGapCount,
            mapPosition: $presentation.liveMapPosition,
            isApplyingCamera: $presentation.liveIsApplyingCamera,
            followsLatestPoint: $presentation.followsLatestPoint,
            pause: { _ = model.pauseRideMap() },
            resume: { _ = model.resumeRideMap() },
            save: { _ = model.saveRideMap() },
            stop: { _ = model.stopRideMap() },
            start: { _ = model.startGpsOnlyRide() },
            discard: { _ = model.discardRideMap() }
        )
    }

    private var historyContent: some View {
        RideMapHistoryContentView(
            isRecording: model.isRideMapRecording,
            isPaused: model.rideMapSnapshot?.state == .paused,
            rides: model.filteredRideMapHistory,
            searchText: $model.rideMapHistorySearchText,
            canLoadMore: model.rideMapHistoryCanLoadMore,
            displayPoints: model.rideMapHistoryDisplayPoints,
            cameraRegion: model.rideMapHistoryCameraRegion,
            endpointMetadata: model.rideMapHistoryEndpointMetadata,
            segments: model.rideMapHistorySegments,
            contextRoutes: model.rideMapHistoryContextRoutes,
            projectionVersion: model.rideMapHistoryProjectionVersion,
            pointsTruncated: model.rideMapHistoryPointsTruncated,
            segmentsOmittedByBudget: model.rideMapHistorySegmentsOmittedByBudget,
            isLoading: model.rideMapHistoryLoading,
            isRouteLoading: model.rideMapHistoryRouteLoading,
            historyError: model.rideMapHistoryError,
            routeError: model.rideMapHistoryRouteError,
            selectedRideID: model.selectedRideMapHistoryID,
            dateFilter: model.rideMapHistoryDateFilter,
            vehicleFilter: model.rideMapHistoryVehicleFilter,
            vehicleFilterOptions: model.rideMapHistoryVehicleIdentities,
            select: { rideID in
                model.rideMapMode = .history
                model.selectRideMapHistory(rideID)
                openHistory?(rideID)
            },
            load: { model.loadRideMapHistory(selecting: model.selectedRideMapHistoryID) },
            loadMore: { model.loadMoreRideMapHistory() },
            returnToLive: {
                model.rideMapMode = .live
            },
            setDateFilter: { model.setRideMapHistoryDateFilter($0) },
            setVehicleFilter: { model.setRideMapHistoryVehicleFilter($0) },
            clearFilters: { model.clearRideMapHistoryFilters() },
            currentVehicleIdentity: model.rideMapVehicleIdentity,
            currentVehicleName: model.rideMapVehicleName,
            vehicleName: model.rideMapVehicleName(for:),
            // Detail and list intentionally share the selected route data, but not a
            // viewport projection. A detail pan must not replace the list's display
            // projection while both destinations remain alive in the navigation stack.
            cameraDidChange: { _ in },
            mapPosition: $presentation.historyMapPosition,
            isApplyingCamera: $presentation.historyIsApplyingCamera
        )
    }

    private var detailContent: some View {
        RideMapHistoryDetailView(
            initialHistoryID: initialHistoryID,
            rides: model.rideMapHistory,
            displayPoints: model.rideMapHistoryDetailDisplayPoints,
            musicTimeline: model.rideMapHistoryDetailMusicTimeline,
            musicHistoryState: model.rideMapHistoryDetailMusicState,
            musicError: model.rideMapHistoryDetailMusicError,
            cameraRegion: model.rideMapHistoryDetailCameraRegion,
            endpointMetadata: model.rideMapHistoryDetailEndpointMetadata,
            segments: model.rideMapHistoryDetailSegments,
            projectionVersion: model.rideMapHistoryDetailProjectionVersion,
            pointsTruncated: model.rideMapHistoryDetailPointsTruncated,
            segmentsOmittedByBudget: model.rideMapHistoryDetailSegmentsOmittedByBudget,
            canonicalBackgroundGapCount: model.rideMapHistoryDetailBackgroundGapCount,
            historyError: model.rideMapHistoryError,
            routeError: model.rideMapHistoryDetailRouteError,
            isLoading: model.rideMapHistoryDetailRouteLoading,
            selectedHistoryID: model.selectedRideMapHistoryID,
            select: { model.selectRideMapHistory($0) },
            load: { model.loadRideMapHistory(selecting: initialHistoryID) },
            retry: { model.loadRideMapHistory(selecting: initialHistoryID) },
            loadRoutePreview: { model.loadRoutePreviewMapHistory() },
            forgetMusicHistory: { model.forgetMusicHistory(for: $0) },
            vehicleName: model.rideMapVehicleName(for:),
            cameraDidChange: { region in
                model.projectRideMapHistoryDetailViewport(RideMapCanvasView.geoBounds(for: region))
            },
            mapPosition: $presentation.detailMapPosition,
            isApplyingCamera: $presentation.detailIsApplyingCamera,
            close: closeDetail ?? { dismiss() }
        )
    }
}

private struct RideMapNavigationHeader: View {
    let showBackButton: Bool
    let back: (() -> Void)?

    @ScaledMetric(relativeTo: .largeTitle) private var headerFontSize: CGFloat = 32

    var body: some View {
        HStack(alignment: .firstTextBaseline, spacing: 12) {
            if showBackButton, let back {
                Button(action: back) {
                    Label(localizedAppText("ride_map.detail_back"), systemImage: "chevron.left")
                }
                .buttonStyle(.plain)
                .foregroundStyle(PevColors.yellow)
            }
            Text("CutOut")
                .font(.system(size: headerFontSize, weight: .black))
                .foregroundStyle(PevColors.yellow)

            Text(localizedAppText("navigation.section.map"))
                .font(.system(size: headerFontSize, weight: .bold))
                .foregroundStyle(PevColors.primaryText)

            Spacer(minLength: 0)
        }
        .padding(.horizontal, 20)
        .padding(.top, 12)
        .padding(.bottom, 14)
        .accessibilityElement(children: showBackButton && back != nil ? .contain : .combine)
        .accessibilityIdentifier("ride-map.navigation-header")
    }
}
