import SwiftUI
import CutoutMobile
import CutoutMobileFFI

struct RideMapRouteView: View {
    private enum Mode: String, CaseIterable {
        case live
        case history
    }

    @Bindable var model: CutoutAppModel
    private let openHistory: ((String) -> Void)?
    private let closeDetail: (() -> Void)?
    private let initialHistoryID: String?
    private let detailOnly: Bool
    @Environment(\.dismiss) private var dismiss
    @State private var mode: Mode

    init(
        model: CutoutAppModel,
        _ openHistory: ((String) -> Void)? = nil,
        initialHistoryID: String? = nil,
        detailOnly: Bool = false,
        closeDetail: (() -> Void)? = nil
    ) {
        self._model = Bindable(wrappedValue: model)
        self.openHistory = openHistory
        self.closeDetail = closeDetail
        self.initialHistoryID = initialHistoryID
        self.detailOnly = detailOnly
        _mode = State(initialValue: initialHistoryID == nil ? .live : .history)
    }

    var body: some View {
        VStack(spacing: 0) {
            if detailOnly {
                detailContent
            } else {
                RideMapNavigationHeader()

                Picker(localizedAppText("navigation.section.map"), selection: $mode) {
                    Text(localizedAppText("ride_map.mode.live")).tag(Mode.live)
                    Text(localizedAppText("ride_map.mode.history")).tag(Mode.history)
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
    }

    private var liveContent: some View {
        RideMapLiveContentView(
            points: model.rideMapPoints,
            routeID: model.rideMapSnapshot?.rideId ?? "live",
            snapshot: model.rideMapSnapshot,
            availability: model.rideMapAvailability,
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
            rides: model.filteredRideMapHistory,
            searchText: $model.rideMapHistorySearchText,
            canLoadMore: model.rideMapHistoryCanLoadMore,
            points: model.rideMapHistoryPoints,
            pointsTruncated: model.rideMapHistoryPointsTruncated,
            selectedRideID: model.selectedRideMapHistoryID,
            select: { rideID in
                model.selectRideMapHistory(rideID)
                openHistory?(rideID)
            },
            load: { model.loadRideMapHistory() },
            loadMore: { model.loadMoreRideMapHistory() },
            returnToLive: { mode = .live }
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
            close: closeDetail ?? { dismiss() }
        )
    }
}

private struct RideMapNavigationHeader: View {
    var body: some View {
        HStack(alignment: .firstTextBaseline, spacing: 12) {
            Text("CutOut")
                .font(.title3.weight(.black))
                .foregroundStyle(PevColors.yellow)

            Text(localizedAppText("navigation.section.map"))
                .font(.title3.weight(.bold))
                .foregroundStyle(PevColors.muted)

            Spacer(minLength: 0)
        }
        .padding(.horizontal, 20)
        .padding(.top, 8)
        .padding(.bottom, 12)
        .accessibilityElement(children: .combine)
        .accessibilityIdentifier("ride-map.navigation-header")
    }
}
