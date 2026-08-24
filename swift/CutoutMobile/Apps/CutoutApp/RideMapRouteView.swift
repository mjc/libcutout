import SwiftUI
import MapKit
import Observation
import CutoutMobile
import CutoutMobileFFI

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
        .preferredColorScheme(.dark)
        .accessibilityIdentifier("ride-map.screen")
        .toolbar {
            if detailOnly {
                ToolbarItem(placement: .cancellationAction) {
                    Button(localizedAppText("ride_map.detail_back"), action: closeDetail ?? { dismiss() })
                }
            }
        }
#if os(iOS)
        // The map route owns its header. Leaving the NavigationStack bar visible
        // adds a second, empty title band above the mockup header.
        .toolbar(detailOnly ? .visible : .hidden, for: .navigationBar)
#endif
        .navigationTitle(detailOnly ? localizedAppText("ride_map.detail_title") : "")
#if os(iOS)
        .navigationBarTitleDisplayMode(.inline)
#endif
    }

    private var liveContent: some View {
        RideMapLiveContentView(
            points: model.rideMapPoints,
            routeID: model.rideMapSnapshot?.rideId ?? "live",
            snapshot: model.rideMapSnapshot,
            availability: model.rideMapAvailability,
            speed: model.speed,
            vehicleName: model.rideMapVehicleName,
            storageError: model.rideMapStorageError,
            mapError: model.rideMapError,
            lastDecision: model.rideMapLastDecision,
            pointsTruncated: model.rideMapLivePointsTruncated,
            mapPosition: $presentation.liveMapPosition,
            isApplyingCamera: $presentation.liveIsApplyingCamera,
            followsLatestPoint: $presentation.followsLatestPoint,
            pause: { _ = model.pauseRideMap() },
            resume: { _ = model.resumeRideMap() },
            save: { _ = model.saveRideMap() },
            stop: { _ = model.stopRideMap() },
            start: { _ = model.startGpsOnlyRide() },
            discard: { _ = model.discardRideMap() },
            refreshDuration: { model.refreshRideMapDuration() }
        )
    }

    private var historyContent: some View {
        RideMapHistoryContentView(
            isRecording: model.isRideMapRecording,
            isPaused: model.rideMapSnapshot?.state == .paused,
            rides: model.filteredRideMapHistory,
            searchText: $model.rideMapHistorySearchText,
            canLoadMore: model.rideMapHistoryCanLoadMore,
            points: model.rideMapHistoryPoints,
            pointsTruncated: model.rideMapHistoryPointsTruncated,
            isLoading: model.rideMapHistoryLoading,
            isRouteLoading: model.rideMapHistoryRouteLoading,
            error: model.rideMapError,
            selectedRideID: model.selectedRideMapHistoryID,
            dateFilter: model.rideMapHistoryDateFilter,
            vehicleFilter: model.rideMapHistoryVehicleFilter,
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
            currentVehicleIdentity: model.rideMapVehicleIdentity,
            currentVehicleName: model.rideMapVehicleName,
            vehicleName: model.rideMapVehicleName(for:),
            mapPosition: $presentation.historyMapPosition,
            isApplyingCamera: $presentation.historyIsApplyingCamera
        )
    }

    private var detailContent: some View {
        RideMapHistoryDetailView(
            initialHistoryID: initialHistoryID,
            rides: model.rideMapHistory,
            points: model.rideMapHistoryPoints,
            pointsTruncated: model.rideMapHistoryPointsTruncated,
            error: model.rideMapError,
            isLoading: model.rideMapHistoryRouteLoading,
            select: { model.selectRideMapHistory($0) },
            load: { model.loadRideMapHistory(selecting: initialHistoryID) },
            retry: { model.loadRideMapHistory(selecting: initialHistoryID) },
            loadFullRide: { model.loadFullRideMapHistory() },
            vehicleName: model.rideMapVehicleName(for:),
            mapPosition: $presentation.detailMapPosition,
            isApplyingCamera: $presentation.detailIsApplyingCamera,
            close: closeDetail ?? { dismiss() }
        )
    }
}

private struct RideMapNavigationHeader: View {
    let showBackButton: Bool
    let back: (() -> Void)?

    var body: some View {
        HStack(alignment: .firstTextBaseline, spacing: 12) {
            if showBackButton {
                Button(action: { back?() }) {
                    Label(localizedAppText("ride_map.detail_back"), systemImage: "chevron.left")
                }
                .buttonStyle(.plain)
                .foregroundStyle(PevColors.yellow)
            }
            Text("CutOut")
                .font(.system(size: 32, weight: .black))
                .foregroundStyle(PevColors.yellow)

            Text(localizedAppText("navigation.section.map"))
                .font(.system(size: 32, weight: .bold))
                .foregroundStyle(PevColors.primaryText)

            Spacer(minLength: 0)
        }
        .padding(.horizontal, 20)
        .padding(.top, 12)
        .padding(.bottom, 14)
        .accessibilityElement(children: .combine)
        .accessibilityIdentifier("ride-map.navigation-header")
    }
}
