import SwiftUI
import CutoutMobile
import CutoutMobileFFI

struct RideMapRouteView: View {
    @Bindable var model: CutoutAppModel
    private let openHistory: ((String) -> Void)?
    private let closeDetail: (() -> Void)?
    private let initialHistoryID: String?
    private let detailOnly: Bool
    private let showBackButton: Bool
    private let back: (() -> Void)?
    @Environment(\.dismiss) private var dismiss
    @State private var mode: CutoutAppModel.RideMapMode

    init(
        model: CutoutAppModel,
        _ openHistory: ((String) -> Void)? = nil,
        initialHistoryID: String? = nil,
        detailOnly: Bool = false,
        closeDetail: (() -> Void)? = nil,
        showBackButton: Bool = false,
        back: (() -> Void)? = nil
    ) {
        self._model = Bindable(wrappedValue: model)
        self.openHistory = openHistory
        self.closeDetail = closeDetail
        self.initialHistoryID = initialHistoryID
        self.detailOnly = detailOnly
        self.showBackButton = showBackButton
        self.back = back
        _mode = State(initialValue: initialHistoryID == nil ? .live : .history)
    }

    var body: some View {
        VStack(spacing: 0) {
            if detailOnly {
                detailContent
            } else {
                RideMapNavigationHeader(showBackButton: showBackButton, back: back)

                Picker(localizedAppText("navigation.section.map"), selection: $mode) {
                    Text(localizedAppText("ride_map.mode.live")).tag(CutoutAppModel.RideMapMode.live)
                    Text(localizedAppText("ride_map.mode.history")).tag(CutoutAppModel.RideMapMode.history)
                }
                .pickerStyle(.segmented)
                .padding(.horizontal, 18)
                .padding(.bottom, 10)
                .accessibilityIdentifier("ride-map.mode-picker")

                if mode == .live {
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
        .onAppear { mode = model.rideMapMode }
        .onChange(of: mode) { _, newMode in model.rideMapMode = newMode }
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
            vehicleName: model.rideMapVehicleName,
            storageError: model.rideMapStorageError,
            mapError: model.rideMapError,
            lastDecision: model.rideMapLastDecision,
            pointsTruncated: model.rideMapLivePointsTruncated,
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
            selectedRideID: model.selectedRideMapHistoryID,
            select: { rideID in
                model.rideMapMode = .history
                model.selectRideMapHistory(rideID)
                openHistory?(rideID)
            },
            load: { model.loadRideMapHistory() },
            loadMore: { model.loadMoreRideMapHistory() },
            returnToLive: {
                mode = .live
                model.rideMapMode = .live
            },
            currentVehicleIdentity: model.rideMapVehicleIdentity,
            currentVehicleName: model.rideMapVehicleName,
            vehicleName: model.rideMapVehicleName(for:)
        )
    }

    private var detailContent: some View {
        RideMapHistoryDetailView(
            initialHistoryID: initialHistoryID,
            rides: model.rideMapHistory,
            points: model.rideMapHistoryPoints,
            pointsTruncated: model.rideMapHistoryPointsTruncated,
            select: { model.selectRideMapHistory($0) },
            load: { model.loadRideMapHistory(selecting: initialHistoryID) },
            loadFullRide: { model.loadFullRideMapHistory() },
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
